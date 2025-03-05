use crate::db::{Symbol, SymbolKind};
use std::path::PathBuf;

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

fn path_symbol(path: PathBuf) -> Option<Symbol> {
    path.file_name()
        .and_then(|name| name.to_str().map(String::from))
        .map(|name| Symbol::new(name, SymbolKind::Command { path: Some(path) }))
}

pub fn executables(path: &str) -> impl Iterator<Item = Symbol> {
    let mut paths = std::env::split_paths(path)
        .flat_map(std::fs::read_dir)
        .flat_map(|dir| dir.filter_map(Result::ok))
        .filter(|entry| entry.metadata().is_ok_and(is_executable))
        .map(|entry| entry.path())
        .collect::<Vec<PathBuf>>();
    paths.sort_unstable();
    paths.dedup();
    paths.into_iter().filter_map(path_symbol)
}

pub fn variables() -> impl Iterator<Item = Symbol> {
    std::env::vars_os().filter_map(|var| var.0.into_string().ok()).map(|name| {
        Symbol::new(name, SymbolKind::Variable { description: None, first_assign_line: None })
    })
}
