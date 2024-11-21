use crate::{db, lsp, rpc};
use serde_json::json;
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
    })
}

fn validate<T: serde::de::DeserializeOwned>(value: Json) -> Result<T, rpc::Error> {
    serde_json::from_value(value).map_err(|error| rpc::Error::invalid_params(error.to_string()))
}

fn handle_request(server: &mut Server, method: &str, _params: Json) -> Result<Json, rpc::Error> {
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
            let params: lsp::DidOpenDocumentParams = validate(params)?;
            let document = db::Document::new(params.document.text);
            server.db.documents.insert(params.document.uri.path, document);
            Ok(())
        }
        "textDocument/didClose" => {
            let params: lsp::DidCloseDocumentParams = validate(params)?;
            server.db.documents.remove(&params.document.uri.path);
            Ok(())
        }
        "textDocument/didChange" => {
            let params: lsp::DidChangeDocumentParams = validate(params)?;
            let document = server
                .db
                .documents
                .get_mut(&params.document.identifier.uri.path)
                .ok_or_else(|| rpc::Error::invalid_params("Can not edit unopened document"))?;
            for change in params.changes {
                document.edit(change.range, &change.text);
            }
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
