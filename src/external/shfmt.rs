use crate::lsp;
use crate::shell::Shell;

fn dialect_flag(shell: Shell) -> &'static str {
    // Treat unsupported shells as POSIX, since shfmt can still format with decent accuracy.
    // -ln is short for --language-dialect, and -p is short for -ln=posix.
    match shell {
        Shell::Bash => "-ln=bash",
        Shell::Ksh => "-ln=mksh",
        _ => "-p",
    }
}

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

pub fn format(
    shell: Shell,
    options: lsp::FormattingOptions,
    document_text: &str,
) -> std::io::Result<String> {
    use std::process::{Command, Stdio};

    // shfmt uses tabs if 0 is given as the indent width.
    let indent = if options.use_spaces { options.tab_size } else { 0 };

    let mut child = Command::new("shfmt")
        .arg(format!("--indent={indent}"))
        .arg(dialect_flag(shell))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    std::io::Write::write_all(&mut child.stdin.take().unwrap(), document_text.as_bytes())?;

    if child.wait()?.success() {
        std::io::read_to_string(child.stdout.take().unwrap()).map(remove_trailing_newline)
    }
    else {
        Err(std::io::Error::other(std::io::read_to_string(child.stderr.take().unwrap())?))
    }
}
