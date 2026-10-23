//! A view that is a WebAssembly component (specification §8.8) against a host session: what
//! starts it, what is routed to and from it, and that nothing it says is written to the
//! program once its view is over.

// A test fails by panicking, and indexes what it has just built.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use pencere::bundle::{Digest, MemoryStore};
use pencere::frame::{Direction, Event, Ids, Message, Outgoing, Split, StreamDecoder, encode};
use pencere::message::HostInfo;
use pencere::session::{Action, Config, Session, Size, State, ViewKind, ViewSource, ViewStart};
use serde_json::{Value, json};

type Host = Session<MemoryStore, Box<dyn FnMut() -> [u8; 16]>>;

/// What ends a view, played against the rig.
type End = fn(&mut Rig);

const TOKEN: &str = "q7Vh2sB0xJm9kLw4TzN1cA";
const OTHER: &str = "Zk3mP8rD5uYb1Xe6HqS0vg";

/// What a component's bytes are, for the tests: the header of an empty one.
const COMPONENT: &[u8] = b"\0asm\x01\0\0\0";

struct Rig {
    host: Host,
    ids: Ids,
    decoder: StreamDecoder,
    /// What the host wrote, as messages, since the last `take`.
    wrote: Vec<Message>,
    /// The other actions since the last `take`.
    acted: Vec<Action>,
}

fn config(wasm: bool) -> Config {
    let mut config = Config::new(
        HostInfo {
            name: "pencere-test".into(),
            version: "0.0.0".into(),
        },
        common::theme(),
        Size {
            width: 800,
            height: 600,
        },
    );
    config.wasm = wasm;
    config
}

impl Rig {
    fn new(config: Config) -> Self {
        let mut tokens = [TOKEN, OTHER]
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
        }
    }

    fn collect(&mut self) {
        for action in self.host.take_actions() {
            if let Action::Write(bytes) = &action {
                let mut events = Vec::new();
                self.decoder.feed(bytes, &mut events);
                for event in events {
                    match event {
                        Event::Message(m) => self.wrote.push(m),
                        Event::Error(e) => panic!("the host wrote a bad frame: {e}"),
                    }
                }
            } else {
                self.acted.push(action);
            }
        }
    }

    fn send(&mut self, kind: &str, digest: Option<&str>, payload: &str) {
        let message = Outgoing {
            kind,
            id: self.ids.next_id(),
            re: None,
            digest,
            payload,
        };
        let mut bytes = Vec::new();
        encode(Direction::ToHost, &message, Split::Fill, &mut bytes).unwrap();
        self.host.feed(&bytes);
        self.collect();
    }

    fn json(&mut self, kind: &str, value: &Value) {
        let text = pencere::json::to_wire(value).unwrap();
        self.send(kind, None, &text);
    }

    /// What the host wrote since the last call.
    fn take(&mut self) -> Vec<Message> {
        std::mem::take(&mut self.wrote)
    }

    fn taken_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.acted)
    }

    /// The `hello`, and the token the host answers with.
    fn hello(&mut self) {
        self.json("hello", &json!({ "versions": [1] }));
        let welcome = self.take();
        assert_eq!(welcome.len(), 1);
        assert_eq!(welcome[0].kind, "welcome");
    }

    /// An `open` of a component of `size` bytes named in the manifest, with `extra` members.
    fn open(&mut self, size: u64, extra: Value) {
        let digest = Digest::of(COMPONENT).to_string();
        let mut body = json!({
            "token": TOKEN,
            "bundle": { "entry": "view.wasm", "files": [
                { "path": "view.wasm", "sha256": digest, "size": size, "type": "application/wasm" }
            ] },
        });
        for (key, value) in extra.as_object().unwrap() {
            body[key] = value.clone();
        }
        self.json("open", &body);
    }

    /// The component's file, as the program sends it.
    fn file(&mut self) {
        let digest = Digest::of(COMPONENT).to_string();
        let payload = pencere::base64::encode(COMPONENT);
        self.send("file", Some(&digest), &payload);
    }

    /// A view that is up: `hello`, `open`, its file.
    fn up(&mut self, extra: Value) -> ViewStart {
        self.hello();
        self.open(COMPONENT.len() as u64, extra);
        let need = self.take();
        assert_eq!(need.len(), 1, "{need:?}");
        assert_eq!(need[0].kind, "need");
        self.file();
        let acted = self.taken_actions();
        let [Action::StartView(start)] = acted.as_slice() else {
            panic!("expected one view to start, got {acted:?}");
        };
        assert_eq!(self.host.state(), State::Open);
        (**start).clone()
    }

    fn module_says(&mut self, text: &str) -> bool {
        let written = self.host.module_message(text);
        self.collect();
        written
    }
}

