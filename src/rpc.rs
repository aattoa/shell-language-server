use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct JsonRpc;

#[derive(Deserialize)]
pub struct Request {
    #[serde(default)]
    pub params: serde_json::Value,
    pub method: String,
    pub id: Option<u32>,
    #[allow(dead_code)]
    pub jsonrpc: JsonRpc,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ResponseKind {
    Result(serde_json::Value),
    Error(Error),
}

#[derive(Serialize)]
pub struct Response {
    pub id: Option<u32>,
    #[serde(flatten)]
    pub kind: ResponseKind,
    pub jsonrpc: JsonRpc,
}

#[derive(Serialize)]
pub struct Error {
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Clone, Copy)]
pub enum ErrorCode {
    ParseError = -32700,
    InvalidRequest = -32600,
    MethodNotFound = -32601,
    InvalidParams = -32602,
    InternalError = -32603,
    RequestFailed = -32803,
}

impl Serialize for ErrorCode {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_i32(*self as i32)
    }
}

impl JsonRpc {
    pub const VERSION: &str = "2.0";
}

struct JsonRpcVisitor;

impl serde::de::Visitor<'_> for JsonRpcVisitor {
    type Value = JsonRpc;
    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str(JsonRpc::VERSION)
    }
    fn visit_str<E: serde::de::Error>(self, str: &str) -> Result<JsonRpc, E> {
        if str == JsonRpc::VERSION { Ok(JsonRpc) } else { Err(E::custom("bad jsonrpc version")) }
    }
}

impl<'de> Deserialize<'de> for JsonRpc {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<JsonRpc, D::Error> {
        d.deserialize_str(JsonRpcVisitor)
    }
}

impl Serialize for JsonRpc {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(JsonRpc::VERSION)
    }
}

fn consume(input: &mut impl Read, bytes: usize) -> bool {
    input.bytes().take(bytes).count() == bytes
}

pub fn write_message(output: &mut impl Write, content: &str) -> io::Result<()> {
    write!(output, "Content-Length: {}\r\n\r\n{}", content.len(), content)?;
    output.flush()
}

pub fn read_message(input: &mut impl Read) -> io::Result<String> {
    let error = |msg| Err(io::Error::new(io::ErrorKind::InvalidInput, msg));

    if !consume(input, "Content-Length: ".len()) {
        return error("Missing Content-Length header.");
    }

    let mut length: usize = 0;
    for byte in input.bytes() {
        let byte = byte?;
        if byte.is_ascii_digit() {
            length *= 10;
            length += (byte - b'0') as usize;
        }
        else if byte == b'\r' {
            break;
        }
        else {
            return error("Unexpected byte.");
        }
    }

    if length == 0 {
        return error("Missing content length.");
    }
    if !consume(input, "\n\r\n".len()) {
        return error("Missing content separator.");
    }

    let mut content = vec![0u8; length];
    input.read_exact(&mut content)?;
    Ok(String::from_utf8_lossy(&content).into_owned())
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Error {
        Error::invalid_params(error.to_string())
    }
}

impl From<std::fmt::Error> for Error {
    fn from(error: std::fmt::Error) -> Self {
        Self::request_failed(format!("Formatting failed: {error}"))
    }
}

impl Error {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Error {
        Error { code, message: message.into() }
    }
    pub fn invalid_params(message: impl Into<String>) -> Error {
        Error::new(ErrorCode::InvalidParams, message)
    }
    pub fn internal_error(message: impl Into<String>) -> Error {
        Error::new(ErrorCode::InternalError, message)
    }
    pub fn request_failed(message: impl Into<String>) -> Error {
        Error::new(ErrorCode::RequestFailed, message)
    }
    pub fn method_not_found(method: &str) -> Error {
        Error::new(ErrorCode::MethodNotFound, format!("Unhandled method: {method}"))
    }
}

impl Response {
    pub fn success(id: Option<u32>, result: serde_json::Value) -> Response {
        Response { id, kind: ResponseKind::Result(result), jsonrpc: JsonRpc }
    }
    pub fn error(id: Option<u32>, error: Error) -> Response {
        Response { id, kind: ResponseKind::Error(error), jsonrpc: JsonRpc }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn read_message() {
        let mut input = "Content-Length: 5\r\n\r\nhelloContent-Length: 6\r\n\r\nworld!".as_bytes();
        assert_eq!(super::read_message(&mut input).unwrap(), "hello");
        assert_eq!(super::read_message(&mut input).unwrap(), "world!");
        assert!(super::read_message(&mut input).is_err());
    }
    #[test]
    fn write_message() {
        let mut output = Vec::new();
        assert!(super::write_message(&mut output, "hello").is_ok());
        assert!(super::write_message(&mut output, "world!").is_ok());
        assert_eq!(output, b"Content-Length: 5\r\n\r\nhelloContent-Length: 6\r\n\r\nworld!");
    }
}
