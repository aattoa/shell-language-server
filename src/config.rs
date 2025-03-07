use crate::shell::{Shell, parse_shell_name};

#[derive(serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Integrate {
    pub man: bool,
    pub help: bool,
    pub shfmt: bool,
    pub shellcheck: bool,
}

#[derive(serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Environment {
    pub path: Option<Box<str>>,
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

impl Default for Integrate {
    fn default() -> Self {
        Self { man: true, help: true, shfmt: true, shellcheck: true }
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
