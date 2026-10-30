//! Reading the fixtures of the specification: the one truth the Rust crate and
//! the TypeScript SDK are both tested from.

#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use pencere_host::base64;
use pencere_host::bundle::{Digest, MemoryStore, Store, Verified};
use pencere_host::frame::{Direction, Event, Ids, Message, Outgoing, Split, StreamDecoder, encode};
use pencere_host::message::HostInfo;
use pencere_host::session::{Action, Config, Offer, Session, Size, ViewKind, ViewStart};
use pencere_host::theme::{Rgb, Theme};
use serde_json::{Value, json};

/// The directory of the draft's fixtures.
pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../spec/draft/fixtures")
}

/// The bytes of a fixture file, by its path in `manifest.json`.
pub fn file(path: &str) -> Vec<u8> {
    let full = root().join(path);
    std::fs::read(&full).unwrap_or_else(|e| panic!("{}: {e}", full.display()))
}

/// `manifest.json`.
pub fn manifest() -> Value {
    serde_json::from_slice(&file("manifest.json")).unwrap()
}

/// The 16 bytes a base64url token stands for.
pub fn token_bytes(token: &str) -> [u8; 16] {
    let standard = format!("{}==", token.replace('-', "+").replace('_', "/"));
    let bytes = base64::decode(standard.as_bytes()).unwrap();
    bytes.try_into().unwrap()
}

/// A store that holds the digests a session fixture's `store` says the host already has, without
/// their bytes, and keeps what it verifies with them: a file a program attached comes out of it
/// again when it is attached again (specification §8.8).
#[derive(Default)]
pub struct Held(pub HashSet<Digest>, pub HashMap<Digest, Arc<[u8]>>);

impl Store for Held {
    fn contains(&self, digest: &Digest) -> bool {
        self.0.contains(digest) || self.1.contains_key(digest)
    }
    fn insert(&mut self, file: Verified) {
        let (digest, bytes) = file.into_parts();
        self.1.insert(digest, Arc::from(bytes));
    }
    fn get(&self, digest: &Digest) -> Option<Arc<[u8]>> {
        self.1.get(digest).cloned()
    }
}

/// An entropy source that yields the tokens a fixture issues, in order.
pub fn issuing(tokens: Vec<[u8; 16]>) -> impl FnMut() -> [u8; 16] + use<> {
    let mut next = tokens.into_iter();
    move || next.next().unwrap_or([0xee; 16])
}

pub fn theme() -> Theme {
    Theme::new(Rgb(0, 0, 0), Rgb(0xee, 0xee, 0xee), "\"Menlo\", monospace")
}

pub fn config(host: &Value) -> Config {
    let host = HostInfo {
        name: host["name"].as_str().unwrap().to_owned(),
        version: host["version"].as_str().unwrap().to_owned(),
    };
    Config::new(
        host,
        theme(),
        Size {
            width: 800,
            height: 600,
        },
    )
}

/// A host's offer of a kind, as an entry of a fixture's `views` writes it.
pub fn offer(entry: &Value) -> Offer {
    let versions: Vec<&str> = entry["versions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    Offer::new(
        ViewKind::from_name(entry["kind"].as_str().unwrap()).unwrap(),
        &versions,
    )
}

/// The bytes of the `Write` actions in `actions`.
pub fn written(actions: &[Action]) -> Vec<u8> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Write(bytes) | Action::WriteView(bytes) => Some(bytes.as_slice()),
            _ => None,
        })
        .flatten()
        .copied()
        .collect()
}

