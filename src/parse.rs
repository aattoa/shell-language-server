use crate::lex::{Lexer, Token, TokenKind};
use crate::{ast, db, lsp};

type ParseResult<T> = Result<T, lsp::Diagnostic>;

struct Context<'a> {
    tokens: Lexer<'a>,
    info: db::DocumentInfo,
}

impl<'a> Context<'a> {
    fn new(input: &str) -> Context {
        Context { tokens: Lexer::new(input), info: db::DocumentInfo::default() }
    }
    fn skip_whitespace(&mut self) {
        while self
            .tokens
            .next_if(|tok| matches!(tok.kind, TokenKind::Space | TokenKind::Comment))
            .is_some()
        {}
    }
    fn skip_empty_lines(&mut self) {
        while self
            .tokens
            .next_if(|tok| {
                matches!(tok.kind, TokenKind::Space | TokenKind::Comment | TokenKind::NewLine)
            })
            .is_some()
        {}
    }
    fn current_range(&mut self) -> lsp::Range {
        self.tokens
            .peek()
            .map(|tok| tok.range)
            .unwrap_or_else(|| lsp::Range::for_position(self.tokens.position()))
    }
    fn error(&mut self, message: impl Into<String>) -> lsp::Diagnostic {
        lsp::Diagnostic::error(self.current_range(), message)
    }
    fn expected(&mut self, description: &str) -> lsp::Diagnostic {
        let found = self.tokens.peek().map_or("the end of input", |tok| tok.kind.show());
        self.error(format!("Expected {}, but found {}", description, found))
    }
    fn require_keyword(&mut self, keyword: &str) -> ParseResult<()> {
        if parse_keyword(self, keyword) { Ok(()) } else { Err(self.expected(keyword)) }
    }
    fn require_token_kind(&mut self, kind: TokenKind) -> ParseResult<Token> {
        self.tokens.next_if_kind(kind).ok_or_else(|| self.expected(kind.show()))
    }
    fn require_statement_end(&mut self) -> ParseResult<()> {
        self.skip_whitespace();
        if self.tokens.next_if(is_statement_end).is_some() {
            self.skip_whitespace();
            Ok(())
        }
        else {
            Err(self.expected("a new line or a semicolon"))
        }
    }
    fn require_token(&mut self) -> ParseResult<Token> {
        self.tokens.next().ok_or_else(|| self.error("Unexpected end of input"))
    }
}

fn is_keyword(token: &Token, keywords: &[&str]) -> bool {
    token.kind == TokenKind::Word && keywords.contains(&token.value.as_ref().unwrap().as_str())
}

fn is_statement_end(token: &Token) -> bool {
    matches!(token.kind, TokenKind::Semi | TokenKind::NewLine)
}

fn identifier(token: Token) -> ast::Identifier {
    ast::Identifier { name: token.value.unwrap(), range: token.range }
}

fn parse_keyword(ctx: &mut Context, keyword: &str) -> bool {
    ctx.tokens
        .next_if(|tok| tok.kind == TokenKind::Word && tok.value.as_ref().unwrap() == keyword)
        .is_some()
}

fn parse_dollar(dollar: Token, ctx: &mut Context) -> ParseResult<ast::Value> {
    parse_expansion(dollar, ctx).map(|exp| exp.map_or(ast::Value::Symbol, ast::Value::Expansion))
}

fn parse_word(ctx: &mut Context) -> ParseResult<Option<ast::Value>> {
    if let Some(word) = ctx.tokens.next_if_kind(TokenKind::Word) {
        Ok(Some(ast::Value::Word(word.value.unwrap())))
    }
    else if let Some(dollar) = ctx.tokens.next_if_kind(TokenKind::Dollar) {
        parse_dollar(dollar, ctx).map(Some)
    }
    else {
        Ok(None)
    }
}

fn parse_simple_value(ctx: &mut Context) -> ParseResult<Option<ast::Value>> {
    if let Some(quote) = ctx.tokens.next_if_kind(TokenKind::DoubleQuote) {
        Ok(Some(ast::Value::DoubleQuotedString(parse_string(ctx, quote))))
    }
    else if let Some(string) = ctx.tokens.next_if_kind(TokenKind::RawString) {
        Ok(Some(ast::Value::RawString(string.value.unwrap())))
    }
    else if ctx.tokens.next_if_kind(TokenKind::Equals).is_some() {
        Ok(Some(ast::Value::Symbol))
    }
    else {
        parse_word(ctx)
    }
}

fn parse_value(ctx: &mut Context) -> ParseResult<Option<ast::Value>> {
    let mut values = Vec::new();
    while let Some(value) = parse_simple_value(ctx)? {
        values.push(value);
    }
    Ok(match values.len() {
        0 => None,
        1 => Some(values.pop().unwrap()),
        _ => Some(ast::Value::Concatenation(values)),
    })
}

