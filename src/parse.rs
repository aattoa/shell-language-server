use crate::lex::{Lexer, Token, TokenKind};
use crate::{ast, lsp};
use std::collections::HashMap;

struct Context<'a> {
    tokens: Lexer<'a>,
    diagnostics: Vec<lsp::Diagnostic>,
    references: HashMap<String, Vec<lsp::Range>>,
}

impl<'a> Context<'a> {
    fn new(input: &str) -> Context {
        Context { tokens: Lexer::new(input), diagnostics: Vec::new(), references: HashMap::new() }
    }
    fn next_token_if(&mut self, kind: TokenKind) -> Option<Token> {
        self.tokens.next_if(|tok| tok.kind == kind)
    }
    fn skip(&mut self, kind: TokenKind) {
        while self.next_token_if(kind).is_some() {}
    }
    fn skip_whitespace(&mut self) {
        self.skip(TokenKind::Space)
    }
    fn skip_empty_lines(&mut self) {
        while self
            .tokens
            .next_if(|token| matches!(token.kind, TokenKind::Space | TokenKind::NewLine))
            .is_some()
        {}
    }
    fn current_range(&mut self) -> lsp::Range {
        match self.tokens.peek() {
            Some(token) => token.range,
            None => lsp::Range::for_position(self.tokens.position()),
        }
    }
    fn error(&mut self, message: impl Into<String>) -> lsp::Diagnostic {
        lsp::Diagnostic::error(self.current_range(), message)
    }
    fn expected(&mut self, description: &str) -> lsp::Diagnostic {
        let found = self.tokens.peek().map_or("the end of input", |tok| tok.kind.show());
        self.error(format!("Expected {}, but found {}", description, found))
    }
    fn require_keyword(&mut self, keyword: &str) -> Result<(), lsp::Diagnostic> {
        if parse_keyword(self, keyword).is_some() { Ok(()) } else { Err(self.expected(keyword)) }
    }
    fn require_statement_end(&mut self) -> Result<(), lsp::Diagnostic> {
        self.skip_whitespace();
        if parse_statement_end(self) {
            self.skip_whitespace();
            Ok(())
        }
        else {
            Err(self.expected("a new line or a semicolon"))
        }
    }
    fn require_token(&mut self) -> Result<Token, lsp::Diagnostic> {
        let message = "Unexpected end of input";
        self.tokens.next().ok_or_else(|| lsp::Diagnostic::error(self.current_range(), message))
    }
    fn identifier(&mut self, token: Token) -> ast::Identifier {
        let name = token.value.unwrap();
        self.references.entry(name.clone()).or_default().push(token.range);
        ast::Identifier { name, range: token.range }
    }
}

fn parse_statement_end(ctx: &mut Context) -> bool {
    ctx.next_token_if(TokenKind::Semicolon)
        .or_else(|| ctx.next_token_if(TokenKind::NewLine))
        .is_some()
}

fn parse_keyword(ctx: &mut Context, keyword: &str) -> Option<Token> {
    ctx.tokens.next_if(|tok| tok.kind == TokenKind::Word && tok.value.as_ref().unwrap() == keyword)
}

fn extract_arguments(ctx: &mut Context) -> Vec<ast::Value> {
    let mut arguments = Vec::new();
    while ctx.tokens.peek().is_some_and(|tok| !is_statement_end(tok)) {
        if let Some(value) = parse_value(ctx) {
            arguments.push(value);
            ctx.skip_whitespace();
        }
        else {
            let diagnostic = ctx.expected("a value");
            ctx.tokens.next();
            ctx.diagnostics.push(diagnostic);
            break;
        }
    }
    arguments
}

fn parse_expansion(dollar: Token, ctx: &mut Context) -> Option<ast::Expansion> {
    if let Some(token) = ctx.next_token_if(TokenKind::Word) {
        Some(ast::Expansion::Simple(ctx.identifier(token)))
    }
    else {
        let message = "This `$` is literal. Use `\\$` to suppress this hint.";
        ctx.diagnostics.push(lsp::Diagnostic::info(dollar.range, message));
        None
    }
}

