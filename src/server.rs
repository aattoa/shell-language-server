use crate::config::{Cmdline, Settings};
use crate::shell::Shell;
use crate::{db, env, external, lsp, parse, rpc};
use serde_json::{Value as Json, from_value, json};
use std::borrow::Cow;
use std::process::ExitCode;

#[derive(Default)]
struct Server {
    db: db::Database,
    settings: Settings,
    initialized: bool,
    exit_code: Option<ExitCode>,
}

fn server_capabilities(settings: &Settings) -> Json {
    json!({
        "textDocumentSync": {
            "openClose": true,
            "change": 2, // incremental
        },
        "diagnosticProvider": {
            "interFileDependencies": false,
            "workspaceDiagnostics": false,
        },
        "semanticTokensProvider": {
            "legend": {
                "tokenTypes": ["keyword", "parameter", "string"],
                "tokenModifiers": [],
            },
            "full": true,
        },
        "hoverProvider": true,
        "definitionProvider": true,
        "referencesProvider": true,
        "documentHighlightProvider": true,
        "documentFormattingProvider": settings.integrate.shfmt,
        "documentRangeFormattingProvider": settings.integrate.shfmt,
        "codeActionProvider": true,
        "renameProvider": { "prepareProvider": true },
        "completionProvider": {},
    })
}

fn document_id(
    db: &db::Database,
    identifier: &lsp::DocumentIdentifier,
) -> Result<db::DocumentId, rpc::Error> {
    db.document_paths.get(&identifier.uri.path).copied().ok_or_else(|| {
        let path = identifier.uri.path.display();
        rpc::Error::invalid_params(format!("Unopened document referenced: '{path}'",))
    })
}

fn get_document<'a>(
    db: &'a db::Database,
    id: &lsp::DocumentIdentifier,
) -> Result<&'a db::Document, rpc::Error> {
    document_id(db, id).map(|id| &db.documents[id])
}

fn compare_position(position: lsp::Position, range: lsp::Range) -> std::cmp::Ordering {
    if range.contains(position) { std::cmp::Ordering::Equal } else { range.start.cmp(&position) }
}

fn find_symbol(info: &db::DocumentInfo, position: lsp::Position) -> Option<db::SymbolReference> {
    info.references
        .binary_search_by(|symbol| compare_position(position, symbol.reference.range))
        .ok()
        .map(|index| info.references[index])
}

fn find_references(
    info: &db::DocumentInfo,
    position: lsp::Position,
) -> impl Iterator<Item = lsp::Reference> + '_ {
    find_symbol(info, position)
        .into_iter()
        .flat_map(|symbol| info.symbols[symbol.id].ref_indices.iter())
        .map(|&index| info.references[index as usize].reference)
}

fn collect_references<T>(
    document: &db::Document,
    position: lsp::Position,
    projection: impl Fn(lsp::Reference) -> T,
) -> Vec<T> {
    find_references(&document.info, position).map(projection).collect()
}

fn find_definition(info: &db::DocumentInfo, position: lsp::Position) -> Option<lsp::Reference> {
    find_references(info, position).find(|reference| reference.kind == lsp::ReferenceKind::Write)
}

fn get_line(document: &db::Document, line: u32) -> Result<&str, rpc::Error> {
    let error = || rpc::Error::invalid_params(format!("Line {line} out of range"));
    document.text.lines().nth(line as usize).ok_or_else(error)
}

fn is_word(char: char) -> bool {
    char.is_alphanumeric() || "_-".contains(char)
}

fn determine_completion_kind(
    prefix: &str,
    cursor: lsp::Position,
) -> (usize, lsp::CompletionItemKind) {
    for (index, char) in prefix.chars().rev().enumerate() {
        let offset = cursor.character as usize - index;
        if "${".contains(char) {
            return (offset, lsp::CompletionItemKind::Variable);
        }
        else if !is_word(char) {
            return (offset, lsp::CompletionItemKind::Function);
        }
    }
    (0, lsp::CompletionItemKind::Function)
}

fn completion(range: lsp::Range, name: &str, kind: lsp::CompletionItemKind) -> Json {
    json!({
        "label": name,
        "kind": kind,
        "textEdit": { "range": range, "newText": name },
    })
}

fn variable_completions(document: &db::Document, range: lsp::Range, prefix: &str) -> Json {
    (document.info.symbols.underlying.iter())
        .filter(|symbol| {
            matches!(symbol.kind, db::SymbolKind::Variable { .. })
                && symbol.name.starts_with(prefix)
        })
        .map(|symbol| completion(range, &symbol.name, lsp::CompletionItemKind::Variable))
        .collect()
}

