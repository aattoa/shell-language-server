use crate::lsp;
use crate::poschars::PosChars;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TokenKind {
    Word,
    RawString,    // 'text'
    Comment,      // # text
    BackQuote,    // `
    DoubleQuote,  // "
    ParenOpen,    // (
    ParenClose,   // )
    BraceOpen,    // {
    BraceClose,   // }
    Less,         // <
    LessLess,     // <<
    LessLessDash, // <<-
    LessAnd,      // <&
    Great,        // >
    GreatGreat,   // >>
    GreatAnd,     // >&
    LessGreat,    // <>
    Clobber,      // >|
    Equals,       // =
    Dollar,       // $
    Pipe,         // |
    PipePipe,     // ||
    And,          // &
    AndAnd,       // &&
    Semi,         // ;
    SemiSemi,     // ;;
    NewLine,
    Space,
    Error, // Input that could not be lexed.
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
    char.is_alphanumeric() || "-_/.,:".contains(char)
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
    let mut comment = String::new();
    while let Some(char) = chars.next_if(|char| char != '\n') {
        comment.push(char);
    }
    (TokenKind::Comment, Some(comment))
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
        '`' => (TokenKind::BackQuote, None),
        '(' => (TokenKind::ParenOpen, None),
        ')' => (TokenKind::ParenClose, None),
        '{' => (TokenKind::BraceOpen, None),
        '}' => (TokenKind::BraceClose, None),
        '<' => (TokenKind::Less, None),
        '>' => (TokenKind::Great, None),
        '|' => (TokenKind::Pipe, None),
        '=' => (TokenKind::Equals, None),
        '$' => (TokenKind::Dollar, None),
        '&' => (TokenKind::And, None),
        ';' => (TokenKind::Semi, None),
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
            TokenKind::BackQuote => "a backquote",
            TokenKind::DoubleQuote => "a double quote",
            TokenKind::ParenOpen => "an opening parenthesis",
            TokenKind::ParenClose => "a closing parenthesis",
            TokenKind::BraceOpen => "an opening brace",
            TokenKind::BraceClose => "a closing brace",
            TokenKind::Less => "'<'",
            TokenKind::LessLess => "'<<'",
            TokenKind::LessLessDash => "'<<-'",
            TokenKind::LessAnd => "'<&'",
            TokenKind::Great => "'>'",
            TokenKind::GreatGreat => "'>>'",
            TokenKind::GreatAnd => "'>&'",
            TokenKind::LessGreat => "'<>'",
            TokenKind::Clobber => "'>|'",
            TokenKind::Equals => "an equals sign",
            TokenKind::Dollar => "a dollar sign",
            TokenKind::Pipe => "'|'",
            TokenKind::PipePipe => "'||'",
            TokenKind::And => "'&'",
            TokenKind::AndAnd => "'&&'",
            TokenKind::Semi => "a semicolon",
            TokenKind::SemiSemi => "a double semicolon",
            TokenKind::NewLine => "a new line",
            TokenKind::Space => "whitespace",
            TokenKind::Error => "a lexical error",
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
