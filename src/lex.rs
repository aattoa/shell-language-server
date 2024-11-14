use crate::db;
use crate::poschars::PosChars;

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TokenKind {
    Word(String),
    RawString(String),
    Comment(String),
    Error(String),
    DoubleQuote,
    ParenOpen,
    ParenClose,
    BraceOpen,
    BraceClose,
    Dollar,
    Pipe,
    Ampersand,
    Semicolon,
    NewLine,
    Space,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub range: db::Range,
}

fn is_word(char: char) -> bool {
    char.is_alphanumeric() || ['_', '-'].contains(&char)
}

fn extract_word_char(chars: &mut PosChars) -> Option<char> {
    chars.next_if(is_word).or_else(|| chars.next_if_eq('\\').map(|ch| chars.next().unwrap_or(ch)))
}

fn extract_word(char: char, chars: &mut PosChars) -> TokenKind {
    let mut word = char.to_string();
    while let Some(char) = extract_word_char(chars) {
        word.push(char);
    }
    TokenKind::Word(word)
}

fn extract_raw_string(chars: &mut PosChars) -> TokenKind {
    let mut string = String::new();
    loop {
        match chars.next() {
            None => return TokenKind::Error("Unterminating raw string".to_owned()),
            Some('\'') => return TokenKind::RawString(string),
            Some(char) => string.push(char),
        }
    }
}

fn extract_comment(chars: &mut PosChars) -> TokenKind {
    TokenKind::Comment(chars.take_while(|&char| char != '\n').collect())
}

fn extract_whitespace(chars: &mut PosChars) -> TokenKind {
    while chars.next_if(|char| char != '\n' && char.is_whitespace()).is_some() {}
    TokenKind::Space
}

fn next_token(char: char, chars: &mut PosChars) -> TokenKind {
    match char {
        '#' => extract_comment(chars),
        '\\' => extract_word(chars.next().unwrap_or(char), chars),
        '\'' => extract_raw_string(chars),
        '"' => TokenKind::DoubleQuote,
        '(' => TokenKind::ParenOpen,
        ')' => TokenKind::ParenClose,
        '{' => TokenKind::BraceOpen,
        '}' => TokenKind::BraceClose,
        '|' => TokenKind::Pipe,
        '$' => TokenKind::Dollar,
        '&' => TokenKind::Ampersand,
        ';' => TokenKind::Semicolon,
        '\n' => TokenKind::NewLine,
        _ if is_word(char) => extract_word(char, chars),
        _ if char.is_whitespace() => extract_whitespace(chars),
        _ => TokenKind::Error(format!("Unexpected character: '{char}'")),
    }
}

pub struct Lexer<'a> {
    chars: PosChars<'a>,
}

impl<'a> Iterator for Lexer<'a> {
    type Item = Token;

    fn next(&mut self) -> Option<Token> {
        let begin = self.chars.position;
        let kind = next_token(self.chars.next()?, &mut self.chars);
        let end = self.chars.position;
        Some(Token { kind, range: db::Range { begin, end } })
    }
}

impl<'a> Lexer<'a> {
    pub fn new(input: &str) -> Lexer {
        Lexer { chars: PosChars::new(input) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens() {
        let lex_vec = |input| Lexer::new(input).map(|token| token.kind).collect::<Vec<_>>();

        assert_eq!(
            lex_vec("hello$world"),
            vec![
                TokenKind::Word("hello".to_owned()),
                TokenKind::Dollar,
                TokenKind::Word("world".to_owned())
            ]
        );
        assert_eq!(lex_vec("hello\\$world"), vec![TokenKind::Word("hello$world".to_owned())]);
    }
}
