#[derive(Clone, Copy)]
pub struct Complete {
    pub env_path: bool,
    pub env_vars: bool,
}

#[derive(Clone, Default)]
pub struct Config {
    pub complete: Complete,
    pub debug: bool,
    pub path: Option<String>,
}

impl Default for Complete {
    fn default() -> Self {
        Self { env_path: true, env_vars: true }
    }
}
