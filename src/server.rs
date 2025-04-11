use crate::config::{self, Cmdline, Settings};
use crate::shell::Shell;
use crate::{db, env, external, lsp, parse, rpc};
use serde_json::{Value as Json, from_value, json};
use std::borrow::Cow;
use std::path::{Path, PathBuf};
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
                "tokenModifiers": ["documentation"],
            },
            "full": true,
        },
        "hoverProvider": true,
        "definitionProvider": true,
        "referencesProvider": true,
        "documentSymbolProvider": true,
        "documentHighlightProvider": true,
        "documentFormattingProvider": settings.integrate.shfmt.enable,
        "documentRangeFormattingProvider": settings.integrate.shfmt.enable,
        "codeActionProvider": true,
        "inlayHintProvider": { "resolveProvider": false },
        "renameProvider": { "prepareProvider": true },
        "completionProvider": { "triggerCharacters": ["$", "{"] },
    })
}

fn document_id(
    db: &db::Database,
    id: &lsp::DocumentIdentifier,
) -> Result<db::DocumentId, rpc::Error> {
    db.document_paths.get(&id.uri.path).copied().ok_or_else(|| {
        let path = id.uri.path.display();
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

fn symbol_references(
    info: &db::DocumentInfo,
    symbol: db::SymbolId,
) -> impl Iterator<Item = lsp::Reference> + '_ {
    info.symbols[symbol].ref_indices.iter().map(|&index| info.references[index as usize].reference)
}

fn find_references(
    info: &db::DocumentInfo,
    position: lsp::Position,
) -> impl Iterator<Item = lsp::Reference> + '_ {
    find_symbol(info, position).into_iter().flat_map(|symbol| symbol_references(info, symbol.id))
}

fn collect_references<T>(
    document: &db::Document,
    position: lsp::Position,
    projection: impl Fn(lsp::Reference) -> T,
) -> Vec<T> {
    find_references(&document.info, position).map(projection).collect()
}

fn is_path(name: &str) -> bool {
    name.contains(std::path::MAIN_SEPARATOR)
}

fn find_definition(
    info: &db::DocumentInfo,
    params: lsp::PositionParams,
    settings: &Settings,
) -> Option<lsp::Location> {
    let symbol = find_symbol(info, params.position)?;
    match info.symbols[symbol.id].kind {
        db::SymbolKind::Command => {
            let name = info.symbols[symbol.id].name.as_str();
            let path = if is_path(name) { name.into() } else { find_executable(name, settings)? };
            env::is_script(&path).then_some(lsp::Location::document(path))
        }
        db::SymbolKind::Parameter(parameter) => {
            let location = match parameter {
                db::Parameter::Function { id, index } => {
                    info.functions[id].parameters.get(index as usize - 1)?
                }
                db::Parameter::Script { index } => {
                    info.script_parameters.as_ref()?.get(index as usize - 1)?
                }
            };
            Some(lsp::Location { uri: params.document.uri, range: location.range })
        }
        db::SymbolKind::Function(_) | db::SymbolKind::Variable(_) => {
            symbol_references(info, symbol.id)
                .find(|reference| reference.kind == lsp::ReferenceKind::Write)
                .map(|reference| lsp::Location { uri: params.document.uri, range: reference.range })
        }
        db::SymbolKind::Error | db::SymbolKind::Builtin | db::SymbolKind::Special(_) => None,
    }
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
            matches!(symbol.kind, db::SymbolKind::Variable(_)) && symbol.name.starts_with(prefix)
        })
        .map(|symbol| completion(range, &symbol.name, lsp::CompletionItemKind::Variable))
        .collect()
}

fn function_completions(document: &db::Document, range: lsp::Range, prefix: &str) -> Json {
    (document.info.symbols.underlying.iter())
        .filter(|symbol| {
            matches!(
                symbol.kind,
                db::SymbolKind::Command | db::SymbolKind::Builtin | db::SymbolKind::Function(_)
            ) && symbol.name.starts_with(prefix)
        })
        .map(|symbol| completion(range, &symbol.name, lsp::CompletionItemKind::Function))
        .collect()
}

fn find_executable(name: &str, settings: &Settings) -> Option<PathBuf> {
    (settings.environment.path.as_deref().map(Cow::Borrowed))
        .or_else(|| env::path_directories().map(Cow::Owned))
        .and_then(|dirs| dirs.iter().find_map(|dir| env::find_executable(name, dir)))
}

