use crate::config;
use crate::shell::Shell;
use std::process::{Command, Stdio};

pub fn documentation(shell: Shell, name: &str, config: &config::Man) -> Option<String> {
    let sections = if shell == Shell::Posix { "1p,1" } else { "1,1p" };

    let mut child = Command::new("man")
        .args(["-s", sections])
        .args(config.arguments.as_slice())
        .args(["--", name])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .ok()?;

    let stdout = std::io::read_to_string(child.stdout.take().unwrap()).ok()?;
    child.wait().ok()?.success().then_some(stdout)
}