/// A session configured as a fixture's `config` says.
pub fn session_of(
    config: &Value,
    tokens: &Value,
) -> Session<Held, impl FnMut() -> [u8; 16] + use<>> {
    let held = config["store"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| Digest::parse(d.as_str().unwrap()).unwrap())
        .collect();
    let issued = tokens
        .as_array()
        .unwrap()
        .iter()
        .map(|t| token_bytes(t.as_str().unwrap()))
        .collect();
    let mut host = self::config(&config["host"]);
    // The kinds of view a host opens, with the versions it speaks, are named in its entry; a
    // host that names none opens web views at 1.0.
    if let Some(views) = config["views"].as_array() {
        host.views = views.iter().map(offer).collect();
    }
    Session::new(host, Held(held, HashMap::new()), issuing(issued))
}

/// The bodies of the frames of a stream, as a terminal's parser would hand them
/// over.
pub fn bodies(bytes: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(start) = bytes[at..].windows(2).position(|w| w == b"\x1b_") {
        let body_start = at + start + 2;
        let end = bytes[body_start..]
            .windows(2)
            .position(|w| w == b"\x1b\\")
            .expect("a frame that ends");
        out.push(&bytes[body_start..body_start + end]);
        at = body_start + end + 2;
    }
    out
}

// ---- a program and a page, played against a host ------------------------------------------

/// The files of the page the tests open: three, like the bundle of the specification's
/// examples, with their own digests.
pub const INDEX: &str = "<!doctype html><script src=app.js></script>";
/// See [`INDEX`].
pub const APP: &str = "document.title = 'containers';";
/// See [`INDEX`].
pub const STYLE: &str = "body { margin: 0 }";

/// The tokens the rig's host issues, in order.
pub const TOKENS: [&str; 3] = [
    "q7Vh2sB0xJm9kLw4TzN1cA",
    "Zk3mP8rD5uYb1Xe6HqS0vg",
    "wwwwwwwwwwwwwwwwwwwwww",
];

/// A host over an in-memory store.
pub type Host = Session<MemoryStore, Box<dyn FnMut() -> [u8; 16]>>;

pub fn host_info() -> HostInfo {
    HostInfo {
        name: "pencere-test".into(),
        version: "0.0.0".into(),
    }
}

/// A host that opens web pages at 1.0 and draws nothing itself.
pub fn web_config() -> Config {
    Config::new(
        host_info(),
        theme(),
        Size {
            width: 800,
            height: 600,
        },
    )
}

/// A host that also opens the kinds of view it draws itself.
pub fn kit_config(views: &[(ViewKind, &[&str])]) -> Config {
    let mut config = web_config();
    config.views = views
        .iter()
        .map(|(kind, versions)| Offer::new(*kind, versions))
        .collect();
    config
}

/// The program's end of a conversation with a host: what the program writes goes in as bytes, and
/// what the host writes is read back as a program reads it.
pub struct Rig {
    pub host: Host,
    ids: Ids,
    decoder: StreamDecoder,
    /// Everything the host wrote, as messages, since the last `take`.
    pub wrote: Vec<Message>,
    /// The other actions since the last `take`.
    pub acted: Vec<Action>,
    /// The token of the last `welcome` that carried one.
    pub token: Option<String>,
}

impl Rig {
    pub fn new() -> Self {
        Self::with(web_config())
    }

