#![allow(dead_code, unused)]

mod db;
mod lex;
mod lsp;
mod poschars;
mod rpc;
mod server;

fn main() {
    let mut server = server::Server::default();
    eprintln!("[debug] Started server.");
    let code = server::run(&mut server);
    eprintln!("[debug] Exiting normally.");
    std::process::exit(code);
}
