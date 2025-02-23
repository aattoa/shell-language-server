#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Shell {
    #[default]
    Posix,
    Bash,
    Zsh,
    Ksh,
    Csh,
    Tcsh,
}

impl Shell {
    pub fn name(self) -> &'static str {
        match self {
            Shell::Posix => "POSIX shell",
            Shell::Bash => "Bourne-again shell",
            Shell::Zsh => "Z shell",
            Shell::Ksh => "Korn shell",
            Shell::Csh => "C shell",
            Shell::Tcsh => "TENEX C shell",
        }
    }
}

pub fn parse_shell_name(str: &str) -> Result<Shell, String> {
    match str {
        "sh" | "dash" => Ok(Shell::Posix),
        "ksh" | "oksh" | "loksh" | "mksh" | "pdksh" => Ok(Shell::Ksh),
        "bash" => Ok(Shell::Bash),
        "zsh" => Ok(Shell::Zsh),
        "csh" => Ok(Shell::Csh),
        "tcsh" => Ok(Shell::Tcsh),
        "" => Err("No shell specified".to_owned()),
        shell => Err(format!("Unrecognized shell: '{shell}'")),
    }
}

pub fn parse_shebang(shebang: &str) -> Result<Shell, String> {
    let str = shebang.trim_ascii().strip_prefix('/').ok_or("Expected an absolute path")?;
    let shell = str
        .strip_prefix("usr/bin/env ")
        .or_else(|| str.strip_prefix("usr/bin/"))
        .or_else(|| str.strip_prefix("bin/"))
        .ok_or("Expected /bin/ or /usr/bin/")?;
    parse_shell_name(shell.split_whitespace().next().unwrap_or(shell))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_shebang() {
        let err = Ok(Shell::Bash);
        assert_eq!(super::parse_shebang("/usr/bin/env bash"), err);
        assert_eq!(super::parse_shebang("/usr/bin/bash"), err);
        assert_eq!(super::parse_shebang("/bin/bash"), err);

        let err = Err("Unrecognized shell: 'hello'".to_owned());
        assert_eq!(super::parse_shebang("/usr/bin/env hello"), err);
        assert_eq!(super::parse_shebang("/usr/bin/hello"), err);
        assert_eq!(super::parse_shebang("/bin/hello"), err);

        let err = Err("No shell specified".to_owned());
        assert_eq!(super::parse_shebang("/usr/bin/"), err);
        assert_eq!(super::parse_shebang("/bin/"), err);

        let err = Err("Expected an absolute path".to_owned());
        assert_eq!(super::parse_shebang("usr/bin/bash"), err);
        assert_eq!(super::parse_shebang("bin/bash"), err);
        assert_eq!(super::parse_shebang("bash"), err);
        assert_eq!(super::parse_shebang(""), err);
    }
}
