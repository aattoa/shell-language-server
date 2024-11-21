use crate::lsp;
use crate::poschars::PosChars;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TokenKind {
    Word,
    RawString,
    Comment,
    Error,
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
    pub value: Option<String>,
    pub range: lsp::Range,
}

pub struct Lexer<'a> {
    chars: PosChars<'a>,
    next: Option<Token>,
}

fn is_word(char: char) -> bool {
    char.is_alphanumeric() || ['_', '-'].contains(&char)
}

fn extract_word_char(chars: &mut PosChars) -> Option<char> {
    chars.next_if(is_word).or_else(|| chars.next_if_eq('\\').map(|ch| chars.next().unwrap_or(ch)))
}

fn extract_word(char: char, chars: &mut PosChars) -> (TokenKind, Option<String>) {
    let mut word = char.to_string();
    while let Some(char) = extract_word_char(chars) {
        word.push(char);
    }
    (TokenKind::Word, Some(word))
}

fn extract_raw_string(chars: &mut PosChars) -> (TokenKind, Option<String>) {
    let mut string = String::new();
    loop {
        match chars.next() {
            None => return (TokenKind::Error, Some("Unterminating raw string".to_owned())),
            Some('\'') => return (TokenKind::RawString, Some(string)),
            Some(char) => string.push(char),
        }
    }
}

fn extract_comment(chars: &mut PosChars) -> (TokenKind, Option<String>) {
    (TokenKind::Comment, Some(chars.take_while(|&char| char != '\n').collect()))
}

fn extract_whitespace(chars: &mut PosChars) -> TokenKind {
    while chars.next_if(|char| char != '\n' && char.is_whitespace()).is_some() {}
    TokenKind::Space
}

fn next_token(char: char, chars: &mut PosChars) -> (TokenKind, Option<String>) {
    match char {
        '#' => extract_comment(chars),
        '\\' => extract_word(chars.next().unwrap_or(char), chars),
        '\'' => extract_raw_string(chars),
        '"' => (TokenKind::DoubleQuote, None),
        '(' => (TokenKind::ParenOpen, None),
        ')' => (TokenKind::ParenClose, None),
        '{' => (TokenKind::BraceOpen, None),
        '}' => (TokenKind::BraceClose, None),
        '|' => (TokenKind::Pipe, None),
        '$' => (TokenKind::Dollar, None),
        '&' => (TokenKind::Ampersand, None),
        ';' => (TokenKind::Semicolon, None),
        '\n' => (TokenKind::NewLine, None),
        _ if is_word(char) => extract_word(char, chars),
        _ if char.is_whitespace() => (extract_whitespace(chars), None),
        _ => (TokenKind::Error, Some(format!("Unexpected character: '{char}'"))),
    }
}

fn lex(lexer: &mut Lexer) -> Option<Token> {
    let start = lexer.chars.position;
    let (kind, value) = next_token(lexer.chars.next()?, &mut lexer.chars);
    let end = lexer.chars.position;
    Some(Token { kind, value, range: lsp::Range { start, end } })
}

impl<'a> Iterator for Lexer<'a> {
    type Item = Token;

    fn next(&mut self) -> Option<Token> {
        self.next.take().or_else(|| lex(self))
    }
}

impl<'a> Lexer<'a> {
    pub fn new(input: &str) -> Lexer {
        Lexer { chars: PosChars::new(input), next: None }
    }
    pub fn position(&self) -> lsp::Position {
        self.chars.position
    }
    pub fn peek(&mut self) -> Option<&Token> {
        if self.next.is_none() {
            self.next = lex(self);
        }
        self.next.as_ref()
    }
    pub fn next_if(&mut self, predicate: impl FnOnce(&Token) -> bool) -> Option<Token> {
        match self.peek() {
            Some(token) if predicate(token) => self.next(),
            _ => None,
        }
    }
    pub fn next_if_kind(&mut self, kind: TokenKind) -> Option<Token> {
        self.next_if(|token| token.kind == kind)
    }
}

impl TokenKind {
    pub fn show(self) -> &'static str {
        match self {
            TokenKind::Word => "a word",
            TokenKind::RawString => "a raw string",
            TokenKind::Comment => "a comment",
            TokenKind::Error => "an error",
            TokenKind::DoubleQuote => "a double quote",
            TokenKind::ParenOpen => "an opening parenthesis",
            TokenKind::ParenClose => "a closing parenthesis",
            TokenKind::BraceOpen => "an opening brace",
            TokenKind::BraceClose => "a closing parenthesis",
            TokenKind::Dollar => "a dollar sign",
            TokenKind::Pipe => "a pipe",
            TokenKind::Ampersand => "an ampersand",
            TokenKind::Semicolon => "a semicolon",
            TokenKind::NewLine => "a new line",
            TokenKind::Space => "whitespace",
        }
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
            vec![TokenKind::Word, TokenKind::Dollar, TokenKind::Word]
        );
        assert_eq!(lex_vec("hello\\$world"), vec![TokenKind::Word]);
    }
}
