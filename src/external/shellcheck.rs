use crate::lsp;

struct LevelVisitor;

impl serde::de::Visitor<'_> for LevelVisitor {
    type Value = lsp::Severity;
    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a shellcheck diagnostic severity level")
    }
    fn visit_str<E: serde::de::Error>(self, str: &str) -> Result<lsp::Severity, E> {
        match str {
            "error" => Ok(lsp::Severity::Error),
            "warning" => Ok(lsp::Severity::Warning),
            "info" => Ok(lsp::Severity::Information),
            "style" => Ok(lsp::Severity::Hint),
            _ => Err(E::custom("bad severity level")),
        }
    }
}

fn deserialize_level<'de, D: serde::Deserializer<'de>>(d: D) -> Result<lsp::Severity, D::Error> {
    d.deserialize_str(LevelVisitor)
}

#[derive(serde::Deserialize)]
struct Comment {
    line: u32,
    column: u32,
    #[serde(rename = "endLine")]
    end_line: u32,
    #[serde(rename = "endColumn")]
    end_column: u32,
    #[serde(deserialize_with = "deserialize_level")]
    level: lsp::Severity,
    code: i32,
    message: String,
}

fn diagnostic(comment: Comment) -> lsp::Diagnostic {
    lsp::Diagnostic {
        range: lsp::Range {
            start: lsp::Position { line: comment.line - 1, character: comment.column - 1 },
            end: lsp::Position { line: comment.end_line - 1, character: comment.end_column - 1 },
        },
        severity: comment.level,
        source: "shellcheck",
        message: comment.message,
        code: comment.code,
        related: Vec::new(),
    }
}

pub struct Info {
    pub diagnostics: Vec<lsp::Diagnostic>,
}

pub fn analyze(path: &str, document: &str) -> std::io::Result<Info> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new(path)
        .args(["--format=json", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    child.stdin.take().unwrap().write_all(document.as_bytes())?;
    let reader = std::io::BufReader::new(child.stdout.take().unwrap());

    child.wait()?;

    let comments: Vec<Comment> = serde_json::from_reader(reader)?;
    Ok(Info { diagnostics: comments.into_iter().map(diagnostic).collect() })
}