fn manual(shell: Shell, name: &str, settings: &Settings) -> Option<String> {
    if settings.integrate.man.enable {
        let name = if is_path(name) { Path::new(name).file_name()?.to_str()? } else { name };
        external::man::documentation(shell, name, &settings.integrate.man)
    }
    else {
        None
    }
}

fn help(shell: Shell, name: &str, settings: &Settings) -> Option<String> {
    if settings.integrate.help.enable { external::help::documentation(shell, name) } else { None }
}

fn describe_variable(kind: db::VariableKind) -> &'static str {
    match kind {
        db::VariableKind::Global => "Variable",
        db::VariableKind::Local => "Local variable",
        db::VariableKind::Environment => "Environment variable",
    }
}

fn special_markdown(special: db::Special) -> String {
    let desc = |name, result| format!("# Special parameter `${name}`\n---\nExpands to {result}.");
    match special {
        db::Special::Zero => desc("0", "the name of the script or the shell"),
        db::Special::Question => desc("?", "the previous command's exit status"),
        db::Special::At => desc("@", "the current positional parameters"),
        db::Special::Star => desc("*", "the current positional parameters"),
        db::Special::Dash => desc("-", "the shell's current option flags"),
    }
}

fn param_description<'a>(text: &'a str, parameters: &[db::Location], index: u16) -> &'a str {
    match parameters.get(index as usize - 1) {
        Some(location) => location.view.string(text),
        None => "This parameter was not declared with a `##@ param` annotation.",
    }
}

