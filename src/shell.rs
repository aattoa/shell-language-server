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

#[rustfmt::skip]
pub fn builtins(shell: Shell) -> &'static [&'static str] {
    match shell {
        Shell::Bash => &[".", ":", "[", "alias", "bg", "bind", "break", "builtin", "caller", "cd", "command", "compgen", "complete", "compopt", "continue", "declare", "dirs", "disown", "echo", "enable", "eval", "exec", "exit", "export", "false", "fc", "fg", "getopts", "hash", "help", "history", "jobs", "kill", "let", "local", "logout", "mapfile", "popd", "printf", "pushd", "pwd", "read", "readarray", "readonly", "return", "set", "shift", "shopt", "source", "suspend", "test", "times", "trap", "true", "type", "typeset", "ulimit", "umask", "unalias", "unset", "wait"],
        Shell::Zsh => &["-", ".", ":", "[", "alias", "autoload", "bg", "bindkey", "break", "builtin", "bye", "cd", "chdir", "command", "compadd", "comparguments", "compcall", "compctl", "compdescribe", "compfiles", "compgroups", "compquote", "compset", "comptags", "comptry", "compvalues", "continue", "declare", "dirs", "disable", "disown", "echo", "echotc", "echoti", "emulate", "enable", "eval", "exec", "exit", "export", "false", "fc", "fg", "float", "functions", "getln", "getopts", "hash", "history", "integer", "jobs", "kill", "let", "limit", "local", "log", "logout", "noglob", "popd", "print", "printf", "private", "pushd", "pushln", "pwd", "r", "read", "readonly", "rehash", "return", "sched", "set", "setopt", "shift", "source", "suspend", "test", "times", "trap", "true", "ttyctl", "type", "typeset", "ulimit", "umask", "unalias", "unfunction", "unhash", "unlimit", "unset", "unsetopt", "vared", "wait", "whence", "where", "which", "zcompile", "zformat", "zle", "zmodload", "zparseopts", "zregexparse", "zstyle"],
        _ => &[".", ":", "break", "continue", "eval", "exec", "exit", "export", "readonly", "return", "set", "shift", "times", "trap", "unset"],
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn parse_shebang() {
        let err = Ok(super::Shell::Bash);
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