fn function_completions(document: &db::Document, range: lsp::Range, prefix: &str) -> Json {
    (document.info.symbols.underlying.iter())
        .filter(|symbol| {
            !matches!(symbol.kind, db::SymbolKind::Variable { .. })
                && symbol.name.starts_with(prefix)
        })
        .map(|symbol| completion(range, &symbol.name, lsp::CompletionItemKind::Function))
        .collect()
}

fn find_executable(name: &str, settings: &Settings) -> Option<std::path::PathBuf> {
    (settings.environment.path.as_deref().map(Cow::Borrowed))
        .or_else(|| env::path_variable().map(Cow::Owned))
        .and_then(|path| env::find_executable(name, &path))
}

fn manual(shell: Shell, name: &str, settings: &Settings) -> Option<String> {
    if settings.integrate.man && !name.contains('/') && !name.contains('\\') {
        external::man::documentation(shell, name)
    }
    else {
        None
    }
}

fn help(shell: Shell, name: &str, settings: &Settings) -> Option<String> {
    if settings.integrate.help { external::help::documentation(shell, name) } else { None }
}

fn symbol_markup(
    document: &db::Document,
    symbol: &db::Symbol,
    settings: &Settings,
) -> Result<lsp::MarkupContent, rpc::Error> {
    use std::fmt::Write;
    match &symbol.kind {
        db::SymbolKind::Variable { description, first_assign_line } => {
            let mut markdown = format!("# Variable `{}`", symbol.name);
            if let Some(desc) = description {
                write!(markdown, "\n{desc}")?;
            }
            if let &Some(line) = first_assign_line {
                write!(
                    markdown,
                    "\n---\nFirst assignment on line {}:\n```sh\n{}\n```",
                    line + 1,
                    get_line(document, line)?.trim()
                )?;
            }
            Ok(lsp::MarkupContent::markdown(markdown))
        }
        db::SymbolKind::Function { description, definition, parameters } => {
            let mut markdown = format!("# Function `{}`", symbol.name);
            if let Some(desc) = description {
                write!(markdown, "\n{desc}")?;
            }
            if !parameters.is_empty() {
                write!(markdown, "\n---\n## Parameters")?;
                for (index, param) in parameters.iter().enumerate() {
                    write!(
                        markdown,
                        "\n- `${}`: {}",
                        index + 1,
                        param.string(&document.text).trim()
                    )?;
                }
            }
            if let Some(db::Location { range, view }) = definition {
                write!(
                    markdown,
                    "\n---\nDefined on line {}:\n```sh\n{}\n```",
                    range.start.line + 1,
                    view.string(&document.text)
                )?;
            }
            Ok(lsp::MarkupContent::markdown(markdown))
        }
        db::SymbolKind::Command => {
            let mut markdown = format!("# Command `{}`", symbol.name);
            if let Some(path) = find_executable(&symbol.name, settings) {
                write!(markdown, "\n---\nPath: `{}`", path.display())?;
            }
            if let Some(manual) = manual(document.info.shell, &symbol.name, settings) {
                write!(markdown, "\n---\n```man\n{manual}\n```")?;
            }
            Ok(lsp::MarkupContent::markdown(markdown))
        }
        db::SymbolKind::Builtin => {
            let mut markdown = format!("# Shell builtin `{}`", symbol.name);
            if let Some(help) = help(document.info.shell, &symbol.name, settings) {
                write!(markdown, "\n---\n```\n{help}\n```")?;
            }
            Ok(lsp::MarkupContent::markdown(markdown))
        }
    }
}

fn symbol_hover(
    document: &db::Document,
    symbol: db::SymbolReference,
    settings: &Settings,
) -> Result<Json, rpc::Error> {
    Ok(json!({
        "contents": symbol_markup(document, &document.info.symbols[symbol.id], settings)?,
        "range": symbol.reference.range,
    }))
}

fn analyze(document: &mut db::Document, settings: &Settings) {
    document.info = parse::parse(&document.text, settings);
    if settings.integrate.shellcheck {
        match external::shellcheck::analyze(document.info.shell, &document.text) {
            Ok(external::shellcheck::Info { diagnostics, actions }) => {
                document.info.diagnostics.extend(diagnostics);
                document.info.actions.extend(actions);
            }
            Err(error) => eprintln!("[debug] Shellcheck failed: {error}"),
        }
    }
}

fn format(
    shell: Shell,
    options: lsp::FormattingOptions,
    document_text: &str,
) -> Result<String, rpc::Error> {
    external::shfmt::format(shell, options, document_text)
        .map_err(|error| rpc::Error::request_failed(error.to_string()))
}