    pub fn with(config: Config) -> Self {
        let mut tokens = TOKENS
            .iter()
            .map(|t| token_bytes(t))
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
        }
    }

    /// The program writes a message; returns its id.
    pub fn send(
        &mut self,
        kind: &str,
        re: Option<u32>,
        digest: Option<&str>,
        payload: &str,
    ) -> u32 {
        let id = self.ids.next_id();
        let message = Outgoing {
            kind,
            id,
            re,
            digest,
            file: pencere_host::frame::FileParams::NONE,
            payload,
        };
        let mut bytes = Vec::new();
        encode(Direction::ToHost, &message, Split::Fill, &mut bytes)
            .unwrap_or_else(|e| panic!("{e:?}: {kind} {payload:?}"));
        self.host.feed(&bytes);
        self.collect();
        id
    }

    /// The program writes a message of a file's (`write`), with its keys.
    pub fn send_keyed(
        &mut self,
        kind: &str,
        file: pencere_host::frame::FileParams<'_>,
        payload: &str,
    ) -> u32 {
        let id = self.ids.next_id();
        let message = Outgoing {
            kind,
            id,
            re: None,
            digest: None,
            file,
            payload,
        };
        let mut bytes = Vec::new();
        encode(Direction::ToHost, &message, Split::Fill, &mut bytes)
            .unwrap_or_else(|e| panic!("{e:?}: {kind} {payload:?}"));
        self.host.feed(&bytes);
        self.collect();
        id
    }

    /// The program writes a message whose payload is `value`.
    pub fn json(&mut self, kind: &str, value: &Value) -> u32 {
        let text = pencere_host::json::to_wire(value).unwrap();
        self.send(kind, None, None, &text)
    }

    /// Reads what the host has queued: its writes as the program decodes them, and the rest.
    pub fn collect(&mut self) {
        for action in self.host.take_actions() {
            if let Action::Write(bytes) | Action::WriteView(bytes) = &action {
                let mut events = Vec::new();
                self.decoder.feed(bytes, &mut events);
                for event in events {
                    match event {
                        Event::Message(m) => {
                            if m.kind == "welcome" {
                                if let Ok(body) = serde_json::from_str::<Value>(&m.payload) {
                                    if let Some(token) = body["token"].as_str() {
                                        self.token = Some(token.to_owned());
                                    }
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
    pub fn take(&mut self) -> (Vec<Message>, Vec<Action>) {
        self.collect();
        (
            std::mem::take(&mut self.wrote),
            std::mem::take(&mut self.acted),
        )
    }

    /// The kinds of the messages the host wrote since the last call.
    pub fn kinds(&mut self) -> Vec<String> {
        self.take().0.into_iter().map(|m| m.kind).collect()
    }

    /// The messages the host passed to the view since the last call, as the text it was given.
    pub fn viewed(&mut self) -> Vec<String> {
        self.collect();
        let (viewed, others): (Vec<Action>, Vec<Action>) = std::mem::take(&mut self.acted)
            .into_iter()
            .partition(|a| matches!(a, Action::ToView(_)));
        self.acted = others;
        viewed
            .into_iter()
            .map(|a| match a {
                Action::ToView(text) => text,
                _ => unreachable!(),
            })
            .collect()
    }

    /// The page sends a message.
    pub fn page(&mut self, text: &str) {
        self.host.view_message(text);
        self.collect();
    }

    /// A `hello` that lists `views`; the `welcome` it is answered with.
    pub fn hello_with(&mut self, views: &Value) -> Value {
        self.json("hello", &json!({"versions": [1], "views": views}));
        let (wrote, _) = self.take();
        let welcome = wrote
            .iter()
            .find(|m| m.kind == "welcome")
            .expect("a welcome");
        serde_json::from_str(&welcome.payload).unwrap()
    }

    /// The `hello` of a program that shows web pages; the token it is answered with.
    pub fn hello(&mut self) -> String {
        self.hello_with(&json!([{"kind": "web", "versions": ["1.0"]}]))["token"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    /// The program sends one of the page's files as a `file` message.
    pub fn send_file(&mut self, body: &str) {
        let digest = Digest::of(body.as_bytes()).to_string();
        self.send(
            "file",
            None,
            Some(&digest),
            &base64::encode(body.as_bytes()),
        );
    }

    /// The program sends all three files of the page.
    pub fn send_files(&mut self) {
        for body in [INDEX, APP, STYLE] {
            self.send_file(body);
        }
    }
}

/// The bundle of the page the tests open.
pub fn web_bundle() -> Value {
    let files: Vec<Value> = [
        ("index.html", INDEX, "text/html"),
        ("app.js", APP, "text/javascript"),
        ("style.css", STYLE, "text/css"),
    ]
    .iter()
    .map(|(path, body, media)| {
        json!({"path": path, "sha256": Digest::of(body.as_bytes()).to_string(), "size": body.len(), "type": media})
    })
    .collect();
    json!({"entry": "index.html", "files": files})
}

/// The payload of an `open` of the page with `token`, `access` and the bundle of
/// [`web_bundle`]; `extra` members are added to it.
pub fn open_web(token: &str, access: Option<Value>, extra: Value) -> Value {
    let mut value = json!({
        "token": token,
        "view": "web",
        "version": "1.0",
        "program": "containers",
        "title": "Containers",
        "bundle": web_bundle(),
    });
    if let Some(access) = access {
        value["access"] = access;
    }
    for (key, member) in extra.as_object().unwrap() {
        value[key] = member.clone();
    }
    value
}

/// The `start` among the actions.
pub fn started(actions: &[Action]) -> Option<&ViewStart> {
    actions.iter().find_map(|action| match action {
        Action::StartView(start) => Some(start.as_ref()),
        _ => None,
    })
}

/// The payload of a `bye` with the given token, as a program writes it.
pub fn bye(token: Option<&str>) -> String {
    match token {
        Some(token) => json!({"token": token, "reason": "signal"}).to_string(),
        None => r#"{"reason":"signal"}"#.to_owned(),
    }
}

/// The payload of a `dismiss` with the given token, as a program writes it.
pub fn dismiss(token: Option<&str>) -> String {
    match token {
        Some(token) => json!({ "token": token }).to_string(),
        None => "{}".to_owned(),
    }
}

impl Rig {
    /// A page that is up: `hello`, an `open` with `access`, the files, and the person's
    /// answer if there is a question (everything that was asked is allowed). Returns the start
    /// and leaves the host with nothing queued.
    pub fn up_web(&mut self, access: Option<Value>) -> ViewStart {
        let token = self.hello();
        self.json("open", &open_web(&token, access, json!({})));
        self.send_files();
        let (_, mut acted) = self.take();
        if let Some(question) = acted.iter().find_map(|a| match a {
            Action::Consent(c) => Some(c.clone()),
            _ => None,
        }) {
            self.host.consent(question.id, question.grant_all());
            let (_, more) = self.take();
            acted.extend(more);
        }
        started(&acted).expect("a view to start").clone()
    }
}

/// A host configured as a `host-to-page` entry says: the roles of the theme it knows, the font,
/// the size of the pane, the person's language and time zone and the setting for motion.
pub fn bridge_config(config: &Value) -> Config {
    let colour = |name: &str| {
        config["theme"][name]
            .as_str()
            .map(|c| Rgb::parse(c).unwrap())
    };
    let mut theme = Theme::new(
        colour("background").unwrap(),
        colour("foreground").unwrap(),
        config["font"].as_str().unwrap_or(""),
    );
    theme.dim = colour("dim");
    theme.accent = colour("accent");
    theme.cursor = colour("cursor");
    theme.border = colour("border");
    theme.success = colour("success");
    theme.error = colour("error");
    theme.warning = colour("warning");
    theme.info = colour("info");
    theme.selection = colour("selection");
    theme.ansi = config["theme"]["ansi"].as_array().map(|palette| {
        let palette: Vec<Rgb> = palette
            .iter()
            .map(|c| Rgb::parse(c.as_str().unwrap()).unwrap())
            .collect();
        palette.try_into().unwrap()
    });
    let mut host = web_config();
    host.view.theme = theme;
    host.view.size = Size {
        width: config["size"]["width"].as_u64().unwrap() as u32,
        height: config["size"]["height"].as_u64().unwrap() as u32,
    };
    host.view.locale = config["locale"].as_str().map(str::to_owned);
    host.view.time_zone = config["timeZone"].as_str().map(str::to_owned);
    host.view.reduce_motion = config["reduceMotion"].as_bool().unwrap_or(false);
    host
}
