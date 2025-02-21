#[derive(Clone, Copy)]
pub struct Complete {
    pub env_path: bool,
    pub env_vars: bool,
}

impl Default for Complete {
    fn default() -> Self {
        Self { env_path: true, env_vars: true }
    }
}

#[derive(Clone)]
pub enum Shellcheck {
    Enable(bool),
    Path(Box<str>),
}

impl Default for Shellcheck {
    fn default() -> Self {
        Self::Enable(true)
    }
}

impl Shellcheck {
    pub fn path(&self) -> Option<&str> {
        match self {
            Shellcheck::Enable(false) => None,
            Shellcheck::Enable(true) => Some("/usr/bin/shellcheck"),
            Shellcheck::Path(path) => Some(path),
        }
    }
}

#[derive(Clone, Default)]
pub struct Config {
    pub debug: bool,
    pub complete: Complete,
    pub path: Option<Box<str>>,
    pub shellcheck: Shellcheck,
}
