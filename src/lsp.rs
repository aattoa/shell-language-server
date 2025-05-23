use crate::config;
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::path::PathBuf;

#[derive(Clone, PartialEq, Eq)]
pub struct DocumentURI {
    pub path: PathBuf,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    pub uri: DocumentURI,
    pub range: Range,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ReferenceKind {
    Read = 2,
    Write = 3,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Reference {
    pub range: Range,
    pub kind: ReferenceKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error = 1,
    Warning = 2,
    Information = 3,
    Hint = 4,
}

#[derive(Serialize)]
pub struct DiagnosticRelated {
    pub location: Location,
    pub message: String,
}

#[derive(Serialize)]
pub struct Diagnostic {
    pub range: Range,
    pub severity: Severity,
    pub source: &'static str,
    pub message: String,
    pub code: i32,
    #[serde(rename = "relatedInformation", skip_serializing_if = "Vec::is_empty")]
    pub related: Vec<DiagnosticRelated>,
}

#[derive(Deserialize)]
pub struct DocumentIdentifier {
    pub uri: DocumentURI,
}

#[derive(Deserialize)]
pub struct VersionedDocumentIdentifier {
    #[serde(flatten)]
    pub identifier: DocumentIdentifier,
    pub version: i32,
}

#[derive(Deserialize)]
pub struct DocumentItem {
    pub uri: DocumentURI,
    #[serde(rename = "languageId")]
    pub language: String,
    pub text: String,
    pub version: i32,
}

#[derive(Deserialize)]
pub struct ContentChange {
    pub range: Range,
    pub text: String,
}

#[derive(Deserialize)]
pub struct InitializeParams {
    #[serde(rename = "initializationOptions")]
    pub settings: Option<config::Settings>,
}

#[derive(Deserialize)]
pub struct DidOpenDocumentParams {
    #[serde(rename = "textDocument")]
    pub document: DocumentItem,
}

#[derive(Deserialize)]
pub struct DidChangeDocumentParams {
    #[serde(rename = "textDocument")]
    pub document: VersionedDocumentIdentifier,
    #[serde(rename = "contentChanges")]
    pub changes: Vec<ContentChange>,
}

#[derive(Deserialize)]
pub struct DocumentIdentifierParams {
    #[serde(rename = "textDocument")]
    pub document: DocumentIdentifier,
}

#[derive(Deserialize)]
pub struct DocumentIdentifierRangeParams {
    #[serde(rename = "textDocument")]
    pub document: DocumentIdentifier,
    pub range: Range,
}

#[derive(Deserialize)]
pub struct PositionParams {
    #[serde(rename = "textDocument")]
    pub document: DocumentIdentifier,
    pub position: Position,
}

#[derive(Deserialize)]
pub struct RenameParams {
    #[serde(flatten)]
    pub position_params: PositionParams,
    #[serde(rename = "newName")]
    pub new_name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MarkupKind {
    Plaintext,
    Markdown,
}

#[derive(Serialize)]
pub struct MarkupContent {
    pub kind: MarkupKind,
    pub value: String,
}

#[derive(Clone, Serialize)]
pub struct TextEdit {
    pub range: Range,
    #[serde(rename = "newText")]
    pub new_text: String,
}

#[derive(Clone, Copy)]
pub enum CompletionItemKind {
    Text = 1,
    Function = 3,
    Variable = 6,
    Snippet = 15,
    File = 17,
    Directory = 18,
}

#[derive(Serialize)]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionItemKind,
    #[serde(rename = "textEdit")]
    pub edit: TextEdit,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<MarkupContent>,
}

#[derive(Clone, Copy)]
pub enum SymbolKind {
    Function = 12,
    Variable = 13,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub range: Range,
    pub selection_range: Range,
}

#[derive(Deserialize)]
pub struct FormattingOptions {
    #[serde(rename = "tabSize")]
    pub tab_size: usize,
    #[serde(rename = "insertSpaces")]
    pub use_spaces: bool,
}

#[derive(Deserialize)]
pub struct FormattingParams {
    #[serde(rename = "textDocument")]
    pub document: DocumentIdentifier,
    pub options: FormattingOptions,
}

#[derive(Deserialize)]
pub struct RangeFormattingParams {
    #[serde(flatten)]
    pub format: FormattingParams,
    pub range: Range,
}

#[derive(Deserialize)]
pub struct SettingsContainer {
    pub shell: config::Settings,
}

#[derive(Deserialize)]
pub struct DidChangeConfigurationParams {
    pub settings: SettingsContainer,
}

#[derive(Clone, Copy)]
pub enum SemanticTokenKind {
    Keyword = 0,
    Parameter = 1,
    String = 2,
}

#[derive(Clone, Copy)]
pub enum SemanticTokenModifier {
    None = 0,
    Documentation = 1,
}

pub struct SemanticToken {
    pub position: Position,
    pub width: u32,
    pub kind: SemanticTokenKind,
    pub modifier: SemanticTokenModifier,
}

#[derive(Default)]
pub struct SemanticTokensData {
    pub data: Vec<SemanticToken>,
}

impl Position {
    pub fn advance(&mut self, char: char) {
        if char == '\n' {
            self.line += 1;
            self.character = 0;
        }
        else {
            self.character += 1;
        }
    }
    pub fn horizontal_offset(self, offset: u32) -> Self {
        Self { line: self.line, character: self.character + offset }
    }
}

impl Range {
    pub const MAX: Self = Self {
        start: Position { line: 0, character: 0 },
        end: Position { line: u32::MAX, character: u32::MAX },
    };
    pub fn for_position(start: Position) -> Self {
        Self { start, end: start.horizontal_offset(1) }
    }
    pub fn contains(self, position: Position) -> bool {
        self.start <= position && position < self.end // End is exclusive
    }
    pub fn contains_range(self, other: Self) -> bool {
        self.start <= other.start && other.end <= self.end
    }
}

impl Diagnostic {
    pub fn new(range: Range, severity: Severity, message: impl Into<String>) -> Self {
        Self {
            range,
            severity,
            source: "shell-language-server",
            message: message.into(),
            code: 0, // todo
            related: Vec::new(),
        }
    }
    pub fn error(range: Range, message: impl Into<String>) -> Self {
        Self::new(range, Severity::Error, message.into())
    }
    pub fn warning(range: Range, message: impl Into<String>) -> Self {
        Self::new(range, Severity::Warning, message.into())
    }
    pub fn info(range: Range, message: impl Into<String>) -> Self {
        Self::new(range, Severity::Information, message.into())
    }
}

impl MarkupContent {
    pub fn markdown(value: String) -> Self {
        Self { kind: MarkupKind::Markdown, value }
    }
    pub fn plaintext(value: String) -> Self {
        Self { kind: MarkupKind::Plaintext, value }
    }
}

impl Location {
    pub fn document(path: PathBuf) -> Self {
        Self { uri: DocumentURI { path }, range: Range::default() }
    }
}

impl Display for DocumentURI {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "file://{}", self.path.display())
    }
}

