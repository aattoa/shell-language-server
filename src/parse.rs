use crate::lex::{self, Lexer, Token, TokenKind};
use crate::shell::Shell;
use crate::{db, lsp};

type ParseResult<T> = Result<T, lsp::Diagnostic>;

struct Context<'a> {
    info: db::DocumentInfo,
    lexer: Lexer<'a>,
    document: &'a str,
}

impl<'a> Context<'a> {
    fn new(document: &'a str) -> Self {
        Self { info: db::DocumentInfo::default(), lexer: Lexer::new(document), document }
    }
    fn error(&mut self, message: impl Into<String>) -> lsp::Diagnostic {
        lsp::Diagnostic::error(self.lexer.current_range(), message)
    }
    fn expected(&mut self, description: &str) -> lsp::Diagnostic {
        let found = self.lexer.peek().map_or("the end of input", |token| token.kind.show());
        self.error(format!("Expected {}, but found {}", description, found))
    }
    fn expect(&mut self, kind: TokenKind) -> ParseResult<Token> {
        self.lexer.next_if_kind(kind).ok_or_else(|| self.expected(kind.show()))
    }
    fn expect_word(&mut self, keyword: &str) -> ParseResult<()> {
        if parse_keyword(self, keyword) { Ok(()) } else { Err(self.expected(keyword)) }
    }
    fn consume(&mut self, kind: TokenKind) -> bool {
        self.lexer.next_if_kind(kind).is_some()
    }
}

fn kind_matches(kinds: &'static [TokenKind]) -> impl Fn(Token) -> bool + Copy {
    |token| kinds.contains(&token.kind)
}

fn is_statement_end(token: Token) -> bool {
    kind_matches(&[TokenKind::Semi, TokenKind::NewLine])(token)
}

fn skip_whitespace(ctx: &mut Context) {
    let predicate = kind_matches(&[TokenKind::Space, TokenKind::Comment]);
    while ctx.lexer.next_if(predicate).is_some() {}
}

fn skip_empty_lines(ctx: &mut Context) {
    let predicate = kind_matches(&[TokenKind::Space, TokenKind::Comment, TokenKind::NewLine]);
    while ctx.lexer.next_if(predicate).is_some() {}
}

fn expect_statement_end(ctx: &mut Context) -> ParseResult<()> {
    skip_whitespace(ctx);
    if ctx.lexer.next_if(is_statement_end).is_some() {
        skip_whitespace(ctx);
        Ok(())
    }
    else {
        Err(ctx.expected("a new line or a semicolon"))
    }
}

fn is_keyword(document: &str, token: Token, keywords: &[&str]) -> bool {
    token.kind == TokenKind::Word && keywords.contains(&token.view.string(document))
}

fn identifier(document: &str, token: Token) -> db::Identifier {
    db::Identifier { name: lex::escape(token.view.string(document)), range: token.range }
}

fn parse_keyword(ctx: &mut Context, keyword: &str) -> bool {
    ctx.lexer
        .next_if(|token| {
            token.kind == TokenKind::Word && token.view.string(ctx.document) == keyword
        })
        .is_some()
}

fn parse_dollar(dollar: Token, ctx: &mut Context) -> ParseResult<()> {
    parse_expansion(dollar, ctx).map(|_| ())
}

fn parse_word(ctx: &mut Context) -> ParseResult<bool> {
    Ok(if ctx.consume(TokenKind::Word) {
        true
    }
    else if let Some(dollar) = ctx.lexer.next_if_kind(TokenKind::Dollar) {
        parse_dollar(dollar, ctx)?;
        true
    }
    else {
        false
    })
}

fn parse_simple_value(ctx: &mut Context) -> ParseResult<bool> {
    if let Some(quote) = ctx.lexer.next_if_kind(TokenKind::DoubleQuote) {
        parse_string(ctx, quote);
        Ok(true)
    }
    else if ctx
        .lexer
        .next_if(kind_matches(&[
            TokenKind::RawString,
            TokenKind::Equals,
            TokenKind::DollarHash,
        ]))
        .is_some()
    {
        Ok(true)
    }
    else {
        parse_word(ctx)
    }
}

