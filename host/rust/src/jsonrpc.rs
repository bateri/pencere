//! The JSON-RPC 2.0 messages that travel in `msg` frames and between the host and
//! its view, as far as the host has to read and write them.
//!
//! A message from outside is read into an [`Envelope`] that keeps its body as
//! raw text: the host routes a message by its id and method, and a hostile
//! stream can make a parsed tree many times larger than the text it came from.

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

/// A JSON-RPC request id: a string or an integer.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Id {
    /// A number.
    Int(i64),
    /// A string.
    Str(String),
}

/// The error code of "method not found".
pub const METHOD_NOT_FOUND: i64 = -32601;
/// The error code of "invalid params".
pub const INVALID_PARAMS: i64 = -32602;
/// The error code of "invalid request".
pub const INVALID_REQUEST: i64 = -32600;
/// The implementation-defined error code MCP Apps uses for a denial.
pub const DENIED: i64 = -32000;

/// A message of any kind, its body left as text.
#[derive(Debug, Deserialize)]
pub struct Envelope {
    jsonrpc: Option<String>,
    /// The id of a request or a response.
    pub id: Option<Id>,
    /// The method of a request or a notification.
    pub method: Option<String>,
    /// The parameters of a request or a notification.
    pub params: Option<Box<RawValue>>,
    /// The result of a successful response.
    pub result: Option<Box<RawValue>>,
    /// The error of a failed response.
    pub error: Option<Box<RawValue>>,
}

/// What an [`Envelope`] turned out to be.
#[derive(Debug)]
pub enum Kind {
    /// A request: it has an id and a method and wants an answer.
    Request {
        /// The id to answer with.
        id: Id,
        /// The method.
        method: String,
    },
    /// A notification: a method and no id.
    Notification {
        /// The method.
        method: String,
    },
    /// A successful response to the request with this id.
    Result {
        /// The id of the request.
        id: Id,
    },
    /// A failed response to the request with this id.
    Failure {
        /// The id of the request.
        id: Id,
    },
}

impl Envelope {
    /// Reads a message. `None` when the text is not a JSON-RPC 2.0 message.
    pub fn parse(text: &str) -> Option<Self> {
        let envelope: Self = crate::json::from_object(text)?;
        (envelope.jsonrpc.as_deref() == Some("2.0")).then_some(envelope)
    }

    /// What the message is. `None` when it is none of the four shapes.
    pub fn kind(&self) -> Option<Kind> {
        match (&self.method, &self.id, &self.result, &self.error) {
            (Some(method), Some(id), None, None) => Some(Kind::Request {
                id: id.clone(),
                method: method.clone(),
            }),
            (Some(method), None, None, None) => Some(Kind::Notification {
                method: method.clone(),
            }),
            (None, Some(id), Some(_), None) => Some(Kind::Result { id: id.clone() }),
            (None, Some(id), None, Some(_)) => Some(Kind::Failure { id: id.clone() }),
            _ => None,
        }
    }

    /// The `params` of the message read as `T`.
    pub fn params_as<T: for<'de> Deserialize<'de>>(&self) -> Option<T> {
        serde_json::from_str(self.params.as_deref()?.get()).ok()
    }

    /// The `result` of the message read as `T`.
    pub fn result_as<T: for<'de> Deserialize<'de>>(&self) -> Option<T> {
        crate::json::from_object(self.result.as_deref()?.get())
    }

    /// The message of the error of a failed response, for people.
    pub fn error_message(&self) -> Option<String> {
        #[derive(Deserialize)]
        struct Error {
            message: Option<String>,
        }
        let error: Error = crate::json::from_object(self.error.as_deref()?.get())?;
        error.message
    }
}

#[derive(Serialize)]
struct Request<'a, P: Serialize> {
    jsonrpc: &'static str,
    id: &'a Id,
    method: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<P>,
}

#[derive(Serialize)]
struct Notification<'a, P: Serialize> {
    jsonrpc: &'static str,
    method: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<P>,
}

#[derive(Serialize)]
struct Response<'a, R: Serialize> {
    jsonrpc: &'static str,
    id: &'a Id,
    result: R,
}

#[derive(Serialize)]
struct Failure<'a> {
    jsonrpc: &'static str,
    id: &'a Id,
    error: ErrorBody<'a>,
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    code: i64,
    message: &'a str,
}

/// A response whose body is somebody else's text, passed on under another id.
#[derive(Serialize)]
struct Relay<'a> {
    jsonrpc: &'static str,
    id: &'a Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<&'a RawValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a RawValue>,
}

// Serialising these shapes cannot fail: their keys are fixed and their values are
// JSON already. The empty string a failure would give is not a message anyone
// sends.
fn text<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

