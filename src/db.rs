use crate::indexvec::IndexVec;
use crate::shell::Shell;
use crate::{db, define_index, lsp};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

define_index!(pub SymbolId as u32);
define_index!(pub FunctionId as u32);
define_index!(pub VariableId as u32);
define_index!(pub DocumentId as u32);

#[derive(Clone, Copy)]
pub struct View {
    pub start: u32,
    pub end: u32,
}

#[derive(Clone, Copy)]
pub struct Location {
    pub range: lsp::Range,
    pub view: db::View,
}

#[derive(Clone, Copy)]
pub enum VariableKind {
    Global,
    Local,
    Environment,
}

pub struct Variable {
    pub description: Option<String>,
    pub first_assignment: Option<Location>,
    pub kind: VariableKind,
}

pub struct Function {
    pub description: Option<String>,
    pub definition: Option<Location>,
    pub parameters: Vec<db::View>,
}

#[derive(Clone, Copy)]
pub enum SymbolKind {
    Variable(VariableId),
    Function(FunctionId),
    Command,
    Builtin,
}

pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub ref_indices: Vec<u32>,
}

#[derive(Clone, Copy)]
pub struct SymbolReference {
    pub reference: lsp::Reference,
    pub id: SymbolId,
}

pub struct Action {
    pub title: String,
    pub edits: Vec<lsp::TextEdit>,
    pub range: lsp::Range,
}

#[derive(Default)]
pub struct DocumentInfo {
    pub diagnostics: Vec<lsp::Diagnostic>,
    pub references: Vec<SymbolReference>,
    pub functions: IndexVec<Function, FunctionId>,
    pub variables: IndexVec<Variable, VariableId>,
    pub symbols: IndexVec<Symbol, SymbolId>,
    pub actions: Vec<Action>,
    pub tokens: lsp::SemanticTokensData,
    pub shell: Shell,
}

#[derive(Default)]
pub struct Document {
    pub text: String,
    pub info: DocumentInfo,
}

#[derive(Default)]
pub struct Database {
    pub documents: IndexVec<Document, DocumentId>,
    pub document_paths: HashMap<PathBuf, DocumentId>,
}

pub fn text_range(text: &str, range: lsp::Range) -> std::ops::Range<usize> {
    let mut chars = text.chars();
    let mut begin = 0;

    for _ in 0..range.start.line {
        for char in chars.by_ref() {
            begin += char.len_utf8();
            if char == '\n' {
                break;
            }
        }
    }

    for char in chars.by_ref().take(range.start.character as usize) {
        begin += char.len_utf8();
    }

    let mut end = begin;
    let mut pos = range.start;

    while pos != range.end {
        let char = chars.next().expect("invalid range");
        pos.advance(char);
        end += char.len_utf8();
    }

    begin..end
}

impl Database {
    pub fn open(&mut self, path: PathBuf, document: Document) {
        self.document_paths.insert(path, self.documents.push(document));
    }
    pub fn close(&mut self, path: &Path) {
        self.documents[self.document_paths[path]] = Document::default();
    }
}

impl DocumentInfo {
    pub fn new_variable(&mut self, name: String, variable: Variable) -> SymbolId {
        self.symbols.push(Symbol::new(name, SymbolKind::Variable(self.variables.push(variable))))
    }
    pub fn new_function(&mut self, name: String, function: Function) -> SymbolId {
        self.symbols.push(Symbol::new(name, SymbolKind::Function(self.functions.push(function))))
    }
    pub fn new_command(&mut self, name: String) -> SymbolId {
        self.symbols.push(Symbol::new(name, SymbolKind::Command))
    }
}

impl Document {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into(), info: DocumentInfo::default() }
    }
    pub fn edit(&mut self, range: lsp::Range, new_text: &str) {
        self.text.replace_range(text_range(&self.text, range), new_text);
    }
}

impl Symbol {
    pub fn new(name: String, kind: SymbolKind) -> Self {
        Self { name, kind, ref_indices: Vec::new() }
    }
}

impl SymbolReference {
    pub fn read(range: lsp::Range, id: SymbolId) -> Self {
        Self { reference: lsp::Reference { range, kind: lsp::ReferenceKind::Read }, id }
    }
    pub fn write(range: lsp::Range, id: SymbolId) -> Self {
        Self { reference: lsp::Reference { range, kind: lsp::ReferenceKind::Write }, id }
    }
}

impl Variable {
    pub fn new(kind: VariableKind) -> Self {
        Self { description: None, first_assignment: None, kind }
    }
}

impl View {
    pub fn string(self, str: &str) -> &str {
        &str[(self.start as usize)..(self.end as usize)]
    }
}

#[cfg(test)]
mod tests {
    use crate::lsp;

    #[test]
    fn edit_document() {
        let pos = |line, character| lsp::Position { line, character };
        let range = |start, end| lsp::Range { start, end };

        let mut document = super::Document::new("lo");
        assert_eq!(document.text, "lo");
        document.edit(range(pos(0, 0), pos(0, 0)), "hel");
        assert_eq!(document.text, "hello");
        document.edit(range(pos(0, 5), pos(0, 5)), ", world");
        assert_eq!(document.text, "hello, world");
        document.edit(range(pos(0, 5), pos(0, 7)), "");
        assert_eq!(document.text, "helloworld");
        document.edit(range(pos(0, 5), pos(0, 5)), "\n\n");
        assert_eq!(document.text, "hello\n\nworld");
        document.edit(range(pos(0, 5), pos(1, 0)), "\n\n");
        assert_eq!(document.text, "hello\n\n\nworld");
    }
}
