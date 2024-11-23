#[derive(Clone, Copy)]
pub struct Complete {
    pub env_path: bool,
    pub env_vars: bool,
}

#[derive(Clone, Copy, Default)]
pub struct Config {
    pub complete: Complete,
    pub debug: bool,
}

impl Default for Complete {
    fn default() -> Complete {
        Complete { env_path: true, env_vars: true }
    }
}
