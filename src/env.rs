#[cfg(unix)]
fn is_executable(data: std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    !data.is_dir() && data.permissions().mode() & 0o100 != 0
}

#[cfg(not(unix))]
fn is_executable(data: std::fs::Metadata) -> bool {
    // Checking for execute permissions on non-unix systems is difficult.
    // Just assume every file is executable.
    !data.is_dir()
}

fn executable_entries(path: &str) -> impl Iterator<Item = std::fs::DirEntry> + '_ {
    std::env::split_paths(path)
        .flat_map(std::fs::read_dir)
        .flat_map(|dir| dir.filter_map(Result::ok))
        .filter(|entry| entry.metadata().is_ok_and(is_executable))
}

pub fn path_variable() -> Option<String> {
    std::env::var("PATH").inspect_err(|error| eprintln!("Could not read $PATH: {error}")).ok()
}

pub fn find_executable(name: &str, path: &str) -> Option<std::path::PathBuf> {
    executable_entries(path).find(|entry| entry.file_name() == name).map(|entry| entry.path())
}

pub fn executables(path: &str) -> impl Iterator<Item = String> {
    let mut names: Vec<String> =
        executable_entries(path).filter_map(|entry| entry.file_name().into_string().ok()).collect();
    names.sort_unstable();
    names.dedup();
    names.into_iter()
}

pub fn variables() -> impl Iterator<Item = String> {
    std::env::vars_os().filter_map(|var| var.0.into_string().ok())
}
