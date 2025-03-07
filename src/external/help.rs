use crate::shell::Shell;
use std::io::Write;
use std::process::{Command, Stdio};

fn zsh_help(name: &str, shell: &str) -> Option<String> {
    let mut child = Command::new(shell)
        .env("PAGER", "cat") // The run-help script uses `more` if `PAGER` is not set.
        .arg("-r") // Enable restricted mode. (principle of least privilege)
        .arg("-s") // Read commands from stdin.
        .stdout(Stdio::piped())
        .stdin(Stdio::piped())
        .spawn()
        .ok()?;

    let script = format!("unalias run-help\nautoload -Uz run-help\nrun-help '{name}'");
    child.stdin.take()?.write_all(script.as_bytes()).ok()?;

    let stdout = std::io::read_to_string(child.stdout.take()?).ok()?;
    child.wait().ok()?.success().then_some(stdout)
}

fn posix_help(name: &str, shell: &str) -> Option<String> {
    let mut child = Command::new(shell)
        .arg("-c") // Read commands from the first non-option argument.
        .arg(format!("help '{name}'"))
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
