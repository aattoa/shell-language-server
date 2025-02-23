use crate::lsp;
use crate::shell::Shell;

pub fn format(
    shell: Shell,
    options: lsp::FormattingOptions,
    shfmt_path: &str,
    document_text: &str,
) -> std::io::Result<String> {
    use std::io::{Read, Write};
    use std::process::{Command, Stdio};

    // shfmt uses tabs if 0 is given as the indent width.
    let indent = if options.use_spaces { options.tab_size } else { 0 };

    // Treat unsupported shells as POSIX, since shfmt can still format with decent accuracy.
    let dialect = match shell {
        Shell::Bash => "bash",
        Shell::Ksh => "mksh",
        _ => "posix",
    };

    let mut child = Command::new(shfmt_path)
        .arg(format!("--indent={indent}"))
        .arg(format!("--language-dialect={dialect}"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut string = String::new();
    child.stdin.take().unwrap().write_all(document_text.as_bytes())?;
    child.stdout.take().unwrap().read_to_string(&mut string)?;

    if child.wait()?.success() {
        Ok(string)
    }
    else {
        let mut error = String::new();
        child.stderr.take().unwrap().read_to_string(&mut error)?;
        Err(std::io::Error::other(error))
    }
}
