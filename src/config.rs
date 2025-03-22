use crate::shell::{Shell, parse_shell_name};

#[derive(serde::Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Shellcheck {
    pub enable: bool,
    pub posix_fallback: bool,
    pub arguments: Vec<String>,
}

#[derive(serde::Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Shfmt {
    pub enable: bool,
    pub posix_fallback: bool,
    pub arguments: Vec<String>,
}

#[derive(serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Help {
    pub enable: bool,
}

#[derive(serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Man {
    pub enable: bool,
    pub arguments: Vec<String>,
}

#[derive(Default, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Integrate {
    pub shellcheck: Shellcheck,
    pub shfmt: Shfmt,
    pub help: Help,
    pub man: Man,
}

#[derive(serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Environment {
    pub path: Option<Vec<std::path::PathBuf>>,
    pub variables: bool,
    pub executables: bool,
}

#[derive(Default, serde::Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub integrate: Integrate,
    pub environment: Environment,
    #[serde(deserialize_with = "deserialize_shell")]
    pub default_shell: Shell,
}

#[derive(Default)]
pub struct Cmdline {
    pub debug: bool,
    pub settings: Settings,
}

impl Default for Shellcheck {
    fn default() -> Self {
        Self { enable: true, posix_fallback: true, arguments: Vec::new() }
    }
}

impl Default for Shfmt {
    fn default() -> Self {
        Self { enable: true, posix_fallback: true, arguments: Vec::new() }
    }
}

impl Default for Help {
    fn default() -> Self {
        Self { enable: true }
    }
}

impl Default for Man {
    fn default() -> Self {
        Self { enable: true, arguments: Vec::new() }
    }
}

impl Default for Environment {
    fn default() -> Self {
        Self { path: None, variables: true, executables: true }
    }
}

struct ShellVisitor;

impl serde::de::Visitor<'_> for ShellVisitor {
    type Value = Shell;
    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a shell name")
    }
    fn visit_str<E: serde::de::Error>(self, shell: &str) -> Result<Shell, E> {
        parse_shell_name(shell).ok().ok_or_else(|| E::custom("unrecognized shell"))
    }
}

fn deserialize_shell<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Shell, D::Error> {
    d.deserialize_str(ShellVisitor)
}