fn parse_dollar(dollar: Token, ctx: &mut Context) -> ast::Value {
    parse_expansion(dollar, ctx).map(ast::Value::Expansion).unwrap_or(ast::Value::Symbol)
}

fn parse_string(quote: Token, ctx: &mut Context) -> Vec<ast::Expansion> {
    let mut expansions = Vec::new();
    while let Some(tok) = ctx.tokens.next() {
        match tok.kind {
            TokenKind::DoubleQuote => return expansions,
            TokenKind::Dollar => {
                if let Some(expansion) = parse_expansion(tok, ctx) {
                    expansions.push(expansion)
                }
            }
            _ => continue,
        }
    }
    ctx.diagnostics.push(lsp::Diagnostic::error(quote.range, "Unterminated string"));
    expansions
}

fn parse_word(ctx: &mut Context) -> Option<ast::Value> {
    ctx.next_token_if(TokenKind::Word)
        .map(|tok| ast::Value::Word(tok.value.unwrap()))
        .or_else(|| ctx.next_token_if(TokenKind::Dollar).map(|dollar| parse_dollar(dollar, ctx)))
}

fn parse_value(ctx: &mut Context) -> Option<ast::Value> {
    ctx.next_token_if(TokenKind::DoubleQuote)
        .map(|quote| ast::Value::DoubleQuotedString(parse_string(quote, ctx)))
        .or_else(|| parse_word(ctx))
}

fn is_keyword<const N: usize>(token: &Token, keywords: [&str; N]) -> bool {
    token.kind == TokenKind::Word && keywords.contains(&token.value.as_ref().unwrap().as_str())
}

fn is_statement_end(token: &Token) -> bool {
    matches!(token.kind, TokenKind::Semicolon | TokenKind::NewLine)
}

fn extract_conditional(ctx: &mut Context) -> Result<ast::Statement, lsp::Diagnostic> {
    let condition = extract_statement(ctx)?;

    ctx.require_keyword("then")?;
    let true_branch = extract_statements_until(ctx, |tok| is_keyword(tok, ["fi", "elif", "else"]));

    let false_branch = if parse_keyword(ctx, "else").is_some() {
        Some(extract_statements_until(ctx, |tok| is_keyword(tok, ["fi"])))
    }
    else if parse_keyword(ctx, "elif").is_some() {
        todo!()
    }
    else {
        None
    };

    ctx.require_keyword("fi")?;
    Ok(ast::Statement::Conditional { condition: Box::new(condition), true_branch, false_branch })
}

fn extract_loop_body(ctx: &mut Context) -> Result<Vec<ast::Statement>, lsp::Diagnostic> {
    ctx.require_keyword("do")?;
    let body = extract_statements_until(ctx, |tok| is_keyword(tok, ["done"]));
    ctx.require_keyword("done")?;
    Ok(body)
}

fn extract_for_loop(ctx: &mut Context) -> Result<ast::Statement, lsp::Diagnostic> {
    let variable = ctx
        .next_token_if(TokenKind::Word)
        .map(|tok| ctx.identifier(tok))
        .ok_or_else(|| ctx.expected("an iterator name"))?;
    ctx.skip_whitespace();
    ctx.require_keyword("in")?;
    ctx.skip_whitespace();
    let values = extract_arguments(ctx);
    if values.is_empty() {
        let diagnostic = ctx.expected("an iterable value");
        ctx.diagnostics.push(diagnostic);
    }
    ctx.require_statement_end()?;
    Ok(ast::Statement::ForLoop { variable, values, body: extract_loop_body(ctx)? })
}

fn extract_while_loop(ctx: &mut Context) -> Result<ast::Statement, lsp::Diagnostic> {
    ctx.skip_whitespace();
    let condition = Box::new(extract_statement(ctx)?);
    let body = extract_loop_body(ctx)?;
    Ok(ast::Statement::WhileLoop { condition, body })
}

