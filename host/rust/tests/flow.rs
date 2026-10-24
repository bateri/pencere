//! A program and a view, played against a host session: the conversation of Appendix C of the
//! specification and what grows from it. The program and the page are scripts in this file; the
//! host is the crate. (Pages and the bridge are `bridge.rs`'s; views the terminal draws itself
//! are `native.rs`'s.)

// A test fails by panicking, and indexes what it has just built.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::{
    APP, Held, INDEX, Rig, STYLE, TOKENS, bye, dismiss, host_info, open_web, started, web_bundle,
    web_config,
};
use pencere_host::base64;
use pencere_host::bundle::{Digest, Store, Verified};
use pencere_host::frame::Message;
use pencere_host::icon::{self, IconFormat};
use pencere_host::sandbox::{self, Directive, Domain, Grant, Permission};
use pencere_host::session::{
    Action, Config, MAX_LABEL_CHARS, Observed, Session, Size, State, ViewKind,
};
use serde_json::{Value, json};

/// The code of the one `closed` among what the host wrote, which has to be a refusal.
fn refusal(wrote: &[Message]) -> String {
    assert_eq!(wrote.len(), 1, "{wrote:?}");
    assert_eq!(wrote[0].kind, "closed");
    let body: Value = serde_json::from_str(&wrote[0].payload).unwrap();
    assert_eq!(body["reason"], "refused", "{body}");
    body["code"].as_str().unwrap().to_owned()
}

/// What an `open` of a page with `token` and the bundle of [`web_bundle`] is refused with, if
/// it is; `change` is given the payload to spoil. The program had a `hello` first.
fn refused_with(change: impl FnOnce(&mut Value)) -> String {
    let mut rig = Rig::new();
    let token = rig.hello();
    let mut body = open_web(&token, None, json!({}));
    change(&mut body);
    rig.json("open", &body);
    let (wrote, acted) = rig.take();
    assert!(acted.is_empty(), "{acted:?}");
    assert_eq!(rig.host.state(), State::Idle);
    refusal(&wrote)
}

fn text_of(file: &str) -> String {
    String::from_utf8(common::file(file)).unwrap()
}

/// The `closed` the host wrote, as `(reason, re)`, and its payload.
fn closed_of(wrote: &[Message]) -> (Value, Option<u32>) {
    let closed: Vec<&Message> = wrote.iter().filter(|m| m.kind == "closed").collect();
    assert_eq!(closed.len(), 1, "{wrote:?}");
    (
        serde_json::from_str(&closed[0].payload).unwrap(),
        closed[0].re(),
    )
}

/// A page whose `open` asks to reach an origin, up to the question to the person.
fn asked_about(rig: &mut Rig, access: Value) -> pencere_host::sandbox::Consent {
    let token = rig.hello();
    rig.json("open", &open_web(&token, Some(access), json!({})));
    rig.send_files();
    let (_, acted) = rig.take();
    let Some(Action::Consent(question)) =
        acted.into_iter().find(|a| matches!(a, Action::Consent(_)))
    else {
        panic!("no question");
    };
    question
}

fn connect() -> Value {
    json!({"connect": ["https://api.example.com"]})
}

// ---- the conversation of Appendix C --------------------------------------------------------------

