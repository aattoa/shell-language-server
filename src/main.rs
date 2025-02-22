#![allow(unused_parens, dead_code)]

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
  --help, -h         Display help information.
  --version, -v      Display version information.
  --no-env-path      Do not complete commands available through $PATH.
  --no-env-vars      Do not complete environment variable names.
  --no-env           Equivalent to --no-env-path --no-env-vars.
  --path=ARG         Use the given argument instead of $PATH.
  --exe=NAME:PATH    Specify the path to an executable. Can be specified multiple times.
  --shellcheck=BOOL  Enable or disable shellcheck integration. Defaults to true.
  --shfmt=BOOL       Enable or disable shfmt integration. Defaults to false.
  --debug            Log every LSP request and response to stderr.";

const DESCRIPTION: &str = "A language server for shell scripts";

const BINARY_NAME: &str =
    if let Some(name) = option_env!("CARGO_BIN_NAME") { name } else { "shell-language-server" };

const PACKAGE_NAME: &str =
    if let Some(name) = option_env!("CARGO_PKG_NAME") { name } else { BINARY_NAME };

const PACKAGE_VERSION: &str =
    if let Some(version) = option_env!("CARGO_PKG_VERSION") { version } else { "unknown" };

fn boolean_arg(arg: &str) -> bool {
    match arg {
        "true" | "yes" | "on" | "1" => true,
        "false" | "no" | "off" | "0" => false,
        _ => {
            eprintln!("Invalid boolean: '{arg}'");
            std::process::exit(1);
        }
    }
}

fn handle_command_line() -> config::Config {
    let mut config = config::Config::default();
    for flag in std::env::args().skip(1) {
        match flag.as_str() {
            "-h" | "--help" => {
                println!("{DESCRIPTION}\n\nUsage: {BINARY_NAME} [options]\n\n{HELP}");
                std::process::exit(0);
            }
            "-v" | "--version" => {
                println!("{PACKAGE_NAME} version {PACKAGE_VERSION}");
                std::process::exit(0);
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
            "--path" | "--exe" | "--shellcheck" | "--shfmt" => {
                eprintln!("Missing argument for '{flag}'. Usage: '{flag}=value'");
                std::process::exit(1);
            }
            arg => {
                if let Some(arg) = arg.strip_prefix("--path=") {
                    config.path = Some(arg.into());
                }
                else if let Some(arg) = arg.strip_prefix("--exe=") {
                    let Some((name, path)) = arg.split_once(':')
                    else {
                        eprintln!("Invalid argument for '--exe', expected NAME:PATH");
                        std::process::exit(1);
                    };
                    let exe = match name {
                        "shellcheck" => &mut config.executables.shellcheck,
                        "shfmt" => &mut config.executables.shfmt,
                        _ => {
                            eprintln!(
                                "Unrecognized name: '{name}'. Recognized names are shellcheck, shfmt."
                            );
                            std::process::exit(1);
                        }
                    };
                    *exe = std::borrow::Cow::Owned(path.into());
                }
                else if let Some(arg) = arg.strip_prefix("--shellcheck=") {
                    config.integration.shellcheck = boolean_arg(arg);
                }
                else if let Some(arg) = arg.strip_prefix("--shfmt=") {
                    config.integration.shfmt = boolean_arg(arg);
                }
                else {
                    eprintln!("Unrecognized argument: {arg}");
                    std::process::exit(1);
                }
            }
        }
    }
    config
}

fn main() {
    let config = handle_command_line();
    let code = server::run(config);
    std::process::exit(code);
}