fn extract_command(name: String, ctx: &mut Context) -> Result<ast::Statement, lsp::Diagnostic> {
    ctx.skip_whitespace();
    match name.as_str() {
        "if" => extract_conditional(ctx),
        "for" => extract_for_loop(ctx),
        "while" => extract_while_loop(ctx),
        _ => Ok(ast::Statement::Command {
            name: ast::Value::Word(name),
            arguments: extract_arguments(ctx),
        }),
    }
}

fn skip_to_next_recovery_point(ctx: &mut Context) {
    for token in ctx.tokens.by_ref() {
        if is_statement_end(&token) {
            break;
        }
    }
}

fn extract_statement(ctx: &mut Context) -> Result<ast::Statement, lsp::Diagnostic> {
    ctx.skip_empty_lines();
    let token = ctx.require_token()?;
    match token.kind {
        TokenKind::Word => {
            let statement = extract_command(token.value.unwrap(), ctx);
            ctx.require_statement_end()?;
            ctx.skip_empty_lines();
            statement
        }
        _ => Err(ctx.expected("a statement")),
    }
}

fn extract_statements_until(ctx: &mut Context, f: impl Fn(&Token) -> bool) -> Vec<ast::Statement> {
    let mut statements = Vec::new();
    while !ctx.tokens.peek().is_none_or(&f) {
        match extract_statement(ctx) {
            Ok(statement) => statements.push(statement),
            Err(diagnostic) => {
                ctx.diagnostics.push(diagnostic);
                skip_to_next_recovery_point(ctx);
            }
        }
    }
    statements
}

pub struct ParseResult {
    pub program: Vec<ast::Statement>,
    pub diagnostics: Vec<lsp::Diagnostic>,
    pub references: HashMap<String, Vec<lsp::Range>>,
}

pub fn parse(input: &str) -> ParseResult {
    let mut ctx = Context::new(input);
    let statements = extract_statements_until(&mut ctx, |_| false);
    ParseResult { program: statements, diagnostics: ctx.diagnostics, references: ctx.references }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(word: impl Into<String>) -> ast::Value {
        ast::Value::Word(word.into())
    }

    fn identifier(name: impl Into<String>) -> ast::Identifier {
        ast::Identifier { name: name.into(), range: lsp::Range::default() }
    }

    fn parse_statements(input: &str) -> Vec<ast::Statement> {
        let result = parse(input);
        if result.diagnostics.is_empty() {
            result.program
        }
        else {
            dbg!(result.diagnostics);
            panic!()
        }
    }

    #[test]
    fn conditional() {
        assert_eq!(
            parse_statements("if ls -la; then\n\tpwd\n\tuname -a\nfi\n"),
            vec![ast::Statement::Conditional {
                condition: Box::new(ast::Statement::Command {
                    name: word("ls"),
                    arguments: vec![word("-la")]
                }),
                true_branch: vec![
                    ast::Statement::Command { name: word("pwd"), arguments: Vec::new() },
                    ast::Statement::Command { name: word("uname"), arguments: vec![word("-a")] }
                ],
                false_branch: None
            }]
        );
    }

    #[test]
    fn for_loop() {
        assert_eq!(
            parse_statements("for x in a b c\ndo\n\techo $x\ndone\n"),
            vec![ast::Statement::ForLoop {
                variable: identifier("x"),
                values: vec![word("a"), word("b"), word("c")],
                body: vec![ast::Statement::Command {
                    name: word("echo"),
                    arguments: vec![ast::Value::Expansion(ast::Expansion::Simple(identifier(
                        "x"
                    )))]
                }],
            }]
        );
    }

    #[test]
    fn while_loop() {
        assert_eq!(
            parse_statements("while true; do echo $x; done\n"),
            vec![ast::Statement::WhileLoop {
                condition: Box::new(ast::Statement::Command {
                    name: word("true"),
                    arguments: Vec::new(),
                }),
                body: vec![ast::Statement::Command {
                    name: word("echo"),
                    arguments: vec![ast::Value::Expansion(ast::Expansion::Simple(identifier(
                        "x"
                    )))]
                }],
            }]
        );
    }
}
