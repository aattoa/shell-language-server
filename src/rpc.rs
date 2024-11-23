use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::io::{Bytes, Read, Write};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct JsonRpc;

#[derive(Debug, Deserialize)]
pub struct Request {
    #[serde(default)]
    pub params: serde_json::Value,
    pub method: String,
    pub jsonrpc: JsonRpc,
    pub id: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct Error {
    pub code: i32,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct Response {
    pub id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Error>,
    pub jsonrpc: JsonRpc,
}

#[repr(i32)]
#[derive(Clone, Copy, Debug, Serialize_repr, Deserialize_repr)]
pub enum ErrorCode {
    ParseError = -32700,
    InvalidRequest = -32600,
    MethodNotFound = -32601,
    InvalidParams = -32602,
    InternalError = -32603,
}

impl JsonRpc {
    pub const VERSION: &str = "2.0";
}

impl<'de> serde::de::Visitor<'de> for JsonRpc {
    type Value = JsonRpc;
    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str(JsonRpc::VERSION)
    }
    fn visit_str<E: serde::de::Error>(self, str: &str) -> Result<JsonRpc, E> {
        if str == JsonRpc::VERSION { Ok(JsonRpc) } else { Err(E::custom("bad jsonrpc version")) }
    }
}

impl Serialize for JsonRpc {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(JsonRpc::VERSION)
    }
}

impl<'de> Deserialize<'de> for JsonRpc {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<JsonRpc, D::Error> {
        d.deserialize_str(JsonRpc)
    }
}

fn consume(mut read: Bytes<impl Read>, slice: &[u8]) -> bool {
    slice.iter().all(|&char| read.next().is_some_and(|byte| byte.is_ok_and(|byte| byte == char)))
}

pub fn write_message(output: &mut impl Write, content: &str) -> bool {
    write!(output, "Content-Length: {}\r\n\r\n{}", content.len(), content)
        .inspect(|()| output.flush().expect("failed to flush output"))
        .is_ok()
}

pub fn read_message(input: &mut impl Read) -> std::io::Result<String> {
    let error = |msg| Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, msg));

    if !consume(input.bytes(), "Content-Length: ".as_bytes()) {
        return error("Missing Content-Length header");
    }

    let mut length: usize = 0;
    let mut content = Vec::new();

    for byte in input.bytes() {
        let byte = byte?;
        if byte.is_ascii_digit() {
            length *= 10;
            length += (byte - b'0') as usize;
        }
        else if byte == b'\r' {
            content.reserve(length);
            break;
        }
        else {
            return error("unexpected byte value");
        }
    }

    if length == 0 {
        return error("Missing content length");
    }
    if !consume(input.bytes(), "\n\r\n".as_bytes()) {
        return error("Missing content separator");
    }

    input.take(length as u64).read_to_end(&mut content).map(|read| {
        assert_eq!(length, read);
        String::from_utf8_lossy(&content).into_owned()
    })
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Error {
        Error::invalid_params(error.to_string())
    }
}

impl Error {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Error {
        Error { code: code as i32, message: message.into() }
    }
    pub fn invalid_params(message: impl Into<String>) -> Error {
        Error::new(ErrorCode::InvalidParams, message)
    }
}

impl Response {
    pub fn success(id: Option<u32>, result: serde_json::Value) -> Response {
        Response { id, result: Some(result), error: None, jsonrpc: JsonRpc }
    }
    pub fn error(id: Option<u32>, error: Error) -> Response {
        Response { id, result: None, error: Some(error), jsonrpc: JsonRpc }
    }
}
