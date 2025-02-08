use crate::lex::{self, Lexer, Token, TokenKind};
use crate::shell::Shell;
use crate::{db, lsp, util};
use std::collections::HashMap;

type ParseResult<T> = Result<T, lsp::Diagnostic>;

struct Context<'a> {
    info: db::DocumentInfo,
    lexer: Lexer<'a>,
    document: &'a str,
    commands: HashMap<String, db::SymbolId>,
    variables: HashMap<String, db::SymbolId>,
    annotations: db::Annotations,
}

impl<'a> Context<'a> {
    fn new(document: &'a str) -> Self {
        Self {
            info: db::DocumentInfo::default(),
            lexer: Lexer::new(document),
            document,
            commands: HashMap::new(),
            variables: HashMap::new(),
            annotations: db::Annotations::default(),
        }
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
    fn emit(&mut self, diagnostic: lsp::Diagnostic) {
        self.info.diagnostics.push(diagnostic);
    }
    fn warn(&mut self, range: lsp::Range, message: impl Into<String>) {
        self.emit(lsp::Diagnostic::warning(range, message))
    }
}

fn symbol(
    annotations: &mut db::Annotations,
    info: &mut db::DocumentInfo,
    symbols: &mut HashMap<String, db::SymbolId>,
    document: &str,
    sym_kind: db::SymbolKind,
    ref_kind: lsp::ReferenceKind,
    word: Token,
) {
    let name = lex::escape(word.view.string(document));
    let id = symbols.get(&name).copied().unwrap_or_else(|| {
        let symbol = db::Symbol::new(
            name.clone(),
            sym_kind,
            (ref_kind == lsp::ReferenceKind::Write)
                .then(|| std::mem::take(annotations))
                .unwrap_or_default(),
        );
        let id = info.symbols.push(symbol);
        symbols.insert(name, id);
        id
    });
    let reference = lsp::Reference { range: word.range, kind: ref_kind };
    info.references.push(db::SymbolReference { reference, id });
}

fn add_var_ref(ctx: &mut Context, word: Token, kind: lsp::ReferenceKind) {
    symbol(
        &mut ctx.annotations,
        &mut ctx.info,
        &mut ctx.variables,
        ctx.document,
        db::SymbolKind::Variable,
        kind,
        word,
    );
}

fn add_cmd_ref(ctx: &mut Context, word: Token, kind: lsp::ReferenceKind) {
    symbol(
        &mut ctx.annotations,
        &mut ctx.info,
        &mut ctx.commands,
        ctx.document,
        db::SymbolKind::Command,
        kind,
        word,
    );
}

fn protected(ctx: &mut Context, callback: impl FnOnce(&mut Context) -> ParseResult<bool>) -> bool {
    match callback(ctx) {
        Ok(result) => result,
        Err(diagnostic) => {
            ctx.emit(diagnostic);
            false
        }
    }
}

const END_KINDS: &[TokenKind] = {
    use TokenKind::*;
    &[NewLine, Semi]
};

const REDIRECT_KINDS: &[TokenKind] = {
    use TokenKind::*;
    &[Great, GreatGreat, Less, LessLess, GreatPipe, GreatAnd, LessAnd]
};

const CONTINUATION_KINDS: &[TokenKind] = {
    use TokenKind::*;
    &[And, AndAnd, Pipe, PipePipe]
};

fn kind_matches(kinds: &'static [TokenKind]) -> impl Copy + Fn(Token) -> bool {
    |token| kinds.contains(&token.kind)
}

fn handle_comment(ctx: &mut Context, token: Token) {
    if token.kind != TokenKind::Comment {
        return;
    }
    let Some(line) = token.view.string(ctx.document).strip_prefix("##@").map(str::trim_start)
    else {
        return;
    };
    let offset = line.find(char::is_whitespace).unwrap_or(line.len());
    let start = token.view.end - line[offset..].trim_start().len() as u32;
    let view = util::View { start, end: token.view.end };
    match &line[..offset] {
        "desc" => ctx.annotations.desc = Some(view),
        "exit" => ctx.annotations.exit = Some(view),
        "stdin" => ctx.annotations.stdin = Some(view),
        "stdout" => ctx.annotations.stdout = Some(view),
        "stderr" => ctx.annotations.stderr = Some(view),
        "param" => ctx.annotations.params.push(view),
        "" => {
            ctx.warn(token.range, "Empty directive");
        }
        directive => {
            ctx.warn(token.range, format!("Unrecognized directive: '{directive}'"));
        }
    }
}

fn skip_whitespace(ctx: &mut Context) {
    const KINDS: &[TokenKind] = &[TokenKind::Space, TokenKind::Comment];
    while let Some(token) = ctx.lexer.next_if(kind_matches(KINDS)) {
        handle_comment(ctx, token);
    }
}

fn skip_empty_lines(ctx: &mut Context) {
    const KINDS: &[TokenKind] = &[TokenKind::Space, TokenKind::Comment, TokenKind::NewLine];
    while let Some(token) = ctx.lexer.next_if(kind_matches(KINDS)) {
        handle_comment(ctx, token);
    }
}

fn expect_statement_end(ctx: &mut Context) -> ParseResult<()> {
    skip_whitespace(ctx);
    if ctx.lexer.next_if(kind_matches(END_KINDS)).is_some() {
        skip_whitespace(ctx);
        Ok(())
    }
    else {
        Err(ctx.expected("a new line or a semicolon"))
    }
}

fn extract_enclosed_statements(ctx: &mut Context, end: impl Copy + Fn(Token) -> bool) {
    while !ctx.lexer.peek().is_none_or(end) {
        skip_empty_lines(ctx);
        if ctx.lexer.peek().is_some_and(end) {
            break;
        }
        if let Err(diagnostic) = extract_statement_up_to(ctx, end) {
            ctx.emit(diagnostic);
            ctx.lexer.next();
        }
        ctx.consume(TokenKind::Semi);
    }
}

fn is_keyword(document: &str, token: Token, keywords: &[&str]) -> bool {
    token.kind == TokenKind::Word && keywords.contains(&token.view.string(document))
}

fn parse_keyword(ctx: &mut Context, keyword: &str) -> bool {
    let predicate =
        |token: Token| token.kind == TokenKind::Word && token.view.string(ctx.document) == keyword;
    ctx.lexer.next_if(predicate).is_some()
}

fn parse_dollar(dollar: Token, ctx: &mut Context) -> ParseResult<()> {
    parse_expansion(dollar, ctx)?;
    Ok(())
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
    const KINDS: &[TokenKind] = {
        use TokenKind::*;
        &[RawString, Equal, DollarHash]
    };
    if let Some(quote) = ctx.lexer.next_if_kind(TokenKind::DoubleQuote) {
        parse_string(ctx, quote);
    }
    else if ctx.consume(TokenKind::BackQuote) {
        extract_enclosed_statements(ctx, kind_matches(&[TokenKind::BackQuote]));
        ctx.expect(TokenKind::BackQuote)?;
    }
    else if ctx.lexer.next_if(kind_matches(KINDS)).is_none() {
        return parse_word(ctx);
    }
    Ok(true)
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

fn skip_redirect(ctx: &mut Context) {
    while ctx.lexer.next_if(kind_matches(REDIRECT_KINDS)).is_some() {
        skip_whitespace(ctx);
        if !protected(ctx, parse_value) {
            let diagnostic = ctx.expected("a filename");
            ctx.emit(diagnostic);
        }
        skip_whitespace(ctx);
    }
}

fn extract_arguments_until(ctx: &mut Context, end: impl Copy + Fn(Token) -> bool) {
    loop {
        skip_whitespace(ctx);
        skip_redirect(ctx);
        if ctx.lexer.peek().is_none_or(end) || !protected(ctx, parse_value) {
            break;
        }
    }
}

fn parse_expansion(dollar: Token, ctx: &mut Context) -> ParseResult<bool> {
    if let Some(word) = ctx.lexer.next_if_kind(TokenKind::Word) {
        add_var_ref(ctx, word, lsp::ReferenceKind::Read);
    }
    else if ctx.consume(TokenKind::BraceOpen) {
        let name = ctx.expect(TokenKind::Word)?;
        add_var_ref(ctx, name, lsp::ReferenceKind::Read);
        ctx.expect(TokenKind::BraceClose)?;
    }
    else if ctx.consume(TokenKind::ParenOpen) {
        extract_enclosed_statements(ctx, kind_matches(&[TokenKind::ParenClose]));
        ctx.expect(TokenKind::ParenClose)?;
    }
    else if !ctx.consume(TokenKind::Dollar) {
        let message = "This `$` is literal. Use `\\$` to suppress this hint.";
        ctx.emit(lsp::Diagnostic::info(dollar.range, message));
        return Ok(false);
    }
    Ok(true)
}

fn parse_string(ctx: &mut Context, quote: Token) {
    while let Some(token) = ctx.lexer.next() {
        match token.kind {
            TokenKind::DoubleQuote => return,
            TokenKind::Dollar => {
                if let Err(diagnostic) = parse_expansion(token, ctx) {
                    ctx.emit(diagnostic);
                }
            }
            TokenKind::BackQuote => {
                extract_enclosed_statements(
                    ctx,
                    kind_matches(&[TokenKind::BackQuote, TokenKind::DoubleQuote]),
                );
                if !ctx.consume(TokenKind::BackQuote) {
                    let diagnostic = ctx.expected("A closing backquote");
                    ctx.emit(diagnostic);
                }
            }
            _ => continue,
        }
    }
    ctx.emit(lsp::Diagnostic::error(quote.range, "Unterminated string"));
}

fn extract_conditional(ctx: &mut Context) -> ParseResult<()> {
    extract_statement(ctx)?;
    ctx.expect_word("then")?;
    extract_statements_until(ctx, |token| is_keyword(ctx.document, token, &["fi", "else", "elif"]));
    if parse_keyword(ctx, "else") {
        extract_statements_until(ctx, |token| is_keyword(ctx.document, token, &["fi"]));
    }
    if parse_keyword(ctx, "elif") {
        return extract_conditional(ctx);
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
    let variable = ctx.expect(TokenKind::Word)?;
    add_var_ref(ctx, variable, lsp::ReferenceKind::Write);
    skip_whitespace(ctx);
    ctx.expect_word("in")?;
    skip_whitespace(ctx);
    extract_arguments_until(ctx, kind_matches(END_KINDS));
    expect_statement_end(ctx)?;
    extract_loop_body(ctx)?;
    Ok(())
}

fn extract_while_loop(ctx: &mut Context) -> ParseResult<()> {
    extract_statement(ctx)?;
    extract_loop_body(ctx)?;
    Ok(())
}

fn parse_pattern(ctx: &mut Context) -> ParseResult<bool> {
    skip_whitespace(ctx);
    if parse_value(ctx)? {
        skip_whitespace(ctx);
        if ctx.consume(TokenKind::Pipe) { parse_pattern(ctx) } else { Ok(true) }
    }
    else {
        Ok(false)
    }
}

fn parse_case_item(ctx: &mut Context) -> ParseResult<bool> {
    skip_empty_lines(ctx);
    let end = |token: Token| {
        token.kind == TokenKind::SemiSemi || is_keyword(ctx.document, token, &["esac"])
    };
    if ctx.lexer.peek().is_some_and(end) {
        return Ok(false);
    }
    let open = ctx.consume(TokenKind::ParenOpen);
    if !parse_pattern(ctx)? {
        return if open { Err(ctx.expected("a pattern")) } else { Ok(false) };
    }
    ctx.expect(TokenKind::ParenClose)?;
    extract_enclosed_statements(ctx, end);
    Ok(true)
}

fn extract_case(ctx: &mut Context) -> ParseResult<()> {
    if !parse_value(ctx)? {
        return Err(ctx.expected("a word"));
    }
    skip_whitespace(ctx);
    ctx.expect_word("in")?;
    skip_empty_lines(ctx);
    if !protected(ctx, parse_case_item) {
        let diagnostic = ctx.expected("at least one pattern");
        ctx.emit(diagnostic);
    }
    while ctx.consume(TokenKind::SemiSemi) && protected(ctx, parse_case_item) {}
    skip_empty_lines(ctx);
    ctx.expect_word("esac")?;
    Ok(())
}

fn extract_function(ctx: &mut Context, word: Token) -> ParseResult<()> {
    skip_whitespace(ctx);
    ctx.expect(TokenKind::ParenClose)?;
    skip_empty_lines(ctx);
    ctx.expect(TokenKind::BraceOpen)?;
    skip_empty_lines(ctx);
    add_cmd_ref(ctx, word, lsp::ReferenceKind::Write);
    extract_statements_until(ctx, kind_matches(&[TokenKind::BraceClose]));
    ctx.expect(TokenKind::BraceClose)?;
    Ok(())
}

fn extract_line_command(
    ctx: &mut Context,
    word: Token,
    end: impl Copy + Fn(Token) -> bool,
) -> ParseResult<()> {
    if ctx.consume(TokenKind::Equal) {
        parse_value(ctx)?;
        skip_whitespace(ctx);
        if ctx.lexer.peek().is_none_or(end) {
            add_var_ref(ctx, word, lsp::ReferenceKind::Write);
        }
        else {
            let word = ctx.expect(TokenKind::Word)?;
            skip_whitespace(ctx);
            extract_line_command(ctx, word, end)?;
        }
    }
    else {
        add_cmd_ref(ctx, word, lsp::ReferenceKind::Read);
        extract_arguments_until(ctx, end);
    }
    Ok(())
}

fn extract_command(
    ctx: &mut Context,
    word: Token,
    end: impl Copy + Fn(Token) -> bool,
) -> ParseResult<()> {
    if ctx.consume(TokenKind::ParenOpen) {
        extract_function(ctx, word)
    }
    else {
        extract_line_command(ctx, word, end)
    }
}

fn extract_statement_up_to(
    ctx: &mut Context,
    end: impl Copy + Fn(Token) -> bool,
) -> ParseResult<()> {
    let end = |token| {
        end(token) || token.kind == TokenKind::NewLine || kind_matches(CONTINUATION_KINDS)(token)
    };
    skip_empty_lines(ctx);
    loop {
        skip_whitespace(ctx);
        if let Some(word) = ctx.lexer.next_if_kind(TokenKind::Word) {
            skip_whitespace(ctx);
            match word.view.string(ctx.document) {
                "if" => extract_conditional(ctx)?,
                "for" => extract_for_loop(ctx)?,
                "while" => extract_while_loop(ctx)?,
                "case" => extract_case(ctx)?,
                _ => extract_command(ctx, word, end)?,
            }
        }
        else if parse_value(ctx)? {
            extract_arguments_until(ctx, end);
        }
        else if ctx.consume(TokenKind::ParenOpen) {
            skip_whitespace(ctx);
            extract_enclosed_statements(ctx, kind_matches(&[TokenKind::ParenClose]));
            ctx.expect(TokenKind::ParenClose)?;
        }
        else if ctx.consume(TokenKind::BraceOpen) {
            skip_whitespace(ctx);
            extract_enclosed_statements(ctx, kind_matches(&[TokenKind::BraceClose]));
            ctx.expect(TokenKind::BraceClose)?;
        }
        else {
            return Err(ctx.expected("a statement"));
        }
        if ctx.lexer.next_if(kind_matches(CONTINUATION_KINDS)).is_none() {
            return Ok(());
        }
    }
}

fn extract_statement(ctx: &mut Context) -> ParseResult<()> {
    skip_whitespace(ctx);
    extract_statement_up_to(ctx, kind_matches(END_KINDS))?;
    skip_whitespace(ctx);
    expect_statement_end(ctx)?;
    skip_whitespace(ctx);
    skip_empty_lines(ctx);
    Ok(())
}

fn extract_statements_until(ctx: &mut Context, predicate: impl Copy + Fn(Token) -> bool) {
    while !ctx.lexer.peek().is_none_or(predicate) {
        if let Err(diagnostic) = extract_statement(ctx) {
            ctx.emit(diagnostic);
            skip_to_next_recovery_point(ctx);
        }
    }
}

fn skip_to_next_recovery_point(ctx: &mut Context) {
    let predicate = kind_matches(END_KINDS);
    for token in ctx.lexer.by_ref() {
        if predicate(token) {
            break;
        }
    }
}

fn set_shell(ctx: &mut Context, comment: Token, shell: Shell) {
    if shell == Shell::Posix {
        return;
    }
    let msg = format!("{} is not supported yet, treating as {}", shell.name(), Shell::Posix.name());
    ctx.warn(comment.range, msg);
}

fn parse_shebang(ctx: &mut Context) {
    if let Some(comment) = ctx.lexer.next_if_kind(TokenKind::Comment) {
        if let Some(shebang) = comment.view.string(ctx.document).strip_prefix("#!") {
            match shebang.parse() {
                Ok(shell) => set_shell(ctx, comment, shell),
                Err(error) => ctx.warn(comment.range, error),
            }
        }
        else {
            handle_comment(ctx, comment);
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

    fn diagnostics(input: &str) -> Vec<super::lsp::Diagnostic> {
        super::parse(input).diagnostics
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
        let diags = diagnostics("echo $\n");
        assert_let!([diag] = diags.as_slice());
        assert!(diag.message.contains("literal"));
    }
}
