#![allow(dead_code)]

mod ast;
mod db;
mod env;
mod lex;
mod lsp;
mod parse;
mod poschars;
mod rpc;
mod server;

fn main() {
    let mut server = server::Server::default();
    server.db.path_executables = env::collect_path_executables();
    server.db.environment_variables = env::collect_variables();
    eprintln!("[debug] Started server.");
    let code = server::run(&mut server);
    eprintln!("[debug] Exiting normally.");
    std::process::exit(code);
}
