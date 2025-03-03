use crate::poschars::PosChars;
use crate::{lsp, util};
use std::borrow::Cow;

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
    LessGreat,    // <>
    Great,        // >
    GreatGreat,   // >>
    GreatAnd,     // >&
    GreatPipe,    // >|
    Equal,        // =
    Dollar,       // $
    DollarHash,   // $#
    Pipe,         // |
    PipePipe,     // ||
    And,          // &
    AndAnd,       // &&
    Semi,         // ;
    SemiSemi,     // ;;
    NewLine,
    Space,
    ErrorUnterminatingRawString,
}

#[derive(Clone, Copy, Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub view: util::View,
    pub range: lsp::Range,
}

pub struct Lexer<'a> {
    chars: PosChars<'a>,
    next: Option<Token>,
}

fn is_word(char: char) -> bool {
    !"#'\"`(){}<>=$|&;".contains(char) && !char.is_whitespace() && !char.is_control()
}

fn extract_word_char(chars: &mut PosChars) -> Option<char> {
    chars.next_if_eq('\\').map(|ch| chars.next().unwrap_or(ch)).or_else(|| chars.next_if(is_word))
}

fn extract_word(_char: char, chars: &mut PosChars) -> TokenKind {
    while extract_word_char(chars).is_some() {}
    TokenKind::Word
}

fn extract_raw_string(chars: &mut PosChars) -> TokenKind {
    loop {
        match chars.next() {
            None => return TokenKind::ErrorUnterminatingRawString,
            Some('\'') => return TokenKind::RawString,
            _ => {}
        }
    }
}

fn extract_comment(chars: &mut PosChars) -> TokenKind {
    while chars.next_if(|char| char != '\n').is_some() {}
    TokenKind::Comment
}

fn extract_whitespace(chars: &mut PosChars) -> TokenKind {
    while chars.next_if(|char| char != '\n' && char.is_whitespace()).is_some() {}
    TokenKind::Space
}

fn next_token(char: char, chars: &mut PosChars) -> TokenKind {
    match char {
        '#' => extract_comment(chars),
        '\'' => extract_raw_string(chars),
        '\\' => extract_word(chars.next().unwrap_or(char), chars),
        '\n' => TokenKind::NewLine,
        '"' => TokenKind::DoubleQuote,
        '`' => TokenKind::BackQuote,
        '(' => TokenKind::ParenOpen,
        ')' => TokenKind::ParenClose,
        '{' => TokenKind::BraceOpen,
        '}' => TokenKind::BraceClose,
        '=' => TokenKind::Equal,

        '<' => {
            if chars.consume('<') {
                if chars.consume('-') { TokenKind::LessLessDash } else { TokenKind::LessLess }
            }
            else if chars.consume('&') {
                TokenKind::LessAnd
            }
            else if chars.consume('>') {
                TokenKind::LessGreat
            }
            else {
                TokenKind::Less
            }
        }

        '>' => {
            if chars.consume('>') {
                TokenKind::GreatGreat
            }
            else if chars.consume('&') {
                TokenKind::GreatAnd
            }
            else if chars.consume('|') {
                TokenKind::GreatPipe
            }
            else {
                TokenKind::Great
            }
        }

        '$' => ({ if chars.consume('#') { TokenKind::DollarHash } else { TokenKind::Dollar } }),
        '|' => ({ if chars.consume('|') { TokenKind::PipePipe } else { TokenKind::Pipe } }),
        '&' => ({ if chars.consume('&') { TokenKind::AndAnd } else { TokenKind::And } }),
        ';' => ({ if chars.consume(';') { TokenKind::SemiSemi } else { TokenKind::Semi } }),

        _ if char.is_whitespace() => extract_whitespace(chars),
        _ => extract_word(char, chars),
    }
}

fn lex(chars: &mut PosChars, next: impl FnOnce(char, &mut PosChars) -> TokenKind) -> Option<Token> {
    let (p1, o1) = (chars.position, chars.offset);
    let kind = next(chars.next()?, chars);
    let (p2, o2) = (chars.position, chars.offset);
    Some(Token {
        kind,
        view: util::View { start: o1, end: o2 },
        range: lsp::Range { start: p1, end: p2 },
    })
}

