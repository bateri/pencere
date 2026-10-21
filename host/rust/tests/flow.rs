//! A program and a view, played against a host session: the conversation of
//! Appendix C of the specification and what grows from it. The program and the
//! view are scripts in this file; the host is the crate.

// A test fails by panicking, and indexes what it has just built.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use pencere::base64;
use pencere::bundle::{Digest, MemoryStore, Store, Verified};
use pencere::frame::{Direction, Event, Ids, Message, Outgoing, Split, StreamDecoder, encode};
use pencere::jsonrpc::Id;
use pencere::message::HostInfo;
use pencere::sandbox::Grant;
use pencere::session::{Action, Config, Observed, Session, Size, State, ViewSource};
use serde_json::{Value, json};

type Host = Session<MemoryStore, Box<dyn FnMut() -> [u8; 16]>>;

const TOKENS: [&str; 3] = [
    "q7Vh2sB0xJm9kLw4TzN1cA",
    "Zk3mP8rD5uYb1Xe6HqS0vg",
    "wwwwwwwwwwwwwwwwwwwwww",
];

struct Rig {
    host: Host,
    ids: Ids,
    decoder: StreamDecoder,
    /// Everything the host wrote, as messages, since the last `take`.
    wrote: Vec<Message>,
    /// The other actions since the last `take`.
    acted: Vec<Action>,
    /// The token of the last `welcome` that carried one.
    token: Option<String>,
    /// The requests the host has made to the program and nobody has answered.
    requests: Vec<(Value, String)>,
}

fn host_info() -> HostInfo {
    HostInfo {
        name: "pencere-test".into(),
        version: "0.0.0".into(),
    }
}

fn config() -> Config {
    Config::new(
        host_info(),
        common::theme(),
        Size {
            width: 800,
            height: 600,
        },
    )
}

impl Rig {
    fn new() -> Self {
        Self::with(config())
    }

    fn with(config: Config) -> Self {
        let mut tokens = TOKENS
            .iter()
            .map(|t| common::token_bytes(t))
            .collect::<Vec<_>>()
            .into_iter();
        let entropy: Box<dyn FnMut() -> [u8; 16]> =
            Box::new(move || tokens.next().unwrap_or([9; 16]));
        Self {
            host: Session::new(config, MemoryStore::new(), entropy),
            ids: Ids::new(),
            decoder: StreamDecoder::new(Direction::ToProgram),
            wrote: Vec::new(),
            acted: Vec::new(),
            token: None,
            requests: Vec::new(),
        }
    }

    /// The program writes a message.
    fn send(&mut self, kind: &str, re: Option<u32>, digest: Option<&str>, payload: &str) -> u32 {
        let id = self.ids.next_id();
        let message = Outgoing {
            kind,
            id,
            re,
            digest,
            payload,
        };
        let mut bytes = Vec::new();
        encode(Direction::ToHost, &message, Split::Fill, &mut bytes).unwrap();
        self.host.feed(&bytes);
        self.collect();
        id
    }

    /// The program writes a JSON-RPC message.
    fn json(&mut self, value: &Value) {
        let text = pencere::json::to_wire(value).unwrap();
        self.send("msg", None, None, &text);
    }

    fn collect(&mut self) {
        for action in self.host.take_actions() {
            if let Action::Write(bytes) = &action {
                let mut events = Vec::new();
                self.decoder.feed(bytes, &mut events);
                for event in events {
                    match event {
                        Event::Message(m) => {
                            if let Ok(body) = serde_json::from_str::<Value>(&m.payload) {
                                if m.kind == "welcome" {
                                    if let Some(token) = body["token"].as_str() {
                                        self.token = Some(token.to_owned());
                                    }
                                }
                                if m.kind == "msg"
                                    && body.get("method").is_some()
                                    && body.get("id").is_some()
                                {
                                    let method =
                                        body["method"].as_str().unwrap_or_default().to_owned();
                                    self.requests.push((body["id"].clone(), method));
                                }
                            }
                            self.wrote.push(m);
                        }
                        Event::Error(e) => panic!("the host wrote a bad frame: {e}"),
                    }
                }
            } else {
                self.acted.push(action);
            }
        }
    }

    /// The messages the host wrote and the actions it took since the last call.
    fn take(&mut self) -> (Vec<Message>, Vec<Action>) {
        self.collect();
        (
            std::mem::take(&mut self.wrote),
            std::mem::take(&mut self.acted),
        )
    }

    /// The JSON the host wrote in its `msg` messages since the last call.
    fn msgs(&mut self) -> Vec<Value> {
        self.collect();
        std::mem::take(&mut self.wrote)
            .iter()
            .filter(|m| m.kind == "msg")
            .map(|m| serde_json::from_str(&m.payload).unwrap())
            .collect()
    }

    /// The view sends a message.
    fn view(&mut self, value: &Value) {
        self.host.view_message(&value.to_string());
        self.collect();
    }

    /// The messages the host passed to the view since the last call.
    fn viewed(&mut self) -> Vec<Value> {
        self.collect();
        let (viewed, others): (Vec<Action>, Vec<Action>) = std::mem::take(&mut self.acted)
            .into_iter()
            .partition(|a| matches!(a, Action::ToView(_)));
        self.acted = others;
        viewed
            .iter()
            .map(|a| match a {
                Action::ToView(text) => serde_json::from_str(text).unwrap(),
                _ => unreachable!(),
            })
            .collect()
    }

