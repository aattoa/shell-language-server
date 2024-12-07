#![allow(dead_code)]

mod ast;
mod config;
mod db;
mod env;
mod lex;
mod lsp;
mod parse;
mod poschars;
mod rpc;
mod server;
mod shell;

const HELP: &str = r"Options:
  --help, -h       Display help information
  --version, -v    Display version information
  --no-env-path    Do not complete commands available through PATH
  --no-env-vars    Do not complete environment variable names
  --no-env         Equivalent to --no-env-path --no-env-vars
  --debug          Log every LSP request and response to stderr";

fn bin_name() -> &'static str {
    option_env!("CARGO_BIN_NAME").unwrap_or("shell-language-server")
}

fn pkg_name() -> &'static str {
    option_env!("CARGO_PKG_NAME").unwrap_or_else(bin_name)
}

fn pkg_version() -> &'static str {
    option_env!("CARGO_PKG_VERSION").unwrap_or("unknown")
}

fn handle_command_line() -> config::Config {
    let mut config = config::Config::default();
    for flag in std::env::args().skip(1) {
        match flag.as_str() {
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
            "-h" | "--help" => {
                println!("Usage {} [options]\n{}", bin_name(), HELP);
                std::process::exit(0);
            }
            "-v" | "--version" => {
                println!("{} version {}", pkg_name(), pkg_version());
                std::process::exit(0);
            }
            arg => {
                eprintln!("Unrecognized argument: {arg}");
                std::process::exit(1);
            }
        }
    }
    config
}

fn main() {
    let config = handle_command_line();
    eprintln!("[debug] Started server.");
    let code = server::run(config);
    eprintln!("[debug] Exiting normally.");
    std::process::exit(code);
}