/// A request, as compact JSON text.
pub fn request<P: Serialize>(id: &Id, method: &str, params: Option<P>) -> String {
    text(&Request {
        jsonrpc: "2.0",
        id,
        method,
        params,
    })
}

/// A notification, as compact JSON text.
pub fn notification<P: Serialize>(method: &str, params: Option<P>) -> String {
    text(&Notification {
        jsonrpc: "2.0",
        method,
        params,
    })
}

/// A notification whose parameters are somebody else's text, as compact JSON text.
pub fn notification_raw(method: &str, params: &RawValue) -> String {
    text(&Notification {
        jsonrpc: "2.0",
        method,
        params: Some(params),
    })
}

/// A successful response, as compact JSON text.
pub fn result<R: Serialize>(id: &Id, result: R) -> String {
    text(&Response {
        jsonrpc: "2.0",
        id,
        result,
    })
}

/// A failed response, as compact JSON text.
pub fn failure(id: &Id, code: i64, message: &str) -> String {
    text(&Failure {
        jsonrpc: "2.0",
        id,
        error: ErrorBody { code, message },
    })
}

/// The response `envelope` carries, under `id` instead of its own, as compact JSON
/// text. `None` when `envelope` is not a response.
pub fn relay(envelope: &Envelope, id: &Id) -> Option<String> {
    let relay = Relay {
        jsonrpc: "2.0",
        id,
        result: envelope.result.as_deref(),
        error: envelope.error.as_deref(),
    };
    (relay.result.is_some() != relay.error.is_some()).then(|| text(&relay))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_four_shapes_are_told_apart() {
        let shape = |text: &str| Envelope::parse(text).and_then(|e| e.kind());
        assert!(matches!(
            shape(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#),
            Some(Kind::Request { id: Id::Int(1), .. })
        ));
        assert!(matches!(
            shape(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#),
            Some(Kind::Notification { .. })
        ));
        assert!(matches!(
            shape(r#"{"jsonrpc":"2.0","id":"a","result":{}}"#),
            Some(Kind::Result { id: Id::Str(_) })
        ));
        assert!(matches!(
            shape(r#"{"jsonrpc":"2.0","id":2,"error":{"code":-1,"message":"x"}}"#),
            Some(Kind::Failure { .. })
        ));
        for bad in [
            r#"{"id":1,"method":"ping"}"#,
            r#"{"jsonrpc":"1.0","id":1,"method":"ping"}"#,
            r#"{"jsonrpc":"2.0","id":1.5,"method":"ping"}"#,
            r#"{"jsonrpc":"2.0","id":1}"#,
            r#"{"jsonrpc":"2.0","id":1,"result":{},"error":{}}"#,
            "[]",
            r#"["2.0",1,"ping"]"#,
            "nope",
        ] {
            assert!(
                Envelope::parse(bad).and_then(|e| e.kind()).is_none(),
                "{bad}"
            );
        }
    }

    #[test]
    fn a_response_is_passed_on_under_another_id_with_its_body_untouched() {
        let from_program = r#"{"jsonrpc":"2.0","id":7,"result":{"content":[{"type":"text","text":"b  c"}],"n":1.50}}"#;
        let envelope = Envelope::parse(from_program).unwrap();
        let passed = relay(&envelope, &Id::Str("view-3".into())).unwrap();
        assert_eq!(
            passed,
            r#"{"jsonrpc":"2.0","id":"view-3","result":{"content":[{"type":"text","text":"b  c"}],"n":1.50}}"#
        );
        let failed =
            Envelope::parse(r#"{"jsonrpc":"2.0","id":7,"error":{"code":-32602,"message":"no"}}"#)
                .unwrap();
        assert_eq!(
            relay(&failed, &Id::Int(2)).unwrap(),
            r#"{"jsonrpc":"2.0","id":2,"error":{"code":-32602,"message":"no"}}"#
        );
        assert_eq!(failed.error_message().as_deref(), Some("no"));
    }

    #[test]
    fn what_is_written_is_compact_and_in_the_order_the_fixtures_use() {
        assert_eq!(
            request(&Id::Int(1), "tools/list", None::<()>),
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#
        );
        assert_eq!(
            request(&Id::Int(2), "tools/call", Some(json!({"name": "t"}))),
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"t"}}"#
        );
        assert_eq!(
            notification("notifications/initialized", None::<()>),
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#
        );
        assert_eq!(
            result(&Id::Int(3), json!({})),
            r#"{"jsonrpc":"2.0","id":3,"result":{}}"#
        );
        assert_eq!(
            failure(&Id::Int(4), DENIED, "no conversation"),
            r#"{"jsonrpc":"2.0","id":4,"error":{"code":-32000,"message":"no conversation"}}"#
        );
    }
}
