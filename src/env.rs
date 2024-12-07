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

fn sorted<T: Ord>(mut vec: Vec<T>) -> Vec<T> {
    vec.sort_unstable();
    vec.dedup();
    vec
}

pub fn collect_path_executables() -> Vec<String> {
    match std::env::var("PATH") {
        Ok(path) => sorted(
            std::env::split_paths(&path)
                .flat_map(std::fs::read_dir)
                .flat_map(|dir| dir.filter_map(Result::ok))
                .filter(|entry| entry.metadata().is_ok_and(is_executable))
                .filter_map(|entry| entry.file_name().into_string().ok())
                .collect(),
        ),
        Err(error) => {
            eprintln!("[debug] Could not read PATH: {error}");
            Vec::new()
        }
    }
}

pub fn collect_variables() -> Vec<String> {
    std::env::vars_os().filter_map(|(name, _)| name.into_string().ok()).collect()
}