fn action_insert_path(
    params: &lsp::CodeActionParams,
    document: &db::Document,
    settings: &Settings,
) -> Option<Json> {
    let db::SymbolReference { reference, id } = find_symbol(&document.info, params.range.start)?;
    let symbol = &document.info.symbols[id];
    if reference.kind == lsp::ReferenceKind::Write
        || matches!(symbol.kind, db::SymbolKind::Variable { .. })
    {
        return None;
    }
    let path = find_executable(&symbol.name, settings)?;
    let edit = lsp::TextEdit { range: reference.range, new_text: String::from(path.to_str()?) };
    Some(json!({
        "title": "Insert full command path",
        "edit": { "changes": { params.document.uri.to_string(): [edit] } }
    }))
}

fn handle_request(server: &mut Server, method: &str, params: Json) -> Result<Json, rpc::Error> {
    match method {
        "initialize" => {
            if std::mem::replace(&mut server.initialized, true) {
                eprintln!("[debug] Received initialize request when initialized");
            }
            if let lsp::InitializeParams { settings: Some(settings) } = from_value(params)? {
                server.settings = settings;
            }
            Ok(json!({
                "capabilities": server_capabilities(&server.settings),
                "serverInfo": { "name": "shell-language-server" },
            }))
        }
        "shutdown" => {
            if !std::mem::replace(&mut server.initialized, false) {
                eprintln!("[debug] Received uninitialize request when uninitialized");
            }
            Ok(Json::Null)
        }
        "textDocument/hover" => {
            let params: lsp::PositionParams = from_value(params)?;
            let document = &server.db.documents[document_id(&server.db, &params.document)?];
            find_symbol(&document.info, params.position)
                .map_or(Ok(Json::Null), |symbol| symbol_hover(document, symbol, &server.settings))
        }
        "textDocument/definition" => {
            let params: lsp::PositionParams = from_value(params)?;
            let document = get_document(&server.db, &params.document)?;
            let loc = |r: lsp::Reference| json!({ "uri": params.document.uri, "range": r.range });
            Ok(find_definition(&document.info, params.position).map_or(Json::Null, loc))
        }
        "textDocument/references" => {
            let params: lsp::PositionParams = from_value(params)?;
            let document = get_document(&server.db, &params.document)?;
            let loc = |r: lsp::Reference| json!({ "uri": params.document.uri, "range": r.range });
            Ok(Json::Array(collect_references(document, params.position, loc)))
        }
        "textDocument/documentHighlight" => {
            let params: lsp::PositionParams = from_value(params)?;
            let document = get_document(&server.db, &params.document)?;
            Ok(Json::Array(collect_references(document, params.position, |r| json!(r))))
        }
        "textDocument/prepareRename" => {
            let params: lsp::PositionParams = from_value(params)?;
            let document = get_document(&server.db, &params.document)?;
            let mut references = find_references(&document.info, params.position);
            Ok(references.next().map_or(Json::Null, |reference| json!(reference.range)))
        }
        "textDocument/rename" => {
            let params: lsp::RenameParams = from_value(params)?;
            let document = get_document(&server.db, &params.position_params.document)?;
            let edit = |r: lsp::Reference| json!({ "range": r.range, "newText": params.new_name });
            let edits = collect_references(document, params.position_params.position, edit);
            Ok(json!({ "changes": { params.position_params.document.uri.to_string(): edits } }))
        }
        "textDocument/diagnostic" => {
            let params: lsp::DocumentIdentifierParams = from_value(params)?;
            let document = get_document(&server.db, &params.document)?;
            Ok(json!({ "kind": "full", "items": document.info.diagnostics }))
        }
        "textDocument/completion" => {
            let params: lsp::PositionParams = from_value(params)?;
            let document = get_document(&server.db, &params.document)?;
            let line = get_line(document, params.position.line)?;
            let line_prefix = &line[..params.position.character as usize];
            let (offset, kind) = determine_completion_kind(line_prefix, params.position);
            let prefix = &line_prefix[offset..];
            let start = lsp::Position { line: params.position.line, character: offset as u32 };
            let range = lsp::Range { start, end: params.position };

            match kind {
                lsp::CompletionItemKind::Variable => {
                    Ok(variable_completions(document, range, prefix))
                }
                lsp::CompletionItemKind::Function => {
                    Ok(function_completions(document, range, prefix))
                }
                _ => Err(rpc::Error::internal_error("completion failure")),
            }
        }
        "textDocument/formatting" => {
            let params: lsp::FormattingParams = from_value(params)?;
            let document = get_document(&server.db, &params.document)?;
            let new_text = format(document.info.shell, params.options, &document.text)?;
            Ok(json!([lsp::TextEdit { range: lsp::Range::MAX, new_text }]))
        }
        "textDocument/rangeFormatting" => {
            let params: lsp::RangeFormattingParams = from_value(params)?;
            let document = get_document(&server.db, &params.format.document)?;
            let text = &document.text[db::text_range(&document.text, params.range)];
            let new_text = format(document.info.shell, params.format.options, text)?;
            Ok(json!([lsp::TextEdit { range: params.range, new_text }]))
        }
        "textDocument/codeAction" => {
            let params: lsp::CodeActionParams = from_value(params)?;
            let document = get_document(&server.db, &params.document)?;
            Ok((document.info.actions.iter())
                .filter(|action| {
                    action.range.contained_by(params.range)
                        || params.range.contained_by(action.range)
                })
                .map(|action| {
                    json!({
                        "title": action.title,
                        "edit": { "changes": { params.document.uri.to_string(): action.edits } }
                    })
                })
                .chain(action_insert_path(&params, document, &server.settings))
                .collect())
        }
        "textDocument/semanticTokens/full" => {
            let params: lsp::DocumentIdentifierParams = from_value(params)?;
            let document = get_document(&server.db, &params.document)?;
            Ok(json!({ "data": document.info.tokens }))
        }
        _ => Err(rpc::Error::method_not_found(method)),
    }
}

