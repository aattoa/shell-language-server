use crate::shell::Shell;
use crate::{config, lsp};
use std::io::Write;
use std::process::{Command, Stdio};

fn remove_trailing_newline(mut string: String) -> String {
    match string.pop() {
        None | Some('\n') => {}
        Some(char) => {
            eprintln!("[debug] shfmt output did not end with a newline");
            string.push(char);
        }
    }
    string
}

fn dialect_flag(shell: Shell, config: &config::Shfmt) -> Option<&'static str> {
    // -ln is short for --language-dialect, and -p is short for -ln=posix.
    match shell {
        Shell::Ksh => Some("-ln=mksh"),
        Shell::Bash => Some("-ln=bash"),
        Shell::Posix => Some("-p"),
        _ => config.posix_fallback.then_some("-p"),
    }
}

pub fn format(
    text: &str,
    shell: Shell,
    config: &config::Shfmt,
    options: lsp::FormattingOptions,
) -> std::io::Result<Option<String>> {
    let Some(dialect_flag) = dialect_flag(shell, config)
    else {
        return Ok(None);
    };

    // shfmt uses tabs if 0 is given as the indent width.
    let indent = if options.use_spaces { options.tab_size } else { 0 };

    let mut child = Command::new("shfmt")
        .args(["--indent", indent.to_string().as_str(), dialect_flag])
        .args(config.arguments.as_slice())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    child.stdin.take().unwrap().write_all(text.as_bytes())?;
    let stdout = std::io::read_to_string(child.stdout.take().unwrap())?;

    if child.wait()?.success() {
        Ok(Some(remove_trailing_newline(stdout)))
    }
    else {
        Err(std::io::Error::other(std::io::read_to_string(child.stderr.take().unwrap())?))
    }
}
