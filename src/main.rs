#![allow(unused_parens, dead_code)]

use std::process::ExitCode;

mod config;
mod db;
mod env;
mod external;
mod indexvec;
mod lex;
mod lsp;
mod parse;
mod poschars;
mod rpc;
mod server;
mod shell;
mod util;

const HELP: &str = r"Options:
  --help, -h          Display help information.
  --version, -v       Display version information.
  --no-env-path       Do not complete commands available through $PATH.
  --no-env-vars       Do not complete environment variable names.
  --no-env            Equivalent to --no-env-path --no-env-vars.
  --path=ARG          Use the given argument instead of $PATH.
  --default-shell=SH  Default to the given shell when a script has no shebang.
  --exe=NAME:PATH     Specify the path to an executable. Can be specified multiple times.
  --shellcheck=BOOL   Enable or disable shellcheck integration. Defaults to true.
  --shfmt=BOOL        Enable or disable shfmt integration. Defaults to false.
  --debug             Log every LSP request and response to standard error.";

const DESCRIPTION: &str = "A language server for shell scripts";

const BINARY_NAME: &str =
    if let Some(name) = option_env!("CARGO_BIN_NAME") { name } else { "shell-language-server" };

const PACKAGE_NAME: &str =
    if let Some(name) = option_env!("CARGO_PKG_NAME") { name } else { BINARY_NAME };

const PACKAGE_VERSION: &str =
    if let Some(version) = option_env!("CARGO_PKG_VERSION") { version } else { "unknown" };

fn cli_error(error: impl std::fmt::Display) -> ExitCode {
    eprintln!("Command line error: {error}");
    ExitCode::from(3)
}

fn boolean_arg(arg: &str) -> Result<bool, ExitCode> {
    match arg {
        "true" | "yes" | "on" | "1" => Ok(true),
        "false" | "no" | "off" | "0" => Ok(false),
        _ => Err(cli_error(format!("Invalid boolean: '{arg}'"))),
    }
}

fn parse_command_line() -> Result<config::Config, ExitCode> {
    let mut config = config::Config::default();
    for flag in std::env::args().skip(1) {
        match flag.as_str() {
            "-h" | "--help" => {
                println!("{DESCRIPTION}\n\nUsage: {BINARY_NAME} [options]\n\n{HELP}");
                return Err(ExitCode::from(0));
            }
            "-v" | "--version" => {
                println!("{PACKAGE_NAME} version {PACKAGE_VERSION}");
                return Err(ExitCode::from(0));
            }
            "--debug" => {
                config.debug = true;
            }
            "--no-env" => {
                config.complete = config::Complete { env_path: false, env_vars: false };
            }
            "--no-env-path" => {
                config.complete.env_path = false;
            }
            "--no-env-vars" => {
                config.complete.env_vars = false;
            }
            "--path" | "--exe" | "--shellcheck" | "--shfmt" | "--default-shell" => {
                return Err(cli_error(format!(
                    "Missing argument for '{flag}'. Usage: '{flag}=value'"
                )));
            }
            arg => {
                if let Some(arg) = arg.strip_prefix("--path=") {
                    config.path = Some(arg.into());
                }
                else if let Some(arg) = arg.strip_prefix("--exe=") {
                    let Some((name, path)) = arg.split_once(':')
                    else {
                        return Err(cli_error("Invalid argument for '--exe', expected NAME:PATH"));
                    };
                    let exe = match name {
                        "sh" => &mut config.executables.sh,
                        "zsh" => &mut config.executables.zsh,
                        "bash" => &mut config.executables.bash,
                        "shellcheck" => &mut config.executables.shellcheck,
                        "shfmt" => &mut config.executables.shfmt,
                        "man" => &mut config.executables.man,
                        _ => {
                            return Err(cli_error(format!(
                                "Unrecognized name: '{name}'. Recognized names are sh, zsh, bash, shellcheck, shfmt, man."
                            )));
                        }
                    };
                    *exe = std::borrow::Cow::Owned(path.into());
                }
                else if let Some(arg) = arg.strip_prefix("--shellcheck=") {
                    config.integration.shellcheck = boolean_arg(arg)?;
                }
                else if let Some(arg) = arg.strip_prefix("--shfmt=") {
                    config.integration.shfmt = boolean_arg(arg)?;
                }
                else if let Some(arg) = arg.strip_prefix("--default-shell=") {
                    match shell::parse_shell_name(arg) {
                        Ok(shell) => config.default_shell = shell,
                        Err(error) => return Err(cli_error(error)),
                    }
                }
                else {
                    return Err(cli_error(format!("Unrecognized argument: {arg}")));
                }
            }
        }
    }
    Ok(config)
}

fn main() -> ExitCode {
    match parse_command_line() {
        Ok(config) => server::run(config),
        Err(code) => code,
    }
}
