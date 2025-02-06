use crate::{config, db, env, lsp, rpc};
use serde_json::{from_value, json};
use std::cmp::Ordering;
type Json = serde_json::Value;

#[derive(Default)]
struct Server {
    pub db: db::Database,
    pub initialized: bool,
    pub exit_code: Option<i32>,
}

fn server_capabilities() -> Json {
    json!({
        "textDocumentSync": {
            "openClose": true,
            "change": 2, // incremental
        },
        "diagnosticProvider": {
            "interFileDependencies": false,
            "workspaceDiagnostics": false,
        },
        "definitionProvider": true,
        "referencesProvider": true,
        "documentHighlightProvider": true,
        "renameProvider": { "prepareProvider": true },
        "completionProvider": {},
    })
}

fn get_document<'db>(
    db: &'db db::Database,
    identifier: &lsp::DocumentIdentifier,
) -> Result<&'db db::Document, rpc::Error> {
    db.documents.get(&identifier.uri.path).ok_or_else(|| {
        rpc::Error::invalid_params(format!(
            "Unopened document referenced: {}",
            identifier.uri.path.display()
        ))
    })
}

fn compare_position(position: lsp::Position, range: lsp::Range) -> Ordering {
    if range.contains(position) {
        Ordering::Equal
    }
    else if range.start < position {
        Ordering::Less
    }
    else {
        Ordering::Greater
    }
}

fn find_references(
    info: &db::DocumentInfo,
    position: lsp::Position,
) -> impl Iterator<Item = lsp::Reference> + '_ {
    info.references
        .binary_search_by(|symbol| compare_position(position, symbol.reference.range))
        .into_iter()
        .flat_map(|index| info.symbols[info.references[index].id].ref_indices.iter())
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
    char.is_alphanumeric() || char == '_' || char == '-'
}

fn determine_completion_kind(
    prefix: &str,
    cursor: lsp::Position,
) -> (usize, lsp::CompletionItemKind) {
    for (index, char) in prefix.chars().rev().enumerate() {
        if "${".contains(char) {
            return (cursor.character as usize - index, lsp::CompletionItemKind::Variable);
        }
        else if !is_word(char) {
            return (cursor.character as usize - index, lsp::CompletionItemKind::Function);
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

fn variable_completions(
    server: &Server,
    document: &db::Document,
    range: lsp::Range,
    prefix: &str,
) -> Json {
    (document.info.symbols.underlying.iter())
        .filter(|symbol| symbol.kind == db::SymbolKind::Variable && symbol.name.starts_with(prefix))
        .map(|symbol| completion(range, &symbol.name, lsp::CompletionItemKind::Variable))
        .chain(
            (server.db.environment_variables.iter())
                .filter(|name| name.starts_with(prefix))
                .map(|name| completion(range, name, lsp::CompletionItemKind::Variable)),
        )
        .collect()
}

fn function_completions(
    server: &Server,
    document: &db::Document,
    range: lsp::Range,
    prefix: &str,
) -> Json {
    (document.info.symbols.underlying.iter())
        .filter(|symbol| symbol.kind == db::SymbolKind::Command && symbol.name.starts_with(prefix))
        .map(|symbol| completion(range, &symbol.name, lsp::CompletionItemKind::Function))
        .chain(
            (server.db.path_executables.iter())
                .filter(|name| name.starts_with(prefix))
                .map(|name| completion(range, name, lsp::CompletionItemKind::Function)),
        )
        .collect()
}

fn handle_request(server: &mut Server, method: &str, params: Json) -> Result<Json, rpc::Error> {
    match method {
        "initialize" => {
            if std::mem::replace(&mut server.initialized, true) {
                eprintln!("[debug] Received initialize request when initialized");
            }
            Ok(json!({
                "capabilities": server_capabilities(),
                "serverInfo": { "name": "shell-language-server" },
            }))
        }
        "shutdown" => {
            if !std::mem::replace(&mut server.initialized, false) {
                eprintln!("[debug] Received uninitialize request when uninitialized");
            }
            Ok(Json::Null)
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
            let params: lsp::PullDiagnosticParams = from_value(params)?;
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
                    Ok(variable_completions(server, document, range, prefix))
                }
                lsp::CompletionItemKind::Function => {
                    Ok(function_completions(server, document, range, prefix))
                }
                _ => Err(rpc::Error::new(rpc::ErrorCode::InternalError, "completion failure")),
            }
        }
        _ => Err(rpc::Error::method_not_found(method)),
    }
}

fn handle_notification(server: &mut Server, method: &str, params: Json) -> Result<(), rpc::Error> {
    match method {
        "initialized" => Ok(()),
        "exit" => {
            server.exit_code = Some(if server.initialized { 1 } else { 0 });
            Ok(())
        }
        "textDocument/didOpen" => {
            let params: lsp::DidOpenDocumentParams = from_value(params)?;
            let mut document = db::Document::new(params.document.text);
            document.analyze();
            server.db.documents.insert(params.document.uri.path, document);
            Ok(())
        }
        "textDocument/didClose" => {
            let params: lsp::DidCloseDocumentParams = from_value(params)?;
            server.db.documents.remove(&params.document.uri.path);
            Ok(())
        }
        "textDocument/didChange" => {
            let params: lsp::DidChangeDocumentParams = from_value(params)?;
            let document = (server.db.documents)
                .get_mut(&params.document.identifier.uri.path)
                .ok_or_else(|| rpc::Error::invalid_params("Can not edit unopened document"))?;
            for change in params.changes {
                document.edit(change.range, &change.text);
            }
            document.analyze();
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
    let code =
        if error.is_data() { rpc::ErrorCode::InvalidParams } else { rpc::ErrorCode::ParseError };
    rpc::Response::error(None, rpc::Error::new(code, error.to_string()))
}

fn handle_message(server: &mut Server, message: &str) -> Option<String> {
    let reply = match serde_json::from_str::<rpc::Request>(message) {
        Ok(request) => dispatch_handle_request(server, request),
        Err(error) => Some(deserialization_error(error)),
    };
    reply.map(|reply| serde_json::to_string(&reply).expect("Reply serialization failed"))
}

pub fn run(config: config::Config) -> i32 {
    let mut server = Server::default();
    if config.complete.env_path {
        server.db.path_executables = (config.path.as_deref())
            .map_or_else(env::collect_path_executables, env::collect_executables);
    }
    if config.complete.env_vars {
        server.db.environment_variables = env::collect_variables();
    }
    let mut stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    while server.exit_code.is_none() {
        match rpc::read_message(&mut stdin) {
            Ok(message) => {
                if config.debug {
                    eprintln!("[debug] --> {}", message);
                }
                if let Some(reply) = handle_message(&mut server, &message) {
                    if config.debug {
                        eprintln!("[debug] <-- {}", reply);
                    }
                    if let Err(error) = rpc::write_message(&mut stdout, &reply) {
                        eprintln!("[debug] Unable to write reply: {error}");
                        return -1;
                    }
                }
            }
            Err(error) => {
                eprintln!("[debug] Unable to read message: {error}");
                return -1;
            }
        }
    }
    server.exit_code.unwrap()
}
