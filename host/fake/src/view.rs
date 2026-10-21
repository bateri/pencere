//! The view, as a script: what a web view in a real terminal does for the host,
//! played from the command line. It sends `ui/initialize`, the notification
//! that it is initialized, and then the tool calls it was given, one after the
//! other as each answer arrives; it answers the host's teardown. It draws
//! nothing: what is being tried is the conversation, and the page of a real
//! view is for a person to look at in a terminal that hosts pencere.

use std::collections::VecDeque;

use serde_json::{Value, json};

use crate::args::Call;
use crate::log;

/// What the script does after a message.
#[derive(Debug, Default)]
pub struct Reaction {
    /// Messages for the host, as JSON text.
    pub send: Vec<String>,
    /// The calls are done (or there was nothing to call).
    pub done: bool,
}

pub struct ScriptedView {
    calls: VecDeque<Call>,
    next_id: i64,
    initialize: Option<i64>,
    call: Option<i64>,
    /// The first tool result has come: the script has begun its calls.
    begun: bool,
    done: bool,
}

fn request(id: i64, method: &str, params: &Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string()
}

fn notification(method: &str) -> String {
    json!({ "jsonrpc": "2.0", "method": method }).to_string()
}

/// A line of a tool result's text, as far as it fits a log line.
fn gist(result: &Value) -> String {
    let text = result
        .pointer("/content/0/text")
        .and_then(Value::as_str)
        .unwrap_or("");
    let line: String = text.lines().next().unwrap_or("").chars().take(60).collect();
    let failed = result.get("isError").and_then(Value::as_bool) == Some(true);
    format!("{}{line:?}", if failed { "error " } else { "" })
}

impl ScriptedView {
    pub fn new(calls: Vec<Call>) -> Self {
        Self {
            calls: calls.into(),
            next_id: 1,
            initialize: None,
            call: None,
            begun: false,
            done: false,
        }
    }

    /// The first thing a view does.
    pub fn start(&mut self) -> Reaction {
        let id = self.take_id();
        self.initialize = Some(id);
        log::line("view", "-> ui/initialize (written for the pane)");
        let params = json!({
            "protocolVersion": "2026-01-26",
            "appInfo": { "name": "pencere-fake-host script", "version": env!("CARGO_PKG_VERSION") },
            "appCapabilities": { "experimental": { "pencere/pane": {} } },
        });
        Reaction {
            send: vec![request(id, "ui/initialize", &params)],
            done: false,
        }
    }

    fn take_id(&mut self) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// What the host sent the view.
    pub fn receive(&mut self, text: &str) -> Reaction {
        let Ok(message) = serde_json::from_str::<Value>(text) else {
            log::line("view", "<- a message that is not JSON");
            return Reaction::default();
        };
        if let Some(method) = message.get("method").and_then(Value::as_str) {
            let params = message.get("params").unwrap_or(&Value::Null);
            return match message.get("id") {
                Some(id) => self.asked(id, method),
                None => self.told(method, params),
            };
        }
        let Some(id) = message.get("id").and_then(Value::as_i64) else {
            return Reaction::default();
        };
        if Some(id) == self.initialize {
            self.initialize = None;
            return self.initialized(&message);
        }
        if Some(id) == self.call {
            self.call = None;
            return match message.get("result") {
                Some(result) => {
                    log::line("view", format!("<- tool result {}", gist(result)));
                    self.next_call()
                }
                None => {
                    let reason = message.pointer("/error/message").and_then(Value::as_str);
                    log::line(
                        "view",
                        format!("<- tool call failed: {}", reason.unwrap_or("?")),
                    );
                    self.next_call()
                }
            };
        }
        log::line("view", "<- an answer to nothing it asked");
        Reaction::default()
    }

    /// A request of the host: the teardown, and anything else it may ask.
    fn asked(&mut self, id: &Value, method: &str) -> Reaction {
        log::line("view", format!("<- {method} (request)"));
        let answer = json!({ "jsonrpc": "2.0", "id": id, "result": {} }).to_string();
        log::line("view", format!("-> answers {method}"));
        Reaction {
            send: vec![answer],
            done: false,
        }
    }

    fn told(&mut self, method: &str, params: &Value) -> Reaction {
        match method {
            "ui/notifications/tool-input" => {
                log::line("view", "<- ui/notifications/tool-input");
                Reaction::default()
            }
            "ui/notifications/tool-result" => {
                log::line(
                    "view",
                    format!("<- ui/notifications/tool-result {}", gist(params)),
                );
                // The calls follow the first result; a later one does not start
                // another while one is in flight.
                if std::mem::replace(&mut self.begun, true) {
                    return Reaction::default();
                }
                self.next_call()
            }
            "ui/notifications/tool-cancelled" => {
                log::line("view", "<- ui/notifications/tool-cancelled");
                self.finish()
            }
            other => {
                log::line("view", format!("<- {other}"));
                Reaction::default()
            }
        }
    }

    fn initialized(&mut self, answer: &Value) -> Reaction {
        let context = answer
            .pointer("/result/hostContext")
            .unwrap_or(&Value::Null);
        let text = |pointer: &str| {
            context
                .pointer(pointer)
                .and_then(Value::as_str)
                .unwrap_or("?")
                .to_owned()
        };
        let number = |pointer: &str| {
            context
                .pointer(pointer)
                .and_then(Value::as_u64)
                .map_or_else(|| "?".to_owned(), |n| n.to_string())
        };
        let pane = answer
            .pointer("/result/hostCapabilities/experimental/pencere~1pane")
            .is_some();
        log::line(
            "view",
            format!(
                "<- ui/initialize result: theme={} displayMode={} container={}x{} pane={pane}",
                text("/theme"),
                text("/displayMode"),
                number("/containerDimensions/width"),
                number("/containerDimensions/height"),
            ),
        );
        log::line("view", "-> ui/notifications/initialized");
        Reaction {
            send: vec![notification("ui/notifications/initialized")],
            done: false,
        }
    }

    /// The next of the calls it was given, or the end.
    fn next_call(&mut self) -> Reaction {
        let Some(call) = self.calls.pop_front() else {
            return self.finish();
        };
        let id = self.take_id();
        self.call = Some(id);
        log::line(
            "view",
            format!("-> tools/call {} {}", call.name, call.arguments),
        );
        let params = json!({ "name": call.name, "arguments": call.arguments });
        Reaction {
            send: vec![request(id, "tools/call", &params)],
            done: false,
        }
    }

    fn finish(&mut self) -> Reaction {
        let first = !self.done;
        self.done = true;
        Reaction {
            send: Vec::new(),
            done: first,
        }
    }
}
