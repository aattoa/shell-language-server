use std::io::Read;
use std::path::{Path, PathBuf};

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

fn executable_entries(directory: &Path) -> impl Iterator<Item = std::fs::DirEntry> {
    std::fs::read_dir(directory)
        .into_iter()
        .flat_map(|dir| dir.filter_map(Result::ok))
        .filter(|entry| entry.metadata().is_ok_and(is_executable))
}

pub fn path_directories() -> Option<Vec<PathBuf>> {
    std::env::var_os("PATH").map(|paths| std::env::split_paths(&paths).collect())
}

pub fn find_executable(name: &str, directory: &Path) -> Option<PathBuf> {
    executable_entries(directory).find(|entry| entry.file_name() == name).map(|entry| entry.path())
}

pub fn executable_names(directory: &Path) -> impl Iterator<Item = String> {
    executable_entries(directory).filter_map(|entry| entry.file_name().into_string().ok())
}

pub fn variables() -> impl Iterator<Item = String> {
    std::env::vars_os().filter_map(|var| var.0.into_string().ok())
}

pub fn is_script(path: &Path) -> bool {
    std::fs::File::open(path).is_ok_and(|mut file| {
        let mut buffer = [0u8; 3];
        file.read_exact(&mut buffer).is_ok() && buffer.as_slice() == b"#!/"
    })
}