fn parse_value(ctx: &mut Context) -> ParseResult<bool> {
    Ok(if parse_simple_value(ctx)? {
        while parse_simple_value(ctx)? {}
        true
    }
    else {
        false
    })
}

fn extract_arguments_until(ctx: &mut Context, pred: impl Fn(Token) -> bool) {
    while !ctx.lexer.peek().is_none_or(&pred) {
        match parse_value(ctx) {
            Ok(true) => skip_whitespace(ctx),
            Ok(false) => {
                let diagnostic = ctx.expected("an argument");
                ctx.info.diagnostics.push(diagnostic);
                ctx.lexer.next();
                break;
            }
            Err(diagnostic) => {
                ctx.info.diagnostics.push(diagnostic);
                break;
            }
        }
    }
}

fn parse_expansion(dollar: Token, ctx: &mut Context) -> ParseResult<bool> {
    Ok(if let Some(word) = ctx.lexer.next_if_kind(TokenKind::Word) {
        ctx.info.add_variable_read(identifier(ctx.document, word));
        true
    }
    else if ctx.consume(TokenKind::BraceOpen) {
        let name = ctx.expect(TokenKind::Word)?;
        ctx.info.add_variable_read(identifier(ctx.document, name));
        ctx.expect(TokenKind::BraceClose)?;
        true
    }
    else if ctx.consume(TokenKind::ParenOpen) {
        extract_statement_up_to(ctx, |token| token.kind == TokenKind::ParenClose)?;
        ctx.expect(TokenKind::ParenClose)?;
        true
    }
    else {
        let message = "This `$` is literal. Use `\\$` to suppress this hint.";
        ctx.info.diagnostics.push(lsp::Diagnostic::info(dollar.range, message));
        false
    })
}

fn parse_string(ctx: &mut Context, quote: Token) {
    while let Some(token) = ctx.lexer.next() {
        match token.kind {
            TokenKind::DoubleQuote => return,
            TokenKind::Dollar => {
                if let Err(diagnostic) = parse_expansion(token, ctx) {
                    ctx.info.diagnostics.push(diagnostic);
                }
            }
            _ => continue,
        }
    }
    ctx.info.diagnostics.push(lsp::Diagnostic::error(quote.range, "Unterminated string"));
}

fn extract_conditional(ctx: &mut Context) -> ParseResult<()> {
    extract_statement(ctx)?;
    ctx.expect_word("then")?;
    extract_statements_until(ctx, |token| is_keyword(ctx.document, token, &["fi", "elif", "else"]));
    if parse_keyword(ctx, "else") {
        extract_statements_until(ctx, |token| is_keyword(ctx.document, token, &["fi"]));
    }
    ctx.expect_word("fi")?;
    Ok(())
}

fn extract_loop_body(ctx: &mut Context) -> ParseResult<()> {
    ctx.expect_word("do")?;
    extract_statements_until(ctx, |token| is_keyword(ctx.document, token, &["done"]));
    ctx.expect_word("done")?;
    Ok(())
}

fn extract_for_loop(ctx: &mut Context) -> ParseResult<()> {
    let variable = ctx.expect(TokenKind::Word).map(|token| identifier(ctx.document, token))?;
    ctx.info.add_variable_write(variable);
    skip_whitespace(ctx);
    ctx.expect_word("in")?;
    skip_whitespace(ctx);
    extract_arguments_until(ctx, is_statement_end);
    expect_statement_end(ctx)?;
    extract_loop_body(ctx)?;
    Ok(())
}

fn extract_while_loop(ctx: &mut Context) -> ParseResult<()> {
    extract_statement(ctx)?;
    extract_loop_body(ctx)?;
    Ok(())
}

fn extract_function(ctx: &mut Context, id: db::Identifier) -> ParseResult<()> {
    skip_whitespace(ctx);
    ctx.expect(TokenKind::ParenClose)?;
    skip_empty_lines(ctx);
    ctx.expect(TokenKind::BraceOpen)?;
    skip_empty_lines(ctx);
    ctx.info.add_function_definition(id);
    extract_statements_until(ctx, |token| token.kind == TokenKind::BraceClose);
    ctx.expect(TokenKind::BraceClose)?;
    Ok(())
}