struct DocumentURIVisitor;

impl serde::de::Visitor<'_> for DocumentURIVisitor {
    type Value = DocumentURI;
    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a URI with file scheme")
    }
    fn visit_str<E: serde::de::Error>(self, str: &str) -> Result<DocumentURI, E> {
        let uri = |path| DocumentURI { path: PathBuf::from(path) };
        str.strip_prefix("file://").map(uri).ok_or_else(|| E::custom("bad URI scheme"))
    }
}

impl<'de> Deserialize<'de> for DocumentURI {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_str(DocumentURIVisitor)
    }
}

impl Serialize for DocumentURI {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl Serialize for SemanticTokensData {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let mut seq = s.serialize_seq(Some(self.data.len() * 5))?;
        let mut prev = Position::default();
        for &SemanticToken { position, width, kind, modifier } in &self.data {
            if position.line != prev.line {
                prev.character = 0;
            }
            let mut elem = |elem: u32| seq.serialize_element(&elem);
            elem(position.line - prev.line)?;
            elem(position.character - prev.character)?;
            elem(width)?;
            elem(kind as u32)?;
            elem(modifier as u32)?;
            prev = position;
        }
        seq.end()
    }
}

macro_rules! serialize_as_i32 {
    ($ty:ty) => {
        impl Serialize for $ty {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_i32(*self as i32)
            }
        }
    };
}

serialize_as_i32!(Severity);
serialize_as_i32!(CompletionItemKind);
serialize_as_i32!(ReferenceKind);
serialize_as_i32!(SymbolKind);