impl Iterator for Lexer<'_> {
    type Item = Token;

    fn next(&mut self) -> Option<Token> {
        self.next.take().or_else(|| lex(&mut self.chars, next_token))
    }
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self { chars: PosChars::new(input), next: None }
    }
    pub fn peek(&mut self) -> Option<Token> {
        if self.next.is_none() {
            self.next = lex(&mut self.chars, next_token);
        }
        self.next
    }
    pub fn next_if(&mut self, predicate: impl FnOnce(Token) -> bool) -> Option<Token> {
        if self.peek().is_some_and(predicate) { self.next() } else { None }
    }
    pub fn next_if_kind(&mut self, kind: TokenKind) -> Option<Token> {
        self.next_if(|token| token.kind == kind)
    }
    pub fn current_range(&mut self) -> lsp::Range {
        if let Some(token) = self.peek() {
            token.range
        }
        else {
            lsp::Range::for_position(self.chars.position)
        }
    }
}

pub fn escape(str: &str) -> Cow<str> {
    if !str.contains('\\') {
        return Cow::Borrowed(str);
    }
    let mut string = String::with_capacity(str.len());
    let mut chars = str.chars();
    while let Some(char) = chars.next() {
        string.push(if char != '\\' { char } else { chars.next().unwrap_or(char) });
    }
    Cow::Owned(string)
}

pub fn is_name(str: &str) -> bool {
    let mut chars = str.chars();
    chars.next().is_some_and(char::is_alphabetic) && chars.all(|c| c.is_alphanumeric() || c == '_')
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
            TokenKind::GreatPipe => "'>|'",
            TokenKind::Equal => "an equals sign",
            TokenKind::Dollar => "a dollar sign",
            TokenKind::DollarHash => "'$#'",
            TokenKind::Pipe => "a pipe",
            TokenKind::PipePipe => "'||'",
            TokenKind::And => "'&'",
            TokenKind::AndAnd => "'&&'",
            TokenKind::Semi => "a semicolon",
            TokenKind::SemiSemi => "a double semicolon",
            TokenKind::NewLine => "a new line",
            TokenKind::Space => "whitespace",
            TokenKind::ErrorUnterminatingRawString => "an unterminating raw string",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TokenKind::*;

    fn tokens(input: &str) -> Vec<super::TokenKind> {
        super::Lexer::new(input).map(|token| token.kind).collect()
    }

    #[test]
    fn basics() {
        assert_eq!(tokens("hello$world"), [Word, Dollar, Word]);
        assert_eq!(tokens("hello\\$world"), [Word]);
    }

    #[test]
    fn operators() {
        assert_eq!(tokens("< << <<- <& > >> >& <> >|"), [
            Less,
            Space,
            LessLess,
            Space,
            LessLessDash,
            Space,
            LessAnd,
            Space,
            Great,
            Space,
            GreatGreat,
            Space,
            GreatAnd,
            Space,
            LessGreat,
            Space,
            GreatPipe,
        ]);
    }

    #[test]
    fn punctuation() {
        assert_eq!(tokens("| || & && ; ;;"), [
            Pipe, Space, PipePipe, Space, And, Space, AndAnd, Space, Semi, Space, SemiSemi
        ]);
    }

    #[test]
    fn escape() {
        assert_eq!(super::escape("hello"), "hello");
        assert_eq!(super::escape("he\\llo"), "hello");
        assert_eq!(super::escape("he\\\\llo"), "he\\llo");
        assert_eq!(super::escape("\\h\\e\\l\\l\\o"), "hello");
    }

    #[test]
    fn is_name() {
        assert!(super::is_name("hello"));
        assert!(super::is_name("hello_world"));
        assert!(super::is_name("helloWorld10"));
        assert!(!super::is_name("10helloWorld10"));
        assert!(!super::is_name("hello-world"));
        assert!(!super::is_name(""));
    }
}
