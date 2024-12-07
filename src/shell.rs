#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shell {
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

impl std::str::FromStr for Shell {
    type Err = String;

    fn from_str(str: &str) -> Result<Shell, String> {
        let str = str.trim_ascii().strip_prefix('/').ok_or("Expected an absolute path")?;
        let shell = str
            .strip_prefix("usr/bin/env ")
            .or_else(|| str.strip_prefix("usr/bin/"))
            .or_else(|| str.strip_prefix("bin/"))
            .ok_or("Expected /bin/ or /usr/bin/")?;
        match shell.split_whitespace().next().unwrap_or(shell) {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(str: &str) -> Result<Shell, String> {
        str.parse()
    }

    #[test]
    fn parse_language() {
        let err = Ok(Shell::Bash);
        assert_eq!(parse("/usr/bin/env bash"), err);
        assert_eq!(parse("/usr/bin/bash"), err);
        assert_eq!(parse("/bin/bash"), err);

        let err = Err("Unrecognized shell: 'hello'".to_owned());
        assert_eq!(parse("/usr/bin/env hello"), err);
        assert_eq!(parse("/usr/bin/hello"), err);
        assert_eq!(parse("/bin/hello"), err);

        let err = Err("No shell specified".to_owned());
        assert_eq!(parse("/usr/bin/"), err);
        assert_eq!(parse("/bin/"), err);

        let err = Err("Expected an absolute path".to_owned());
        assert_eq!(parse("usr/bin/bash"), err);
        assert_eq!(parse("bin/bash"), err);
        assert_eq!(parse("bash"), err);
        assert_eq!(parse(""), err);
    }
}