fn extract_arguments_until(ctx: &mut Context, pred: impl Fn(&Token) -> bool) -> Vec<ast::Value> {
    let mut arguments = Vec::new();
    while !ctx.tokens.peek().is_none_or(&pred) {
        match parse_value(ctx) {
            Ok(Some(value)) => {
                arguments.push(value);
                ctx.skip_whitespace();
            }
            Ok(None) => {
                let diagnostic = ctx.expected("a value");
                ctx.info.diagnostics.push(diagnostic);
                ctx.tokens.next();
                break;
            }
            Err(diagnostic) => {
                ctx.info.diagnostics.push(diagnostic);
                break;
            }
        }
    }
    arguments
}

fn parse_expansion(dollar: Token, ctx: &mut Context) -> ParseResult<Option<ast::Expansion>> {
    if let Some(word) = ctx.tokens.next_if_kind(TokenKind::Word) {
        let identifier = identifier(word);
        ctx.info.add_variable_read(identifier.clone());
        Ok(Some(ast::Expansion::Simple(identifier)))
    }
    else if ctx.tokens.next_if_kind(TokenKind::BraceOpen).is_some() {
        let identifier = identifier(ctx.require_token_kind(TokenKind::Word)?);
        ctx.info.add_variable_read(identifier.clone());
        ctx.require_token_kind(TokenKind::BraceClose)?;
        Ok(Some(ast::Expansion::Simple(identifier)))
    }
    else if ctx.tokens.next_if_kind(TokenKind::ParenOpen).is_some() {
        let statement = extract_statement_up_to(ctx, |tok| tok.kind == TokenKind::ParenClose)?;
        ctx.require_token_kind(TokenKind::ParenClose)?;
        Ok(Some(ast::Expansion::Statement(Box::new(statement))))
    }
    else {
        let message = "This `$` is literal. Use `\\$` to suppress this hint.";
        ctx.info.diagnostics.push(lsp::Diagnostic::info(dollar.range, message));
        Ok(None)
    }
}

fn parse_string(ctx: &mut Context, quote: Token) -> Vec<ast::Expansion> {
    let mut expansions = Vec::new();
    while let Some(tok) = ctx.tokens.next() {
        match tok.kind {
            TokenKind::DoubleQuote => return expansions,
            TokenKind::Dollar => match parse_expansion(tok, ctx) {
                Ok(None) => {}
                Ok(Some(expansion)) => expansions.push(expansion),
                Err(diagnostic) => ctx.info.diagnostics.push(diagnostic),
            },
            _ => continue,
        }
    }
    ctx.info.diagnostics.push(lsp::Diagnostic::error(quote.range, "Unterminated string"));
    expansions
}

fn extract_conditional(ctx: &mut Context) -> ParseResult<ast::Statement> {
    let condition = Box::new(extract_statement(ctx)?);
    ctx.require_keyword("then")?;
    let true_branch = extract_statements_until(ctx, |tok| is_keyword(tok, &["fi", "elif", "else"]));
    let false_branch = if parse_keyword(ctx, "else") {
        Some(extract_statements_until(ctx, |tok| is_keyword(tok, &["fi"])))
    }
    else {
        None
    };
    ctx.require_keyword("fi")?;
    Ok(ast::Statement::Conditional { condition, true_branch, false_branch })
}

fn extract_loop_body(ctx: &mut Context) -> ParseResult<Vec<ast::Statement>> {
    ctx.require_keyword("do")?;
    let body = extract_statements_until(ctx, |tok| is_keyword(tok, &["done"]));
    ctx.require_keyword("done")?;
    Ok(body)
}

fn extract_for_loop(ctx: &mut Context) -> ParseResult<ast::Statement> {
    let variable = ctx.require_token_kind(TokenKind::Word).map(identifier)?;
    ctx.info.add_variable_write(variable.clone());
    ctx.skip_whitespace();
    ctx.require_keyword("in")?;
    ctx.skip_whitespace();
    let values = extract_arguments_until(ctx, is_statement_end);
    if values.is_empty() {
        let diagnostic = ctx.expected("an iterable value");
        ctx.info.diagnostics.push(diagnostic);
    }
    ctx.require_statement_end()?;
    Ok(ast::Statement::ForLoop { variable, values, body: extract_loop_body(ctx)? })
}

fn extract_while_loop(ctx: &mut Context) -> ParseResult<ast::Statement> {
    let condition = Box::new(extract_statement(ctx)?);
    let body = extract_loop_body(ctx)?;
    Ok(ast::Statement::WhileLoop { condition, body })
}

fn extract_function(ctx: &mut Context, id: ast::Identifier) -> ParseResult<ast::Statement> {
    ctx.skip_whitespace();
    ctx.require_token_kind(TokenKind::ParenClose)?;
    ctx.skip_empty_lines();
    ctx.require_token_kind(TokenKind::BraceOpen)?;
    ctx.skip_empty_lines();
    ctx.info.add_function_definition(id.clone());
    let body = extract_statements_until(ctx, |tok| tok.kind == TokenKind::BraceClose);
    ctx.require_token_kind(TokenKind::BraceClose)?;
    Ok(ast::Statement::FunctionDefinition { id, body })
}