    fn hello(&mut self) -> String {
        self.send("hello", None, None, r#"{"versions":[1]}"#);
        let (wrote, _) = self.take();
        let welcome = wrote
            .iter()
            .find(|m| m.kind == "welcome")
            .expect("a welcome");
        let body: Value = serde_json::from_str(&welcome.payload).unwrap();
        body["token"].as_str().unwrap().to_owned()
    }
}

const INDEX: &str = "<!doctype html><script src=app.js></script>";
const APP: &str = "document.title = 'containers';";
const STYLE: &str = "body { margin: 0 }";

fn bundle(meta: Option<Value>) -> Value {
    let files: Vec<Value> = [("index.html", INDEX, "text/html"), ("app.js", APP, "text/javascript"), ("style.css", STYLE, "text/css")]
        .iter()
        .map(|(path, body, media)| {
            json!({"path": path, "sha256": Digest::of(body.as_bytes()).to_string(), "size": body.len(), "type": media})
        })
        .collect();
    let mut value = json!({"entry": "index.html", "files": files});
    if let Some(meta) = meta {
        value["meta"] = meta;
    }
    value
}

fn open_payload(token: &str, bundle: Option<Value>) -> String {
    let mut value =
        json!({"token": token, "tool": {"name": "list_containers", "arguments": {"all": true}}});
    if let Some(bundle) = bundle {
        value["bundle"] = bundle;
    }
    pencere::json::to_wire(&value).unwrap()
}

/// The payload of a `bye` with the given token, as a program writes it.
fn bye(token: Option<&str>) -> String {
    match token {
        Some(token) => json!({"token": token, "reason": "signal"}).to_string(),
        None => r#"{"reason":"signal"}"#.to_owned(),
    }
}

fn send_file(rig: &mut Rig, body: &str) {
    let digest = Digest::of(body.as_bytes()).to_string();
    rig.send(
        "file",
        None,
        Some(&digest),
        &base64::encode(body.as_bytes()),
    );
}

fn tools() -> Value {
    json!([
        {"name": "list_containers", "description": "List", "inputSchema": {"type": "object"},
         "_meta": {"ui": {"resourceUri": "ui://containers/view"}}},
        {"name": "stop_container", "inputSchema": {"type": "object"},
         "_meta": {"ui": {"resourceUri": "ui://containers/view", "visibility": ["app"]}}},
        {"name": "model_only", "inputSchema": {"type": "object"},
         "_meta": {"ui": {"visibility": ["model"]}}},
        {"name": "plain", "inputSchema": {"type": "object"}},
    ])
}

/// Answers the host's requests the way a program does, up to the moment the view
/// starts (or the person is asked).
fn start_up(rig: &mut Rig, tools: Value) -> Vec<Action> {
    let initialize = rig.msgs();
    assert_eq!(initialize.len(), 1, "{initialize:?}");
    assert_eq!(initialize[0]["method"], "initialize");
    let id = initialize[0]["id"].clone();
    rig.json(&json!({"jsonrpc": "2.0", "id": id, "result": {
        "protocolVersion": "2025-11-25", "capabilities": {"tools": {}}, "serverInfo": {"name": "containers", "version": "0.0.0"}}}));
    let next = rig.msgs();
    assert_eq!(next.len(), 2, "{next:?}");
    assert_eq!(next[0]["method"], "notifications/initialized");
    assert_eq!(next[1]["method"], "tools/list");
    let id = next[1]["id"].clone();
    rig.json(&json!({"jsonrpc": "2.0", "id": id, "result": {"tools": tools}}));
    rig.collect();
    std::mem::take(&mut rig.acted)
}

fn view_started(actions: &[Action]) -> &pencere::session::ViewStart {
    actions
        .iter()
        .find_map(|a| match a {
            Action::StartView(start) => Some(start.as_ref()),
            _ => None,
        })
        .expect("a view to start")
}

/// The view's handshake and the call of the tool; returns the id of the call.
fn view_handshake(rig: &mut Rig, pane: bool) -> Value {
    let mut capabilities = json!({"availableDisplayModes": ["inline"]});
    if pane {
        capabilities["experimental"] = json!({"pencere/pane": {}});
    }
    rig.view(&json!({"jsonrpc": "2.0", "id": 1, "method": "ui/initialize", "params": {
        "protocolVersion": "2026-01-26", "appInfo": {"name": "v", "version": "1"}, "appCapabilities": capabilities}}));
    let answer = rig.viewed();
    assert_eq!(answer.len(), 1);
    let result = answer[0]["result"].clone();
    rig.view(&json!({"jsonrpc": "2.0", "method": "ui/notifications/initialized"}));
    result
}

/// A host in the state of Appendix C: the view runs and the tool was called.
fn running(meta: Option<Value>) -> Rig {
    let mut rig = Rig::new();
    let token = rig.hello();
    rig.send(
        "open",
        None,
        None,
        &open_payload(&token, Some(bundle(meta))),
    );
    // Nothing is held: three files are asked for.
    let (wrote, _) = rig.take();
    let need: Value = serde_json::from_str(&wrote.last().unwrap().payload).unwrap();
    assert_eq!(need["missing"].as_array().unwrap().len(), 3);
    for body in [INDEX, APP, STYLE] {
        send_file(&mut rig, body);
    }
    let actions = start_up(&mut rig, tools());
    if let Some(consent) = actions.iter().find_map(|a| match a {
        Action::Consent(c) => Some(c.clone()),
        _ => None,
    }) {
        rig.host.consent(consent.grant_all());
        rig.collect();
    } else {
        rig.acted.extend(actions);
    }
    rig
}

#[test]
fn appendix_c_is_this_conversation_byte_for_byte() {
    // The host's store already holds index.html and style.css.
    let mut rig = Rig::new();
    rig.host
        .store_mut()
        .insert(Verified::from_bytes(INDEX.as_bytes().to_vec()));
    rig.host
        .store_mut()
        .insert(Verified::from_bytes(STYLE.as_bytes().to_vec()));
    let host_bytes = |rig: &mut Rig| {
        let actions = rig.host.take_actions();
        common::written(&actions)
    };
    // 1 and 2: the query and its answer, before the answer to CSI c.
    rig.host.feed(&common::file("to-host/hello-fence.frame"));
    assert_eq!(
        host_bytes(&mut rig),
        common::file("to-program/welcome.frame")
    );
    // 3 and 4: the open, and the digests the host lacks (fixtures carry other digests
    // than this test's files, so only the shape is compared here).
    let token = "q7Vh2sB0xJm9kLw4TzN1cA";
    // The hello of the fixture was the program's message 1; its open is message 2.
    assert_eq!(rig.ids.next_id(), 1);
    rig.send("open", None, None, &open_payload(token, Some(bundle(None))));
    let (wrote, _) = rig.take();
    let need: Value = serde_json::from_str(&wrote.last().unwrap().payload).unwrap();
    assert_eq!(
        need["missing"],
        json!([Digest::of(APP.as_bytes()).to_string()])
    );
    assert_eq!(wrote.last().unwrap().re(), Some(2));
    // 5: the file; 6: the host begins an MCP session with it.
    send_file(&mut rig, APP);
    let (wrote, _) = rig.take();
    let initialize = wrote.last().unwrap();
    assert_eq!((initialize.kind.as_str(), initialize.id), ("msg", 3));
    let fixture = common::file("to-program/msg-initialize.frame");
    let mut expected = Vec::new();
    encode(
        Direction::ToProgram,
        &Outgoing {
            kind: "msg",
            id: 3,
            re: None,
            digest: None,
            payload: &initialize.payload,
        },
        Split::Fill,
        &mut expected,
    )
    .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&expected),
        String::from_utf8_lossy(&fixture)
    );
}

#[test]
fn the_first_mcp_request_is_the_one_in_the_fixtures() {
    let mut rig = Rig::new();
    let token = rig.hello();
    rig.send("open", None, None, &open_payload(&token, None));
    let (wrote, _) = rig.take();
    // No manifest, so no need: the MCP session begins at once.
    assert_eq!(wrote.len(), 1);
    let fixture = common::file("to-program/msg-initialize.frame");
    let mut events = Vec::new();
    StreamDecoder::new(Direction::ToProgram).feed(&fixture, &mut events);
    let [Event::Message(fixture)] = events.as_slice() else {
        panic!("{events:?}")
    };
    assert_eq!(wrote[0].payload, fixture.payload);
}

#[test]
fn a_view_with_a_bundle_runs_and_gets_what_its_program_gave_it() {
    let mut rig = running(None);
    let (_, acted) = rig.take();
    let start = view_started(&acted);
    assert_eq!(start.source, ViewSource::Bundle);
    assert_eq!(start.tool, "list_containers");
    assert_eq!(rig.host.state(), State::Open);
    // The bundle is served from the store, and only what the manifest lists.
    let (media, bytes) = rig.host.serve("app.js").unwrap();
    assert_eq!(
        (media.as_str(), &bytes[..]),
        ("text/javascript", APP.as_bytes())
    );
    assert!(rig.host.serve("../app.js").is_none());
    assert!(rig.host.serve("other.js").is_none());

    let result = view_handshake(&mut rig, true);
    assert_eq!(result["protocolVersion"], "2026-01-26");
    assert_eq!(result["hostInfo"]["name"], "pencere-test");
    let caps = &result["hostCapabilities"];
    assert_eq!(caps["experimental"], json!({"pencere/pane": {}}));
    assert!(caps["serverTools"].is_object() && caps["serverResources"].is_object());
    assert!(caps["openLinks"].is_object() && caps["logging"].is_object());
    assert_eq!(caps["sandbox"], json!({}));
    let context = &result["hostContext"];
    assert_eq!(context["theme"], "dark");
    assert_eq!(context["displayMode"], "inline");
    assert_eq!(context["availableDisplayModes"], json!(["inline"]));
    assert_eq!(
        context["containerDimensions"],
        json!({"width": 800, "height": 600})
    );
    assert_eq!(context["platform"], "desktop");
    assert_eq!(context["toolInfo"]["tool"]["name"], "list_containers");
    assert_eq!(
        context["styles"]["variables"]["--color-background-primary"],
        "#000000"
    );
    assert_eq!(
        context["styles"]["variables"]["--font-mono"],
        "\"Menlo\", monospace"
    );

    // After `initialized` the view gets the input, and the tool is called.
    let told = rig.viewed();
    assert_eq!(told[0]["method"], "ui/notifications/tool-input");
    assert_eq!(told[0]["params"], json!({"arguments": {"all": true}}));
    let calls = rig.msgs();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0]["method"], "tools/call");
    assert_eq!(
        calls[0]["params"],
        json!({"name": "list_containers", "arguments": {"all": true}})
    );
    // ... and the result goes to the view as it came.
    rig.json(&json!({"jsonrpc": "2.0", "id": calls[0]["id"], "result": {
        "content": [{"type": "text", "text": "3 containers \u{e9}"}], "structuredContent": {"n": 3}}}));
    let told = rig.viewed();
    assert_eq!(told.len(), 1);
    assert_eq!(told[0]["method"], "ui/notifications/tool-result");
    assert_eq!(told[0]["params"]["structuredContent"], json!({"n": 3}));
    assert_eq!(
        told[0]["params"]["content"][0]["text"],
        "3 containers \u{e9}"
    );
}

