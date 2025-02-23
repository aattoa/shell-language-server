use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::path::PathBuf;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DocumentURI {
    pub path: PathBuf,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Debug, Serialize, Deserialize)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug, Serialize, Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Location {
    pub uri: DocumentURI,
    pub range: Range,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ReferenceKind {
    Read = 2,
    Write = 3,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
pub struct Reference {
    pub range: Range,
    pub kind: ReferenceKind,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Severity {
    Error = 1,
    Warning = 2,
    Information = 3,
    Hint = 4,
}

#[derive(Debug, Serialize)]
pub struct DiagnosticRelated {
    pub location: Location,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct Diagnostic {
    pub range: Range,
    pub severity: Severity,
    pub source: &'static str,
    pub message: String,
    pub code: i32,
    #[serde(rename = "relatedInformation")]
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

#[derive(Serialize, Deserialize)]
pub struct ContentChange {
    pub range: Range,
    pub text: String,
}

#[derive(Deserialize)]
pub struct DidOpenDocumentParams {
    #[serde(rename = "textDocument")]
    pub document: DocumentItem,
}

#[derive(Deserialize)]
pub struct DidCloseDocumentParams {
    #[serde(rename = "textDocument")]
    pub document: DocumentIdentifier,
}

#[derive(Deserialize)]
pub struct DidChangeDocumentParams {
    #[serde(rename = "textDocument")]
    pub document: VersionedDocumentIdentifier,
    #[serde(rename = "contentChanges")]
    pub changes: Vec<ContentChange>,
}

#[derive(Deserialize)]
pub struct PullDiagnosticParams {
    #[serde(rename = "textDocument")]
    pub document: DocumentIdentifier,
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

pub enum MarkupKind {
    Plaintext,
    Markdown,
}

#[derive(Serialize)]
pub struct MarkupContent {
    pub kind: MarkupKind,
    pub value: String,
}

#[derive(Serialize)]
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
}

impl Range {
    pub const MAX: Self = Self {
        start: Position { line: 0, character: 0 },
        end: Position { line: u32::MAX, character: u32::MAX },
    };
    pub fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }
    pub fn for_position(start: Position) -> Self {
        Self { start, end: Position { line: start.line, character: start.character + 1 } }
    }
    pub fn contains(self, position: Position) -> bool {
        self.start <= position && position < self.end // End is exclusive
    }
}

impl Reference {
    pub fn read(range: Range) -> Self {
        Self { range, kind: ReferenceKind::Read }
    }
    pub fn write(range: Range) -> Self {
        Self { range, kind: ReferenceKind::Write }
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

impl Serialize for Severity {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_i32(*self as i32)
    }
}

impl Serialize for CompletionItemKind {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_i32(*self as i32)
    }
}

impl Serialize for ReferenceKind {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_i32(*self as i32)
    }
}

impl Serialize for MarkupKind {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(match self {
            MarkupKind::Plaintext => "plaintext",
            MarkupKind::Markdown => "markdown",
        })
    }
}

impl Display for DocumentURI {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "file://{}", self.path.display())
    }
}

impl Serialize for DocumentURI {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
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
