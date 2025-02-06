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

pub fn collect_executables(path: &str) -> Vec<String> {
    let mut vec = std::env::split_paths(&path)
        .flat_map(std::fs::read_dir)
        .flat_map(|dir| dir.filter_map(Result::ok))
        .filter(|entry| entry.metadata().is_ok_and(is_executable))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect::<Vec<String>>();
    vec.sort_unstable();
    vec.dedup();
    vec
}

pub fn collect_path_executables() -> Vec<String> {
    let log = |error| {
        eprintln!("[debug] Could not read PATH: {error}");
    };
    std::env::var("PATH").map_err(log).as_deref().map(collect_executables).unwrap_or_default()
}

pub fn collect_variables() -> Vec<String> {
    std::env::vars_os().filter_map(|var| var.0.into_string().ok()).collect()
}