#[test]
fn a_view_that_does_not_know_the_pane_is_told_inline_and_not_the_extension() {
    let mut rig = running(None);
    let result = view_handshake(&mut rig, false);
    assert!(result["hostCapabilities"].get("experimental").is_none());
    assert_eq!(result["hostContext"]["displayMode"], "inline");
    // A view that lists no `inline` is given the first of `fullscreen` and `pip` it lists.
    let mut rig = running(None);
    rig.view(
        &json!({"jsonrpc": "2.0", "id": 1, "method": "ui/initialize", "params": {
        "appCapabilities": {"availableDisplayModes": ["pip", "fullscreen"]}}}),
    );
    let answer = rig.viewed();
    assert_eq!(
        answer[0]["result"]["hostContext"]["displayMode"],
        "fullscreen"
    );
    assert_eq!(
        answer[0]["result"]["hostContext"]["availableDisplayModes"],
        json!(["fullscreen"])
    );
    rig.view(&json!({"jsonrpc": "2.0", "id": 2, "method": "ui/request-display-mode", "params": {"mode": "inline"}}));
    assert_eq!(rig.viewed()[0]["result"], json!({"mode": "fullscreen"}));
}

#[test]
fn a_view_calls_only_the_tools_its_visibility_allows_and_ids_are_two_spaces() {
    let mut rig = running(None);
    view_handshake(&mut rig, true);
    rig.take();
    // `stop_container` has `app` in its visibility: passed on under the host's id.
    rig.view(&json!({"jsonrpc": "2.0", "id": "view-1", "method": "tools/call", "params": {"name": "stop_container", "arguments": {"id": "c1"}}}));
    let passed = rig.msgs();
    assert_eq!(passed.len(), 1);
    assert_eq!(passed[0]["method"], "tools/call");
    assert_ne!(passed[0]["id"], json!("view-1"));
    assert_eq!(passed[0]["params"]["arguments"], json!({"id": "c1"}));
    rig.json(&json!({"jsonrpc": "2.0", "id": passed[0]["id"], "result": {"content": []}}));
    let back = rig.viewed();
    assert_eq!(
        back,
        vec![json!({"jsonrpc": "2.0", "id": "view-1", "result": {"content": []}})]
    );
    // The tool the view is for has the default visibility, which includes `app`.
    rig.view(&json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {"name": "list_containers"}}));
    assert_eq!(rig.msgs().len(), 1);
    // A tool for the model alone, one that does not exist, and a malformed call are refused
    // and never reach the program.
    for params in [
        json!({"name": "model_only"}),
        json!({"name": "nope"}),
        json!({}),
        json!({"name": 5}),
    ] {
        rig.view(&json!({"jsonrpc": "2.0", "id": 6, "method": "tools/call", "params": params}));
        assert!(rig.msgs().is_empty(), "{params}");
        let answer = rig.viewed();
        assert_eq!(answer[0]["error"]["code"], -32602, "{params}");
    }
    // Resources are read through the host.
    rig.view(&json!({"jsonrpc": "2.0", "id": 7, "method": "resources/read", "params": {"uri": "ui://containers/view"}}));
    assert_eq!(rig.msgs()[0]["method"], "resources/read");
    rig.view(&json!({"jsonrpc": "2.0", "id": 8, "method": "resources/read", "params": {}}));
    assert!(rig.msgs().is_empty());
    // Messages of the host's own are never passed on.
    for method in [
        "ui/initialize",
        "ui/resource-teardown",
        "initialize",
        "tools/list",
        "sampling/createMessage",
    ] {
        rig.view(&json!({"jsonrpc": "2.0", "id": 9, "method": method}));
        assert!(rig.msgs().is_empty(), "{method}");
    }
}

#[test]
fn a_program_that_does_not_answer_fails_the_requests_it_was_given_when_the_wait_runs_out() {
    let mut rig = running(None);
    view_handshake(&mut rig, true);
    rig.take();
    rig.view(&json!({"jsonrpc": "2.0", "id": "a", "method": "tools/call", "params": {"name": "stop_container"}}));
    rig.view(&json!({"jsonrpc": "2.0", "id": "b", "method": "resources/read", "params": {"uri": "ui://x"}}));
    rig.take();
    rig.host.requests_timed_out();
    let told = rig.viewed();
    // The two requests of the view, and the call of the tool that was still out.
    assert_eq!(told.len(), 3);
    assert_eq!(told[0]["id"], "a");
    assert_eq!(told[1]["error"]["code"], -32000);
    assert_eq!(told[2]["method"], "ui/notifications/tool-cancelled");
    // An answer that comes late has nobody to go to.
    rig.json(&json!({"jsonrpc": "2.0", "id": 4, "result": {}}));
    assert!(rig.viewed().is_empty());
    assert_eq!(rig.host.state(), State::Open);
}

#[test]
fn a_terminal_has_no_conversation_and_a_view_that_asks_is_told_so() {
    let mut rig = running(None);
    view_handshake(&mut rig, true);
    rig.take();
    for method in ["ui/message", "ui/update-model-context"] {
        rig.view(&json!({"jsonrpc": "2.0", "id": 3, "method": method, "params": {}}));
        let answer = rig.viewed();
        assert_eq!(answer[0]["error"]["code"], -32000, "{method}");
    }
    rig.view(&json!({"jsonrpc": "2.0", "id": 4, "method": "ping"}));
    assert_eq!(rig.viewed()[0]["result"], json!({}));
    rig.view(&json!({"jsonrpc": "2.0", "method": "notifications/message", "params": {"level": "info", "data": "hi"}}));
    let (_, acted) = rig.take();
    assert!(matches!(acted.as_slice(), [Action::ViewLog(text)] if text.contains("\"hi\"")));
}

#[test]
fn nothing_is_sent_to_a_view_before_it_has_initialized_and_it_must_initialize_first() {
    let mut rig = running(None);
    rig.view(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "list_containers"}}));
    assert_eq!(rig.viewed()[0]["error"]["code"], -32600);
    assert!(rig.msgs().is_empty());
    rig.view(&json!({"jsonrpc": "2.0", "id": 2, "method": "ui/initialize", "params": {}}));
    rig.take();
    // Resizing before `initialized` says nothing: the answer carries the size.
    rig.host.set_size(Size {
        width: 100,
        height: 50,
    });
    assert!(rig.viewed().is_empty());
    rig.view(&json!({"jsonrpc": "2.0", "method": "ui/notifications/initialized"}));
    rig.take();
    rig.host.set_size(Size {
        width: 120,
        height: 60,
    });
    let told = rig.viewed();
    assert_eq!(told[0]["method"], "ui/notifications/host-context-changed");
    assert_eq!(
        told[0]["params"],
        json!({"containerDimensions": {"width": 120, "height": 60}})
    );
    let mut light = common::theme();
    light.background = pencere::theme::Rgb(255, 255, 255);
    light.foreground = pencere::theme::Rgb(0, 0, 0);
    rig.host.set_theme(light);
    let told = rig.viewed();
    assert_eq!(told[0]["params"]["theme"], "light");
    assert_eq!(
        told[0]["params"]["styles"]["variables"]["--color-background-primary"],
        "#ffffff"
    );
}