fn handle_notification(server: &mut Server, method: &str, params: Json) -> Result<(), rpc::Error> {
    match method {
        "initialized" => Ok(()),
        "exit" => {
            server.exit_code = Some(ExitCode::from(server.initialized as u8));
            Ok(())
        }
        "textDocument/didOpen" => {
            let params: lsp::DidOpenDocumentParams = from_value(params)?;
            let mut document = db::Document::new(params.document.text);
            analyze(&mut document, &server.settings);
            server.db.open(params.document.uri.path, document);
            Ok(())
        }
        "textDocument/didClose" => {
            let params: lsp::DocumentIdentifierParams = from_value(params)?;
            server.db.close(&params.document.uri.path);
            Ok(())
        }
        "textDocument/didChange" => {
            let params: lsp::DidChangeDocumentParams = from_value(params)?;
            let id = document_id(&server.db, &params.document.identifier)?;
            let document = &mut server.db.documents[id];
            for change in params.changes {
                document.edit(change.range, &change.text);
            }
            analyze(document, &server.settings);
            Ok(())
        }
        "workspace/didChangeConfiguration" => {
            let params: lsp::DidChangeConfigurationParams = from_value(params)?;
            server.settings = params.settings.shell;
            Ok(())
        }
        _ => {
            if method.starts_with("$/") {
                Ok(()) // Implementation dependent notifications may be ignored.
            }
            else {
                Err(rpc::Error::method_not_found(method))
            }
        }
    }
}

fn dispatch_handle_request(server: &mut Server, message: rpc::Request) -> Option<rpc::Response> {
    if message.id.is_some() {
        Some(match handle_request(server, &message.method, message.params) {
            Ok(result) => rpc::Response::success(message.id, result),
            Err(error) => rpc::Response::error(message.id, error),
        })
    }
    else {
        handle_notification(server, &message.method, message.params)
            .err()
            .map(|error| rpc::Response::error(None, error))
    }
}

fn deserialization_error(error: serde_json::Error) -> rpc::Response {
    use rpc::ErrorCode::*;
    let code = if error.is_data() { InvalidParams } else { ParseError };
    rpc::Response::error(None, rpc::Error::new(code, error.to_string()))
}

fn handle_message(server: &mut Server, message: &str) -> Option<String> {
    let reply = match serde_json::from_str(message) {
        Ok(request) => dispatch_handle_request(server, request),
        Err(error) => Some(deserialization_error(error)),
    };
    reply.map(|reply| serde_json::to_string(&reply).expect("Reply serialization failed"))
}

pub fn run(cmdline: Cmdline) -> ExitCode {
    let mut server = Server { settings: cmdline.settings, ..Server::default() };
    let mut stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();

    loop {
        if let Some(code) = server.exit_code {
            return code;
        }
        match rpc::read_message(&mut stdin) {
            Ok(message) => {
                if cmdline.debug {
                    eprintln!("[debug] --> {}", message);
                }
                if let Some(reply) = handle_message(&mut server, &message) {
                    if cmdline.debug {
                        eprintln!("[debug] <-- {}", reply);
                    }
                    if let Err(error) = rpc::write_message(&mut stdout, &reply) {
                        eprintln!("[debug] Unable to write reply: {error}");
                        return ExitCode::from(2);
                    }
                }
            }
            Err(error) => {
                eprintln!("[debug] Unable to read message: {error}");
                return ExitCode::from(2);
            }
        }
    }
}
