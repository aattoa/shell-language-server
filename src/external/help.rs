use crate::shell::Shell;
use std::io::Write;
use std::process::{Command, Stdio};

fn zsh_help(name: &str, shell: &str) -> Option<String> {
    let mut child = Command::new(shell)
        .env("PAGER", "cat") // The run-help script uses `more` if `PAGER` is not set.
        .args(["-r", "-s", "--", name])
        .stdout(Stdio::piped())
        .stdin(Stdio::piped())
        .spawn()
        .ok()?;

    const SCRIPT: &[u8] = b"unalias run-help\nautoload -Uz run-help\nrun-help \"$1\"";
    child.stdin.take()?.write_all(SCRIPT).ok()?;

    let stdout = std::io::read_to_string(child.stdout.take()?).ok()?;
    child.wait().ok()?.success().then_some(stdout)
}

fn posix_help(name: &str, shell: &str) -> Option<String> {
    let mut child = Command::new(shell)
        .args(["-c", "help \"$1\"", "--", name])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .ok()?;

    let stdout = std::io::read_to_string(child.stdout.take()?).ok()?;
    child.wait().ok()?.success().then_some(stdout)
}

pub fn documentation(shell: Shell, name: &str) -> Option<String> {
    match shell {
        Shell::Zsh => zsh_help(name, "zsh"),
        Shell::Bash => posix_help(name, "bash"),
        _ => posix_help(name, "sh"),
    }
}