fn assert_wrote_nothing(rig: &mut Rig, text: &str) {
    let before = rig.host.diagnostics().ignored;
    assert!(!rig.module_says(text));
    assert!(rig.take().is_empty(), "the program was written to");
    assert!(rig.host.diagnostics().ignored > before);
}

#[test]
fn a_view_that_is_a_component_starts_without_mcp_and_says_what_it_is() {
    let mut rig = Rig::new(config(true));
    let start = rig.up(json!({ "program": "demopen", "title": "Containers" }));
    assert_eq!(start.kind, ViewKind::Wasm);
    assert_eq!(start.source, ViewSource::Bundle);
    assert_eq!(start.program.as_deref(), Some("demopen"));
    assert_eq!(start.title.as_deref(), Some("Containers"));
    assert!(start.policy.is_empty() && start.allow.is_empty());
    // No MCP session was started: nothing but the `need` was written.
    assert!(rig.take().is_empty());
    // The component is served from the store, like any bundle.
    let (media, bytes) = rig.host.serve("view.wasm").unwrap();
    assert_eq!(media, "application/wasm");
    assert_eq!(&*bytes, COMPONENT);
}

#[test]
fn what_the_program_calls_itself_is_cut_and_cleaned_like_any_label() {
    let mut rig = Rig::new(config(true));
    let start = rig.up(json!({ "program": "de\u{202e}mo\npen", "title": 7 }));
    assert_eq!(start.program.as_deref(), Some("demopen"));
    assert_eq!(start.title, None);
}

#[test]
fn a_tool_in_the_open_of_a_component_is_not_read() {
    let mut rig = Rig::new(config(true));
    let start = rig.up(json!({ "tool": 3 }));
    assert_eq!(start.kind, ViewKind::Wasm);
    assert!(start.tool.is_empty());
}

#[test]
fn a_msg_of_the_program_reaches_the_view_as_it_is_and_is_not_read() {
    let mut rig = Rig::new(config(true));
    rig.up(json!({}));
    let text = pencere::json::to_wire(&json!({ "push": { "id": 2, "value": [1.5, "caf\u{e9}"] } }))
        .unwrap();
    rig.send("msg", None, &text);
    assert_eq!(rig.taken_actions(), vec![Action::ToView(text)]);
    assert!(rig.take().is_empty());
}

#[test]
fn a_msg_that_is_not_an_object_or_comes_before_the_view_runs_goes_nowhere() {
    let mut rig = Rig::new(config(true));
    rig.hello();
    rig.open(COMPONENT.len() as u64, json!({}));
    rig.take();
    // The file is still awaited: the component is not up.
    rig.json("msg", &json!({ "push": { "id": 1 } }));
    assert!(rig.taken_actions().is_empty());
    rig.file();
    rig.taken_actions();
    rig.send("msg", None, "[1,2]");
    rig.send("msg", None, "\"text\"");
    assert!(rig.taken_actions().is_empty());
    assert_eq!(rig.host.diagnostics().ignored, 3);
}

#[test]
fn a_message_of_the_component_is_written_to_the_program_while_its_view_is_open() {
    let mut rig = Rig::new(config(true));
    rig.up(json!({}));
    let text = r#"{"call":{"id":1,"method":"list"}}"#;
    assert!(rig.module_says(text));
    let wrote = rig.take();
    assert_eq!(wrote.len(), 1);
    assert_eq!(wrote[0].kind, "msg");
    assert_eq!(wrote[0].payload, text);
    // Not an object: not written.
    assert_wrote_nothing(&mut rig, "[1]");
    // Over what one message may be: not written.
    let big = format!("{{\"pad\":\"{}\"}}", "a".repeat(1024 * 1024));
    assert_wrote_nothing(&mut rig, &big);
}

