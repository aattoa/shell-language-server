use crate::config::Settings;
use crate::lex::{self, Lexer, Token, TokenKind};
use crate::shell::{self, Shell};
use crate::{db, env, lsp};
use std::borrow::Cow;
use std::collections::HashMap;

type ParseResult<T> = Result<T, lsp::Diagnostic>;

struct Annotations {
    params: Vec<db::View>,
    desc: Option<String>,
}

struct Context<'a> {
    info: db::DocumentInfo,
    lexer: Lexer<'a>,
    document: &'a str,
    commands: HashMap<String, db::SymbolId>,
    variables: HashMap<String, db::SymbolId>,
    locals: Option<HashMap<String, db::SymbolId>>,
    annotations: Annotations,
}

impl<'a> Context<'a> {
    fn new(document: &'a str, shell: Shell) -> Self {
        Self {
            info: db::DocumentInfo { shell, ..db::DocumentInfo::default() },
            lexer: Lexer::new(document),
            document,
            commands: HashMap::new(),
            variables: HashMap::new(),
            locals: None,
            annotations: Annotations { params: Vec::new(), desc: None },
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
    fn inform(&mut self, range: lsp::Range, message: impl Into<String>) {
        self.emit(lsp::Diagnostic::info(range, message))
    }
}

fn location(first: Token, last: Token) -> db::Location {
    db::Location {
        range: lsp::Range { start: first.range.start, end: last.range.end },
        view: db::View { start: first.view.start, end: last.view.end },
    }
}

fn command_symbol(ctx: &mut Context, word: Token) -> db::SymbolId {
    let name = lex::escape(word.view.string(ctx.document));
    ctx.commands.get(name.as_ref()).copied().unwrap_or_else(|| {
        let name = name.into_owned();
        let id = ctx.info.new_command(name.clone());
        ctx.commands.insert(name, id);
        id
    })
}

fn new_variable(ctx: &mut Context, name: String) -> db::SymbolId {
    let var = db::Variable::new(db::VariableKind::Global);
    let id = ctx.info.new_variable(name.clone(), var);
    ctx.variables.insert(name, id);
    id
}

fn variable_symbol(ctx: &mut Context, word: Token) -> db::SymbolId {
    let name = lex::escape(word.view.string(ctx.document));
    (ctx.locals.as_ref())
        .and_then(|locals| locals.get(name.as_ref()).copied())
        .or_else(|| ctx.variables.get(name.as_ref()).copied())
        .unwrap_or_else(|| new_variable(ctx, name.into_owned()))
}

fn add_cmd_ref(ctx: &mut Context, word: Token) {
    let id = command_symbol(ctx, word);
    ctx.info.references.push(db::SymbolReference::read(word.range, id));
}

fn add_var_read(ctx: &mut Context, word: Token) -> db::SymbolId {
    let id = variable_symbol(ctx, word);
    ctx.info.references.push(db::SymbolReference::read(word.range, id));
    id
}

fn add_var_write(ctx: &mut Context, word: Token) -> db::SymbolId {
    let id = variable_symbol(ctx, word);
    ctx.info.references.push(db::SymbolReference::write(word.range, id));
    id
}

fn define_function(ctx: &mut Context, word: Token) -> db::SymbolId {
    let name = lex::escape(word.view.string(ctx.document)).into_owned();
    let id = ctx.info.new_function(name.clone(), db::Function {
        description: ctx.annotations.desc.take(),
        definition: None,
        parameters: std::mem::take(&mut ctx.annotations.params),
    });
    ctx.info.references.push(db::SymbolReference::write(word.range, id));
    ctx.commands.insert(name, id);
    id
}

fn unset_function(ctx: &mut Context, word: Token) {
    let name = lex::escape(word.view.string(ctx.document));
    if let Some(&id) = ctx.commands.get(name.as_ref()) {
        if let db::SymbolKind::Function { .. } = ctx.info.symbols[id].kind {
            ctx.info.references.push(db::SymbolReference::write(word.range, id));
            ctx.commands.remove(name.as_ref());
            return;
        }
    }
    ctx.warn(word.range, format!("'{name}' is not a function"));
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

fn is_identifier(str: &str, shell: Shell) -> bool {
    if shell == Shell::Bash { str.chars().all(|char| char != '$') } else { lex::is_name(str) }
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

fn add_description(ctx: &mut Context, annotation: db::View) {
    let string = annotation.string(ctx.document).trim_end();
    if let Some(desc) = &mut ctx.annotations.desc {
        desc.push('\n');
        desc.push_str(string);
    }
    else {
        ctx.annotations.desc = Some(String::from(string));
    }
}

fn parse_comment(ctx: &mut Context, token: Token) {
    if token.kind != TokenKind::Comment {
        return;
    }
    if let Some(line) = token.view.string(ctx.document).strip_prefix("##@").map(str::trim_start) {
        let offset = line.find(char::is_whitespace).unwrap_or(line.len());
        let arg_width = line[offset..].trim_start().len() as u32;
        let annotation = db::View { start: token.view.end - arg_width, end: token.view.end };

        ctx.info.tokens.data.push(lsp::SemanticToken {
            position: token.range.start,
            width: token.range.end.character - token.range.start.character - arg_width,
            kind: lsp::SemanticTokenKind::Keyword,
            modifier: lsp::SemanticTokenModifier::Documentation,
        });

        let remaining = |kind| lsp::SemanticToken {
            position: lsp::Position {
                character: token.range.end.character - arg_width,
                ..token.range.end
            },
            width: arg_width,
            kind,
            modifier: lsp::SemanticTokenModifier::Documentation,
        };

        match &line[..offset] {
            "desc" => {
                ctx.info.tokens.data.push(remaining(lsp::SemanticTokenKind::String));
                add_description(ctx, annotation);
            }
            "param" => {
                ctx.info.tokens.data.push(remaining(lsp::SemanticTokenKind::Parameter));
                ctx.annotations.params.push(annotation);
            }
            "" => ctx.warn(token.range, "Missing directive"),
            directive => ctx.warn(token.range, format!("Unrecognized directive: '{directive}'")),
        }
    }
}

fn skip_whitespace(ctx: &mut Context) {
    const KINDS: &[TokenKind] = &[TokenKind::Space, TokenKind::Comment];
    while let Some(token) = ctx.lexer.next_if(kind_matches(KINDS)) {
        parse_comment(ctx, token);
    }
}

fn skip_empty_lines(ctx: &mut Context) {
    const KINDS: &[TokenKind] = &[TokenKind::Space, TokenKind::Comment, TokenKind::NewLine];
    while let Some(token) = ctx.lexer.next_if(kind_matches(KINDS)) {
        parse_comment(ctx, token);
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

fn parse_word(ctx: &mut Context) -> ParseResult<bool> {
    if let Some(dollar) = ctx.lexer.next_if_kind(TokenKind::Dollar) {
        extract_potential_expansion(dollar, ctx)?;
    }
    else if !ctx.consume(TokenKind::Word) {
        return Ok(false);
    }
    Ok(true)
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
    if parse_simple_value(ctx)? {
        while parse_simple_value(ctx)? {}
        Ok(true)
    }
    else {
        Ok(false)
    }
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

fn extract_potential_expansion(dollar: Token, ctx: &mut Context) -> ParseResult<()> {
    if let Some(word) = ctx.lexer.next_if_kind(TokenKind::Word) {
        add_var_read(ctx, word);
    }
    else if ctx.consume(TokenKind::BraceOpen) {
        let name = ctx.expect(TokenKind::Word)?;
        add_var_read(ctx, name);
        ctx.expect(TokenKind::BraceClose)?;
    }
    else if ctx.consume(TokenKind::ParenOpen) {
        extract_enclosed_statements(ctx, kind_matches(&[TokenKind::ParenClose]));
        ctx.expect(TokenKind::ParenClose)?;
    }
    else if !ctx.consume(TokenKind::Dollar) {
        ctx.inform(dollar.range, "This `$` is literal. Use `\\$` to suppress this hint.")
    }
    Ok(())
}

fn parse_string(ctx: &mut Context, quote: Token) {
    while let Some(token) = ctx.lexer.next() {
        match token.kind {
            TokenKind::DoubleQuote => return,
            TokenKind::Dollar => {
                if let Err(diagnostic) = extract_potential_expansion(token, ctx) {
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
    add_var_assign(ctx, variable);
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

fn extract_builtin_local(ctx: &mut Context) -> ParseResult<()> {
    skip_whitespace(ctx);
    while let Some(word) = ctx.lexer.next_if_kind(TokenKind::Word) {
        if let Some(locals) = &mut ctx.locals {
            let name = lex::escape(word.view.string(ctx.document)).into_owned();
            let id = ctx.info.new_variable(name.clone(), db::Variable {
                description: ctx.annotations.desc.take(),
                first_assignment: Some(db::Location { range: word.range, view: word.view }),
                kind: db::VariableKind::Local,
            });
            ctx.info.references.push(db::SymbolReference::write(word.range, id));
            locals.insert(name, id);
        }
        else {
            ctx.warn(word.range, "`local` is invalid outside of a function");
        }
        if ctx.consume(TokenKind::Equal) {
            parse_value(ctx)?;
        }
        skip_whitespace(ctx);
    }
    Ok(())
}

fn extract_builtin_variable_declaration(ctx: &mut Context) -> ParseResult<()> {
    skip_whitespace(ctx);
    while let Some(word) = ctx.lexer.next_if_kind(TokenKind::Word) {
        add_var_assign(ctx, word);
        if ctx.consume(TokenKind::Equal) {
            parse_value(ctx)?;
        }
        skip_whitespace(ctx);
    }
    Ok(())
}

fn extract_builtin_unset(ctx: &mut Context) -> ParseResult<()> {
    skip_whitespace(ctx);
    let is_function = if parse_keyword(ctx, "-f") {
        skip_whitespace(ctx);
        true
    }
    else if parse_keyword(ctx, "-v") {
        skip_whitespace(ctx);
        false
    }
    else {
        false
    };
    while let Some(word) = ctx.lexer.next_if_kind(TokenKind::Word) {
        if is_function {
            unset_function(ctx, word);
        }
        else {
            add_var_write(ctx, word);
        }
        skip_whitespace(ctx);
    }
    Ok(())
}

fn set_function_location(ctx: &mut Context, sym_id: db::SymbolId, location: db::Location) {
    match ctx.info.symbols[sym_id].kind {
        db::SymbolKind::Function(var_id) => {
            let db::Function { definition, .. } = &mut ctx.info.functions[var_id];
            *definition = Some(location);
        }
        _ => unreachable!(),
    }
}

fn extract_function(ctx: &mut Context, word: Token) -> ParseResult<()> {
    if !is_identifier(word.view.string(ctx.document), ctx.info.shell) {
        ctx.warn(word.range, "Invalid function name");
    }

    let id = define_function(ctx, word);
    let old_locals = std::mem::replace(&mut ctx.locals, Some(HashMap::new()));

    let result = (|| {
        skip_whitespace(ctx);
        ctx.expect(TokenKind::ParenClose)?;
        skip_empty_lines(ctx);
        ctx.expect(TokenKind::BraceOpen)?;
        skip_empty_lines(ctx);
        extract_statements_until(ctx, kind_matches(&[TokenKind::BraceClose]));
        let last = ctx.expect(TokenKind::BraceClose)?;
        set_function_location(ctx, id, location(word, last));
        Ok(())
    })();

    ctx.locals = old_locals;
    result
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
            add_var_assign(ctx, word);
        }
        else {
            let word = ctx.expect(TokenKind::Word)?;
            skip_whitespace(ctx);
            extract_line_command(ctx, word, end)?;
        }
    }
    else {
        let command = lex::escape(word.view.string(ctx.document));
        if let Some(&id) = ctx.commands.get(command.as_ref()) {
            if matches!(ctx.info.symbols[id].kind, db::SymbolKind::Builtin) {
                ctx.info.tokens.data.push(lsp::SemanticToken {
                    position: word.range.start,
                    width: word.range.end.character - word.range.start.character,
                    kind: lsp::SemanticTokenKind::Keyword,
                    modifier: lsp::SemanticTokenModifier::None,
                });
                ctx.info.references.push(db::SymbolReference::read(word.range, id));
                match command.as_ref() {
                    "export" | "readonly" => extract_builtin_variable_declaration(ctx)?,
                    "unset" => extract_builtin_unset(ctx)?,
                    "local" => extract_builtin_local(ctx)?,
                    _ => extract_arguments_until(ctx, end),
                }
                return Ok(());
            }
        }
        add_cmd_ref(ctx, word);
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

fn parse_shebang(ctx: &mut Context) {
    if let Some(comment) = ctx.lexer.next_if_kind(TokenKind::Comment) {
        if let Some(shebang) = comment.view.string(ctx.document).strip_prefix("#!") {
            match shell::parse_shebang(shebang) {
                Ok(shell) => ctx.info.shell = shell,
                Err(error) => ctx.warn(comment.range, error),
            }
        }
        else {
            parse_comment(ctx, comment);
        }
    }
}

fn collect_references(info: &mut db::DocumentInfo) {
    info.references.sort_unstable_by_key(|symbol| symbol.reference.range.start);
    for (index, symbol) in info.references.iter().enumerate() {
        info.symbols[symbol.id].ref_indices.push(index as u32);
    }
}

// TODO: Share symbols between documents.
fn prepare_environment(ctx: &mut Context, settings: &Settings) {
    if settings.environment.variables {
        for name in env::variables() {
            let var = db::Variable::new(db::VariableKind::Environment);
            ctx.variables.insert(name.clone(), ctx.info.new_variable(name, var));
        }
    }
    if settings.environment.executables {
        if let Some(dirs) = (settings.environment.path.as_deref().map(Cow::Borrowed))
            .or_else(|| env::path_directories().map(Cow::Owned))
        {
            let mut names: Vec<String> =
                dirs.iter().flat_map(|dir| env::executable_names(dir)).collect();
            names.sort_unstable();
            names.dedup();
            for name in names {
                ctx.commands.insert(name.clone(), ctx.info.new_command(name));
            }
        }
    }
    for name in shell::builtins(ctx.info.shell).iter().copied().map(String::from) {
        let symbol = ctx.info.symbols.push(db::Symbol::new(name.clone(), db::SymbolKind::Builtin));
        ctx.commands.insert(name, symbol);
    }
}

pub fn parse(input: &str, settings: &Settings) -> db::DocumentInfo {
    let mut ctx = Context::new(input, settings.default_shell);
    parse_shebang(&mut ctx);
    prepare_environment(&mut ctx, settings);
    skip_empty_lines(&mut ctx);
    extract_statements_until(&mut ctx, |_| false);
    collect_references(&mut ctx.info);
    ctx.info
}

fn add_var_assign(ctx: &mut Context, word: Token) {
    let sym_id = add_var_write(ctx, word);
    match ctx.info.symbols[sym_id].kind {
        db::SymbolKind::Variable(var_id) => {
            let var = &mut ctx.info.variables[var_id];
            if var.first_assignment.is_none() {
                var.first_assignment = Some(db::Location { range: word.range, view: word.view });
                var.description = ctx.annotations.desc.take();
            }
        }
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use crate::config::Settings;

    fn diagnostics(input: &str) -> Vec<crate::lsp::Diagnostic> {
        super::parse(input, &Settings::default()).diagnostics
    }

    #[test]
    fn conditional() {
        assert!(diagnostics("if ls -la; then\n\tpwd\n\tuname -a\nfi\n").is_empty());
    }

    #[test]
    fn for_loop() {
        assert!(diagnostics("for x in a b c\ndo\n\techo $x\ndone\n").is_empty());
    }

    #[test]
    fn while_loop() {
        assert!(diagnostics("while true; do echo $x; done\n").is_empty());
    }

    #[test]
    fn assignment() {
        assert!(diagnostics("a=b c=d e f\n").is_empty());
    }

    #[test]
    fn dollar() {
        if let [diag] = diagnostics("echo $\n").as_slice() {
            assert!(diag.message.contains("literal"));
        }
        else {
            panic!();
        }
    }
}
