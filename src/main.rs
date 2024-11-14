mod db;
mod lex;
mod poschars;

fn main() {
    for line in std::io::stdin().lines() {
        let tokens: Vec<_> = lex::Lexer::new(&line.unwrap()).map(|token| token.kind).collect();
        std::println!("tokens: {:?}", tokens);
    }
}