#[test]
fn the_program_ending_tears_the_view_down_and_nothing_is_written_to_it() {
    let mut rig = running(None);
    view_handshake(&mut rig, true);
    rig.take();
    rig.send("bye", None, None, &bye(Some(TOKENS[0])));
    let (wrote, acted) = rig.take();
    assert!(wrote.is_empty(), "{wrote:?}");
    // The view is told, and given the time to answer. The call of the tool is still out.
    let told: Vec<Value> = acted
        .iter()
        .map(|a| match a {
            Action::ToView(text) => serde_json::from_str(text).unwrap(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(told[0]["method"], "ui/notifications/tool-cancelled");
    let teardown = told[1].clone();
    assert_eq!(teardown["method"], "ui/resource-teardown");
    assert_eq!(rig.host.state(), State::Closing);
    rig.view(&json!({"jsonrpc": "2.0", "id": teardown["id"], "result": {}}));
    let (wrote, acted) = rig.take();
    assert!(wrote.is_empty());
    assert_eq!(acted, vec![Action::DestroyView]);
    assert_eq!(rig.host.state(), State::Ended);
    // The program has gone: no frame, whatever arrives.
    rig.send("open", None, None, &open_payload(TOKENS[0], None));
    rig.send("hello", None, None, "not json");
    assert!(rig.take().0.is_empty());
    // A hello says a program is there again.
    let token = rig.hello();
    assert_eq!(token, TOKENS[1]);
    assert_eq!(rig.host.state(), State::Offered);
}

#[test]
fn a_bye_ends_the_view_only_with_the_token_that_opened_it() {
    // The view runs; the token that opened it is TOKENS[0].
    let mut rig = running(None);
    view_handshake(&mut rig, true);
    rig.take();
    for payload in [
        bye(None),
        bye(Some(TOKENS[1])),
        bye(Some("AAAAAAAAAAAAAAAAAAAAAA")),
        r#"{"token":7}"#.to_owned(),
        r#"{"token":["q7Vh2sB0xJm9kLw4TzN1cA"]}"#.to_owned(),
        "[]".to_owned(),
        "not json".to_owned(),
    ] {
        rig.send("bye", None, None, &payload);
        let (wrote, acted) = rig.take();
        assert!(wrote.is_empty() && acted.is_empty(), "{payload}");
        assert_eq!(rig.host.state(), State::Open, "{payload}");
    }
    // A hello is answered with a new token while the view runs. That token is the
    // pane's next offer and not the view's, and an open with it, which is busy,
    // does not take the place of the token that opened the view.
    let offered = rig.hello();
    assert_eq!(offered, TOKENS[1]);
    rig.send("bye", None, None, &bye(Some(&offered)));
    assert!(rig.take().1.is_empty());
    assert_eq!(rig.host.state(), State::Open);
    rig.send("open", None, None, &open_payload(&offered, None));
    let (wrote, _) = rig.take();
    assert_eq!(wrote[0].kind, "closed");
    assert!(wrote[0].payload.contains("busy"), "{wrote:?}");
    assert_eq!(rig.host.state(), State::Open);
    // The token that opened it ends it.
    rig.send("bye", None, None, &bye(Some(TOKENS[0])));
    let (wrote, acted) = rig.take();
    assert!(wrote.is_empty());
    assert_eq!(rig.host.state(), State::Closing);
    let teardown = acted
        .iter()
        .find_map(|a| match a {
            Action::ToView(text) => {
                let value: Value = serde_json::from_str(text).unwrap();
                (value["method"] == "ui/resource-teardown").then_some(value)
            }
            _ => None,
        })
        .expect("a teardown");
    rig.view(&json!({"jsonrpc": "2.0", "id": teardown["id"], "result": {}}));
    assert_eq!(rig.host.state(), State::Ended);
    // The view is gone and so is its token: the same `bye` is no one's now.
    rig.send("bye", None, None, &bye(Some(TOKENS[0])));
    assert!(rig.take().0.is_empty());
}

#[test]
fn a_bye_when_no_view_is_opening_or_open_changes_nothing() {
    let mut rig = Rig::new();
    // Nothing yet.
    rig.send("bye", None, None, &bye(Some(TOKENS[0])));
    assert_eq!(rig.host.state(), State::Idle);
    // A token is offered and not used: the token of the offer is not a view's.
    let token = rig.hello();
    rig.send("bye", None, None, &bye(Some(&token)));
    assert_eq!(rig.host.state(), State::Offered);
    // The offer still opens a view.
    rig.send("open", None, None, &open_payload(&token, None));
    assert_eq!(rig.host.state(), State::Opening);
}

#[test]
fn a_bye_ends_an_open_that_is_still_waiting_for_files_and_one_that_is_closing() {
    let mut rig = Rig::new();
    let token = rig.hello();
    rig.send(
        "open",
        None,
        None,
        &open_payload(&token, Some(bundle(None))),
    );
    rig.take();
    assert_eq!(rig.host.state(), State::Opening);
    rig.send("bye", None, None, &bye(None));
    assert_eq!(rig.host.state(), State::Opening);
    rig.send("bye", None, None, &bye(Some(&token)));
    let (wrote, acted) = rig.take();
    assert!(wrote.is_empty() && acted.is_empty(), "{wrote:?} {acted:?}");
    assert_eq!(rig.host.state(), State::Ended);

    // A view being closed by the person: the program's `bye` with its token
    // still counts, and nothing is written when the teardown ends.
    let mut rig = running(None);
    view_handshake(&mut rig, true);
    rig.take();
    rig.host.close_view();
    assert_eq!(rig.host.state(), State::Closing);
    let (_, acted) = rig.take();
    let teardown = acted
        .iter()
        .find_map(|a| match a {
            Action::ToView(text) => {
                let value: Value = serde_json::from_str(text).unwrap();
                (value["method"] == "ui/resource-teardown").then_some(value)
            }
            _ => None,
        })
        .expect("a teardown");
    rig.send("bye", None, None, &bye(Some(TOKENS[1])));
    rig.send("bye", None, None, &bye(Some(TOKENS[0])));
    rig.view(&json!({"jsonrpc": "2.0", "id": teardown["id"], "result": {}}));
    let (wrote, _) = rig.take();
    assert!(wrote.is_empty(), "{wrote:?}");
    assert_eq!(rig.host.state(), State::Ended);
}

#[test]
fn the_person_closing_the_view_tells_the_program_after_the_view_is_gone() {
    let mut rig = running(None);
    view_handshake(&mut rig, true);
    rig.take();
    // The call of the tool is still out when the person closes.
    rig.host.close_view();
    rig.collect();
    let (wrote, acted) = rig.take();
    assert!(
        wrote.is_empty(),
        "the program is told after the view is gone"
    );
    let texts: Vec<Value> = acted
        .iter()
        .filter_map(|a| match a {
            Action::ToView(t) => Some(serde_json::from_str(t).unwrap()),
            _ => None,
        })
        .collect();
    assert_eq!(texts[0]["method"], "ui/notifications/tool-cancelled");
    assert_eq!(texts[1]["method"], "ui/resource-teardown");
    assert_eq!(texts[1]["params"], json!({"reason": "user"}));
    // A teardown that is never answered ends when the wait does.
    rig.host.timed_out();
    let (wrote, acted) = rig.take();
    assert_eq!(acted, vec![Action::DestroyView]);
    assert_eq!(wrote.len(), 1);
    assert_eq!(wrote[0].kind, "closed");
    assert_eq!(wrote[0].re(), None);
    assert_eq!(
        serde_json::from_str::<Value>(&wrote[0].payload).unwrap(),
        json!({"reason": "user"})
    );
    // The program is there still, and the token is gone: it must ask again.
    assert_eq!(rig.host.state(), State::Idle);
    rig.send("open", None, None, &open_payload(TOKENS[0], None));
    let (wrote, _) = rig.take();
    assert_eq!(
        serde_json::from_str::<Value>(&wrote[0].payload).unwrap(),
        json!({"reason": "refused", "code": "token"})
    );
}

#[test]
fn a_view_that_asks_to_close_is_closed_and_the_program_told() {
    let mut rig = running(None);
    view_handshake(&mut rig, true);
    rig.take();
    rig.view(&json!({"jsonrpc": "2.0", "method": "ui/notifications/request-teardown"}));
    let told = rig.viewed();
    let id = told.last().unwrap()["id"].clone();
    rig.view(&json!({"jsonrpc": "2.0", "id": id, "result": {}}));
    let (wrote, acted) = rig.take();
    assert_eq!(acted, vec![Action::DestroyView]);
    assert_eq!(
        serde_json::from_str::<Value>(&wrote[0].payload).unwrap(),
        json!({"reason": "view"})
    );
}

#[test]
fn what_the_view_had_in_flight_fails_when_it_closes() {
    let mut rig = running(None);
    view_handshake(&mut rig, true);
    rig.take();
    rig.view(&json!({"jsonrpc": "2.0", "id": 41, "method": "tools/call", "params": {"name": "stop_container"}}));
    rig.view(&json!({"jsonrpc": "2.0", "id": 42, "method": "ui/open-link", "params": {"url": "https://example.com"}}));
    rig.take();
    rig.host.close_view();
    let told = rig.viewed();
    let errors: Vec<&Value> = told.iter().filter(|m| m.get("error").is_some()).collect();
    assert_eq!(errors.len(), 2);
}

#[test]
fn a_view_declaring_domains_is_not_started_before_the_person_has_answered() {
    let meta = json!({"csp": {"connectDomains": ["https://api.example.com", "http://localhost:3000"]}, "permissions": {"camera": {}}, "prefersBorder": false});
    let mut rig = Rig::new();
    let token = rig.hello();
    rig.send(
        "open",
        None,
        None,
        &open_payload(&token, Some(bundle(Some(meta)))),
    );
    rig.take();
    for body in [INDEX, APP, STYLE] {
        send_file(&mut rig, body);
    }
    let actions = start_up(&mut rig, tools());
    let [Action::Consent(consent)] = actions.as_slice() else {
        panic!("{actions:?}")
    };
    assert_eq!(consent.domains.len(), 2);
    assert_eq!(
        consent.domains[1].domain.locality(),
        pencere::sandbox::Locality::Loopback
    );
    assert_eq!(rig.host.state(), State::Opening);
    // The person allows the loopback origin only, and the camera is refused.
    let mut grant = Grant::none();
    grant.allow_domain(
        pencere::sandbox::Directive::Connect,
        pencere::sandbox::Domain::parse("http://localhost:3000").unwrap(),
    );
    // ... and asks for something the view never declared, which is cut away.
    grant.allow_domain(
        pencere::sandbox::Directive::Connect,
        pencere::sandbox::Domain::parse("https://evil.example").unwrap(),
    );
    grant.allow_permission(pencere::sandbox::Permission::Microphone);
    rig.host.consent(grant);
    let (_, acted) = rig.take();
    let start = view_started(&acted);
    assert!(
        start
            .policy
            .contains("connect-src 'self' http://localhost:3000;"),
        "{}",
        start.policy
    );
    assert!(!start.policy.contains("api.example.com") && !start.policy.contains("evil"));
    assert_eq!(start.allow, "");
    let result = view_handshake(&mut rig, true);
    assert_eq!(
        result["hostCapabilities"]["sandbox"],
        json!({"csp": {"connectDomains": ["http://localhost:3000"]}})
    );
}

#[test]
fn a_view_without_a_bundle_is_read_from_the_program() {
    let mut rig = Rig::new();
    let token = rig.hello();
    rig.send("open", None, None, &open_payload(&token, None));
    let actions = start_up(&mut rig, tools());
    assert!(actions.is_empty(), "{actions:?}");
    let read = rig.msgs();
    assert_eq!(read[0]["method"], "resources/read");
    assert_eq!(read[0]["params"], json!({"uri": "ui://containers/view"}));
    let html = "<!doctype html><p>hi</p>";
    rig.json(
        &json!({"jsonrpc": "2.0", "id": read[0]["id"], "result": {"contents": [{
        "uri": "ui://containers/view", "mimeType": "text/html;profile=mcp-app",
        "blob": base64::encode(html.as_bytes()),
        "_meta": {"ui": {"csp": {"resourceDomains": ["https://cdn.example.com"]}}}}]}}),
    );
    let (_, acted) = rig.take();
    let [Action::Consent(consent)] = acted.as_slice() else {
        panic!("{acted:?}")
    };
    rig.host.consent(consent.grant_all());
    let (_, acted) = rig.take();
    let start = view_started(&acted);
    assert_eq!(start.source, ViewSource::Document(html.to_owned()));
    assert!(
        start
            .policy
            .contains("script-src 'self' 'unsafe-inline' https://cdn.example.com;")
    );
    assert!(rig.host.serve("index.html").is_none());
}

#[test]
fn an_open_that_cannot_be_served_ends_with_an_error_and_the_program_is_told() {
    let closed_with = |rig: &mut Rig| -> (Value, Option<u32>) {
        let (wrote, acted) = rig.take();
        assert!(!acted.iter().any(|a| matches!(a, Action::StartView(_))));
        let closed = wrote
            .iter()
            .rfind(|m| m.kind == "closed")
            .expect("a closed");
        (serde_json::from_str(&closed.payload).unwrap(), closed.re())
    };
    // The program has no such tool.
    let mut rig = Rig::new();
    let token = rig.hello();
    let open = rig.send("open", None, None, &open_payload(&token, None));
    start_up(
        &mut rig,
        json!([{"name": "other", "_meta": {"ui": {"resourceUri": "ui://x"}}}]),
    );
    assert_eq!(
        closed_with(&mut rig),
        (json!({"reason": "error", "code": "mcp"}), Some(open))
    );
    assert_eq!(rig.host.state(), State::Idle);
    // The tool has no view.
    let mut rig = Rig::new();
    let token = rig.hello();
    rig.send("open", None, None, &open_payload(&token, None));
    start_up(&mut rig, json!([{"name": "list_containers"}]));
    assert_eq!(
        closed_with(&mut rig).0,
        json!({"reason": "error", "code": "mcp"})
    );
    // The program does not answer `initialize` with a result.
    let mut rig = Rig::new();
    let token = rig.hello();
    rig.send("open", None, None, &open_payload(&token, None));
    let id = rig.msgs()[0]["id"].clone();
    rig.json(&json!({"jsonrpc": "2.0", "id": id, "error": {"code": -1, "message": "no"}}));
    assert_eq!(
        closed_with(&mut rig).0,
        json!({"reason": "error", "code": "mcp"})
    );
    // The resource is not an MCP App's.
    let mut rig = Rig::new();
    let token = rig.hello();
    rig.send("open", None, None, &open_payload(&token, None));
    start_up(&mut rig, tools());
    let read = rig.msgs();
    rig.json(
        &json!({"jsonrpc": "2.0", "id": read[0]["id"], "result": {"contents": [{
        "uri": "ui://containers/view", "mimeType": "text/plain", "text": "x"}]}}),
    );
    assert_eq!(
        closed_with(&mut rig).0,
        json!({"reason": "error", "code": "mcp"})
    );
    // A file that is not what its digest says.
    let mut rig = Rig::new();
    let token = rig.hello();
    let open = rig.send(
        "open",
        None,
        None,
        &open_payload(&token, Some(bundle(None))),
    );
    rig.take();
    let digest = Digest::of(APP.as_bytes()).to_string();
    rig.send(
        "file",
        None,
        Some(&digest),
        &base64::encode(b"something else"),
    );
    assert_eq!(
        closed_with(&mut rig),
        (json!({"reason": "error", "code": "bundle"}), Some(open))
    );
    assert!(rig.host.store().is_empty());
    // Files that never arrive.
    let mut rig = Rig::new();
    let token = rig.hello();
    let open = rig.send(
        "open",
        None,
        None,
        &open_payload(&token, Some(bundle(None))),
    );
    rig.take();
    send_file(&mut rig, INDEX);
    rig.host.timed_out();
    assert_eq!(
        closed_with(&mut rig),
        (json!({"reason": "error", "code": "timeout"}), Some(open))
    );
    // The files a host kept stay kept: the next open does not need them again.
    assert_eq!(rig.host.store().len(), 1);
}

#[test]
fn tools_come_in_pages() {
    let mut rig = Rig::new();
    let token = rig.hello();
    rig.send("open", None, None, &open_payload(&token, None));
    let id = rig.msgs()[0]["id"].clone();
    rig.json(&json!({"jsonrpc": "2.0", "id": id, "result": {}}));
    let next = rig.msgs();
    let id = next[1]["id"].clone();
    rig.json(&json!({"jsonrpc": "2.0", "id": id, "result": {"tools": [{"name": "a"}], "nextCursor": "c2"}}));
    let page = rig.msgs();
    assert_eq!(
        (page[0]["method"].clone(), page[0]["params"].clone()),
        (json!("tools/list"), json!({"cursor": "c2"}))
    );
    rig.json(&json!({"jsonrpc": "2.0", "id": page[0]["id"], "result": {"tools": tools()}}));
    let read = rig.msgs();
    assert_eq!(read[0]["method"], "resources/read");
}

#[test]
fn what_cannot_be_answered_is_refused_with_a_code_and_the_token_is_spent_by_the_first() {
    let code = |rig: &mut Rig| -> String {
        let (wrote, _) = rig.take();
        let closed = wrote.last().expect("a closed");
        assert_eq!(closed.kind, "closed");
        serde_json::from_str::<Value>(&closed.payload).unwrap()["code"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    // screen
    let mut rig = Rig::new();
    let token = rig.hello();
    rig.host.observe(Observed {
        alternate_screen: Some(false),
        ..Observed::default()
    });
    rig.send("open", None, None, &open_payload(&token, None));
    assert_eq!(code(&mut rig), "screen");
    rig.send("open", None, None, &open_payload(&token, None));
    assert_eq!(code(&mut rig), "token");
    // policy
    let mut rig = Rig::new();
    let token = rig.hello();
    rig.host.allow_opens(false);
    rig.send("open", None, None, &open_payload(&token, None));
    assert_eq!(code(&mut rig), "policy");
    // unsupported: a bundle for a host without the feature
    let mut config = config();
    config.bundle = false;
    let mut rig = Rig::with(config);
    rig.send("hello", None, None, r#"{"versions":[1]}"#);
    let (wrote, _) = rig.take();
    assert!(!wrote[0].payload.contains("features"));
    rig.send(
        "open",
        None,
        None,
        &open_payload(TOKENS[0], Some(bundle(None))),
    );
    assert_eq!(code(&mut rig), "unsupported");
    // manifest, then limit, in that order
    let mut rig = Rig::new();
    let token = rig.hello();
    let mut bad = bundle(None);
    bad["entry"] = json!("missing");
    rig.send("open", None, None, &open_payload(&token, Some(bad)));
    assert_eq!(code(&mut rig), "manifest");
    let token = rig.hello();
    let mut big = bundle(None);
    big["files"][0]["size"] = json!(9 * 1024 * 1024);
    rig.send("open", None, None, &open_payload(&token, Some(big)));
    assert_eq!(code(&mut rig), "limit");
    // busy: a view is opening
    let token = rig.hello();
    rig.send(
        "open",
        None,
        None,
        &open_payload(&token, Some(bundle(None))),
    );
    rig.take();
    let token = rig.hello();
    rig.send("open", None, None, &open_payload(&token, None));
    assert_eq!(code(&mut rig), "busy");
    // the refusal did not disturb the open that is running
    assert_eq!(rig.host.state(), State::Opening);
    // not JSON: ignored, and the token stays
    let mut rig = Rig::new();
    let token = rig.hello();
    rig.send("open", None, None, "{not json");
    assert!(rig.take().0.is_empty());
    rig.send("open", None, None, &open_payload(&token, None));
    assert_eq!(rig.host.state(), State::Opening);
}

#[test]
fn a_hostile_stream_cannot_open_a_view_or_use_up_a_token() {
    let mut rig = Rig::new();
    let token = rig.hello();
    // A stream that wrote frames without reading the answer guesses.
    for guess in [
        "",
        "q7Vh2sB0xJm9kLw4TzN1cB",
        "AAAAAAAAAAAAAAAAAAAAAA",
        "q7Vh2sB0xJm9kLw4TzN1c",
    ] {
        rig.send("open", None, None, &open_payload(guess, None));
        let (wrote, _) = rig.take();
        assert_eq!(
            serde_json::from_str::<Value>(&wrote[0].payload).unwrap()["code"],
            "token"
        );
    }
    rig.send("open", None, None, r#"{"tool":{"name":"x"}}"#);
    rig.send("open", None, None, r#"{"token":7}"#);
    rig.send("open", None, None, "[]");
    assert_eq!(rig.take().0.len(), 3);
    assert_eq!(rig.host.state(), State::Offered);
    // The program that read its answer still can.
    rig.send("open", None, None, &open_payload(&token, None));
    assert_eq!(rig.host.state(), State::Opening);
}

#[test]
fn a_hello_gets_a_new_token_and_is_answered_only_when_the_pane_looks_as_a_waiting_program_leaves_it()
 {
    // Hello after hello: the first token is worthless.
    let mut rig = Rig::new();
    let first = rig.hello();
    let second = rig.hello();
    assert_ne!(first, second);
    rig.send("open", None, None, &open_payload(&first, None));
    assert_eq!(rig.host.state(), State::Offered);
    rig.take();
    // §6.2: a prompt is being edited, a shell holds the tty, the tty is cooked.
    for observed in [
        Observed {
            prompt_editing: true,
            ..Observed::default()
        },
        Observed {
            shell_in_foreground: Some(true),
            ..Observed::default()
        },
        Observed {
            tty_canonical_or_echo: Some(true),
            ..Observed::default()
        },
    ] {
        let mut rig = Rig::new();
        rig.host.observe(observed);
        rig.send("hello", None, None, r#"{"versions":[1]}"#);
        assert!(rig.take().0.is_empty(), "{observed:?}");
        assert_eq!(rig.host.state(), State::Idle);
        // Observed to be fine again, the same program is answered.
        rig.host.observe(Observed {
            alternate_screen: Some(true),
            shell_in_foreground: Some(false),
            tty_canonical_or_echo: Some(false),
            ..Observed::default()
        });
        rig.send("hello", None, None, r#"{"versions":[1]}"#);
        assert_eq!(rig.take().0.len(), 1);
    }
    // While a prompt is edited every frame is ignored, an open too.
    let mut rig = Rig::new();
    let token = rig.hello();
    rig.host.observe(Observed {
        prompt_editing: true,
        ..Observed::default()
    });
    rig.send("open", None, None, &open_payload(&token, None));
    assert!(rig.take().0.is_empty());
    assert_eq!(rig.host.state(), State::Offered);
    // A hello that shares no version is answered without a token.
    let mut rig = Rig::new();
    rig.send("hello", None, None, r#"{"versions":[2,3]}"#);
    let (wrote, _) = rig.take();
    assert_eq!(
        wrote[0].payload,
        r#"{"supported":[1],"host":{"name":"pencere-test","version":"0.0.0"}}"#
    );
    // A hello that is not one is ignored.
    for payload in ["{}", r#"{"versions":"1"}"#, r#"{"versions":[-1]}"#, "x"] {
        rig.send("hello", None, None, payload);
    }
    assert!(rig.take().0.is_empty());
}

#[test]
fn a_restart_is_an_unsolicited_welcome_with_a_new_token() {
    let mut rig = Rig::new();
    rig.hello();
    rig.take();
    rig.host.restart();
    let actions = rig.host.take_actions();
    assert_eq!(
        common::written(&actions),
        common::file("to-program/welcome-restart.frame")
    );
    // The program opens again with the new token.
    rig.send("open", None, None, &open_payload(TOKENS[1], None));
    assert_eq!(rig.host.state(), State::Opening);
    // A restart while a view runs closes it first, without telling the program it was closed.
    let mut rig = running(None);
    view_handshake(&mut rig, true);
    rig.take();
    rig.host.restart();
    let (wrote, _) = rig.take();
    assert!(wrote.is_empty());
    let id = {
        let told = rig.host.take_actions();
        assert!(told.is_empty());
        None::<u32>
    };
    let _ = id;
}

#[test]
fn a_restart_after_a_running_view_waits_for_the_teardown_and_then_offers_a_token() {
    let mut rig = running(None);
    view_handshake(&mut rig, true);
    rig.take();
    rig.host.restart();
    let (wrote, acted) = rig.take();
    assert!(wrote.is_empty());
    let teardown = acted
        .iter()
        .filter_map(|a| match a {
            Action::ToView(t) => Some(serde_json::from_str::<Value>(t).unwrap()),
            _ => None,
        })
        .find(|m| m["method"] == "ui/resource-teardown")
        .unwrap();
    assert_eq!(teardown["params"], json!({"reason": "restart"}));
    rig.view(&json!({"jsonrpc": "2.0", "id": teardown["id"], "result": {}}));
    let (wrote, acted) = rig.take();
    assert_eq!(acted, vec![Action::DestroyView]);
    assert_eq!(wrote.len(), 1);
    let body: Value = serde_json::from_str(&wrote[0].payload).unwrap();
    assert_eq!(body["restart"], true);
    assert_eq!(wrote[0].re(), None);
    assert_eq!(rig.host.state(), State::Offered);
    // The offer expires when the wait the host chose does.
    rig.host.timed_out();
    assert_eq!(rig.host.state(), State::Idle);
}

#[test]
fn links_open_on_the_person_s_machine_after_the_person_agrees() {
    let mut rig = running(None);
    view_handshake(&mut rig, true);
    rig.take();
    rig.view(&json!({"jsonrpc": "2.0", "id": 10, "method": "ui/open-link", "params": {"url": "https://example.com/a"}}));
    let (_, acted) = rig.take();
    let [Action::OpenLink { link, url }] = acted.as_slice() else {
        panic!("{acted:?}")
    };
    assert_eq!(url, "https://example.com/a");
    assert!(rig.viewed().is_empty());
    rig.host.link_decision(*link, true);
    assert_eq!(
        rig.viewed(),
        vec![json!({"jsonrpc": "2.0", "id": 10, "result": {}})]
    );
    rig.view(&json!({"jsonrpc": "2.0", "id": 11, "method": "ui/open-link", "params": {"url": "https://example.com/b"}}));
    let (_, acted) = rig.take();
    let [Action::OpenLink { link, .. }] = acted.as_slice() else {
        panic!("{acted:?}")
    };
    rig.host.link_decision(*link, false);
    assert_eq!(rig.viewed()[0]["error"]["code"], -32000);
    for url in [
        "file:///etc/passwd",
        "javascript:alert(1)",
        "ssh://host",
        "",
    ] {
        rig.view(
            &json!({"jsonrpc": "2.0", "id": 12, "method": "ui/open-link", "params": {"url": url}}),
        );
        let (_, acted) = rig.take();
        assert!(
            acted.iter().all(|a| !matches!(a, Action::OpenLink { .. })),
            "{url}"
        );
    }
    // An answer for a link that is not waiting does nothing.
    rig.host.link_decision(999, true);
    assert!(rig.viewed().is_empty());
}

#[test]
fn the_view_cannot_make_the_host_hold_unbounded_work() {
    let mut rig = running(None);
    view_handshake(&mut rig, true);
    rig.take();
    for n in 0..64 {
        rig.view(&json!({"jsonrpc": "2.0", "id": n, "method": "tools/call", "params": {"name": "stop_container"}}));
    }
    assert_eq!(rig.msgs().len(), 64);
    rig.view(&json!({"jsonrpc": "2.0", "id": 99, "method": "tools/call", "params": {"name": "stop_container"}}));
    assert!(rig.msgs().is_empty());
    assert_eq!(rig.viewed()[0]["error"]["code"], -32000);
    // A request too large to send to the program is refused, not truncated.
    let mut rig = running(None);
    view_handshake(&mut rig, true);
    rig.take();
    // 800 000 bytes of text from the view are 2 400 000 bytes of escapes on the wire.
    let big = "\u{e9}".repeat(400_000);
    rig.view(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "stop_container", "arguments": {"a": big}}}));
    assert!(rig.msgs().is_empty());
    assert_eq!(rig.viewed()[0]["error"]["code"], -32000);
    // A message over the limit is dropped before it is read.
    let huge = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"ping","params":"{}"}}"#,
        "x".repeat(1024 * 1024)
    );
    rig.host.view_message(&huge);
    assert!(rig.viewed().is_empty());
    assert!(rig.host.diagnostics().ignored >= 1);
}

#[test]
fn everything_the_host_writes_is_printable_ascii_even_when_the_host_is_named_with_other_characters()
{
    let mut host = host_info();
    host.name = "p\u{e9}ncere\nterm \u{7f}\u{1b}".into();
    let mut config = Config::new(
        host,
        common::theme(),
        Size {
            width: 1,
            height: 1,
        },
    );
    config.view.user_agent = "\u{2603}".into();
    let mut rig = Rig::with(config);
    rig.hello();
    let token = TOKENS[0];
    rig.send("open", None, None, &open_payload(token, None));
    let id = rig.msgs()[0]["id"].clone();
    // `rig.take` decoded the host's frames with the stream decoder, which would
    // have reported any raw byte a frame may not carry; check the bytes themselves too.
    rig.json(&json!({"jsonrpc": "2.0", "id": id, "result": {}}));
    let (wrote, _) = rig.take();
    for message in wrote {
        assert!(
            pencere::json::is_frame_text(&message.payload),
            "{message:?}"
        );
    }
}

#[test]
fn a_view_message_written_to_the_program_keeps_its_letters_as_escapes() {
    let mut rig = running(None);
    view_handshake(&mut rig, true);
    rig.take();
    rig.view(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "stop_container", "arguments": {"text": "caf\u{e9} \u{1f600}\n\u{7f}"}}}));
    let (wrote, _) = rig.take();
    let message = wrote.last().unwrap();
    assert!(
        pencere::json::is_frame_text(&message.payload),
        "{}",
        message.payload
    );
    let value: Value = serde_json::from_str(&message.payload).unwrap();
    assert_eq!(
        value["params"]["arguments"]["text"],
        "caf\u{e9} \u{1f600}\n\u{7f}"
    );
}

#[test]
fn messages_at_the_wrong_moment_or_from_the_wrong_side_are_ignored() {
    let mut rig = Rig::new();
    // Nothing is open: a file, a msg, a closed, a welcome, a need and a kind nobody knows.
    rig.send("file", None, Some(&Digest::of(b"x").to_string()), "eA==");
    rig.send("msg", None, None, r#"{"jsonrpc":"2.0","id":1,"result":{}}"#);
    rig.send("closed", None, None, "{}");
    rig.send("welcome", None, None, "{}");
    rig.send("need", None, None, "{}");
    rig.send("zzz", None, None, "{}");
    rig.send("file", None, None, "eA==");
    assert!(rig.take().0.is_empty());
    assert_eq!(rig.host.state(), State::Idle);
    assert!(rig.host.diagnostics().ignored >= 7);
    // A file nobody asked for, during an open that is asking for others, is not kept.
    let mut rig = Rig::new();
    let token = rig.hello();
    rig.send(
        "open",
        None,
        None,
        &open_payload(&token, Some(bundle(None))),
    );
    rig.take();
    rig.send(
        "file",
        None,
        Some(&Digest::of(b"surprise").to_string()),
        &base64::encode(b"surprise"),
    );
    assert!(rig.host.store().is_empty());
    assert_eq!(rig.host.state(), State::Opening);
}

#[test]
fn a_store_that_forgets_between_the_need_and_the_end_fails_the_open_rather_than_serve_a_hole() {
    #[derive(Default)]
    struct Forgetful(std::collections::HashSet<Digest>);
    impl Store for Forgetful {
        fn contains(&self, digest: &Digest) -> bool {
            self.0.contains(digest) && *digest != Digest::of(INDEX.as_bytes())
        }
        fn insert(&mut self, file: Verified) {
            self.0.insert(*file.digest());
        }
        fn get(&self, _: &Digest) -> Option<std::sync::Arc<[u8]>> {
            None
        }
    }
    let mut tokens = TOKENS
        .iter()
        .map(|t| common::token_bytes(t))
        .collect::<Vec<_>>()
        .into_iter();
    let mut host = Session::new(config(), Forgetful::default(), move || {
        tokens.next().unwrap_or([9; 16])
    });
    let mut ids = Ids::new();
    let mut send =
        |host: &mut Session<Forgetful, _>, kind: &str, d: Option<&str>, payload: &str| {
            let mut bytes = Vec::new();
            let message = Outgoing {
                kind,
                id: ids.next_id(),
                re: None,
                digest: d,
                payload,
            };
            encode(Direction::ToHost, &message, Split::Fill, &mut bytes).unwrap();
            host.feed(&bytes);
        };
    send(&mut host, "hello", None, r#"{"versions":[1]}"#);
    send(
        &mut host,
        "open",
        None,
        &open_payload(TOKENS[0], Some(bundle(None))),
    );
    for body in [INDEX, APP, STYLE] {
        let digest = Digest::of(body.as_bytes()).to_string();
        send(
            &mut host,
            "file",
            Some(&digest),
            &base64::encode(body.as_bytes()),
        );
    }
    assert_eq!(host.state(), State::Idle);
    let closed = common::written(&host.take_actions());
    assert!(String::from_utf8_lossy(&closed).contains(r#"{"reason":"error","code":"bundle"}"#));
}

#[test]
fn the_ids_of_what_the_host_writes_count_up_by_one_across_every_kind() {
    let mut rig = Rig::new();
    rig.send("hello", None, None, r#"{"versions":[1]}"#);
    rig.send(
        "open",
        None,
        None,
        &open_payload(TOKENS[0], Some(bundle(None))),
    );
    for body in [INDEX, APP, STYLE] {
        send_file(&mut rig, body);
    }
    let (wrote, _) = rig.take();
    let ids: Vec<u32> = wrote.iter().map(|m| m.id).collect();
    assert_eq!(ids, vec![1, 2, 3]);
    assert_eq!(
        wrote.iter().map(|m| m.kind.as_str()).collect::<Vec<_>>(),
        vec!["welcome", "need", "msg"]
    );
    assert_eq!(wrote[0].re(), Some(1));
    assert_eq!(wrote[1].re(), Some(2));
    assert_eq!(wrote[2].re(), None);
    let _ = Id::Int(0);
}

#[test]
fn a_restart_asked_for_while_a_view_closes_is_offered_when_it_has_closed() {
    let mut rig = running(None);
    view_handshake(&mut rig, true);
    rig.take();
    rig.host.close_view();
    rig.host.restart();
    rig.take();
    rig.host.timed_out();
    let (wrote, acted) = rig.take();
    assert_eq!(acted, vec![Action::DestroyView]);
    // The program is told the person closed the view, then offered a new token.
    assert_eq!(wrote.len(), 2);
    assert_eq!(wrote[0].kind, "closed");
    assert_eq!(wrote[1].kind, "welcome");
    assert_eq!(
        serde_json::from_str::<Value>(&wrote[1].payload).unwrap()["restart"],
        true
    );
    assert_eq!(rig.host.state(), State::Offered);
}

/// A small xorshift: the same choices on every run.
struct Dice(u64);

impl Dice {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

#[test]
fn a_session_survives_any_order_of_messages_and_only_ever_writes_frames_a_tty_may_carry() {
    let digests: Vec<String> = [INDEX, APP, STYLE]
        .iter()
        .map(|b| Digest::of(b.as_bytes()).to_string())
        .collect();
    let mut dice = Dice(0x2545_f491_4f6c_dd1d);
    let mut visited = std::collections::HashSet::new();
    for round in 0..400 {
        let mut rig = Rig::new();
        // The token of the last open the program wrote, for a `bye` that says whose it is.
        let mut opened = String::new();
        for _ in 0..80 {
            let token = TOKENS[dice.below(TOKENS.len())];
            match dice.below(26) {
                0 | 1 => {
                    rig.send("hello", None, None, r#"{"versions":[1]}"#);
                }
                2 | 3 => {
                    let with_bundle = dice.below(2) == 0;
                    let token = rig
                        .token
                        .clone()
                        .filter(|_| dice.below(5) != 0)
                        .unwrap_or_else(|| token.to_owned());
                    let payload = open_payload(&token, with_bundle.then(|| bundle(None)));
                    rig.send("open", None, None, &payload);
                    opened = token;
                }
                4..=6 => {
                    let n = dice.below(3);
                    let body = [INDEX, APP, STYLE][n];
                    let good = dice.below(4) != 0;
                    let payload = if good {
                        base64::encode(body.as_bytes())
                    } else {
                        "!!".to_owned()
                    };
                    rig.send("file", None, Some(&digests[n]), &payload);
                }
                7 => {
                    let payload = match dice.below(3) {
                        0 => "{}".to_owned(),
                        1 => bye(Some(token)),
                        _ => bye(Some(&opened)),
                    };
                    rig.send("bye", None, None, &payload);
                }
                8..=12 => {
                    // Answer the oldest request the host has out, the way a program does.
                    rig.collect();
                    if !rig.requests.is_empty() {
                        let (id, method) = rig.requests.remove(0);
                        let result = match method.as_str() {
                            "tools/list" => json!({"tools": tools()}),
                            "resources/read" => json!({"contents": [{
                                "uri": "ui://containers/view", "mimeType": "text/html;profile=mcp-app", "text": "<p>"}]}),
                            "tools/call" => json!({"content": []}),
                            _ => json!({}),
                        };
                        rig.json(&json!({"jsonrpc": "2.0", "id": id, "result": result}));
                    }
                }
                13 => {
                    let id = dice.below(6) as i64;
                    rig.json(&json!({"jsonrpc": "2.0", "id": id, "error": {"code": -1, "message": "no"}}));
                }
                14 => {
                    rig.json(&json!({"jsonrpc": "2.0", "id": 77, "method": "ping"}));
                }
                15 => {
                    rig.view(&json!({"jsonrpc": "2.0", "id": 1, "method": "ui/initialize", "params": {}}));
                }
                16 => {
                    rig.view(&json!({"jsonrpc": "2.0", "method": "ui/notifications/initialized"}));
                }
                17 => {
                    rig.view(&json!({"jsonrpc": "2.0", "id": dice.below(5), "method": "tools/call", "params": {"name": "stop_container"}}));
                }
                18 => {
                    rig.view(&json!({"jsonrpc": "2.0", "id": 3, "method": "ui/open-link", "params": {"url": "https://example.com"}}));
                }
                19 => {
                    let (_, acted) = rig.take();
                    for action in &acted {
                        match action {
                            Action::Consent(c) => rig.host.consent(c.grant_all()),
                            Action::ToView(text) => {
                                let value: Value = serde_json::from_str(text).unwrap();
                                if value["method"] == "ui/resource-teardown" {
                                    rig.view(
                                        &json!({"jsonrpc": "2.0", "id": value["id"], "result": {}}),
                                    );
                                }
                            }
                            Action::OpenLink { link, .. } => rig.host.link_decision(*link, true),
                            _ => {}
                        }
                    }
                }
                20 => rig.host.close_view(),
                21 => rig.host.program_ended(),
                22 => rig.host.restart(),
                23 => rig.host.timed_out(),
                24 => rig.host.requests_timed_out(),
                _ => {
                    rig.host.set_size(Size {
                        width: 1 + dice.below(500) as u32,
                        height: 9,
                    });
                    rig.host.view_message("not json at all");
                }
            }
            // Whatever the host wrote is frames, and only frames.
            rig.collect();
            for message in rig.wrote.drain(..) {
                assert!(
                    pencere::json::is_frame_text(&message.payload),
                    "round {round}: {message:?}"
                );
            }
            visited.insert(format!("{:?}", rig.host.state()));
        }
    }
    // The dice reached every state, so the survival is not vacuous.
    for state in ["Idle", "Offered", "Opening", "Open", "Closing", "Ended"] {
        assert!(visited.contains(state), "never in {state}: {visited:?}");
    }
}
