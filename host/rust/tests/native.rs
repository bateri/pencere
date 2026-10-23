//! A view that is a WebAssembly component or a Lua script (specification §8.8) against a host
//! session: what starts it, what is routed to and from it, and that nothing it says is written
//! to the program once its view is over.

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

fn config(views: &[ViewKind]) -> Config {
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
    config.views = views.to_vec();
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

    /// The `hello` of a program that can show either kind and prefers a component, and the
    /// token the host answers with.
    fn hello(&mut self) -> Value {
        self.hello_with(&["wasm", "web"])
    }

    /// A `hello` that lists `views`; the `welcome` it is answered with.
    fn hello_with(&mut self, views: &[&str]) -> Value {
        self.json("hello", &json!({ "versions": [1], "views": views }));
        let welcome = self.take();
        assert_eq!(welcome.len(), 1);
        assert_eq!(welcome[0].kind, "welcome");
        serde_json::from_str(&welcome[0].payload).unwrap()
    }

    /// An `open` of a component of `size` bytes named in the manifest, with `extra` members.
    fn open(&mut self, size: u64, extra: Value) {
        let digest = Digest::of(COMPONENT).to_string();
        let mut body = json!({
            "token": TOKEN,
            "view": "wasm",
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
    let mut rig = Rig::new(config(&[ViewKind::Wasm, ViewKind::Web]));
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
    let mut rig = Rig::new(config(&[ViewKind::Wasm, ViewKind::Web]));
    let start = rig.up(json!({ "program": "de\u{202e}mo\npen", "title": 7 }));
    assert_eq!(start.program.as_deref(), Some("demopen"));
    assert_eq!(start.title, None);
}

#[test]
fn a_tool_in_the_open_of_a_component_is_not_read() {
    let mut rig = Rig::new(config(&[ViewKind::Wasm, ViewKind::Web]));
    let start = rig.up(json!({ "tool": 3 }));
    assert_eq!(start.kind, ViewKind::Wasm);
    assert!(start.tool.is_empty());
}

#[test]
fn a_msg_of_the_program_reaches_the_view_as_it_is_and_is_not_read() {
    let mut rig = Rig::new(config(&[ViewKind::Wasm, ViewKind::Web]));
    rig.up(json!({}));
    let text = pencere::json::to_wire(&json!({ "push": { "id": 2, "value": [1.5, "caf\u{e9}"] } }))
        .unwrap();
    rig.send("msg", None, &text);
    assert_eq!(rig.taken_actions(), vec![Action::ToView(text)]);
    assert!(rig.take().is_empty());
}

#[test]
fn a_msg_that_is_not_an_object_or_comes_before_the_view_runs_goes_nowhere() {
    let mut rig = Rig::new(config(&[ViewKind::Wasm, ViewKind::Web]));
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
    let mut rig = Rig::new(config(&[ViewKind::Wasm, ViewKind::Web]));
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
    let mut rig = Rig::new(config(&[ViewKind::Wasm, ViewKind::Web]));
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
            rig.host.view_failed("the page trapped");
            rig.collect();
        }),
    ];
    for (name, end) in endings {
        let mut rig = Rig::new(config(&[ViewKind::Wasm, ViewKind::Web]));
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
    let mut rig = Rig::new(config(&[ViewKind::Wasm, ViewKind::Web]));
    rig.up(json!({}));
    rig.host
        .view_failed("script:12: attempt to index nil with 'x'");
    rig.collect();
    let wrote = rig.take();
    assert_eq!(wrote.len(), 1);
    assert_eq!(wrote[0].kind, "closed");
    let body: Value = serde_json::from_str(&wrote[0].payload).unwrap();
    assert_eq!(
        body,
        json!({ "reason": "error", "code": "view", "message": "script:12: attempt to index nil with 'x'" })
    );

    let mut rig = Rig::new(config(&[ViewKind::Wasm, ViewKind::Web]));
    rig.up(json!({}));
    rig.host.close_view();
    rig.collect();
    let body: Value = serde_json::from_str(&rig.take()[0].payload).unwrap();
    assert_eq!(body, json!({ "reason": "user" }));
}

#[test]
fn a_host_that_does_not_show_components_leaves_them_out_and_refuses_one() {
    let mut rig = Rig::new(config(&[ViewKind::Web]));
    assert_eq!(rig.hello()["views"], json!(["web"]));
    rig.open(COMPONENT.len() as u64, json!({}));
    assert_eq!(refusal(&mut rig), "unsupported");
    assert_eq!(rig.host.state(), State::Idle);
}

#[test]
fn a_component_over_the_hosts_limit_is_refused_before_it_is_asked_for() {
    let mut config = config(&[ViewKind::Wasm, ViewKind::Web]);
    config.native_entry_limit = 4;
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
    let mut rig = Rig::new(config(&[ViewKind::Wasm, ViewKind::Web]));
    rig.up(json!({}));
    rig.host
        .view_message(r#"{"jsonrpc":"2.0","id":1,"method":"ui/initialize","params":{}}"#);
    rig.collect();
    assert!(rig.taken_actions().is_empty());
    assert!(rig.take().is_empty());
}

#[test]
fn a_restart_ends_the_view_and_offers_a_new_token_without_a_word_to_the_component() {
    let mut rig = Rig::new(config(&[ViewKind::Wasm, ViewKind::Web]));
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
    let mut rig = Rig::new(config(&[ViewKind::Wasm, ViewKind::Web]));
    rig.hello();
    rig.json("open", &json!({ "token": TOKEN, "view": "web" }));
    assert_eq!(refusal(&mut rig), "manifest");
}

/// The code of the one `closed` the host wrote.
fn refusal(rig: &mut Rig) -> String {
    let wrote = rig.take();
    assert_eq!(wrote.len(), 1, "{wrote:?}");
    assert_eq!(wrote[0].kind, "closed");
    let body: Value = serde_json::from_str(&wrote[0].payload).unwrap();
    body["code"].as_str().unwrap().to_owned()
}

/// A bundle of one file of `media_type`, named `entry`.
fn one_file_bundle(entry: &str, media_type: &str) -> Value {
    let digest = Digest::of(COMPONENT).to_string();
    json!({ "entry": entry, "files": [
        { "path": entry, "sha256": digest, "size": COMPONENT.len(), "type": media_type }
    ] })
}

#[test]
fn the_welcome_lists_the_kinds_the_host_opens_among_the_ones_the_program_listed_in_its_order() {
    let mut both = Rig::new(config(&[ViewKind::Wasm, ViewKind::Web]));
    assert_eq!(
        both.hello_with(&["web", "wasm"])["views"],
        json!(["web", "wasm"])
    );
    let mut both = Rig::new(config(&[ViewKind::Wasm, ViewKind::Web]));
    assert_eq!(
        both.hello_with(&["wasm", "web"])["views"],
        json!(["wasm", "web"])
    );
    // The host only eliminates: a kind it does not open, one it has never heard of and one
    // listed twice leave the rest as the program had them.
    let mut web = Rig::new(config(&[ViewKind::Web]));
    let welcome = web.hello_with(&["lua", "wasm", "web", "wasm", "web"]);
    assert_eq!(welcome["views"], json!(["web"]));
    assert_eq!(welcome["features"], json!(["bundle", "dismiss"]));
}

#[test]
fn a_host_that_opens_nothing_the_program_listed_still_answers_with_an_empty_list() {
    let mut web = Rig::new(config(&[ViewKind::Web]));
    let welcome = web.hello_with(&["wasm"]);
    assert_eq!(welcome["views"], json!([]));
    assert_eq!(welcome["token"], TOKEN);
    // Nothing can be opened with the token it carries.
    web.json(
        "open",
        &json!({ "token": TOKEN, "view": "wasm", "bundle": one_file_bundle("view.wasm", "application/wasm") }),
    );
    assert_eq!(refusal(&mut web), "unsupported");
}

#[test]
fn a_host_without_bundles_does_not_offer_a_component() {
    let mut config = config(&[ViewKind::Wasm, ViewKind::Web]);
    config.bundle = false;
    let mut rig = Rig::new(config);
    assert_eq!(rig.hello_with(&["wasm", "web"])["views"], json!(["web"]));
}

#[test]
fn a_hello_that_lists_no_kind_of_view_is_not_answered() {
    for hello in [
        json!({ "versions": [1] }),
        json!({ "versions": [1], "views": [] }),
        json!({ "versions": [1], "views": "wasm" }),
        json!({ "versions": [1], "views": [1] }),
    ] {
        let mut rig = Rig::new(config(&[ViewKind::Wasm, ViewKind::Web]));
        rig.json("hello", &hello);
        assert!(rig.take().is_empty(), "{hello}");
        assert_eq!(rig.host.state(), State::Idle);
    }
}

#[test]
fn an_open_has_to_name_a_kind_the_welcome_offered() {
    // Missing, not a string: the payload is malformed.
    for view in [None, Some(json!(7)), Some(json!(null))] {
        let mut rig = Rig::new(config(&[ViewKind::Wasm, ViewKind::Web]));
        rig.hello();
        let mut body =
            json!({ "token": TOKEN, "bundle": one_file_bundle("view.wasm", "application/wasm") });
        if let Some(view) = view {
            body["view"] = view;
        }
        rig.json("open", &body);
        assert_eq!(refusal(&mut rig), "manifest", "{body}");
    }
    // A name outside the table, and a kind the welcome did not offer: unsupported.
    let component = one_file_bundle("view.wasm", "application/wasm");
    for (listed, open) in [
        (
            &["wasm", "web"][..],
            json!({ "view": "gopher", "bundle": component }),
        ),
        (&["wasm"], json!({ "view": "web", "tool": { "name": "t" } })),
        (&["web"], json!({ "view": "wasm", "bundle": component })),
    ] {
        let mut rig = Rig::new(config(&[ViewKind::Wasm, ViewKind::Web]));
        rig.hello_with(listed);
        let mut body = open.clone();
        body["token"] = json!(TOKEN);
        rig.json("open", &body);
        assert_eq!(refusal(&mut rig), "unsupported", "{listed:?} {open}");
        assert!(rig.taken_actions().is_empty());
    }
}

#[test]
fn a_bundle_whose_entry_is_not_what_the_view_says_is_refused_and_the_host_infers_nothing() {
    let wasm_in_html = one_file_bundle("index.html", "text/html");
    let html_in_wasm = one_file_bundle("view.wasm", "application/wasm");
    for (view, bundle) in [
        ("wasm", Some(wasm_in_html.clone())),
        ("wasm", None),
        ("web", Some(html_in_wasm.clone())),
    ] {
        let mut rig = Rig::new(config(&[ViewKind::Wasm, ViewKind::Web]));
        rig.hello();
        let mut body = json!({ "token": TOKEN, "view": view, "tool": { "name": "t" } });
        if let Some(bundle) = bundle {
            body["bundle"] = bundle;
        }
        rig.json("open", &body);
        assert_eq!(refusal(&mut rig), "manifest", "{body}");
        assert_eq!(rig.host.state(), State::Idle);
    }
    // The token is spent whatever comes of the open, as before.
    let mut rig = Rig::new(config(&[ViewKind::Wasm, ViewKind::Web]));
    rig.hello();
    rig.json("open", &json!({ "token": TOKEN, "view": "wasm" }));
    assert_eq!(refusal(&mut rig), "manifest");
    rig.json("open", &json!({ "token": TOKEN, "view": "wasm" }));
    assert_eq!(refusal(&mut rig), "token");
}

#[test]
fn a_restart_offers_every_kind_the_host_opens_in_its_own_order() {
    let mut rig = Rig::new(config(&[ViewKind::Wasm, ViewKind::Web]));
    rig.up(json!({}));
    rig.host.restart();
    rig.collect();
    let welcome: Value = serde_json::from_str(&rig.take()[0].payload).unwrap();
    assert_eq!(welcome["views"], json!(["wasm", "web"]));
    // A program opens what it likes of them with the new token.
    rig.json(
        "open",
        &json!({ "token": OTHER, "view": "wasm", "bundle": one_file_bundle("view.wasm", "application/wasm") }),
    );
    assert!(rig.take().iter().all(|m| m.kind != "closed"));
}

// --- A script ------------------------------------------------------------------------------------

/// What a script is, for the tests.
const SCRIPT: &[u8] = b"return { render = function() end }";

impl Rig {
    /// An `open` of a script named in the manifest, with `extra` members.
    fn open_script(&mut self, extra: Value) {
        let digest = Digest::of(SCRIPT).to_string();
        let mut body = json!({
            "token": TOKEN,
            "view": "lua",
            "bundle": { "entry": "view.lua", "files": [
                { "path": "view.lua", "sha256": digest, "size": SCRIPT.len(), "type": "text/x-lua" }
            ] },
        });
        for (key, value) in extra.as_object().unwrap() {
            body[key] = value.clone();
        }
        self.json("open", &body);
    }

    /// A script view that is up: `hello`, `open`, its file.
    fn script_up(&mut self) -> ViewStart {
        self.hello_with(&["lua", "web"]);
        self.open_script(json!({ "program": "demopen" }));
        let need = self.take();
        assert_eq!(need.len(), 1, "{need:?}");
        assert_eq!(need[0].kind, "need");
        let digest = Digest::of(SCRIPT).to_string();
        self.send("file", Some(&digest), &pencere::base64::encode(SCRIPT));
        let acted = self.taken_actions();
        let [Action::StartView(start)] = acted.as_slice() else {
            panic!("expected one view to start, got {acted:?}");
        };
        (**start).clone()
    }
}

#[test]
fn a_view_that_is_a_script_starts_like_a_component_and_is_served_as_text() {
    let mut rig = Rig::new(config(&[ViewKind::Wasm, ViewKind::Lua, ViewKind::Web]));
    let start = rig.script_up();
    assert_eq!(start.kind, ViewKind::Lua);
    assert!(ViewKind::Lua.is_native() && ViewKind::Wasm.is_native() && !ViewKind::Web.is_native());
    assert_eq!(start.program.as_deref(), Some("demopen"));
    assert!(start.tool.is_empty() && start.policy.is_empty());
    assert_eq!(rig.host.state(), State::Open);
    // No MCP session: nothing but the `need`.
    assert!(rig.take().is_empty());
    let (media, bytes) = rig.host.serve("view.lua").unwrap();
    assert_eq!(media, "text/x-lua");
    assert_eq!(&*bytes, SCRIPT);
}

#[test]
fn a_script_gets_the_messages_of_a_component_and_nothing_it_says_is_written_once_it_is_over() {
    let mut rig = Rig::new(config(&[ViewKind::Wasm, ViewKind::Lua, ViewKind::Web]));
    rig.script_up();
    let text = pencere::json::to_wire(&json!({ "push": { "id": 1, "value": 2 } })).unwrap();
    rig.send("msg", None, &text);
    assert_eq!(rig.taken_actions(), vec![Action::ToView(text)]);
    let said = r#"{"call":{"id":1,"method":"list"}}"#;
    assert!(rig.module_says(said));
    assert_eq!(rig.take()[0].payload, said);
    rig.host.view_failed("script:3: boom");
    rig.collect();
    rig.take();
    assert_wrote_nothing(&mut rig, said);
}

#[test]
fn a_script_view_is_not_a_component_and_a_component_is_not_a_script() {
    // The entry has to be of the kind the program said.
    let mut rig = Rig::new(config(&[ViewKind::Wasm, ViewKind::Lua, ViewKind::Web]));
    rig.hello_with(&["lua", "wasm"]);
    rig.open(COMPONENT.len() as u64, json!({ "view": "lua" }));
    assert_eq!(refusal(&mut rig), "manifest");
    let mut rig = Rig::new(config(&[ViewKind::Wasm, ViewKind::Lua, ViewKind::Web]));
    rig.hello_with(&["lua", "wasm"]);
    let digest = Digest::of(SCRIPT).to_string();
    rig.json(
        "open",
        &json!({ "token": TOKEN, "view": "wasm", "bundle": { "entry": "view.lua", "files": [
            { "path": "view.lua", "sha256": digest, "size": SCRIPT.len(), "type": "text/x-lua" }
        ] } }),
    );
    assert_eq!(refusal(&mut rig), "manifest");
    // A script has no document to stand on without its bundle either.
    let mut rig = Rig::new(config(&[ViewKind::Wasm, ViewKind::Lua, ViewKind::Web]));
    rig.hello_with(&["lua"]);
    rig.json("open", &json!({ "token": TOKEN, "view": "lua" }));
    assert_eq!(refusal(&mut rig), "manifest");
}

#[test]
fn a_host_that_does_not_run_scripts_leaves_them_out_and_refuses_one() {
    let mut rig = Rig::new(config(&[ViewKind::Wasm, ViewKind::Web]));
    assert_eq!(rig.hello_with(&["lua", "web"])["views"], json!(["web"]));
    rig.open_script(json!({}));
    assert_eq!(refusal(&mut rig), "unsupported");
    // And one that runs only scripts offers only them.
    let mut rig = Rig::new(config(&[ViewKind::Lua]));
    assert_eq!(rig.hello_with(&["wasm", "lua"])["views"], json!(["lua"]));
}

#[test]
fn a_script_over_the_hosts_limit_is_refused_before_it_is_asked_for() {
    let mut config = config(&[ViewKind::Lua]);
    config.native_entry_limit = 4;
    let mut rig = Rig::new(config);
    rig.hello_with(&["lua"]);
    rig.open_script(json!({}));
    let wrote = rig.take();
    let body: Value = serde_json::from_str(&wrote[0].payload).unwrap();
    assert_eq!(body["code"], "limit");
}

#[test]
fn the_words_a_view_failed_with_are_cut_and_cleaned_before_the_program_is_told_them() {
    let mut rig = Rig::new(config(&[ViewKind::Lua]));
    rig.script_up();
    let long = "x".repeat(2000);
    rig.host
        .view_failed(&format!("\u{1b}[31mscript:1:\u{202e} {long}\n"));
    rig.collect();
    let wrote = rig.take();
    let body: Value = serde_json::from_str(&wrote[0].payload).unwrap();
    let message = body["message"].as_str().unwrap();
    // No escape, no override, no newline; at most the most the protocol allows; blanks trimmed.
    assert!(message.starts_with("[31mscript:1: xxx"), "{message}");
    assert_eq!(message.chars().count(), pencere::session::MAX_FAILURE_CHARS);
    // A view with nothing to say says nothing.
    let mut rig = Rig::new(config(&[ViewKind::Lua]));
    rig.script_up();
    rig.host.view_failed(" \n ");
    rig.collect();
    let body: Value = serde_json::from_str(&rig.take()[0].payload).unwrap();
    assert_eq!(body, json!({ "reason": "error", "code": "view" }));
}