fn symbol_markup(
    document: &db::Document,
    symbol: &db::Symbol,
    settings: &Settings,
) -> Result<lsp::MarkupContent, rpc::Error> {
    use std::fmt::Write;
    match symbol.kind {
        db::SymbolKind::Variable(id) => {
            let variable = &document.info.variables[id];
            let mut markdown = format!("# {} `{}`", describe_variable(variable.kind), symbol.name);
            if let Some(desc) = &variable.description {
                write!(markdown, "\n---\n{desc}")?;
            }
            if let Some(location) = variable.first_assignment {
                write!(
                    markdown,
                    "\n---\nFirst assignment on line {}:\n```sh\n{}\n```",
                    location.range.start.line + 1,
                    get_line(document, location.range.start.line)?.trim()
                )?;
            }
            else {
                write!(markdown, "\n---\nThis variable is not defined in this document.")?;
            }
            Ok(lsp::MarkupContent::markdown(markdown))
        }
        db::SymbolKind::Function(id) => {
            let db::Function { description, definition, parameters } = &document.info.functions[id];
            let mut markdown = format!("# Function `{}`", symbol.name);
            if let Some(desc) = description {
                write!(markdown, "\n---\n{desc}")?;
            }
            if !parameters.is_empty() {
                write!(markdown, "\n---\nParameters")?;
                for (index, param) in parameters.iter().copied().enumerate() {
                    write!(
                        markdown,
                        "\n- `${}`: {}",
                        index + 1,
                        param.view.string(&document.text).trim()
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
        db::SymbolKind::Parameter(db::Parameter::Function { id, index }) => {
            Ok(lsp::MarkupContent::markdown(format!(
                "# Function parameter `${index}`\n---\n{}",
                param_description(&document.text, &document.info.functions[id].parameters, index)
            )))
        }
        db::SymbolKind::Parameter(db::Parameter::Script { index }) => {
            let parameters = document.info.script_parameters.as_deref().unwrap_or(&[]);
            Ok(lsp::MarkupContent::markdown(format!(
                "# Script parameter `${index}`\n---\n{}",
                param_description(&document.text, parameters, index)
            )))
        }
        db::SymbolKind::Special(special) => {
            Ok(lsp::MarkupContent::markdown(special_markdown(special)))
        }
        db::SymbolKind::Error => Ok(lsp::MarkupContent::plaintext(String::from("Error"))),
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
    if settings.integrate.shellcheck.enable {
        match external::shellcheck::analyze(
            &document.text,
            document.info.shell,
            &settings.integrate.shellcheck,
        ) {
            Ok(external::shellcheck::Info { diagnostics, actions }) => {
                document.info.diagnostics.extend(diagnostics);
                document.info.actions.extend(actions);
            }
            Err(error) => eprintln!("[debug] Shellcheck failed: {error}"),
        }
    }
}

fn action_insert_path(
    params: &lsp::DocumentIdentifierRangeParams,
    document: &db::Document,
    settings: &Settings,
) -> Option<Json> {
    let db::SymbolReference { reference, id } = find_symbol(&document.info, params.range.start)?;
    let symbol = &document.info.symbols[id];
    if reference.kind == lsp::ReferenceKind::Write
        || matches!(symbol.kind, db::SymbolKind::Variable(_))
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

fn document_symbol(info: &db::DocumentInfo, symbol: &db::Symbol) -> Option<lsp::DocumentSymbol> {
    let sym = |kind, range| lsp::DocumentSymbol {
        name: symbol.name.clone(),
        kind,
        range,
        selection_range: range,
    };
    match symbol.kind {
        db::SymbolKind::Function(id) => {
            Some(sym(lsp::SymbolKind::Function, info.functions[id].definition?.range))
        }
        db::SymbolKind::Variable(id) => {
            Some(sym(lsp::SymbolKind::Variable, info.variables[id].first_assignment?.range))
        }
        _ => None,
    }
}

fn document_symbols(info: &db::DocumentInfo) -> Json {
    let mut symbols: Vec<lsp::DocumentSymbol> =
        info.symbols.underlying.iter().filter_map(|symbol| document_symbol(info, symbol)).collect();
    symbols.sort_by_key(|symbol| symbol.range.start.line);
    json!(symbols)
}

fn format(
    text: &str,
    range: lsp::Range,
    shell: Shell,
    config: &config::Shfmt,
    options: lsp::FormattingOptions,
) -> std::io::Result<Json> {
    if config.enable {
        if let Some(new_text) = external::shfmt::format(text, shell, config, options)? {
            return Ok(json!([lsp::TextEdit { range, new_text }]));
        }
    }
    Ok(json!([]))
}

fn initialize(server: &mut Server, params: lsp::InitializeParams) -> Json {
    if std::mem::replace(&mut server.initialized, true) {
        eprintln!("[debug] Received initialize request when initialized");
    }
    if let Some(settings) = params.settings {
        server.settings = settings;
    }
    if server.settings.integrate.shellcheck.enable && !external::exists("shellcheck") {
        server.settings.integrate.shellcheck.enable = false;
    }
    if server.settings.integrate.shfmt.enable && !external::exists("shfmt") {
        server.settings.integrate.shfmt.enable = false;
    }
    if server.settings.integrate.man.enable && !external::exists("man") {
        server.settings.integrate.man.enable = false;
    }
    json!({
        "capabilities": server_capabilities(&server.settings),
        "serverInfo": { "name": "shell-language-server" },
    })
}

fn parameter_hints(params: &[db::Location], range: lsp::Range) -> impl Iterator<Item = Json> + '_ {
    (params.iter().map(|location| location.range.start).enumerate())
        .filter(move |&(_, position)| range.contains(position))
        .map(|(index, position)| {
            json!({
                "position": position,
                "label": format!("${}:", index + 1),
                "kind": 2, // Parameter
                "paddingRight": true,
            })
        })
}

fn handle_request(server: &mut Server, method: &str, params: Json) -> Result<Json, rpc::Error> {
    match method {
        "initialize" => Ok(initialize(server, from_value(params)?)),
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
            let definition = find_definition(&document.info, params, &server.settings);
            Ok(definition.map_or(Json::Null, |location| json!(location)))
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
        "textDocument/inlayHint" => {
            let lsp::DocumentIdentifierRangeParams { document, range } = from_value(params)?;
            let document = get_document(&server.db, &document)?;
            Ok((document.info.functions.underlying.iter())
                .map(|function| function.parameters.as_slice())
                .chain(document.info.script_parameters.iter().map(Vec::as_slice))
                .flat_map(|params| parameter_hints(params, range))
                .collect())
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
            Ok(format(
                &document.text,
                lsp::Range::MAX,
                document.info.shell,
                &server.settings.integrate.shfmt,
                params.options,
            )?)
        }
        "textDocument/rangeFormatting" => {
            let params: lsp::RangeFormattingParams = from_value(params)?;
            let document = get_document(&server.db, &params.format.document)?;
            Ok(format(
                &document.text[db::text_range(&document.text, params.range)],
                params.range,
                document.info.shell,
                &server.settings.integrate.shfmt,
                params.format.options,
            )?)
        }
        "textDocument/codeAction" => {
            let params: lsp::DocumentIdentifierRangeParams = from_value(params)?;
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
        "textDocument/documentSymbol" => {
            let params: lsp::DocumentIdentifierParams = from_value(params)?;
            let document = get_document(&server.db, &params.document)?;
            Ok(document_symbols(&document.info))
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