#[test]
fn appendix_c_is_this_conversation_byte_for_byte() {
    let manifest = common::manifest();
    let entry = manifest["bridge"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["file"] == "bridge/context.json")
        .unwrap();
    // The host's store already holds index.html and style.css of the bundle.
    let held = Held(
        [
            "aeba6c19a632d2153a9e336a88b385d438c474a996d5be1408f6cd2ca6898e54",
            "72735d03f2b7e5a9ddb8ebe1c5fde2da88aa74292ba7ac608490a94f0ebbac0e",
        ]
        .iter()
        .map(|d| Digest::parse(d).unwrap())
        .collect(),
    );
    let mut host = Session::new(
        common::bridge_config(&entry["config"]),
        held,
        common::issuing(vec![common::token_bytes(TOKENS[0])]),
    );
    let feed = |host: &mut Session<_, _>, file: &str| {
        host.feed(&common::file(file));
        host.take_actions()
    };
    let to_program = |actions: &[Action], file: &str| {
        assert_eq!(
            String::from_utf8_lossy(&common::written(actions)),
            String::from_utf8_lossy(&common::file(file)),
            "{file}"
        );
    };
    // 1, 2: the query, and the answer, which comes before the answer to `CSI c`.
    let actions = feed(&mut host, "to-host/hello-fence.frame");
    to_program(&actions, "to-program/welcome.frame");
    // 3, 4: the open, and what the host lacks.
    let actions = feed(&mut host, "to-host/open-web.frame");
    to_program(&actions, "to-program/need.frame");
    // 5, 6: the file arrives, and the person is asked about what the page asked to reach.
    let actions = feed(&mut host, "to-host/file-chunked.frame");
    let [Action::Consent(question)] = actions.as_slice() else {
        panic!("{actions:?}");
    };
    assert_eq!(question.program.as_deref(), Some("containers"));
    assert_eq!(question.title.as_deref(), Some("Containers"));
    assert_eq!(question.permissions, vec![Permission::ClipboardWrite]);
    assert_eq!(question.domains.len(), 1);
    // The person allows the origin and not the capability.
    let mut grant = Grant::none();
    grant.allow_domain(
        Directive::Connect,
        Domain::parse("https://api.example.com").unwrap(),
    );
    host.consent(question.id, grant);
    let actions = host.take_actions();
    let start = started(&actions).unwrap();
    assert_eq!(start.kind, ViewKind::Web);
    assert_eq!(start.version, "1.0");
    assert_eq!(
        start.context.as_deref(),
        Some(text_of("bridge/context.json").as_str())
    );
    // 7: the page calls the program, and the host passes the call on as a `msg`, unread; the
    // page gives up another call.
    host.view_message(r#"{"call":{"id":1,"method":"list","params":{"all":true}}}"#);
    to_program(&host.take_actions(), "to-program/msg-call.frame");
    host.view_message(r#"{"cancel":{"id":2}}"#);
    to_program(&host.take_actions(), "to-program/msg-cancel.frame");
    // 8: the program answers, and the host passes the answer to the page.
    let actions = feed(&mut host, "to-host/msg-response.frame");
    assert_eq!(
        actions,
        vec![Action::ToView(
            r#"{"response":{"id":1,"result":{"containers":[{"id":"c1","name":"web","state":"running"}]}}}"#
                .to_owned()
        )]
    );
    // 9: ⌃C: the program says `bye`, and is answered, last.
    let actions = feed(&mut host, "to-host/bye.frame");
    assert!(actions.contains(&Action::DestroyView));
    to_program(&actions, "to-program/closed-program.frame");
    assert_eq!(host.state(), State::Ended);
}

// ---- the hello ------------------------------------------------------------------------------------

#[test]
fn a_welcome_offers_each_kind_at_the_versions_both_speak_in_the_programs_order() {
    let mut config =
        common::kit_config(&[(ViewKind::Wasm, &["0.4", "0.5"]), (ViewKind::Web, &["1.0"])]);
    config
        .views
        .push(pencere_host::session::Offer::new(ViewKind::Lua, &[]));
    let mut rig = Rig::with(config);
    let welcome = rig.hello_with(&json!([
        {"kind": "wasm", "versions": ["0.6", "0.5", "0.5", "0.4"]},
        {"kind": "lua", "versions": ["0.5"]},
        {"kind": "gopher", "versions": ["1.0"]},
        {"kind": "web", "versions": ["1.0", "0.9"]},
        {"kind": "wasm", "versions": ["0.4"]},
    ]));
    assert_eq!(
        welcome["views"],
        json!([
            {"kind": "wasm", "versions": ["0.5", "0.4"]},
            {"kind": "web", "versions": ["1.0"]},
        ])
    );
    // A kind listed twice counts once, at its first place, whatever the second says.
    let mut rig = Rig::new();
    let welcome = rig.hello_with(&json!([
        {"kind": "web", "versions": ["0.9"]},
        {"kind": "web", "versions": ["1.0"]},
    ]));
    assert_eq!(welcome["views"], json!([]));
    assert!(welcome["token"].is_string());
    // No version listed is no version in common.
    let welcome = rig.hello_with(&json!([{"kind": "web", "versions": []}]));
    assert_eq!(welcome["views"], json!([]));
}

#[test]
fn a_hello_gets_a_new_token_and_is_answered_only_when_the_pane_looks_as_a_waiting_program_leaves_it()
 {
    let hello = r#"{"versions":[1],"views":[{"kind":"web","versions":["1.0"]}]}"#;
    // Hello after hello: the first token is worthless.
    let mut rig = Rig::new();
    let first = rig.hello();
    let second = rig.hello();
    assert_ne!(first, second);
    rig.json("open", &open_web(&first, None, json!({})));
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
        rig.send("hello", None, None, hello);
        assert!(rig.take().0.is_empty(), "{observed:?}");
        assert_eq!(rig.host.state(), State::Idle);
        // Observed to be fine again, the same program is answered.
        rig.host.observe(Observed {
            alternate_screen: Some(true),
            shell_in_foreground: Some(false),
            tty_canonical_or_echo: Some(false),
            ..Observed::default()
        });
        rig.send("hello", None, None, hello);
        assert_eq!(rig.take().0.len(), 1);
    }
    // While a prompt is edited every frame is ignored, an open too.
    let mut rig = Rig::new();
    let token = rig.hello();
    rig.host.observe(Observed {
        prompt_editing: true,
        ..Observed::default()
    });
    rig.json("open", &open_web(&token, None, json!({})));
    assert!(rig.take().0.is_empty());
    assert_eq!(rig.host.state(), State::Offered);
    // A refusal is written where an answer would be, so it is held to the same checks: a stream
    // that only writes cannot have text typed into a shell by sending an `open` it has no token
    // for.
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
        let forged = open_web("AAAAAAAAAAAAAAAAAAAAAA", None, json!({}));
        rig.json("open", &forged);
        assert!(rig.take().0.is_empty(), "{observed:?}");
        // Seen to be fine, the same frame is refused as any other.
        rig.host.observe(Observed::default());
        rig.json("open", &forged);
        assert_eq!(refusal(&rig.take().0), "token");
    }
    // A hello that shares no protocol version is answered without a token.
    let mut rig = Rig::new();
    rig.send(
        "hello",
        None,
        None,
        r#"{"versions":[2,3],"views":[{"kind":"web","versions":["1.0"]}]}"#,
    );
    let (wrote, _) = rig.take();
    assert_eq!(
        wrote[0].payload,
        r#"{"supported":[1],"host":{"name":"pencere-test","version":"0.0.0"}}"#
    );
    assert_eq!(rig.host.state(), State::Idle);
    // A hello that is not one is ignored.
    for payload in [
        "{}",
        r#"{"versions":"1"}"#,
        r#"{"versions":[-1]}"#,
        r#"{"versions":[1]}"#,
        r#"{"versions":[1],"views":["web"]}"#,
        "x",
    ] {
        rig.send("hello", None, None, payload);
    }
    assert!(rig.take().0.is_empty());
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
        rig.json("open", &open_web(guess, None, json!({})));
        let (wrote, _) = rig.take();
        assert_eq!(refusal(&wrote), "token");
    }
    rig.send("open", None, None, r#"{"view":"web"}"#);
    rig.send("open", None, None, r#"{"token":7}"#);
    rig.send("open", None, None, "[]");
    assert_eq!(rig.take().0.len(), 3);
    assert_eq!(rig.host.state(), State::Offered);
    // The program that read its answer still can.
    rig.json("open", &open_web(&token, None, json!({})));
    assert_eq!(rig.host.state(), State::Opening);
}

// ---- refusing an open ------------------------------------------------------------------------------

#[test]
fn what_cannot_be_answered_is_refused_with_a_code_in_the_order_of_the_specification() {
    // `manifest`: the payload is malformed.
    assert_eq!(
        refused_with(|b| drop(b.as_object_mut().unwrap().remove("view"))),
        "manifest"
    );
    assert_eq!(refused_with(|b| b["view"] = json!(7)), "manifest");
    assert_eq!(
        refused_with(|b| drop(b.as_object_mut().unwrap().remove("version"))),
        "manifest"
    );
    assert_eq!(refused_with(|b| b["version"] = json!(1.0)), "manifest");
    assert_eq!(
        refused_with(|b| drop(b.as_object_mut().unwrap().remove("bundle"))),
        "manifest"
    );
    assert_eq!(
        refused_with(|b| b["bundle"]["entry"] = json!("missing.html")),
        "manifest"
    );
    assert_eq!(
        refused_with(|b| b["bundle"]["files"][0]["path"] = json!("../index.html")),
        "manifest"
    );
    assert_eq!(
        refused_with(|b| b["bundle"] = json!({"entry": "a"})),
        "manifest"
    );
    assert_eq!(
        refused_with(|b| b["bundle"]["files"][0]["type"] = json!("text/plain")),
        "manifest"
    );
    // The type of the entry is that of the kind, and the host infers nothing.
    assert_eq!(
        refused_with(|b| {
            b["view"] = json!("wasm");
            b["version"] = json!("0.5");
        }),
        "manifest"
    );
    // An `access` of the wrong shape.
    for access in [
        json!(5),
        json!([]),
        json!({"connect": "https://a.example"}),
        json!({"connect": [5]}),
        json!({"connect": null}),
        json!({"resources": {}}),
        json!({"permissions": "camera"}),
        json!({"permissions": [3]}),
    ] {
        assert_eq!(
            refused_with(|b| b["access"] = access.clone()),
            "manifest",
            "{access}"
        );
    }
    // `limit`: a limit of the bundle is exceeded.
    assert_eq!(
        refused_with(|b| b["bundle"]["files"][1]["size"] = json!(9 * 1024 * 1024)),
        "limit"
    );
    // `unsupported`: a kind the table has not got, or the welcome did not list, and a version
    // of a kind that it listed which it did not.
    assert_eq!(refused_with(|b| b["view"] = json!("gopher")), "unsupported");
    assert_eq!(refused_with(|b| b["version"] = json!("1.1")), "unsupported");
    assert_eq!(refused_with(|b| b["version"] = json!("")), "unsupported");
    // The checks that come first win: an `access` of the wrong shape is a `manifest` even for a
    // version nobody speaks, and so is a missing bundle for a kind nobody opens.
    assert_eq!(
        refused_with(|b| {
            b["access"] = json!(5);
            b["version"] = json!("9.9");
        }),
        "manifest"
    );
    assert_eq!(
        refused_with(|b| {
            b["view"] = json!("gopher");
            drop(b.as_object_mut().unwrap().remove("bundle"));
        }),
        "manifest"
    );
    // `policy`: the host's own policy refuses, after everything it can tell the program.
    let mut rig = Rig::new();
    rig.host.allow_opens(false);
    let token = rig.hello();
    rig.json("open", &open_web(&token, None, json!({"version": "9.9"})));
    assert_eq!(refusal(&rig.take().0), "unsupported");
    let token = rig.hello();
    rig.json("open", &open_web(&token, None, json!({})));
    assert_eq!(refusal(&rig.take().0), "policy");
    assert_eq!(rig.host.state(), State::Idle);
    // `screen` comes before the payload is read, `busy` before `screen`.
    let mut rig = Rig::new();
    rig.host.observe(Observed {
        alternate_screen: Some(false),
        ..Observed::default()
    });
    let token = rig.hello();
    rig.send("open", None, None, &format!(r#"{{"token":"{token}"}}"#));
    assert_eq!(refusal(&rig.take().0), "screen");
    // The token is spent whatever comes of the open.
    rig.json("open", &open_web(&token, None, json!({})));
    assert_eq!(refusal(&rig.take().0), "token");
}

#[test]
fn a_second_view_cannot_open_while_one_is_opening_or_open_and_the_first_goes_on() {
    let mut rig = Rig::new();
    let first = rig.hello();
    rig.json("open", &open_web(&first, None, json!({})));
    rig.take();
    let second = rig.hello();
    rig.json("open", &open_web(&second, None, json!({})));
    assert_eq!(refusal(&rig.take().0), "busy");
    assert_eq!(rig.host.state(), State::Opening);
    // The first goes on: its files are still wanted, and it starts.
    rig.send_files();
    let (_, acted) = rig.take();
    assert!(started(&acted).is_some());
    // The same while it runs.
    let third = rig.hello();
    rig.json("open", &open_web(&third, None, json!({})));
    assert_eq!(refusal(&rig.take().0), "busy");
    assert_eq!(rig.host.state(), State::Open);
}

// ---- starting a view --------------------------------------------------------------------------------

#[test]
fn a_page_that_asks_for_nothing_starts_at_once_with_the_restrictive_policy() {
    let mut rig = Rig::new();
    let start = rig.up_web(None);
    assert_eq!(start.kind, ViewKind::Web);
    assert_eq!(start.version, "1.0");
    assert_eq!(start.policy, sandbox::DEFAULT_POLICY);
    assert_eq!(start.allow, "");
    assert!(start.grant.is_empty());
    assert_eq!(start.program.as_deref(), Some("containers"));
    assert_eq!(start.title.as_deref(), Some("Containers"));
    assert!(start.icons.is_empty());
    assert!(start.context.as_deref().unwrap().starts_with(
        r##"{"context":{"mode":"dark","colors":{"background":"#000000","foreground":"#eeeeee","##
    ));
    assert_eq!(rig.host.state(), State::Open);
    // The bundle is served from the store, each file with the type of its manifest.
    let (media, bytes) = rig.host.serve("app.js").unwrap();
    assert_eq!(
        (media.as_str(), &*bytes),
        ("text/javascript", APP.as_bytes())
    );
    assert!(rig.host.serve("secret.txt").is_none());
    // An `access` that asks for nothing is an `access` that asks for nothing: no question.
    for access in [
        json!({}),
        json!({"connect": []}),
        json!({"connect": ["not an origin", "ftp://x.example"], "permissions": ["teleport"]}),
    ] {
        let mut rig = Rig::new();
        let start = rig.up_web(Some(access.clone()));
        assert_eq!(start.policy, sandbox::DEFAULT_POLICY, "{access}");
    }
    // The files held already, the view starts at the `need` and nothing is waited for.
    let token = rig.hello();
    rig.host.close_view();
    rig.take();
    rig.json("open", &open_web(&token, None, json!({})));
    let (wrote, acted) = rig.take();
    assert_eq!(wrote[0].payload, r#"{"missing":[]}"#);
    assert!(started(&acted).is_some());
}

#[test]
fn a_page_that_asks_to_reach_something_is_not_started_before_the_person_has_answered() {
    let access = json!({
        "connect": ["https://api.example.com", "not an origin"],
        "resources": ["https://cdn.example.com"],
        "permissions": ["camera", "clipboardWrite", "teleport"],
    });
    let mut rig = Rig::new();
    let question = asked_about(&mut rig, access.clone());
    assert_eq!(rig.host.state(), State::Opening);
    assert_eq!(question.id, 1);
    assert_eq!(question.program.as_deref(), Some("containers"));
    assert_eq!(question.title.as_deref(), Some("Containers"));
    assert_eq!(question.domains.len(), 2);
    assert_eq!(
        question.permissions,
        vec![Permission::Camera, Permission::ClipboardWrite]
    );
    // The page is not served before it starts.
    assert!(rig.host.serve("index.html").is_none());
    // The answer is cut down to what the page asked for, whatever it says.
    let mut greedy = question.grant_all();
    greedy.allow_domain(
        Directive::Connect,
        Domain::parse("https://evil.example").unwrap(),
    );
    greedy.allow_permission(Permission::Microphone);
    let mut some = Grant::none();
    some.allow_domain(
        Directive::Connect,
        Domain::parse("https://api.example.com").unwrap(),
    );
    some.allow_permission(Permission::Camera);
    some.allow_permission(Permission::Microphone);
    for (answer, expected_policy, expected_allow, expected_access) in [
        (
            greedy,
            Some("connect-src 'self' https://api.example.com;"),
            "camera clipboard-write",
            Some(
                r#""access":{"connect":["https://api.example.com"],"resources":["https://cdn.example.com"],"permissions":["camera","clipboardWrite"]}"#,
            ),
        ),
        (
            some,
            Some("connect-src 'self' https://api.example.com;"),
            "camera",
            Some(r#""access":{"connect":["https://api.example.com"],"permissions":["camera"]}"#),
        ),
        (Grant::none(), None, "", None),
    ] {
        let mut rig = Rig::new();
        let question = asked_about(&mut rig, access.clone());
        rig.host.consent(question.id, answer);
        let (_, acted) = rig.take();
        let start = started(&acted).unwrap();
        assert_eq!(rig.host.state(), State::Open);
        match expected_policy {
            Some(piece) => {
                assert!(start.policy.contains(piece), "{}", start.policy);
                assert!(!start.policy.contains("evil"));
                assert!(
                    start
                        .policy
                        .contains("script-src 'self' 'unsafe-inline' https://cdn.example.com")
                        == expected_access.unwrap().contains("resources")
                );
            }
            None => assert_eq!(start.policy, sandbox::DEFAULT_POLICY),
        }
        assert_eq!(start.allow, expected_allow);
        let context = start.context.as_deref().unwrap();
        match expected_access {
            Some(access) => assert!(context.contains(access), "{context}"),
            None => assert!(!context.contains("access"), "{context}"),
        }
    }
}

#[test]
fn an_answer_to_a_question_that_was_closed_does_not_answer_the_next_one() {
    let mut rig = Rig::new();
    let first = asked_about(&mut rig, connect());
    // The program gives the view up and opens another, which asks the same thing.
    let token = rig.token.clone().unwrap();
    rig.send("dismiss", None, None, &dismiss(Some(&token)));
    rig.take();
    let second = asked_about(&mut rig, connect());
    assert_ne!(first.id, second.id);
    // The click on the first question comes late: it settles nothing.
    let ignored = rig.host.diagnostics().ignored;
    rig.host.consent(first.id, first.grant_all());
    let (wrote, acted) = rig.take();
    assert!(wrote.is_empty() && acted.is_empty());
    assert_eq!(rig.host.diagnostics().ignored, ignored + 1);
    assert_eq!(rig.host.state(), State::Opening);
    // The second question's own answer starts the view, once.
    rig.host.consent(second.id, second.grant_all());
    assert!(started(&rig.take().1).is_some());
    rig.host.consent(second.id, second.grant_all());
    assert!(rig.take().1.is_empty());
}

#[test]
fn a_question_is_taken_back_whatever_ends_the_open_it_was_for() {
    type End = fn(&mut Rig, &str);
    let ends: [(&str, End, &str); 6] = [
        (
            "the program's dismiss",
            |rig, token| {
                rig.send("dismiss", None, None, &dismiss(Some(token)));
            },
            "program",
        ),
        (
            "the program's bye",
            |rig, token| {
                rig.send("bye", None, None, &bye(Some(token)));
            },
            "program",
        ),
        (
            "the person closing the view",
            |rig, _| rig.host.close_view(),
            "user",
        ),
        (
            "the program ending",
            |rig, _| rig.host.program_ended(),
            "none",
        ),
        ("a restart", |rig, _| rig.host.restart(), "none"),
        (
            "the wait running out",
            |rig, _| rig.host.timed_out(),
            "error",
        ),
    ];
    for (name, end, reason) in ends {
        let mut rig = Rig::new();
        let question = asked_about(&mut rig, connect());
        let token = rig.token.clone().unwrap();
        end(&mut rig, &token);
        let (wrote, acted) = rig.take();
        let withdrawn: Vec<u32> = acted
            .iter()
            .filter_map(|a| match a {
                Action::WithdrawConsent(id) => Some(*id),
                _ => None,
            })
            .collect();
        assert_eq!(withdrawn, vec![question.id], "{name}");
        // No view ran: nothing to tear down.
        assert!(!acted.contains(&Action::DestroyView), "{name}");
        if reason != "none" {
            let (body, _) = closed_of(&wrote);
            assert_eq!(body["reason"], reason, "{name}");
        }
        // And an answer on its way settles nothing.
        rig.host.consent(question.id, question.grant_all());
        assert!(rig.take().1.is_empty(), "{name}");
    }
    // A question that was answered is not taken back when the view ends.
    let mut rig = Rig::new();
    let question = asked_about(&mut rig, connect());
    rig.host.consent(question.id, question.grant_all());
    rig.take();
    rig.host.close_view();
    let (_, acted) = rig.take();
    assert_eq!(acted, vec![Action::DestroyView]);
}

#[test]
fn the_names_the_program_gave_are_cut_and_cleaned_before_the_terminal_has_them() {
    let mut rig = Rig::new();
    let long = "é".repeat(MAX_LABEL_CHARS + 50);
    let token = rig.hello();
    rig.json(
        "open",
        &open_web(
            &token,
            None,
            json!({"program": "con\u{1b}[2Jtai\u{202e}ners\n", "title": long}),
        ),
    );
    rig.send_files();
    let (_, acted) = rig.take();
    let start = started(&acted).unwrap();
    assert_eq!(start.program.as_deref(), Some("con[2Jtainers"));
    assert_eq!(
        start.title.as_deref().unwrap().chars().count(),
        MAX_LABEL_CHARS
    );
    // A name that is not one, or has nothing left, is no name, and the open goes on.
    for (program, title) in [(json!(7), json!(null)), (json!("\u{1b}\u{7}"), json!("  "))] {
        let mut rig = Rig::new();
        let token = rig.hello();
        rig.json(
            "open",
            &open_web(&token, None, json!({"program": program, "title": title})),
        );
        rig.send_files();
        let (_, acted) = rig.take();
        let start = started(&acted).unwrap();
        assert_eq!((&start.program, &start.title), (&None, &None));
    }
    // The question names them the same way.
    let mut rig = Rig::new();
    let token = rig.hello();
    rig.json(
        "open",
        &open_web(
            &token,
            Some(connect()),
            json!({"program": "pro\u{202e}gram", "title": "ti\ntle"}),
        ),
    );
    rig.send_files();
    let (_, acted) = rig.take();
    let Some(Action::Consent(question)) = acted.first() else {
        panic!("{acted:?}");
    };
    assert_eq!(question.program.as_deref(), Some("program"));
    assert_eq!(question.title.as_deref(), Some("title"));
}

/// A PNG of `side` pixels, with no pixels in it: a header, for the icon module to read.
fn png_uri(side: u32) -> String {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.extend_from_slice(&[0, 0, 0, 13]);
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&side.to_be_bytes());
    bytes.extend_from_slice(&side.to_be_bytes());
    bytes.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
    // An empty image-data chunk follows the header: all the icon module reads.
    bytes.extend_from_slice(&[0, 0, 0, 0]);
    bytes.extend_from_slice(b"IDAT");
    bytes.extend_from_slice(&[0, 0, 0, 0]);
    format!("data:image/png;base64,{}", base64::encode(&bytes))
}

#[test]
fn the_icons_of_the_program_reach_the_terminal_and_the_ones_it_cannot_take_do_not_stop_the_open() {
    let icons = json!([
        {"src": png_uri(16), "mimeType": "image/png", "sizes": ["16x16"], "theme": "dark"},
        {"src": "https://example.com/icon.png"},
        {"src": png_uri(64)},
        {"src": png_uri(4096)},
    ]);
    let mut rig = Rig::new();
    let token = rig.hello();
    rig.json("open", &open_web(&token, None, json!({"icons": icons})));
    rig.send_files();
    let (_, acted) = rig.take();
    let start = started(&acted).unwrap();
    let widths: Vec<u32> = start.icons.iter().map(|i| i.width).collect();
    assert_eq!(widths, vec![16, 64]);
    assert_eq!(start.icons[0].format, IconFormat::Png);
    assert_eq!(
        icon::pick(&start.icons, true, 16).map(|i| i.width),
        Some(16)
    );
    // An `icons` that is not a list says nothing, and is no reason to refuse.
    for icons in [json!("x"), json!({"src": png_uri(16)}), json!(3)] {
        let mut rig = Rig::new();
        let token = rig.hello();
        rig.json("open", &open_web(&token, None, json!({"icons": icons})));
        rig.send_files();
        let (_, acted) = rig.take();
        assert!(started(&acted).unwrap().icons.is_empty());
    }
    // The question the terminal asks in its own name carries none of them: the type has no
    // place for a picture.
    let mut rig = Rig::new();
    let token = rig.hello();
    rig.json(
        "open",
        &open_web(
            &token,
            Some(connect()),
            json!({"icons": [{"src": png_uri(16)}]}),
        ),
    );
    rig.send_files();
    let (_, acted) = rig.take();
    assert!(matches!(acted.as_slice(), [Action::Consent(_)]));
}

// ---- the files of a bundle ----------------------------------------------------------------------

#[test]
fn an_open_names_the_files_it_lacks_and_ends_with_an_error_when_one_is_wrong_or_late() {
    // A file that is not what its digest says ends the open.
    let mut rig = Rig::new();
    let token = rig.hello();
    rig.json("open", &open_web(&token, None, json!({})));
    let open_id = 2;
    let (wrote, _) = rig.take();
    assert_eq!(wrote[0].kind, "need");
    let need: Value = serde_json::from_str(&wrote[0].payload).unwrap();
    assert_eq!(need["missing"].as_array().unwrap().len(), 3);
    rig.send(
        "file",
        None,
        Some(&Digest::of(INDEX.as_bytes()).to_string()),
        &base64::encode(b"not the index"),
    );
    let (wrote, acted) = rig.take();
    let (body, re) = closed_of(&wrote);
    assert_eq!(body, json!({"reason": "error", "code": "bundle"}));
    assert_eq!(re, Some(open_id));
    assert!(acted.is_empty(), "no view ran: {acted:?}");
    assert_eq!(rig.host.state(), State::Idle);
    // Files nobody asked for, twice, or not base64, are ignored, and the open goes on.
    let mut rig = Rig::new();
    let token = rig.hello();
    rig.json("open", &open_web(&token, None, json!({})));
    rig.take();
    rig.send(
        "file",
        None,
        Some(&Digest::of(b"surprise").to_string()),
        &base64::encode(b"surprise"),
    );
    assert!(rig.host.store().is_empty());
    rig.send_file(INDEX);
    rig.send_file(INDEX);
    rig.send("file", None, None, "eA==");
    assert_eq!(rig.host.store().len(), 1);
    assert_eq!(rig.host.state(), State::Opening);
    assert_eq!(rig.host.diagnostics().ignored, 3);
    // The wait runs out: the open ends with `timeout`.
    rig.host.timed_out();
    let (body, re) = closed_of(&rig.take().0);
    assert_eq!(body, json!({"reason": "error", "code": "timeout"}));
    assert_eq!(re, Some(2));
    assert_eq!(rig.host.state(), State::Idle);
    // Nothing is waited for in any other state.
    rig.host.timed_out();
    assert!(rig.take().0.is_empty());
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
    let mut host = Session::new(web_config(), Forgetful::default(), move || {
        tokens.next().unwrap_or([9; 16])
    });
    let mut ids = pencere_host::frame::Ids::new();
    let mut send =
        |host: &mut Session<Forgetful, _>, kind: &str, d: Option<&str>, payload: &str| {
            let mut bytes = Vec::new();
            let message = pencere_host::frame::Outgoing {
                kind,
                id: ids.next_id(),
                re: None,
                digest: d,
                payload,
            };
            pencere_host::frame::encode(
                pencere_host::frame::Direction::ToHost,
                &message,
                pencere_host::frame::Split::Fill,
                &mut bytes,
            )
            .unwrap();
            host.feed(&bytes);
        };
    send(
        &mut host,
        "hello",
        None,
        r#"{"versions":[1],"views":[{"kind":"web","versions":["1.0"]}]}"#,
    );
    send(
        &mut host,
        "open",
        None,
        &open_web(TOKENS[0], None, json!({})).to_string(),
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

// ---- closing --------------------------------------------------------------------------------------

#[test]
fn the_person_closing_the_view_is_instant_and_the_program_is_told() {
    let mut rig = Rig::new();
    rig.up_web(None);
    rig.host.close_view();
    let (wrote, acted) = rig.take();
    assert_eq!(acted, vec![Action::DestroyView]);
    let (body, re) = closed_of(&wrote);
    assert_eq!((body, re), (json!({"reason": "user"}), None));
    assert_eq!(rig.host.state(), State::Idle);
    assert!(rig.host.bundle().is_none());
    // Nothing to close, nothing said; and the program, still there, may go on.
    rig.host.close_view();
    assert!(rig.take().0.is_empty());
    assert!(!rig.hello().is_empty());
    // An open that is still waiting for its files is closed the same way, and answered.
    let mut rig = Rig::new();
    let token = rig.hello();
    rig.json("open", &open_web(&token, None, json!({})));
    rig.take();
    rig.host.close_view();
    let (wrote, acted) = rig.take();
    assert!(acted.is_empty());
    let (body, re) = closed_of(&wrote);
    assert_eq!((body, re), (json!({"reason": "user"}), Some(2)));
}

/// The one frame the host writes for a program that said `bye`: the `closed` that answers it
/// (§10.3), and nothing else.
fn assert_bye_answered(wrote: &[Message], bye: u32) {
    assert_eq!(wrote.len(), 1, "{wrote:?}");
    assert_eq!(wrote[0].kind, "closed");
    assert_eq!(wrote[0].re(), Some(bye), "{wrote:?}");
    assert_eq!(wrote[0].payload, r#"{"reason":"program"}"#);
}

#[test]
fn a_bye_ends_the_view_only_with_the_token_that_opened_it() {
    let mut rig = Rig::new();
    rig.up_web(None);
    let opened = rig.token.clone().unwrap();
    // Without a token, with another, with a wrong kind of value, and from a hello that came
    // later: nothing happens. The stream may be written by anyone.
    let later = rig.hello();
    for payload in [
        bye(None),
        bye(Some("AAAAAAAAAAAAAAAAAAAAAA")),
        bye(Some(&later)),
        r#"{"token":7}"#.to_owned(),
        "[]".to_owned(),
        "{".to_owned(),
    ] {
        rig.send("bye", None, None, &payload);
        let (wrote, acted) = rig.take();
        assert!(wrote.is_empty() && acted.is_empty(), "{payload}");
        assert_eq!(rig.host.state(), State::Open);
    }
    // With the token the view was opened with, the view is gone, and the program is answered.
    let id = rig.send("bye", None, None, &bye(Some(&opened)));
    let (wrote, acted) = rig.take();
    assert_eq!(acted, vec![Action::DestroyView]);
    assert_bye_answered(&wrote, id);
    assert_eq!(rig.host.state(), State::Ended);
    // Nothing is written once the program has ended, until a hello says it reads again.
    rig.host.close_view();
    rig.page(r#"{"call":{"id":1,"method":"late"}}"#);
    rig.json("open", &open_web(&later, None, json!({})));
    assert!(rig.take().0.is_empty());
    rig.send(
        "hello",
        None,
        None,
        r#"{"versions":[1],"views":[{"kind":"web","versions":["1.0"]}]}"#,
    );
    assert_eq!(rig.take().0.len(), 1);
    assert_eq!(rig.host.state(), State::Offered);
}

#[test]
fn a_bye_when_no_view_is_opening_or_open_changes_nothing() {
    let mut rig = Rig::new();
    let token = rig.hello();
    // The token of a hello is not the token of a view.
    rig.send("bye", None, None, &bye(Some(&token)));
    assert!(rig.take().0.is_empty());
    assert_eq!(rig.host.state(), State::Offered);
    // After the view is gone its token is forgotten.
    let mut rig = Rig::new();
    rig.up_web(None);
    let opened = rig.token.clone().unwrap();
    rig.host.close_view();
    rig.take();
    rig.send("bye", None, None, &bye(Some(&opened)));
    assert!(rig.take().0.is_empty());
    assert_eq!(rig.host.state(), State::Idle);
}

#[test]
fn a_bye_ends_an_open_that_is_still_waiting_for_files() {
    let mut rig = Rig::new();
    let token = rig.hello();
    rig.json("open", &open_web(&token, None, json!({})));
    rig.take();
    // An `open` that was refused as busy did not take the place of the token.
    let other = rig.hello();
    rig.json("open", &open_web(&other, None, json!({})));
    assert_eq!(refusal(&rig.take().0), "busy");
    rig.send("bye", None, None, &bye(Some(&other)));
    assert!(rig.take().0.is_empty());
    assert_eq!(rig.host.state(), State::Opening);
    let id = rig.send("bye", None, None, &bye(Some(&token)));
    let (wrote, acted) = rig.take();
    assert_bye_answered(&wrote, id);
    assert!(acted.is_empty(), "no view ran: {acted:?}");
    assert_eq!(rig.host.state(), State::Ended);
}

#[test]
fn a_dismiss_closes_the_view_and_the_program_is_told_once_it_is_gone() {
    let mut rig = Rig::new();
    rig.up_web(None);
    let opened = rig.token.clone().unwrap();
    // The token of another hello, none, or the wrong kind of value: nothing happens.
    let later = rig.hello();
    for payload in [
        dismiss(None),
        dismiss(Some(&later)),
        r#"{"token":3}"#.to_owned(),
    ] {
        rig.send("dismiss", None, None, &payload);
        let (wrote, acted) = rig.take();
        assert!(wrote.is_empty() && acted.is_empty(), "{payload}");
    }
    assert_eq!(rig.host.state(), State::Open);
    let id = rig.send("dismiss", None, None, &dismiss(Some(&opened)));
    let (wrote, acted) = rig.take();
    assert_eq!(acted, vec![Action::DestroyView]);
    let (body, re) = closed_of(&wrote);
    assert_eq!((body, re), (json!({"reason": "program"}), Some(id)));
    // The pane is idle; the program is still there, and the token it got meanwhile is good.
    assert_eq!(rig.host.state(), State::Offered);
    rig.json("open", &open_web(&later, None, json!({})));
    let (wrote, acted) = rig.take();
    assert_eq!(wrote[0].payload, r#"{"missing":[]}"#);
    assert!(started(&acted).is_some());
    // A view that is still opening is given up without a view to destroy.
    let mut rig = Rig::new();
    let token = rig.hello();
    rig.json("open", &open_web(&token, None, json!({})));
    rig.take();
    let id = rig.send("dismiss", None, None, &dismiss(Some(&token)));
    let (wrote, acted) = rig.take();
    assert!(acted.is_empty());
    assert_eq!(closed_of(&wrote), (json!({"reason": "program"}), Some(id)));
    // A dismiss when nothing is opening or open changes nothing.
    rig.send("dismiss", None, None, &dismiss(Some(&token)));
    assert!(rig.take().0.is_empty());
}

#[test]
fn a_token_given_while_a_view_runs_is_still_good_after_the_view_closes() {
    for end in [
        (|rig: &mut Rig| rig.host.close_view()) as fn(&mut Rig),
        |rig| rig.page(r#"{"close":{}}"#),
        |rig| rig.host.view_failed("it broke"),
    ] {
        let mut rig = Rig::new();
        rig.up_web(None);
        let later = rig.hello();
        end(&mut rig);
        rig.take();
        rig.json("open", &open_web(&later, None, json!({})));
        let (wrote, acted) = rig.take();
        assert_eq!(wrote[0].kind, "need");
        assert!(started(&acted).is_some());
    }
    // The end of the program ends the token it had, and a hello says there is one again.
    let mut rig = Rig::new();
    rig.up_web(None);
    rig.hello();
    rig.host.program_ended();
    rig.take();
    assert_eq!(rig.host.state(), State::Ended);
    let later = rig.hello();
    assert_eq!(rig.host.state(), State::Offered);
    rig.json("open", &open_web(&later, None, json!({})));
    assert!(
        rig.take()
            .1
            .iter()
            .any(|a| matches!(a, Action::StartView(_)))
    );
}

#[test]
fn a_failed_page_closes_with_the_words_it_failed_with() {
    let mut rig = Rig::new();
    rig.up_web(None);
    rig.host.view_failed("the web view's process ended");
    let (wrote, acted) = rig.take();
    assert_eq!(acted, vec![Action::DestroyView]);
    assert_eq!(
        closed_of(&wrote).0,
        json!({"reason": "error", "code": "view", "message": "the web view's process ended"})
    );
}

// ---- restart -----------------------------------------------------------------------------------

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
    rig.json("open", &open_web(TOKENS[1], None, json!({})));
    assert_eq!(rig.host.state(), State::Opening);
}

#[test]
fn a_restart_while_a_view_runs_closes_it_at_once_without_telling_the_program_it_was_closed() {
    let mut rig = Rig::new();
    rig.up_web(None);
    rig.host.restart();
    let (wrote, acted) = rig.take();
    assert_eq!(acted, vec![Action::DestroyView]);
    assert_eq!(wrote.len(), 1);
    let body: Value = serde_json::from_str(&wrote[0].payload).unwrap();
    assert_eq!(body["restart"], true);
    assert_eq!(wrote[0].re(), None);
    assert_eq!(body["views"], json!([{"kind": "web", "versions": ["1.0"]}]));
    assert_eq!(rig.host.state(), State::Offered);
    // The offer expires when the wait the host chose does.
    rig.host.timed_out();
    assert_eq!(rig.host.state(), State::Idle);
    // Told twice, the second offer replaces the first token.
    rig.host.restart();
    rig.host.restart();
    let (wrote, _) = rig.take();
    assert_eq!(wrote.len(), 2);
    // A restart is not offered where a hello would not be answered.
    let mut rig = Rig::new();
    rig.host.observe(Observed {
        shell_in_foreground: Some(true),
        ..Observed::default()
    });
    rig.host.restart();
    assert!(rig.take().0.is_empty());
}

// ---- what the host writes -----------------------------------------------------------------------

#[test]
fn the_ids_of_what_the_host_writes_count_up_by_one_across_every_kind() {
    let mut rig = Rig::new();
    rig.up_web(None);
    let token = rig.token.clone().unwrap();
    rig.page(r#"{"call":{"id":1,"method":"m"}}"#);
    rig.send("bye", None, None, &bye(Some(&token)));
    let mut ids = Vec::new();
    let mut rig2 = Rig::new();
    rig2.send(
        "hello",
        None,
        None,
        r#"{"versions":[1],"views":[{"kind":"web","versions":["1.0"]}]}"#,
    );
    rig2.json("open", &open_web(TOKENS[0], None, json!({})));
    rig2.send_files();
    rig2.page(r#"{"call":{"id":1,"method":"m"}}"#);
    let (wrote, _) = rig2.take();
    ids.extend(wrote.iter().map(|m| m.id));
    assert_eq!(ids, vec![1, 2, 3]);
    assert_eq!(
        wrote.iter().map(|m| m.kind.as_str()).collect::<Vec<_>>(),
        vec!["welcome", "need", "msg"]
    );
    assert_eq!(wrote[0].re(), Some(1));
    assert_eq!(wrote[1].re(), Some(2));
    assert_eq!(wrote[2].re(), None);
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
    config.view.locale = Some("tr-\u{e9}\u{2603}\n".into());
    config.view.time_zone = Some("Europe/\u{7f}".into());
    let mut rig = Rig::with(config);
    let start = rig.up_web(None);
    // The context is JSON text for a web view; it keeps its letters as escapes, too.
    assert!(pencere_host::json::is_frame_text(
        start.context.as_deref().unwrap()
    ));
    let context: Value = serde_json::from_str(start.context.as_deref().unwrap()).unwrap();
    assert_eq!(context["context"]["locale"], "tr-\u{e9}\u{2603}\n");
    rig.page(&json!({"call": {"id": 1, "method": "m", "params": {"text": "caf\u{e9} \u{1f600}\n\u{7f}"}}}).to_string());
    let (wrote, _) = rig.take();
    for message in &wrote {
        assert!(
            pencere_host::json::is_frame_text(&message.payload),
            "{message:?}"
        );
    }
    let value: Value = serde_json::from_str(&wrote[0].payload).unwrap();
    assert_eq!(
        value["call"]["params"]["text"],
        "caf\u{e9} \u{1f600}\n\u{7f}"
    );
    // The program's end, the only frame left to write, is ASCII as well.
    let token = rig.token.clone().unwrap();
    rig.send("bye", None, None, &bye(Some(&token)));
    for message in rig.take().0 {
        assert!(pencere_host::json::is_frame_text(&message.payload));
    }
}

#[test]
fn messages_at_the_wrong_moment_or_from_the_wrong_side_are_ignored() {
    let mut rig = Rig::new();
    // Nothing is open: a file, a msg, a closed, a welcome, a need and a kind nobody knows.
    rig.send("file", None, Some(&Digest::of(b"x").to_string()), "eA==");
    rig.send("msg", None, None, r#"{"response":{"id":1,"result":{}}}"#);
    rig.send("closed", None, None, "{}");
    rig.send("welcome", None, None, "{}");
    rig.send("need", None, None, "{}");
    rig.send("zzz", None, None, "{}");
    rig.send("file", None, None, "eA==");
    assert!(rig.take().0.is_empty());
    assert_eq!(rig.host.state(), State::Idle);
    assert!(rig.host.diagnostics().ignored >= 7);
    // A view message when no view runs is nobody's.
    rig.page(r#"{"call":{"id":1,"method":"m"}}"#);
    rig.host.module_message(r#"{"call":{"id":1,"method":"m"}}"#);
    rig.host.link_decision(1, true);
    rig.host.consent(1, Grant::none());
    rig.host.view_failed("late");
    assert!(rig.take().0.is_empty());
    assert_eq!(rig.host.state(), State::Idle);
}

// ---- any order of messages ---------------------------------------------------------------------

/// A small deterministic dice (xorshift): the crate has no randomness and neither do its tests.
struct Dice(u64);

impl Dice {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
}

#[test]
fn a_session_survives_any_order_of_messages_and_only_ever_writes_frames_a_tty_may_carry() {
    let digests: Vec<String> = [INDEX, APP, STYLE]
        .iter()
        .map(|b| Digest::of(b.as_bytes()).to_string())
        .collect();
    let pages = [
        r#"{"call":{"id":1,"method":"list"}}"#,
        r#"{"context":{}}"#,
        r#"{"close":{}}"#,
        r#"{"link":{"url":"https://example.com/a"}}"#,
        r#"{"link":{"url":"file:///etc/passwd"}}"#,
        r#"{"cancel":{"id":1}}"#,
        "not json at all",
        "[1]",
    ];
    let programs = [
        r#"{"response":{"id":1,"result":{}}}"#,
        r#"{"context":{"mode":"light"}}"#,
        r#"{"push":{"id":1,"value":"caf\u00e9"}}"#,
        "[]",
        "{",
    ];
    let mut dice = Dice(0x2545_f491_4f6c_dd1d);
    let mut visited = std::collections::HashSet::new();
    let mut reached_a_page = false;
    for round in 0..2000 {
        let mut rig = Rig::new();
        // The token of the last open the program wrote, for a `bye` that says whose it is.
        let mut opened = String::new();
        for _ in 0..80 {
            let token = TOKENS[dice.below(TOKENS.len())];
            match dice.below(26) {
                0 | 1 => {
                    rig.hello();
                }
                2 | 3 => {
                    let token = rig
                        .token
                        .clone()
                        .filter(|_| dice.below(5) != 0)
                        .unwrap_or_else(|| token.to_owned());
                    let access = (dice.below(2) == 0).then(connect);
                    let mut body = open_web(&token, access, json!({}));
                    if dice.below(5) == 0 {
                        body["bundle"] = Value::Null;
                    }
                    rig.json("open", &body);
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
                8..=11 => rig.page(pages[dice.below(pages.len())]),
                12 | 13 => {
                    rig.send("msg", None, None, programs[dice.below(programs.len())]);
                }
                14 | 15 => {
                    let (_, acted) = rig.take();
                    for action in &acted {
                        match action {
                            Action::Consent(c) => rig.host.consent(c.id, c.grant_all()),
                            Action::OpenLink { link, .. } => {
                                rig.host.link_decision(*link, dice.below(2) == 0);
                            }
                            Action::ToView(text) => {
                                serde_json::from_str::<Value>(text).unwrap();
                            }
                            _ => {}
                        }
                    }
                }
                16 => rig.host.close_view(),
                17 => rig.host.program_ended(),
                18 => rig.host.restart(),
                19 => rig.host.timed_out(),
                20 => {
                    let payload = match dice.below(3) {
                        0 => "{}".to_owned(),
                        1 => dismiss(Some(token)),
                        _ => dismiss(Some(&opened)),
                    };
                    rig.send("dismiss", None, None, &payload);
                }
                21 => {
                    rig.host.set_size(Size {
                        width: 1 + dice.below(500) as u32,
                        height: 9,
                    });
                    rig.host.set_reduce_motion(dice.below(2) == 0);
                }
                22 => rig
                    .host
                    .set_locale(Some(format!("l{}", dice.below(3))), None),
                23 => rig.host.view_failed("it broke\n"),
                24 => {
                    rig.host
                        .module_message(programs[dice.below(programs.len())]);
                }
                _ => rig.host.link_decision(dice.below(9) as u32, true),
            }
            // Whatever the host wrote is frames, and only frames; whatever it gives the view is
            // JSON.
            rig.collect();
            for message in rig.wrote.drain(..) {
                assert!(
                    pencere_host::json::is_frame_text(&message.payload),
                    "round {round}: {message:?}"
                );
            }
            for action in &rig.acted {
                match action {
                    Action::ToView(text) => {
                        serde_json::from_str::<Value>(text).unwrap();
                    }
                    Action::StartView(_) => reached_a_page = true,
                    _ => {}
                }
            }
            rig.acted.clear();
            visited.insert(format!("{:?}", rig.host.state()));
        }
    }
    // The dice reached every state, so the survival is not vacuous.
    for state in ["Idle", "Offered", "Opening", "Open", "Ended"] {
        assert!(visited.contains(state), "never in {state}: {visited:?}");
    }
    assert!(reached_a_page);
}

#[test]
fn the_bundle_is_the_pages_and_only_while_it_lasts() {
    let mut rig = Rig::new();
    let token = rig.hello();
    rig.json("open", &open_web(&token, None, json!({})));
    // Opening: the manifest is there, nothing is served yet.
    assert_eq!(rig.host.bundle().unwrap().entry(), "index.html");
    assert!(rig.host.serve("index.html").is_none());
    rig.send_files();
    assert_eq!(rig.host.serve("index.html").unwrap().0, "text/html");
    assert_eq!(web_bundle()["files"].as_array().unwrap().len(), 3);
    rig.host.close_view();
    assert!(rig.host.bundle().is_none());
    assert!(rig.host.serve("index.html").is_none());
    // The store keeps what it verified, to be trimmed by the host.
    assert_eq!(rig.host.store().len(), 3);
    rig.host
        .store_mut()
        .insert(Verified::from_bytes(b"x".to_vec()));
    assert_eq!(rig.host.store().len(), 4);
}
