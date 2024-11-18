use crate::{db, rpc};
type Json = serde_json::Value;

#[derive(Default)]
pub struct Server {
    pub db: db::Database,
    pub initialized: bool,
    pub exit_code: Option<i32>,
}

fn handle_request(server: &mut Server, method: &str) -> Json {
    match method {
        "initialize" => {
            if std::mem::replace(&mut server.initialized, true) {
                eprintln!("Received initialize request when already initialized");
            }
            serde_json::json!({
                "capabilities": {
                    "hoverProvider": true,
                },
            })
        }
        "shutdown" => {
            if !std::mem::replace(&mut server.initialized, false) {
                eprintln!("Received uninitialize request when already uninitialized");
            }
            Json::Null
        }
        "textDocument/hover" => serde_json::json!({
            "contents": {
                "kind": "markdown",
                "value": "testing `testing`",
            },
        }),
        _ => panic!("unhandled method: {method}"),
    }
}

fn handle_notification(server: &mut Server, method: &str) {
    match method {
        "initialized" => {}
        "exit" => {
            server.exit_code = Some(if server.initialized { 1 } else { 0 });
        }
        _ => eprintln!("unhandled notification: {method}"),
    }
}

fn dispatch_handle_message(server: &mut Server, message: rpc::Request) -> Option<rpc::Response> {
    if message.id.is_some() {
        let result = handle_request(server, &message.method);
        Some(rpc::Response {
            id: message.id,
            result: Some(result),
            error: None,
            jsonrpc: rpc::JsonRpc,
        })
    }
    else {
        handle_notification(server, &message.method);
        None
    }
}

fn handle_message(server: &mut Server, message: &str) -> Option<String> {
    match serde_json::from_str::<rpc::Request>(message) {
        Ok(message) => {
            let reply = dispatch_handle_message(server, message);
            reply.map(|reply| serde_json::to_string(&reply).expect("reply serialization failed"))
        }
        Err(_) => todo!(),
    }
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
                eprintln!("[debug] Unable to read message: {}", error);
                return -1;
            }
        }
    }
    server.exit_code.unwrap()
}
