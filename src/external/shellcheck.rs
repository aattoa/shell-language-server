use crate::shell::Shell;
use crate::{config, db, lsp};
use std::io::Write;
use std::process::{Command, Stdio};

struct LevelVisitor;

impl serde::de::Visitor<'_> for LevelVisitor {
    type Value = lsp::Severity;
    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a shellcheck diagnostic severity level")
    }
    fn visit_str<E: serde::de::Error>(self, level: &str) -> Result<lsp::Severity, E> {
        Ok(match level {
            "error" => lsp::Severity::Error,
            "warning" => lsp::Severity::Warning,
            "info" => lsp::Severity::Information,
            "style" => lsp::Severity::Hint,
            _ => {
                // Better to accept unknown levels than to outright fail, since
                // it is not inconceivable that Shellcheck might introduces new ones.
                eprintln!("Unknown Shellcheck severity level: '{level}'. Defaulting to error.");
                lsp::Severity::Error
            }
        })
    }
}

fn deserialize_level<'de, D: serde::Deserializer<'de>>(d: D) -> Result<lsp::Severity, D::Error> {
    d.deserialize_str(LevelVisitor)
}

#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Range {
    line: u32,
    column: u32,
    end_line: u32,
    end_column: u32,
}

#[derive(serde::Deserialize)]
struct Replacement {
    #[serde(flatten)]
    range: Range,
    #[serde(rename = "replacement")]
    new_text: String,
}

#[derive(serde::Deserialize)]
struct Fix {
    replacements: Vec<Replacement>,
}

/// The Shellcheck manual refers to diagnostics as "comments".
#[derive(serde::Deserialize)]
struct Comment {
    #[serde(flatten)]
    range: Range,
    #[serde(deserialize_with = "deserialize_level")]
    level: lsp::Severity,
    code: i32,
    message: String,
}

#[derive(serde::Deserialize)]
struct Item {
    #[serde(flatten)]
    comment: Comment,
    fix: Option<Fix>,
}

fn range(range: Range) -> lsp::Range {
    lsp::Range {
        start: lsp::Position { line: range.line - 1, character: range.column - 1 },
        end: lsp::Position { line: range.end_line - 1, character: range.end_column - 1 },
    }
}

fn diagnostic(comment: Comment) -> lsp::Diagnostic {
    lsp::Diagnostic {
        range: range(comment.range),
        severity: comment.level,
        source: "shellcheck",
        message: comment.message,
        code: comment.code,
        related: Vec::new(),
    }
}

fn text_edit(replacement: Replacement) -> lsp::TextEdit {
    lsp::TextEdit { range: range(replacement.range), new_text: replacement.new_text }
}

/// Identify diagnostics that are not helpful by themselves.
fn is_context_diagnostic(code: i32) -> bool {
    matches!(code, 1009 | 1072 | 1073) // https://www.shellcheck.net/wiki
}

#[derive(Default)]
pub struct Info {
    pub diagnostics: Vec<lsp::Diagnostic>,
    pub actions: Vec<db::Action>,
}

fn info(items: Vec<Item>) -> Info {
    let mut info = Info {
        diagnostics: Vec::with_capacity(items.len()),
        actions: Vec::with_capacity(items.len()),
    };
    for Item { comment, fix } in items {
        if let Some(fix) = fix {
            info.actions.push(db::Action {
                kind: db::ActionKind::Edit {
                    title: format!("SC{}: {}", comment.code, comment.message),
                    edits: fix.replacements.into_iter().map(text_edit).collect(),
                },
                range: range(comment.range),
            });
        }
        if is_context_diagnostic(comment.code) {
            continue;
        }
        info.actions.push(db::Action {
            kind: db::ActionKind::DisableShellcheck { code: comment.code },
            range: range(comment.range),
        });
        info.diagnostics.push(diagnostic(comment));
    }
    info
}

fn shell_flag(shell: Shell, config: &config::Shellcheck) -> Option<&'static str> {
    match shell {
        Shell::Ksh => Some("--shell=ksh"),
        Shell::Bash => Some("--shell=bash"),
        Shell::Posix => Some("--shell=sh"),
        _ => config.posix_fallback.then_some("--shell=sh"),
    }
}

pub fn analyze(text: &str, shell: Shell, config: &config::Shellcheck) -> std::io::Result<Info> {
    let Some(shell_flag) = shell_flag(shell, config)
    else {
        return Ok(Info::default());
    };

    let mut child = Command::new("shellcheck")
        .args(config.arguments.as_slice())
        .args([shell_flag, "--format=json", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    child.stdin.take().unwrap().write_all(text.as_bytes())?;
    let items: Vec<Item> = serde_json::from_reader(child.stdout.take().unwrap())?;

    child.wait()?;
    Ok(info(items))
}
