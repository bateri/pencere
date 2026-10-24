//! A view that is a WebAssembly component or a Lua script (specification §9.8) against a host
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

use common::{Rig, TOKENS, kit_config};
use pencere_host::bundle::Digest;
use pencere_host::session::{Action, Config, MAX_VIEW_MESSAGE, State, ViewKind, ViewStart};
use serde_json::{Value, json};

/// What ends a view, played against the rig.
type End = fn(&mut Rig);

const TOKEN: &str = TOKENS[0];
const OTHER: &str = TOKENS[1];

/// What a component's bytes are, for the tests: the header of an empty one.
const COMPONENT: &[u8] = b"\0asm\x01\0\0\0";

/// What a script is, for the tests.
const SCRIPT: &[u8] = b"return { render = function() end }";

/// The version of the kit's interface the tests speak.
const KIT: &str = "0.5";

/// The versions the tests' hosts speak of a kind, and a program lists of it.
fn versions(kind: ViewKind) -> &'static [&'static str] {
    match kind {
        ViewKind::Web => &["1.0"],
        _ => &[KIT],
    }
}

/// A host that opens `kinds`, each at the version the tests speak of it.
fn config(kinds: &[ViewKind]) -> Config {
    let views: Vec<(ViewKind, &[&str])> = kinds.iter().map(|k| (*k, versions(*k))).collect();
    kit_config(&views)
}

fn rig(kinds: &[ViewKind]) -> Rig {
    Rig::with(config(kinds))
}

/// A `hello` that lists `names` of kinds, each at the version the tests speak of it (a name
/// the table has not got at `1.0`); the `welcome` it is answered with.
fn hello_with(rig: &mut Rig, names: &[&str]) -> Value {
    let views: Vec<Value> = names
        .iter()
        .map(|name| {
            let versions = ViewKind::from_name(name).map_or(&["1.0"][..], versions);
            json!({"kind": name, "versions": versions})
        })
        .collect();
    rig.hello_with(&Value::Array(views))
}

/// The `hello` of a program that can show either kind and prefers a component.
fn hello(rig: &mut Rig) -> Value {
    hello_with(rig, &["wasm", "web"])
}

/// What a host that opens these kinds answers a program that listed them in this order:
/// `views` of its `welcome`.
fn offers(kinds: &[ViewKind]) -> Value {
    Value::Array(
        kinds
            .iter()
            .map(|kind| json!({"kind": kind.name(), "versions": versions(*kind)}))
            .collect(),
    )
}

/// An `open` of a component of `size` bytes named in the manifest, with `extra` members.
fn open(rig: &mut Rig, size: u64, extra: &Value) {
    let digest = Digest::of(COMPONENT).to_string();
    let mut body = json!({
        "token": TOKEN,
        "view": "wasm",
        "version": KIT,
        "bundle": { "entry": "view.wasm", "files": [
            { "path": "view.wasm", "sha256": digest, "size": size, "type": "application/wasm" }
        ] },
    });
    for (key, value) in extra.as_object().unwrap() {
        body[key] = value.clone();
    }
    rig.json("open", &body);
}

/// The component's file, as the program sends it.
fn file(rig: &mut Rig) {
    let digest = Digest::of(COMPONENT).to_string();
    let payload = pencere_host::base64::encode(COMPONENT);
    rig.send("file", None, Some(&digest), &payload);
}

/// A view that is up: `hello`, `open`, its file.
fn up(rig: &mut Rig, extra: &Value) -> ViewStart {
    hello(rig);
    open(rig, COMPONENT.len() as u64, extra);
    let (need, acted) = rig.take();
    assert_eq!(need.len(), 1, "{need:?}");
    assert_eq!(need[0].kind, "need");
    assert!(acted.is_empty(), "{acted:?}");
    file(rig);
    let (_, acted) = rig.take();
    let [Action::StartView(start)] = acted.as_slice() else {
        panic!("expected one view to start, got {acted:?}");
    };
    assert_eq!(rig.host.state(), State::Open);
    (**start).clone()
}

fn module_says(rig: &mut Rig, text: &str) -> bool {
    let written = rig.host.module_message(text);
    rig.collect();
    written
}

fn assert_wrote_nothing(rig: &mut Rig, text: &str) {
    let before = rig.host.diagnostics().ignored;
    assert!(!module_says(rig, text));
    assert!(rig.take().0.is_empty(), "the program was written to");
    assert!(rig.host.diagnostics().ignored > before);
}

