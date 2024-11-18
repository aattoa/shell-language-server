use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug, Serialize, Deserialize)]
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
    pub uri: String,
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

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct DiagnosticRelated {
    pub location: Location,
    pub message: String,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Diagnostic {
    pub range: Range,
    pub severity: Severity,
    pub message: String,
    #[serde(rename = "relatedInformation")]
    pub related: Vec<DiagnosticRelated>,
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
