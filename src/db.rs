use crate::{ast, lsp, parse};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Default)]
pub struct DocumentInfo {
    pub diagnostics: Vec<lsp::Diagnostic>,
    pub functions: HashMap<String, Vec<lsp::Reference>>,
    pub variables: HashMap<String, Vec<lsp::Reference>>,
    pub commands: HashMap<String, Vec<lsp::Reference>>,
}

pub struct Document {
    pub text: String,
    pub info: DocumentInfo,
}

#[derive(Default)]
pub struct Database {
    pub documents: HashMap<PathBuf, Document>,
    pub path_executables: Vec<String>,
    pub environment_variables: Vec<String>,
}

impl Document {
    pub fn new(text: impl Into<String>) -> Document {
        Document { text: text.into(), info: DocumentInfo::default() }
    }
    pub fn edit(&mut self, range: lsp::Range, new_text: &str) {
        self.text.replace_range(text_range(&self.text, range), new_text);
    }
    pub fn analyze(&mut self) {
        self.info = parse::parse(&self.text).info
    }
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

    for char in chars.by_ref().take(range.start.column as usize) {
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

fn add_reference(refs: &mut Vec<lsp::Reference>, new: lsp::Reference) {
    if let Err(index) = refs.binary_search_by(|rf| rf.range.start.cmp(&new.range.start)) {
        refs.insert(index, new);
    }
}

impl DocumentInfo {
    pub fn add_variable_read(&mut self, id: ast::Identifier) {
        add_reference(self.variables.entry(id.name).or_default(), lsp::Reference::read(id.range))
    }
    pub fn add_variable_write(&mut self, id: ast::Identifier) {
        add_reference(self.variables.entry(id.name).or_default(), lsp::Reference::write(id.range));
    }
    pub fn add_function_definition(&mut self, id: ast::Identifier) {
        let references = vec![lsp::Reference::write(id.range)];
        if self.functions.insert(id.name, references).is_some() {
            let message = "Function redefinition is not yet supported";
            self.diagnostics.push(lsp::Diagnostic::error(id.range, message));
        }
    }
    pub fn add_command_reference(&mut self, id: ast::Identifier) {
        if let Some(references) = self.functions.get_mut(&id.name) {
            add_reference(references, lsp::Reference::read(id.range));
        }
        else {
            self.commands.entry(id.name).or_default().push(lsp::Reference::read(id.range));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_document() {
        let pos = |line, column| lsp::Position { line, column };
        let range = |start, end| lsp::Range { start, end };

        let mut document = Document::new("lo");
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
