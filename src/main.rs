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

const HELP: &str = r"Options:
  --help, -h           Display help information.
  --version, -v        Display version information.
  --settings-json=ARG  Provide server initialization settings.
  --debug              Log all LSP communication to standard error.";

const DESCRIPTION: &str = "A language server for shell scripts";

const BINARY_NAME: &str =
    if let Some(name) = option_env!("CARGO_BIN_NAME") { name } else { "shell-language-server" };

const PACKAGE_NAME: &str =
    if let Some(name) = option_env!("CARGO_PKG_NAME") { name } else { BINARY_NAME };

const PACKAGE_VERSION: &str =
    if let Some(version) = option_env!("CARGO_PKG_VERSION") { version } else { "unknown" };

fn settings_json(json: &str) -> Result<config::Settings, ExitCode> {
    serde_json::from_str(json).map_err(|error| {
        eprintln!("Could not parse settings: {error}");
        ExitCode::from(3)
    })
}

fn parse_command_line() -> Result<config::Cmdline, ExitCode> {
    let mut cmdline = config::Cmdline::default();
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
                cmdline.debug = true;
            }
            "--settings-json" => {
                eprintln!("Missing argument for {flag}. Usage: {flag}=ARG");
                return Err(ExitCode::from(3));
            }
            arg => {
                if let Some(arg) = arg.strip_prefix("--settings-json=") {
                    cmdline.settings = settings_json(arg)?;
                }
                else {
                    eprintln!("Unrecognized argument: {arg}");
                    return Err(ExitCode::from(3));
                }
            }
        }
    }
    Ok(cmdline)
}

fn main() -> ExitCode {
    match parse_command_line() {
        Ok(cmdline) => server::run(cmdline),
        Err(code) => code,
    }
}
