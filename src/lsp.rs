use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::path::PathBuf;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DocumentURI {
    pub path: PathBuf,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Debug, Serialize, Deserialize)]
pub struct Position {
    pub line: u32,
    #[serde(rename = "character")]
    pub column: u32,
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

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum Severity {
    Error = 1,
    Warning = 2,
    Information = 3,
    Hint = 4,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiagnosticRelated {
    pub location: Location,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Diagnostic {
    pub range: Range,
    pub severity: Severity,
    pub message: String,
    #[serde(rename = "relatedInformation")]
    pub related: Vec<DiagnosticRelated>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocumentIdentifier {
    pub uri: DocumentURI,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VersionedDocumentIdentifier {
    #[serde(flatten)]
    pub identifier: DocumentIdentifier,
    pub version: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocumentItem {
    pub uri: DocumentURI,
    #[serde(rename = "languageId")]
    pub language: String,
    pub text: String,
    pub version: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DidOpenDocumentParams {
    #[serde(rename = "textDocument")]
    pub document: DocumentItem,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DidCloseDocumentParams {
    #[serde(rename = "textDocument")]
    pub document: DocumentIdentifier,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContentChange {
    pub range: Range,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DidChangeDocumentParams {
    #[serde(rename = "textDocument")]
    pub document: VersionedDocumentIdentifier,
    #[serde(rename = "contentChanges")]
    pub changes: Vec<ContentChange>,
}

impl Position {
    pub fn advance(&mut self, char: char) {
        if char == '\n' {
            self.line += 1;
            self.column = 0;
        }
        else {
            self.column += 1;
        }
    }
}

impl Range {
    pub fn for_position(start: Position) -> Range {
        Range { start, end: Position { column: start.column + 1, ..start } }
    }
    pub fn contains(self, position: Position) -> bool {
        self.start <= position && position < self.end // End is exclusive
    }
}

impl Diagnostic {
    pub fn new(range: Range, severity: Severity, message: String) -> Diagnostic {
        Diagnostic { range, severity, message, related: Vec::new() }
    }
    pub fn error(range: Range, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(range, Severity::Error, message.into())
    }
    pub fn info(range: Range, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(range, Severity::Information, message.into())
    }
}

impl Serialize for DocumentURI {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&format!("file://{}", self.path.display()))
    }
}

struct DocumentURIVisitor;

impl<'de> serde::de::Visitor<'de> for DocumentURIVisitor {
    type Value = DocumentURI;
    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a URI with file:// scheme")
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