#[test]
fn nothing_the_component_says_is_written_once_its_view_is_over_however_it_ended() {
    let text = r#"{"call":{"id":1,"method":"list"}}"#;
    // Before any view.
    let mut rig = Rig::new(config(true));
    rig.hello();
    assert_wrote_nothing(&mut rig, text);

    let endings: [(&str, End); 5] = [
        ("the program's bye", |rig| {
            rig.json("bye", &json!({ "token": TOKEN, "reason": "exit" }))
        }),
        ("the program's dismiss", |rig| {
            rig.json("dismiss", &json!({ "token": TOKEN }))
        }),
        ("the person closing the view", |rig| {
            rig.host.close_view();
            rig.collect();
        }),
        ("the program ending", |rig| {
            rig.host.program_ended();
            rig.collect();
        }),
        ("the view failing", |rig| {
            rig.host.view_failed();
            rig.collect();
        }),
    ];
    for (name, end) in endings {
        let mut rig = Rig::new(config(true));
        rig.up(json!({}));
        assert!(rig.module_says(text), "{name}: open");
        rig.take();
        end(&mut rig);
        let acted = rig.taken_actions();
        assert!(
            acted.contains(&Action::DestroyView),
            "{name}: the view is destroyed, got {acted:?}"
        );
        rig.take();
        assert_wrote_nothing(&mut rig, text);
        // And not after the program has said hello again either: a view that is gone is gone.
        if name != "the program ending" {
            rig.hello();
            assert_wrote_nothing(&mut rig, text);
        }
    }
}

#[test]
fn the_end_of_a_view_tells_the_program_why() {
    let mut rig = Rig::new(config(true));
    rig.up(json!({}));
    rig.host.view_failed();
    rig.collect();
    let wrote = rig.take();
    assert_eq!(wrote.len(), 1);
    assert_eq!(wrote[0].kind, "closed");
    let body: Value = serde_json::from_str(&wrote[0].payload).unwrap();
    assert_eq!(body, json!({ "reason": "error", "code": "view" }));

    let mut rig = Rig::new(config(true));
    rig.up(json!({}));
    rig.host.close_view();
    rig.collect();
    let body: Value = serde_json::from_str(&rig.take()[0].payload).unwrap();
    assert_eq!(body, json!({ "reason": "user" }));
}

#[test]
fn a_host_that_does_not_show_components_says_so_and_refuses_one() {
    let mut rig = Rig::new(config(false));
    rig.hello();
    rig.open(COMPONENT.len() as u64, json!({}));
    let wrote = rig.take();
    assert_eq!(wrote.len(), 1);
    assert_eq!(wrote[0].kind, "closed");
    let body: Value = serde_json::from_str(&wrote[0].payload).unwrap();
    assert_eq!(body["code"], "unsupported");
    assert_eq!(rig.host.state(), State::Idle);
}

#[test]
fn a_component_over_the_hosts_limit_is_refused_before_it_is_asked_for() {
    let mut config = config(true);
    config.wasm_entry_limit = 4;
    let mut rig = Rig::new(config);
    rig.hello();
    rig.open(COMPONENT.len() as u64, json!({}));
    let wrote = rig.take();
    assert_eq!(wrote.len(), 1);
    let body: Value = serde_json::from_str(&wrote[0].payload).unwrap();
    assert_eq!(body["code"], "limit");
    assert!(rig.taken_actions().is_empty());
}

#[test]
fn the_view_s_own_mcp_apps_messages_are_not_a_component_s_to_send() {
    let mut rig = Rig::new(config(true));
    rig.up(json!({}));
    rig.host
        .view_message(r#"{"jsonrpc":"2.0","id":1,"method":"ui/initialize","params":{}}"#);
    rig.collect();
    assert!(rig.taken_actions().is_empty());
    assert!(rig.take().is_empty());
}

#[test]
fn a_restart_ends_the_view_and_offers_a_new_token_without_a_word_to_the_component() {
    let mut rig = Rig::new(config(true));
    rig.up(json!({}));
    rig.host.restart();
    rig.collect();
    let acted = rig.taken_actions();
    assert!(acted.contains(&Action::DestroyView), "{acted:?}");
    let wrote = rig.take();
    assert_eq!(wrote.len(), 1);
    assert_eq!(wrote[0].kind, "welcome");
    let body: Value = serde_json::from_str(&wrote[0].payload).unwrap();
    assert_eq!(body["restart"], true);
    assert_eq!(body["token"], OTHER);
}

#[test]
fn a_web_open_still_needs_its_tool() {
    let mut rig = Rig::new(config(true));
    rig.hello();
    rig.json("open", &json!({ "token": TOKEN }));
    let wrote = rig.take();
    assert_eq!(wrote.len(), 1);
    let body: Value = serde_json::from_str(&wrote[0].payload).unwrap();
    assert_eq!(body["code"], "manifest");
}
