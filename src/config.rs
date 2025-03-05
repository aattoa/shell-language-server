use crate::shell::Shell;
use std::borrow::Cow;

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
pub struct Executables {
    pub sh: Cow<'static, str>,
    pub zsh: Cow<'static, str>,
    pub bash: Cow<'static, str>,
    pub shellcheck: Cow<'static, str>,
    pub shfmt: Cow<'static, str>,
    pub man: Cow<'static, str>,
}

impl Default for Executables {
    fn default() -> Self {
        Self {
            sh: Cow::Borrowed("/bin/sh"),
            zsh: Cow::Borrowed("/bin/zsh"),
            bash: Cow::Borrowed("/bin/bash"),
            shellcheck: Cow::Borrowed("/usr/bin/shellcheck"),
            shfmt: Cow::Borrowed("/usr/bin/shfmt"),
            man: Cow::Borrowed("/usr/bin/man"),
        }
    }
}

#[derive(Clone, Copy)]
pub struct Integration {
    pub shellcheck: bool,
    pub shfmt: bool,
    pub man: bool,
    pub help: bool,
}

impl Default for Integration {
    fn default() -> Self {
        Self { shellcheck: true, shfmt: false, man: true, help: true }
    }
}

#[derive(Clone, Default)]
pub struct Config {
    pub debug: bool,
    pub complete: Complete,
    pub path: Option<Box<str>>,
    pub executables: Executables,
    pub integration: Integration,
    pub default_shell: Shell,
}
