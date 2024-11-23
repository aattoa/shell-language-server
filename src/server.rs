use crate::{db, lsp, rpc};
use serde_json::{from_value, json};
use std::cmp::Ordering;
type Json = serde_json::Value;

#[derive(Default)]
pub struct Server {
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

fn search<'db>(
    mut references: impl Iterator<Item = &'db Vec<lsp::Reference>>,
    position: lsp::Position,
) -> Option<&'db [lsp::Reference]> {
    let comparator = |reference: &lsp::Reference| {
        if reference.range.contains(position) {
            Ordering::Equal
        }
        else if reference.range.start < position {
            Ordering::Less
        }
        else {
            Ordering::Greater
        }
    };
    references.find(|ranges| ranges.binary_search_by(comparator).is_ok()).map(Vec::as_slice)
}

fn find_references(info: &db::DocumentInfo, position: lsp::Position) -> &[lsp::Reference] {
    search(info.variables.values(), position)
        .or_else(|| search(info.functions.values(), position))
        .or_else(|| search(info.commands.values(), position))
        .unwrap_or(&[])
}

fn collect_references<T>(
    document: &db::Document,
    position: lsp::Position,
    projection: impl Fn(lsp::Reference) -> T,
) -> Vec<T> {
    find_references(&document.info, position).iter().copied().map(projection).collect()
}

fn find_definition(info: &db::DocumentInfo, position: lsp::Position) -> Option<lsp::Reference> {
    search(info.variables.values(), position)
        .and_then(|references| {
            references.iter().find(|reference| reference.kind == lsp::ReferenceKind::Write)
        })
        .or_else(|| {
            search(info.functions.values(), position).and_then(|references| references.first())
        })
        .copied()
}

fn handle_request(server: &mut Server, method: &str, params: Json) -> Result<Json, rpc::Error> {
    match method {
        "initialize" => {
            if std::mem::replace(&mut server.initialized, true) {
                eprintln!("Received initialize request when initialized");
            }
            Ok(json!({
                "capabilities": server_capabilities(),
                "serverInfo": { "name": "shell-language-server" },
            }))
        }
        "shutdown" => {
            if !std::mem::replace(&mut server.initialized, false) {
                eprintln!("Received uninitialize request when uninitialized");
            }
            Ok(Json::Null)
        }
        "textDocument/definition" => {
            let params: lsp::PositionParams = from_value(params)?;
            let document = get_document(&server.db, &params.document)?;
            let loc = |r: lsp::Reference| json!({ "uri": params.document.uri, "range": r.range });
            Ok(find_definition(&document.info, params.position).map(loc).unwrap_or_default())
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
            if find_references(&document.info, params.position).is_empty() {
                Ok(Json::Null)
            }
            else {
                Ok(json!({ "defaultBehavior": true }))
            }
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
        _ => Err(rpc::Error::new(
            rpc::ErrorCode::MethodNotFound,
            format!("Unhandled request: {method}"),
        )),
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
            let document = server
                .db
                .documents
                .get_mut(&params.document.identifier.uri.path)
                .ok_or_else(|| rpc::Error::invalid_params("Can not edit unopened document"))?;
            for change in params.changes {
                document.edit(change.range, &change.text);
            }
            document.analyze();
            Ok(())
        }
        _ => Err(rpc::Error::new(
            rpc::ErrorCode::MethodNotFound,
            format!("Unhandled notification: {method}"),
        )),
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

pub fn run(server: &mut Server) -> i32 {
    let mut stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    while server.exit_code.is_none() {
        match rpc::read_message(&mut stdin) {
            Ok(message) => {
                eprintln!("[debug] --> {}", message);
                if let Some(reply) = handle_message(server, &message) {
                    eprintln!("[debug] <-- {}", reply);
                    rpc::write_message(&mut stdout, &reply);
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