/// The code of the one `closed` the host wrote.
fn refusal(rig: &mut Rig) -> String {
    let (wrote, _) = rig.take();
    assert_eq!(wrote.len(), 1, "{wrote:?}");
    assert_eq!(wrote[0].kind, "closed");
    let body: Value = serde_json::from_str(&wrote[0].payload).unwrap();
    assert_eq!(body["reason"], "refused");
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
fn a_view_that_is_a_component_starts_as_a_view_of_the_kit_and_says_what_it_is() {
    let mut rig = rig(&[ViewKind::Wasm, ViewKind::Web]);
    let start = up(
        &mut rig,
        &json!({ "program": "demopen", "title": "Containers" }),
    );
    assert_eq!(start.kind, ViewKind::Wasm);
    assert_eq!(start.version, KIT);
    assert_eq!(start.program.as_deref(), Some("demopen"));
    assert_eq!(start.title.as_deref(), Some("Containers"));
    assert!(start.policy.is_empty() && start.allow.is_empty());
    assert!(start.grant.is_empty());
    // The kit tells the component about the terminal, not the host.
    assert_eq!(start.context, None);
    // Nothing but the `need` was written.
    assert!(rig.take().0.is_empty());
    // The component is served from the store, like any bundle.
    let (media, bytes) = rig.host.serve("view.wasm").unwrap();
    assert_eq!(media, "application/wasm");
    assert_eq!(&*bytes, COMPONENT);
}

#[test]
fn what_the_program_calls_itself_is_cut_and_cleaned_like_any_label() {
    let mut rig = rig(&[ViewKind::Wasm, ViewKind::Web]);
    let start = up(
        &mut rig,
        &json!({ "program": "de\u{202e}mo\npen", "title": 7 }),
    );
    assert_eq!(start.program.as_deref(), Some("demopen"));
    assert_eq!(start.title, None);
}

#[test]
fn an_access_in_the_open_of_a_component_is_not_read() {
    for access in [
        json!(3),
        json!({"connect": ["https://a.example"]}),
        json!({"connect": 5}),
    ] {
        let mut rig = rig(&[ViewKind::Wasm, ViewKind::Web]);
        let start = up(&mut rig, &json!({ "access": access }));
        assert_eq!(start.kind, ViewKind::Wasm);
        // No question to the person, no policy, nothing allowed.
        assert!(start.policy.is_empty() && start.grant.is_empty());
    }
}

#[test]
fn a_restart_offers_what_a_hello_that_listed_everything_is_offered() {
    // One rule makes both: the same kinds, in the host's order, at the same versions.
    let kinds = [ViewKind::Wasm, ViewKind::Lua, ViewKind::Web];
    let mut asked = rig(&kinds);
    let welcome = hello_with(&mut asked, &["wasm", "lua", "web"]);
    let mut restarted = rig(&kinds);
    restarted.host.restart();
    let (wrote, _) = restarted.take();
    assert_eq!(wrote.len(), 1);
    let offered: Value = serde_json::from_str(&wrote[0].payload).unwrap();
    assert_eq!(offered["restart"], true);
    assert_eq!(offered["views"], welcome["views"]);
    assert_eq!(offered["views"].as_array().unwrap().len(), 3);
}

#[test]
fn a_msg_of_the_program_reaches_the_view_as_it_is_and_is_not_read() {
    let mut rig = rig(&[ViewKind::Wasm, ViewKind::Web]);
    up(&mut rig, &json!({}));
    let text =
        pencere_host::json::to_wire(&json!({ "push": { "id": 2, "value": [1.5, "caf\u{e9}"] } }))
            .unwrap();
    rig.send("msg", None, None, &text);
    assert_eq!(rig.take().1, vec![Action::ToView(text)]);
    // Named like the host's own messages for a page, it is still the component's.
    for own in [
        r#"{"context":{}}"#,
        r#"{"close":{}}"#,
        r#"{"link":{"url":"https://a.example"}}"#,
    ] {
        rig.send("msg", None, None, own);
        assert_eq!(rig.take().1, vec![Action::ToView(own.to_owned())]);
    }
    assert!(rig.take().0.is_empty());
}

#[test]
fn a_msg_that_is_not_an_object_or_comes_before_the_view_runs_goes_nowhere() {
    let mut rig = rig(&[ViewKind::Wasm, ViewKind::Web]);
    hello(&mut rig);
    open(&mut rig, COMPONENT.len() as u64, &json!({}));
    rig.take();
    // The file is still awaited: the component is not up.
    rig.json("msg", &json!({ "push": { "id": 1 } }));
    assert!(rig.take().1.is_empty());
    file(&mut rig);
    rig.take();
    rig.send("msg", None, None, "[1,2]");
    rig.send("msg", None, None, "\"text\"");
    assert!(rig.take().1.is_empty());
    assert_eq!(rig.host.diagnostics().ignored, 3);
    // It has to be an object, not only begin like one: the host reads it as it does a page's.
    for broken in [r#"{"push":"#, r#"{"push":{"id":1}} {}"#, "{,}", "{1:2}"] {
        rig.send("msg", None, None, broken);
        assert!(rig.take().1.is_empty(), "{broken}");
    }
    assert_eq!(rig.host.diagnostics().ignored, 7);
}

#[test]
fn a_message_of_the_component_is_written_to_the_program_while_its_view_is_open() {
    let mut rig = rig(&[ViewKind::Wasm, ViewKind::Web]);
    up(&mut rig, &json!({}));
    let text = r#"{"call":{"id":1,"method":"list"}}"#;
    assert!(module_says(&mut rig, text));
    let (wrote, _) = rig.take();
    assert_eq!(wrote.len(), 1);
    assert_eq!(wrote[0].kind, "msg");
    assert_eq!(wrote[0].payload, text);
    // Not an object, or only begins like one: not written.
    assert_wrote_nothing(&mut rig, "[1]");
    assert_wrote_nothing(&mut rig, r#"{"call":"#);
    assert_wrote_nothing(&mut rig, r#"{"call":{}} {"call":{}}"#);
    // Whitespace between the tokens is accepted and is not written: a frame has no byte for it.
    assert!(module_says(
        &mut rig,
        "{\n\t\"call\" : {\r\n \"id\": 1,\n\"method\":\"list\"}\n}\n"
    ));
    let (wrote, _) = rig.take();
    assert_eq!(wrote.len(), 1);
    assert_eq!(wrote[0].payload, r#"{"call" : { "id": 1,"method":"list"}}"#);
    // Named like the host's own messages for a page, it is still the component's.
    assert!(module_says(&mut rig, r#"{"close":{}}"#));
    assert_eq!(rig.take().0.len(), 1);
    assert_eq!(rig.host.state(), State::Open);
}

#[test]
fn a_message_of_the_component_over_the_limit_ends_the_view_and_the_program_is_told() {
    let mut rig = rig(&[ViewKind::Wasm, ViewKind::Web]);
    up(&mut rig, &json!({}));
    // At the limit, to the byte, it is written; one byte more is a failure of the view.
    let pad = "a".repeat(MAX_VIEW_MESSAGE - r#"{"pad":""}"#.len());
    assert!(module_says(&mut rig, &format!(r#"{{"pad":"{pad}"}}"#)));
    assert_eq!(rig.take().0.len(), 1);
    assert!(!module_says(&mut rig, &format!(r#"{{"pad":"{pad}a"}}"#)));
    let (wrote, acted) = rig.take();
    assert_eq!(acted, vec![Action::DestroyView]);
    assert_eq!(wrote.len(), 1);
    assert_eq!(wrote[0].kind, "closed");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&wrote[0].payload).unwrap(),
        json!({"reason": "error", "code": "view", "message": "a message of the view was over 1 MiB"})
    );
    assert_eq!(rig.host.state(), State::Idle);
}

#[test]
fn nothing_the_component_says_is_written_once_its_view_is_over_however_it_ended() {
    let text = r#"{"call":{"id":1,"method":"list"}}"#;
    // Before any view.
    let mut before = rig(&[ViewKind::Wasm, ViewKind::Web]);
    hello(&mut before);
    assert_wrote_nothing(&mut before, text);

    let endings: [(&str, End); 5] = [
        ("the program's bye", |rig| {
            rig.json("bye", &json!({ "token": TOKEN, "reason": "exit" }));
        }),
        ("the program's dismiss", |rig| {
            rig.json("dismiss", &json!({ "token": TOKEN }));
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
        let mut rig = rig(&[ViewKind::Wasm, ViewKind::Web]);
        up(&mut rig, &json!({}));
        assert!(module_says(&mut rig, text), "{name}: open");
        rig.take();
        end(&mut rig);
        // The view is gone at once, whatever ended it: nothing is waited for.
        let (_, acted) = rig.take();
        assert!(
            acted.contains(&Action::DestroyView),
            "{name}: the view is destroyed, got {acted:?}"
        );
        assert_wrote_nothing(&mut rig, text);
        // And not after the program has said hello again either: a view that is gone is gone.
        if name != "the program ending" {
            hello(&mut rig);
            assert_wrote_nothing(&mut rig, text);
        }
    }
}

#[test]
fn the_end_of_a_view_tells_the_program_why() {
    let mut rig = rig(&[ViewKind::Wasm, ViewKind::Web]);
    up(&mut rig, &json!({}));
    rig.host
        .view_failed("script:12: attempt to index nil with 'x'");
    let (wrote, _) = rig.take();
    assert_eq!(wrote.len(), 1);
    assert_eq!(wrote[0].kind, "closed");
    let body: Value = serde_json::from_str(&wrote[0].payload).unwrap();
    assert_eq!(
        body,
        json!({ "reason": "error", "code": "view", "message": "script:12: attempt to index nil with 'x'" })
    );

    let mut rig = self::rig(&[ViewKind::Wasm, ViewKind::Web]);
    up(&mut rig, &json!({}));
    rig.host.close_view();
    let body: Value = serde_json::from_str(&rig.take().0[0].payload).unwrap();
    assert_eq!(body, json!({ "reason": "user" }));
}

#[test]
fn a_welcome_says_the_versions_a_kind_opens_at_when_the_program_speaks_none_of_them() {
    let mut rig = rig(&[ViewKind::Wasm, ViewKind::Web]);
    // The program speaks wasm 9.9 and web 1.0: the host opens wasm, at 0.5 only.
    let welcome = rig.hello_with(&json!([
        {"kind": "wasm", "versions": ["9.9"]},
        {"kind": "lua", "versions": ["9.9"]},
        {"kind": "web", "versions": ["1.0"]},
    ]));
    assert_eq!(
        welcome["views"],
        json!([{"kind": "web", "versions": ["1.0"]}])
    );
    // Only a kind the host opens is named, with the versions it opens, in its own order; `lua`
    // is not opened at all and `web` is shared.
    assert_eq!(
        welcome["opens"],
        json!([{"kind": "wasm", "versions": ["0.5"]}])
    );
    // A program that shares a version of every kind it lists is told nothing of the sort.
    let welcome = hello(&mut rig);
    assert!(welcome.get("opens").is_none(), "{welcome}");
}

#[test]
fn a_view_whose_words_wait_for_a_program_that_does_not_read_is_closed() {
    use pencere_host::session::MAX_BACKLOG;
    let mut rig = rig(&[ViewKind::Wasm, ViewKind::Web]);
    up(&mut rig, &json!({}));
    // What waits, up to the limit, is waited for.
    rig.host.backlog(MAX_BACKLOG);
    let (wrote, acted) = rig.take();
    assert!(wrote.is_empty() && !acted.contains(&Action::DestroyView));
    // Past it the view is closed as a view that failed is.
    rig.host.backlog(MAX_BACKLOG + 1);
    let (wrote, acted) = rig.take();
    assert!(acted.contains(&Action::DestroyView), "{acted:?}");
    assert_eq!(wrote.len(), 1);
    assert_eq!(wrote[0].kind, "closed");
    let body: Value = serde_json::from_str(&wrote[0].payload).unwrap();
    assert_eq!(body["reason"], "error");
    assert_eq!(body["code"], "view");
    // A view that is gone has nothing to close.
    rig.host.backlog(MAX_BACKLOG + 1);
    let (wrote, acted) = rig.take();
    assert!(wrote.is_empty() && acted.is_empty());
}

#[test]
fn a_host_that_does_not_show_components_leaves_them_out_and_refuses_one() {
    let mut rig = rig(&[ViewKind::Web]);
    assert_eq!(hello(&mut rig)["views"], offers(&[ViewKind::Web]));
    open(&mut rig, COMPONENT.len() as u64, &json!({}));
    assert_eq!(refusal(&mut rig), "unsupported");
    assert_eq!(rig.host.state(), State::Idle);
}

#[test]
fn a_component_over_the_hosts_limit_is_refused_before_it_is_asked_for() {
    let mut config = config(&[ViewKind::Wasm, ViewKind::Web]);
    config.native_entry_limit = 4;
    let mut rig = Rig::with(config);
    hello(&mut rig);
    open(&mut rig, COMPONENT.len() as u64, &json!({}));
    let (wrote, acted) = rig.take();
    assert_eq!(wrote.len(), 1);
    let body: Value = serde_json::from_str(&wrote[0].payload).unwrap();
    assert_eq!(body["code"], "limit");
    assert!(acted.is_empty());
}

#[test]
fn a_pages_message_channel_is_not_a_components_to_use() {
    let mut rig = rig(&[ViewKind::Wasm, ViewKind::Web]);
    up(&mut rig, &json!({}));
    for text in [
        r#"{"call":{"id":1,"method":"list"}}"#,
        r#"{"context":{}}"#,
        r#"{"close":{}}"#,
    ] {
        rig.host.view_message(text);
        let (wrote, acted) = rig.take();
        assert!(wrote.is_empty() && acted.is_empty(), "{text}");
    }
    assert_eq!(rig.host.state(), State::Open);
    assert_eq!(rig.host.page_context(), None);
}

#[test]
fn a_restart_ends_the_view_and_offers_a_new_token_without_a_word_to_the_component() {
    let mut rig = rig(&[ViewKind::Wasm, ViewKind::Web]);
    up(&mut rig, &json!({}));
    rig.host.restart();
    let (wrote, acted) = rig.take();
    assert!(acted.contains(&Action::DestroyView), "{acted:?}");
    assert_eq!(wrote.len(), 1);
    assert_eq!(wrote[0].kind, "welcome");
    let body: Value = serde_json::from_str(&wrote[0].payload).unwrap();
    assert_eq!(body["restart"], true);
    assert_eq!(body["token"], OTHER);
}

#[test]
fn a_web_open_needs_its_bundle_like_any_other() {
    let mut rig = rig(&[ViewKind::Wasm, ViewKind::Web]);
    hello(&mut rig);
    rig.json(
        "open",
        &json!({ "token": TOKEN, "view": "web", "version": "1.0" }),
    );
    assert_eq!(refusal(&mut rig), "manifest");
}

#[test]
fn the_welcome_lists_the_kinds_the_host_opens_among_the_ones_the_program_listed_in_its_order() {
    let mut both = rig(&[ViewKind::Wasm, ViewKind::Web]);
    assert_eq!(
        hello_with(&mut both, &["web", "wasm"])["views"],
        offers(&[ViewKind::Web, ViewKind::Wasm])
    );
    let mut both = rig(&[ViewKind::Wasm, ViewKind::Web]);
    assert_eq!(
        hello_with(&mut both, &["wasm", "web"])["views"],
        offers(&[ViewKind::Wasm, ViewKind::Web])
    );
    // The host only eliminates: a kind it does not open, one it has never heard of and one
    // listed twice leave the rest as the program had them.
    let mut web = rig(&[ViewKind::Web]);
    let welcome = hello_with(&mut web, &["lua", "gopher", "wasm", "web", "wasm", "web"]);
    assert_eq!(welcome["views"], offers(&[ViewKind::Web]));
    assert_eq!(welcome["features"], json!(["dismiss"]));
}

#[test]
fn a_host_that_opens_nothing_the_program_listed_still_answers_with_an_empty_list() {
    let mut web = rig(&[ViewKind::Web]);
    let welcome = hello_with(&mut web, &["wasm"]);
    assert_eq!(welcome["views"], json!([]));
    assert_eq!(welcome["token"], TOKEN);
    // Nothing can be opened with the token it carries.
    web.json(
        "open",
        &json!({ "token": TOKEN, "view": "wasm", "version": KIT, "bundle": one_file_bundle("view.wasm", "application/wasm") }),
    );
    assert_eq!(refusal(&mut web), "unsupported");
}

#[test]
fn a_hello_that_lists_no_kind_of_view_is_not_answered() {
    for hello in [
        json!({ "versions": [1] }),
        json!({ "versions": [1], "views": [] }),
        json!({ "versions": [1], "views": "wasm" }),
        json!({ "versions": [1], "views": [1] }),
        // The kinds alone, without their versions, are not a list of kinds with versions.
        json!({ "versions": [1], "views": ["wasm"] }),
        json!({ "versions": [1], "views": [{"kind": "wasm"}] }),
        json!({ "versions": [1], "views": [{"kind": "wasm", "versions": "0.5"}] }),
        json!({ "versions": [1], "views": [{"kind": "wasm", "versions": [0.5]}] }),
        json!({ "versions": [1], "views": [{"versions": ["0.5"]}] }),
        json!({ "versions": [1], "views": [{"kind": "wasm", "versions": ["0.5"]}, "web"] }),
    ] {
        let mut rig = rig(&[ViewKind::Wasm, ViewKind::Web]);
        rig.json("hello", &hello);
        assert!(rig.take().0.is_empty(), "{hello}");
        assert_eq!(rig.host.state(), State::Idle);
    }
}

#[test]
fn an_open_has_to_name_a_kind_and_a_version_the_welcome_offered() {
    // Missing, not a string: the payload is malformed.
    for (view, version) in [
        (None, Some(json!(KIT))),
        (Some(json!(7)), Some(json!(KIT))),
        (Some(json!(null)), Some(json!(KIT))),
        (Some(json!("wasm")), None),
        (Some(json!("wasm")), Some(json!(5))),
        (Some(json!("wasm")), Some(json!(0.5))),
        (Some(json!("wasm")), Some(json!(null))),
    ] {
        let mut rig = rig(&[ViewKind::Wasm, ViewKind::Web]);
        hello(&mut rig);
        let mut body =
            json!({ "token": TOKEN, "bundle": one_file_bundle("view.wasm", "application/wasm") });
        if let Some(view) = view {
            body["view"] = view;
        }
        if let Some(version) = version {
            body["version"] = version;
        }
        rig.json("open", &body);
        assert_eq!(refusal(&mut rig), "manifest", "{body}");
    }
    // A name outside the table, a kind the welcome did not offer and a version of a kind it did
    // not list: unsupported.
    let component = one_file_bundle("view.wasm", "application/wasm");
    let page = common::web_bundle();
    for (listed, open) in [
        (
            &["wasm", "web"][..],
            json!({ "view": "gopher", "version": "1.0", "bundle": component }),
        ),
        (
            &["wasm"],
            json!({ "view": "web", "version": "1.0", "bundle": page }),
        ),
        (
            &["web"],
            json!({ "view": "wasm", "version": KIT, "bundle": component }),
        ),
        (
            &["wasm", "web"],
            json!({ "view": "wasm", "version": "0.6", "bundle": component }),
        ),
        (
            &["wasm", "web"],
            json!({ "view": "wasm", "version": "0.5.1", "bundle": component }),
        ),
        (
            &["wasm", "web"],
            json!({ "view": "wasm", "version": "", "bundle": component }),
        ),
    ] {
        let mut rig = rig(&[ViewKind::Wasm, ViewKind::Web]);
        hello_with(&mut rig, listed);
        let mut body = open.clone();
        body["token"] = json!(TOKEN);
        rig.json("open", &body);
        assert_eq!(refusal(&mut rig), "unsupported", "{listed:?} {open}");
        assert!(rig.take().1.is_empty());
    }
}

#[test]
fn a_bundle_whose_entry_is_not_what_the_view_says_is_refused_and_the_host_infers_nothing() {
    let wasm_in_html = one_file_bundle("index.html", "text/html");
    let html_in_wasm = one_file_bundle("view.wasm", "application/wasm");
    for (view, version, bundle) in [
        ("wasm", KIT, Some(wasm_in_html.clone())),
        ("wasm", KIT, None),
        ("web", "1.0", Some(html_in_wasm.clone())),
        ("web", "1.0", None),
    ] {
        let mut rig = rig(&[ViewKind::Wasm, ViewKind::Web]);
        hello(&mut rig);
        let mut body = json!({ "token": TOKEN, "view": view, "version": version });
        if let Some(bundle) = bundle {
            body["bundle"] = bundle;
        }
        rig.json("open", &body);
        assert_eq!(refusal(&mut rig), "manifest", "{body}");
        assert_eq!(rig.host.state(), State::Idle);
    }
    // The token is spent whatever comes of the open, as before.
    let mut rig = rig(&[ViewKind::Wasm, ViewKind::Web]);
    hello(&mut rig);
    rig.json(
        "open",
        &json!({ "token": TOKEN, "view": "wasm", "version": KIT }),
    );
    assert_eq!(refusal(&mut rig), "manifest");
    rig.json(
        "open",
        &json!({ "token": TOKEN, "view": "wasm", "version": KIT }),
    );
    assert_eq!(refusal(&mut rig), "token");
}

#[test]
fn a_restart_offers_every_kind_the_host_opens_in_its_own_order_with_every_version() {
    let mut config = common::kit_config(&[
        (ViewKind::Wasm, &["0.5", "0.4"]),
        (ViewKind::Lua, &[KIT]),
        (ViewKind::Web, &["1.0"]),
    ]);
    // A kind listed twice counts once, at its first place.
    config
        .views
        .push(pencere_host::session::Offer::new(ViewKind::Web, &["9.9"]));
    let mut rig = Rig::with(config);
    let token = rig.hello_with(&json!([{"kind": "wasm", "versions": ["0.5"]}]));
    assert_eq!(
        token["views"],
        json!([{"kind": "wasm", "versions": ["0.5"]}])
    );
    rig.host.restart();
    let (wrote, _) = rig.take();
    let welcome: Value = serde_json::from_str(&wrote[0].payload).unwrap();
    assert_eq!(
        welcome["views"],
        json!([
            {"kind": "wasm", "versions": ["0.5", "0.4"]},
            {"kind": "lua", "versions": [KIT]},
            {"kind": "web", "versions": ["1.0"]},
        ])
    );
    // A program opens what it likes of them with the new token, at any of the versions.
    let mut body = json!({ "token": OTHER, "view": "wasm", "version": "0.4",
        "bundle": one_file_bundle("view.wasm", "application/wasm") });
    rig.json("open", &body);
    assert!(rig.take().0.iter().all(|m| m.kind != "closed"));
    // And what the restart did not offer is refused.
    rig.host.restart();
    rig.take();
    body["token"] = json!(TOKENS[2]);
    body["version"] = json!("0.3");
    rig.json("open", &body);
    assert_eq!(refusal(&mut rig), "unsupported");
}

// --- A script ------------------------------------------------------------------------------------

/// An `open` of a script named in the manifest, with `extra` members.
fn open_script(rig: &mut Rig, extra: &Value) {
    let digest = Digest::of(SCRIPT).to_string();
    let mut body = json!({
        "token": TOKEN,
        "view": "lua",
        "version": KIT,
        "bundle": { "entry": "view.lua", "files": [
            { "path": "view.lua", "sha256": digest, "size": SCRIPT.len(), "type": "text/x-lua" }
        ] },
    });
    for (key, value) in extra.as_object().unwrap() {
        body[key] = value.clone();
    }
    rig.json("open", &body);
}

/// A script view that is up: `hello`, `open`, its file.
fn script_up(rig: &mut Rig) -> ViewStart {
    hello_with(rig, &["lua", "web"]);
    open_script(rig, &json!({ "program": "demopen" }));
    let (need, _) = rig.take();
    assert_eq!(need.len(), 1, "{need:?}");
    assert_eq!(need[0].kind, "need");
    let digest = Digest::of(SCRIPT).to_string();
    rig.send(
        "file",
        None,
        Some(&digest),
        &pencere_host::base64::encode(SCRIPT),
    );
    let (_, acted) = rig.take();
    let [Action::StartView(start)] = acted.as_slice() else {
        panic!("expected one view to start, got {acted:?}");
    };
    (**start).clone()
}

#[test]
fn a_view_that_is_a_script_starts_like_a_component_and_is_served_as_text() {
    let mut rig = rig(&[ViewKind::Wasm, ViewKind::Lua, ViewKind::Web]);
    let start = script_up(&mut rig);
    assert_eq!(start.kind, ViewKind::Lua);
    assert!(ViewKind::Lua.is_native() && ViewKind::Wasm.is_native() && !ViewKind::Web.is_native());
    assert_eq!(start.program.as_deref(), Some("demopen"));
    assert!(start.policy.is_empty() && start.context.is_none());
    assert_eq!(rig.host.state(), State::Open);
    // Nothing but the `need`.
    assert!(rig.take().0.is_empty());
    let (media, bytes) = rig.host.serve("view.lua").unwrap();
    assert_eq!(media, "text/x-lua");
    assert_eq!(&*bytes, SCRIPT);
}

#[test]
fn a_script_gets_the_messages_of_a_component_and_nothing_it_says_is_written_once_it_is_over() {
    let mut rig = rig(&[ViewKind::Wasm, ViewKind::Lua, ViewKind::Web]);
    script_up(&mut rig);
    let text = pencere_host::json::to_wire(&json!({ "push": { "id": 1, "value": 2 } })).unwrap();
    rig.send("msg", None, None, &text);
    assert_eq!(rig.take().1, vec![Action::ToView(text)]);
    let said = r#"{"call":{"id":1,"method":"list"}}"#;
    assert!(module_says(&mut rig, said));
    assert_eq!(rig.take().0[0].payload, said);
    rig.host.view_failed("script:3: boom");
    rig.take();
    assert_wrote_nothing(&mut rig, said);
}

#[test]
fn a_script_view_is_not_a_component_and_a_component_is_not_a_script() {
    // The entry has to be of the kind the program said.
    let mut rig = rig(&[ViewKind::Wasm, ViewKind::Lua, ViewKind::Web]);
    hello_with(&mut rig, &["lua", "wasm"]);
    open(&mut rig, COMPONENT.len() as u64, &json!({ "view": "lua" }));
    assert_eq!(refusal(&mut rig), "manifest");
    let mut rig = self::rig(&[ViewKind::Wasm, ViewKind::Lua, ViewKind::Web]);
    hello_with(&mut rig, &["lua", "wasm"]);
    let digest = Digest::of(SCRIPT).to_string();
    rig.json(
        "open",
        &json!({ "token": TOKEN, "view": "wasm", "version": KIT, "bundle": { "entry": "view.lua", "files": [
            { "path": "view.lua", "sha256": digest, "size": SCRIPT.len(), "type": "text/x-lua" }
        ] } }),
    );
    assert_eq!(refusal(&mut rig), "manifest");
    // A script has no document to stand on without its bundle either.
    let mut rig = self::rig(&[ViewKind::Wasm, ViewKind::Lua, ViewKind::Web]);
    hello_with(&mut rig, &["lua"]);
    rig.json(
        "open",
        &json!({ "token": TOKEN, "view": "lua", "version": KIT }),
    );
    assert_eq!(refusal(&mut rig), "manifest");
}

#[test]
fn a_host_that_does_not_run_scripts_leaves_them_out_and_refuses_one() {
    let mut rig = rig(&[ViewKind::Wasm, ViewKind::Web]);
    assert_eq!(
        hello_with(&mut rig, &["lua", "web"])["views"],
        offers(&[ViewKind::Web])
    );
    open_script(&mut rig, &json!({}));
    assert_eq!(refusal(&mut rig), "unsupported");
    // And one that runs only scripts offers only them.
    let mut rig = self::rig(&[ViewKind::Lua]);
    assert_eq!(
        hello_with(&mut rig, &["wasm", "lua"])["views"],
        offers(&[ViewKind::Lua])
    );
}

#[test]
fn a_script_over_the_hosts_limit_is_refused_before_it_is_asked_for() {
    let mut config = config(&[ViewKind::Lua]);
    config.native_entry_limit = 4;
    let mut rig = Rig::with(config);
    hello_with(&mut rig, &["lua"]);
    open_script(&mut rig, &json!({}));
    let (wrote, _) = rig.take();
    let body: Value = serde_json::from_str(&wrote[0].payload).unwrap();
    assert_eq!(body["code"], "limit");
}

#[test]
fn the_words_a_view_failed_with_are_cut_and_cleaned_before_the_program_is_told_them() {
    let mut rig = rig(&[ViewKind::Lua]);
    script_up(&mut rig);
    let long = "x".repeat(2000);
    rig.host
        .view_failed(&format!("\u{1b}[31mscript:1:\u{202e} {long}\n"));
    let (wrote, _) = rig.take();
    let body: Value = serde_json::from_str(&wrote[0].payload).unwrap();
    let message = body["message"].as_str().unwrap();
    // No escape, no override, no newline; at most the most the protocol allows; blanks trimmed.
    assert!(message.starts_with("[31mscript:1: xxx"), "{message}");
    assert_eq!(
        message.chars().count(),
        pencere_host::session::MAX_FAILURE_CHARS
    );
    // A view with nothing to say says nothing.
    let mut rig = self::rig(&[ViewKind::Lua]);
    script_up(&mut rig);
    rig.host.view_failed(" \n ");
    let body: Value = serde_json::from_str(&rig.take().0[0].payload).unwrap();
    assert_eq!(body, json!({ "reason": "error", "code": "view" }));
}

#[test]
fn the_words_of_a_failure_told_when_no_view_is_up_are_not_left_for_another_close() {
    let mut rig = rig(&[ViewKind::Wasm, ViewKind::Web]);
    hello(&mut rig);
    // No view yet: nothing to fail, nothing to remember.
    rig.host.view_failed("script:1: late");
    rig.take();
    // A bundle that fails verification is a close of its own, with its own code and no words.
    open(&mut rig, COMPONENT.len() as u64, &json!({}));
    rig.take();
    let digest = Digest::of(COMPONENT).to_string();
    rig.send(
        "file",
        None,
        Some(&digest),
        &pencere_host::base64::encode(b"not the bytes"),
    );
    let (wrote, _) = rig.take();
    let body: Value = serde_json::from_str(&wrote.last().unwrap().payload).unwrap();
    assert_eq!(body, json!({ "reason": "error", "code": "bundle" }));
}