fn extract_line_command(
    ctx: &mut Context,
    id: ast::Identifier,
    end: impl Fn(&Token) -> bool,
) -> ParseResult<ast::Statement> {
    if ctx.tokens.next_if_kind(TokenKind::Equals).is_some() {
        let value = parse_value(ctx)?.unwrap_or_else(|| ast::Value::RawString(String::new()));
        let assignment = ast::Assignment { id: id.clone(), value };
        ctx.skip_whitespace();
        if ctx.tokens.peek().is_none_or(&end) {
            ctx.info.add_variable_write(id);
            Ok(ast::Statement::VariableAssignment(assignment))
        }
        else {
            let id = identifier(ctx.require_token_kind(TokenKind::Word)?);
            ctx.skip_whitespace();
            let statement = Box::new(extract_line_command(ctx, id, end)?);
            Ok(ast::Statement::ScopedAssignment { assignment, statement })
        }
    }
    else {
        ctx.info.add_command_reference(id.clone());
        Ok(ast::Statement::Command {
            name: ast::Value::Word(id.name),
            arguments: extract_arguments_until(ctx, end),
        })
    }
}

fn extract_command(
    ctx: &mut Context,
    id: ast::Identifier,
    end: impl Fn(&Token) -> bool,
) -> ParseResult<ast::Statement> {
    ctx.skip_whitespace();
    match id.name.as_str() {
        "if" => extract_conditional(ctx),
        "for" => extract_for_loop(ctx),
        "while" => extract_while_loop(ctx),
        _ => {
            if ctx.tokens.next_if_kind(TokenKind::ParenOpen).is_some() {
                extract_function(ctx, id)
            }
            else {
                extract_line_command(ctx, id, end)
            }
        }
    }
}

fn skip_to_next_recovery_point(ctx: &mut Context) {
    for tok in ctx.tokens.by_ref() {
        if is_statement_end(&tok) {
            break;
        }
    }
}

fn extract_statement_up_to(
    ctx: &mut Context,
    end: impl Fn(&Token) -> bool,
) -> ParseResult<ast::Statement> {
    ctx.skip_empty_lines();
    let tok = ctx.require_token()?;
    if tok.kind == TokenKind::Word {
        extract_command(ctx, identifier(tok), end)
    }
    else {
        Err(ctx.expected("a statement"))
    }
}

fn extract_statement(ctx: &mut Context) -> ParseResult<ast::Statement> {
    let statement = extract_statement_up_to(ctx, is_statement_end)?;
    ctx.require_statement_end()?;
    ctx.skip_empty_lines();
    Ok(statement)
}

fn extract_statements_until(ctx: &mut Context, f: impl Fn(&Token) -> bool) -> Vec<ast::Statement> {
    let mut statements = Vec::new();
    while !ctx.tokens.peek().is_none_or(&f) {
        match extract_statement(ctx) {
            Ok(statement) => statements.push(statement),
            Err(diagnostic) => {
                ctx.info.diagnostics.push(diagnostic);
                skip_to_next_recovery_point(ctx);
            }
        }
    }
    statements
}

pub struct ParseData {
    pub program: Vec<ast::Statement>,
    pub info: db::DocumentInfo,
}

pub fn parse(input: &str) -> ParseData {
    let mut ctx = Context::new(input);
    let program = extract_statements_until(&mut ctx, |_| false);
    ParseData { program, info: ctx.info }
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
    fn command(name: &str, arguments: Vec<ast::Value>) -> ast::Statement {
        ast::Statement::Command { name: word(name), arguments: arguments.to_vec() }
    }

    fn parse_statements(input: &str) -> Vec<ast::Statement> {
        let result = parse(input);
        if result.info.diagnostics.is_empty() {
            result.program
        }
        else {
            dbg!(result.info.diagnostics);
            panic!()
        }
    }

    #[test]
    fn conditional() {
        assert_eq!(
            parse_statements("if ls -la; then\n\tpwd\n\tuname -a\nfi\n"),
            vec![ast::Statement::Conditional {
                condition: Box::new(command("ls", vec![word("-la")])),
                true_branch: vec![command("pwd", vec![]), command("uname", vec![word("-a")])],
                false_branch: None,
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
                body: vec![command(
                    "echo",
                    vec![ast::Value::Expansion(ast::Expansion::Simple(identifier(
                        "x"
                    )))]
                )],
            }]
        );
    }

    #[test]
    fn while_loop() {
        assert_eq!(
            parse_statements("while true; do echo $x; done\n"),
            vec![ast::Statement::WhileLoop {
                condition: Box::new(command("true", vec![])),
                body: vec![command(
                    "echo",
                    vec![ast::Value::Expansion(ast::Expansion::Simple(identifier(
                        "x"
                    )))]
                )],
            }]
        );
    }

    #[test]
    fn assignment() {
        assert_eq!(
            parse_statements("a=b c=d e f\n"),
            vec![ast::Statement::ScopedAssignment {
                assignment: ast::Assignment { id: identifier("a"), value: word("b") },
                statement: Box::new(ast::Statement::ScopedAssignment {
                    assignment: ast::Assignment { id: identifier("c"), value: word("d") },
                    statement: Box::new(command("e", vec![word("f")])),
                })
            }]
        );
    }
}
