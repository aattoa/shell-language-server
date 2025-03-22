pub mod help;
pub mod man;
pub mod shellcheck;
pub mod shfmt;

pub fn exists(name: &str) -> bool {
    std::process::Command::new(name)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .arg("--version")
        .status()
        .is_ok_and(|status| status.success())
}