fn extract_line_command(
    ctx: &mut Context,
    id: db::Identifier,
    end: impl Fn(Token) -> bool,
) -> ParseResult<()> {
    if ctx.consume(TokenKind::Equals) {
        parse_value(ctx)?;
        skip_whitespace(ctx);
        if ctx.lexer.peek().is_none_or(&end) {
            ctx.info.add_variable_write(id);
        }
        else {
            let word = ctx.expect(TokenKind::Word)?;
            skip_whitespace(ctx);
            extract_line_command(ctx, identifier(ctx.document, word), end)?;
        }
    }
    else {
        ctx.info.add_command_reference(id);
        extract_arguments_until(ctx, end);
    }
    Ok(())
}

fn extract_command(
    ctx: &mut Context,
    id: db::Identifier,
    end: impl Fn(Token) -> bool,
) -> ParseResult<()> {
    skip_whitespace(ctx);
    match id.name.as_str() {
        "if" => extract_conditional(ctx),
        "for" => extract_for_loop(ctx),
        "while" => extract_while_loop(ctx),
        _ => {
            if ctx.consume(TokenKind::ParenOpen) {
                extract_function(ctx, id)
            }
            else {
                extract_line_command(ctx, id, end)
            }
        }
    }
}

fn skip_to_next_recovery_point(ctx: &mut Context) {
    for token in ctx.lexer.by_ref() {
        if is_statement_end(token) {
            break;
        }
    }
}

fn extract_statement_up_to(ctx: &mut Context, end: impl Fn(Token) -> bool) -> ParseResult<()> {
    skip_empty_lines(ctx);
    let token = ctx.expect(TokenKind::Word)?;
    extract_command(ctx, identifier(ctx.document, token), end)
}

fn extract_statement(ctx: &mut Context) -> ParseResult<()> {
    extract_statement_up_to(ctx, is_statement_end)?;
    expect_statement_end(ctx)?;
    skip_empty_lines(ctx);
    Ok(())
}

fn extract_statements_until(ctx: &mut Context, f: impl Fn(Token) -> bool) {
    while !ctx.lexer.peek().is_none_or(&f) {
        if let Err(diagnostic) = extract_statement(ctx) {
            ctx.info.diagnostics.push(diagnostic);
            skip_to_next_recovery_point(ctx);
        }
    }
}

fn parse_shebang(ctx: &mut Context) {
    if let Some(comment) = ctx.lexer.next_if_kind(TokenKind::Comment) {
        if let Some(shebang) = comment.view.string(ctx.document).strip_prefix("#!") {
            match shebang.parse() {
                Ok(Shell::Posix) => {}
                Ok(shell) => {
                    let message = format!("{} is not supported yet", shell.name());
                    ctx.info.diagnostics.push(lsp::Diagnostic::warning(comment.range, message));
                }
                Err(error) => {
                    ctx.info.diagnostics.push(lsp::Diagnostic::warning(comment.range, error));
                }
            }
        }
    }
}

pub fn parse(input: &str) -> db::DocumentInfo {
    let mut ctx = Context::new(input);
    parse_shebang(&mut ctx);
    skip_empty_lines(&mut ctx);
    extract_statements_until(&mut ctx, |_| false);
    ctx.info
}

#[cfg(test)]
mod tests {
    use crate::assert_let;

    fn diagnostics(input: &str) -> Vec<String> {
        super::parse(input).diagnostics.into_iter().map(|diagnostic| diagnostic.message).collect()
    }

    #[test]
    fn conditional() {
        assert_let!([] = diagnostics("if ls -la; then\n\tpwd\n\tuname -a\nfi\n").as_slice());
    }

    #[test]
    fn for_loop() {
        assert_let!([] = diagnostics("for x in a b c\ndo\n\techo $x\ndone\n").as_slice());
    }

    #[test]
    fn while_loop() {
        assert_let!([] = diagnostics("while true; do echo $x; done\n").as_slice());
    }

    #[test]
    fn assignment() {
        assert_let!([] = diagnostics("a=b c=d e f\n").as_slice());
    }

    #[test]
    fn dollar() {
        let messages = diagnostics("echo $\n");
        assert_let!([message] = messages.as_slice());
        assert!(message.contains("literal"));
    }
}
