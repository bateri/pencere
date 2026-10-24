//! The bridge of a page (specification §9.4 to §9.7): every `bridge` entry of the fixtures'
//! manifest against a host that shows pages, and what grows from it: when the page is told its
//! context again, who may ask the person about a link, and what the host never writes to the
//! program once the page is gone.

// A test fails by panicking, and indexes what it has just built.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::{Rig, bridge_config, bye, dismiss, kit_config, started, web_config};
use pencere::bundle::{Store, Verified};
use pencere::sandbox::Grant;
use pencere::session::{Action, Config, Size, State, ViewKind};
use pencere::theme::{Rgb, Theme};
use serde_json::{Value, json};

/// A rig whose store already holds the files of the page, so that an `open` needs none.
fn holding(config: Config) -> Rig {
    let mut rig = Rig::with(config);
    for body in [common::INDEX, common::APP, common::STYLE] {
        rig.host
            .store_mut()
            .insert(Verified::from_bytes(body.as_bytes().to_vec()));
    }
    rig
}

fn text_of(file: &str) -> String {
    String::from_utf8(common::file(file)).unwrap()
}

#[test]
fn every_bridge_entry_is_what_the_host_does_with_it() {
    let manifest = common::manifest();
    let entries = manifest["bridge"].as_array().unwrap();
    assert_eq!(entries.len(), 11);
    for entry in entries {
        let file = entry["file"].as_str().unwrap();
        let text = text_of(file);
        match (
            entry["direction"].as_str().unwrap(),
            entry["outcome"].as_str().unwrap(),
        ) {
            ("host-to-page", "written") => {
                // The context the page is first told, once it is up, is the entry byte for byte
                // when the host is configured as the entry says.
                let config = &entry["config"];
                let mut rig = holding(bridge_config(config));
                let start = rig.up_web(config.get("access").cloned());
                assert_eq!(start.context.as_deref(), Some(text.as_str()), "{file}");
                assert_eq!(
                    rig.host.page_context().as_deref(),
                    Some(text.as_str()),
                    "{file}"
                );
            }
            ("page-to-host", outcome) => {
                let mut rig = Rig::new();
                rig.up_web(None);
                let before = *rig.host.diagnostics();
                rig.page(&text);
                let (wrote, acted) = rig.take();
                let after = *rig.host.diagnostics();
                match outcome {
                    "context" => {
                        assert!(wrote.is_empty(), "{file}");
                        let told = rig.host.page_context().unwrap();
                        assert_eq!(acted, vec![Action::ToView(told)], "{file}");
                        assert_eq!(
                            (after.read, after.ignored),
                            (before.read + 1, before.ignored)
                        );
                    }
                    "close" => {
                        assert_eq!(acted, vec![Action::DestroyView], "{file}");
                        assert_eq!(wrote.len(), 1, "{file}");
                        assert_eq!(wrote[0].kind, "closed");
                        assert_eq!(wrote[0].payload, r#"{"reason":"view"}"#);
                        assert_eq!(rig.host.state(), State::Idle);
                        assert_eq!(
                            (after.read, after.ignored),
                            (before.read + 1, before.ignored)
                        );
                    }
                    "link" => {
                        assert!(wrote.is_empty(), "{file}");
                        let [Action::OpenLink { url, .. }] = acted.as_slice() else {
                            panic!("{file}: {acted:?}");
                        };
                        assert_eq!(url, entry["url"].as_str().unwrap());
                        assert_eq!(
                            (after.read, after.ignored),
                            (before.read + 1, before.ignored)
                        );
                    }
                    "passed" => {
                        assert!(acted.is_empty(), "{file}");
                        assert_eq!(wrote.len(), 1, "{file}");
                        assert_eq!(wrote[0].kind, "msg");
                        assert_eq!(wrote[0].payload, text, "{file}");
                        assert_eq!((after.read, after.ignored), (before.read, before.ignored));
                    }
                    "dropped" => {
                        assert!(wrote.is_empty() && acted.is_empty(), "{file}");
                        assert_eq!(after.ignored, before.ignored + 1, "{file}");
                        assert_eq!(rig.host.state(), State::Open);
                    }
                    other => panic!("{file}: {other}"),
                }
            }
            ("program-to-page", outcome) => {
                let mut rig = Rig::new();
                rig.up_web(None);
                let before = rig.host.diagnostics().ignored;
                rig.send("msg", None, None, &text);
                let (wrote, acted) = rig.take();
                assert!(wrote.is_empty(), "{file}");
                match outcome {
                    "passed" => assert_eq!(acted, vec![Action::ToView(text)], "{file}"),
                    "dropped" => {
                        assert!(acted.is_empty(), "{file}");
                        assert_eq!(rig.host.diagnostics().ignored, before + 1, "{file}");
                    }
                    other => panic!("{file}: {other}"),
                }
            }
            other => panic!("{file}: {other:?}"),
        }
    }
}

#[test]
fn the_first_context_is_the_one_in_the_start_and_says_what_the_person_allowed() {
    let mut rig = Rig::new();
    let start = rig.up_web(Some(
        json!({"connect": ["https://api.example.com"], "permissions": ["camera"]}),
    ));
    let context: Value = serde_json::from_str(start.context.as_deref().unwrap()).unwrap();
    assert_eq!(
        context["context"]["access"],
        json!({"connect": ["https://api.example.com"], "permissions": ["camera"]})
    );
    // Allowed nothing, the member is left out; and so are a locale and a time zone the host does
    // not know.
    let mut rig = Rig::new();
    let token = rig.hello();
    rig.json(
        "open",
        &common::open_web(
            &token,
            Some(json!({"connect": ["https://a.example"]})),
            json!({}),
        ),
    );
    rig.send_files();
    let (_, acted) = rig.take();
    let Some(Action::Consent(question)) = acted.iter().find(|a| matches!(a, Action::Consent(_)))
    else {
        panic!("{acted:?}");
    };
    rig.host.consent(question.id, Grant::none());
    let (_, acted) = rig.take();
    let start = started(&acted).unwrap();
    assert_eq!(start.grant, Grant::none());
    let context: Value = serde_json::from_str(start.context.as_deref().unwrap()).unwrap();
    for absent in ["access", "locale", "timeZone"] {
        assert!(context["context"].get(absent).is_none(), "{absent}");
    }
    assert_eq!(context["context"]["reduceMotion"], false);
    assert_eq!(context["context"]["font"], "\"Menlo\", monospace");
}

#[test]
fn a_page_is_told_its_context_again_when_a_member_changes_and_only_then() {
    let mut rig = Rig::new();
    let start = rig.up_web(None);
    let first = start.context.unwrap();
    // Nothing changed: nothing is told.
    rig.host.set_size(Size {
        width: 800,
        height: 600,
    });
    rig.host.set_theme(common::theme());
    rig.host.set_locale(None, None);
    rig.host.set_reduce_motion(false);
    assert!(rig.viewed().is_empty());

    let told = |rig: &mut Rig| -> Value {
        let viewed = rig.viewed();
        assert_eq!(viewed.len(), 1, "{viewed:?}");
        serde_json::from_str(&viewed[0]).unwrap()
    };
    rig.host.set_size(Size {
        width: 640,
        height: 480,
    });
    let context = told(&mut rig);
    assert_eq!(
        context["context"]["size"],
        json!({"width": 640, "height": 480})
    );
    rig.host
        .set_theme(Theme::new(Rgb(0xff, 0xff, 0xff), Rgb(0, 0, 0), "monospace"));
    let context = told(&mut rig);
    assert_eq!(context["context"]["mode"], "light");
    assert_eq!(context["context"]["colors"]["background"], "#ffffff");
    assert_eq!(context["context"]["font"], "monospace");
    rig.host
        .set_locale(Some("tr-TR".into()), Some("Europe/Istanbul".into()));
    let context = told(&mut rig);
    assert_eq!(context["context"]["locale"], "tr-TR");
    assert_eq!(context["context"]["timeZone"], "Europe/Istanbul");
    rig.host.set_reduce_motion(true);
    let context = told(&mut rig);
    assert_eq!(context["context"]["reduceMotion"], true);
    // Back to where it began: told again, and the text is what the start had.
    rig.host.set_size(Size {
        width: 800,
        height: 600,
    });
    rig.host.set_theme(common::theme());
    rig.host.set_locale(None, None);
    rig.host.set_reduce_motion(false);
    let viewed = rig.viewed();
    assert_eq!(viewed.last(), Some(&first));
    // Nothing is written to the program for any of it.
    assert!(rig.take().0.is_empty());
    // Asked, the page is told the current one whether or not it changed.
    rig.page(r#"{"context":{}}"#);
    assert_eq!(rig.viewed(), vec![first]);
}

#[test]
fn a_context_is_for_a_page_and_for_a_page_that_runs() {
    // Nothing runs: nothing is told, and a request is not a message of anybody's.
    let mut rig = Rig::new();
    rig.host.set_size(Size {
        width: 1,
        height: 1,
    });
    rig.page(r#"{"context":{}}"#);
    assert!(rig.viewed().is_empty());
    assert_eq!(rig.host.page_context(), None);
    // A view the terminal draws itself is told by the kit, not by the host.
    let mut rig = Rig::with(kit_config(&[(ViewKind::Wasm, &["0.5"])]));
    let token = rig.hello_with(&json!([{"kind": "wasm", "versions": ["0.5"]}]))["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let body = b"\0asm\x01\0\0\0";
    let digest = pencere::bundle::Digest::of(body).to_string();
    rig.json(
        "open",
        &json!({"token": token, "view": "wasm", "version": "0.5", "bundle": {"entry": "v.wasm",
            "files": [{"path": "v.wasm", "sha256": digest, "size": body.len(), "type": "application/wasm"}]}}),
    );
    rig.send("file", None, Some(&digest), &pencere::base64::encode(body));
    let (_, acted) = rig.take();
    let start = started(&acted).unwrap();
    assert_eq!(start.context, None);
    rig.host.set_size(Size {
        width: 3,
        height: 3,
    });
    rig.host.set_reduce_motion(true);
    assert!(rig.viewed().is_empty());
    assert_eq!(rig.host.page_context(), None);
    // And a page's message channel is not the component's: it is dropped.
    rig.page(r#"{"call":{"id":1,"method":"list"}}"#);
    assert!(rig.take().0.is_empty());
}

#[test]
fn the_page_that_asks_for_its_context_gets_it_whatever_the_value() {
    let mut rig = Rig::new();
    rig.up_web(None);
    for request in [
        r#"{"context":{}}"#,
        r#"{"context":null}"#,
        r#"{"context":5}"#,
        r#"{"context":{"mode":"dark"}}"#,
        r#"{"context":[[[]]]}"#,
    ] {
        rig.page(request);
        let viewed = rig.viewed();
        assert_eq!(viewed.len(), 1, "{request}");
        assert!(viewed[0].starts_with(r#"{"context":{"mode":"#), "{request}");
    }
    assert!(rig.take().0.is_empty());
}

#[test]
fn a_page_that_closes_is_gone_at_once_and_the_program_is_told() {
    let mut rig = Rig::new();
    let start = rig.up_web(None);
    assert_eq!(start.kind, ViewKind::Web);
    assert!(rig.host.serve("index.html").is_some());
    rig.page(r#"{"close":{}}"#);
    let (wrote, acted) = rig.take();
    assert_eq!(acted, vec![Action::DestroyView]);
    assert_eq!(wrote.len(), 1);
    assert_eq!(
        (
            wrote[0].kind.as_str(),
            wrote[0].re(),
            wrote[0].payload.as_str()
        ),
        ("closed", None, r#"{"reason":"view"}"#)
    );
    assert_eq!(rig.host.state(), State::Idle);
    assert!(rig.host.serve("index.html").is_none());
    assert!(rig.host.bundle().is_none());
    // The view is gone: a `close` changes nothing, and nothing the page says is written.
    let ignored = rig.host.diagnostics().ignored;
    rig.page(r#"{"close":{}}"#);
    rig.page(r#"{"call":{"id":9,"method":"late"}}"#);
    assert!(rig.take().0.is_empty());
    assert_eq!(rig.host.diagnostics().ignored, ignored + 2);
    // The token of the view is forgotten with it: a `bye` with it is nobody's.
    let token = rig.token.clone().unwrap();
    rig.send("bye", None, None, &bye(Some(&token)));
    assert!(rig.take().0.is_empty());
    // And the program, still there, may ask again.
    assert!(!rig.hello().is_empty());
}

#[test]
fn a_page_asks_to_open_a_link_and_is_never_answered() {
    let mut rig = Rig::new();
    rig.up_web(None);
    rig.page(r#"{"link":{"url":"https://example.com/docs"}}"#);
    let (wrote, acted) = rig.take();
    assert!(wrote.is_empty());
    let [Action::OpenLink { link, url }] = acted.as_slice() else {
        panic!("{acted:?}");
    };
    assert_eq!(url, "https://example.com/docs");
    let link = *link;
    // Whatever the person says, the page hears nothing and the program neither.
    rig.host.link_decision(link, true);
    rig.host.link_decision(link, false);
    rig.host.link_decision(9999, true);
    let (wrote, acted) = rig.take();
    assert!(wrote.is_empty() && acted.is_empty());
}

#[test]
fn only_a_link_the_host_may_open_is_asked_about() {
    let mut rig = Rig::new();
    rig.up_web(None);
    let long = format!("https://example.com/{}", "a".repeat(8192));
    let at_the_limit = format!(
        "https://example.com/{}",
        "a".repeat(8192 - "https://example.com/".len())
    );
    for bad in [
        json!({"link": {"url": "file:///etc/passwd"}}),
        json!({"link": {"url": "javascript:alert(1)"}}),
        json!({"link": {"url": "ftp://example.com"}}),
        json!({"link": {"url": "https://exa mple.com"}}),
        json!({"link": {"url": "https://example.com/\u{e9}"}}),
        json!({"link": {"url": "https://example.com/\n"}}),
        json!({"link": {"url": long}}),
        json!({"link": {"url": 5}}),
        json!({"link": {}}),
        json!({"link": 5}),
        json!({"link": "https://example.com"}),
        json!({"link": ["https://example.com"]}),
    ] {
        let before = rig.host.diagnostics().ignored;
        rig.page(&bad.to_string());
        let (wrote, acted) = rig.take();
        assert!(wrote.is_empty() && acted.is_empty(), "{bad}");
        assert_eq!(rig.host.diagnostics().ignored, before + 1, "{bad}");
    }
    rig.page(&json!({"link": {"url": at_the_limit}}).to_string());
    assert_eq!(rig.take().1.len(), 1);
    // A host that opens no links opens none.
    let mut config = web_config();
    config.open_links = false;
    let mut rig = Rig::with(config);
    rig.up_web(None);
    rig.page(r#"{"link":{"url":"https://example.com"}}"#);
    let (wrote, acted) = rig.take();
    assert!(wrote.is_empty() && acted.is_empty());
}

#[test]
fn a_page_cannot_have_more_questions_open_than_the_host_allows() {
    let mut rig = Rig::new();
    rig.up_web(None);
    let mut asked = Vec::new();
    for n in 0..pencere::session::MAX_OPEN_LINKS {
        rig.page(&json!({"link": {"url": format!("https://example.com/{n}")}}).to_string());
        let (_, acted) = rig.take();
        let [Action::OpenLink { link, .. }] = acted.as_slice() else {
            panic!("{n}: {acted:?}");
        };
        asked.push(*link);
    }
    let ignored = rig.host.diagnostics().ignored;
    rig.page(r#"{"link":{"url":"https://example.com/one-too-many"}}"#);
    assert!(rig.take().1.is_empty());
    assert_eq!(rig.host.diagnostics().ignored, ignored + 1);
    // An answered question makes room; the ids are not reused for a question that is open.
    rig.host.link_decision(asked[3], false);
    rig.page(r#"{"link":{"url":"https://example.com/room"}}"#);
    let (_, acted) = rig.take();
    let [Action::OpenLink { link, .. }] = acted.as_slice() else {
        panic!("{acted:?}");
    };
    assert!(!asked.contains(link));
}

#[test]
fn the_names_of_the_hosts_own_messages_are_not_the_programs_to_use() {
    let mut rig = Rig::new();
    rig.up_web(None);
    let ignored = rig.host.diagnostics().ignored;
    let mut dropped = 0;
    let deep = format!(r#"{{"context":{}{}}}"#, "[".repeat(300), "]".repeat(300));
    for forged in [
        r#"{"context":{"mode":"dark"}}"#,
        r#"{"close":{}}"#,
        r#"{"link":{"url":"https://evil.example"}}"#,
        r#"{ "context" : {} }"#,
        // The name is the same after the escapes are read, which is how a page reads it.
        r#"{"cont\u0065xt":{}}"#,
        r#"{"\u0063lose":{}}"#,
        // One member as a page reads it: the same name twice.
        r#"{"context":{},"context":{"mode":"dark"}}"#,
        r#"{"close":{},"close":1}"#,
        // Too deep for the host to read is not a message it can tell the name of.
        deep.as_str(),
        // Not objects at all, or not JSON.
        r#"["context"]"#,
        r#""context""#,
        "7",
        "null",
        r#"{"context":"#,
    ] {
        rig.send("msg", None, None, forged);
        dropped += 1;
        let (wrote, acted) = rig.take();
        assert!(wrote.is_empty() && acted.is_empty(), "{forged}");
        assert_eq!(
            rig.host.diagnostics().ignored,
            ignored + dropped,
            "{forged}"
        );
    }
    // Any other object goes to the page as it is, the host's names among other members or not.
    for passed in [
        r#"{"response":{"id":1,"result":{}}}"#,
        r#"{"push":{"id":1,"value":{"context":{}}}}"#,
        r#"{"context":{},"call":{"id":1,"method":"m"}}"#,
        r#"{"contexts":{}}"#,
        r#"{"Context":{}}"#,
        r#"{}"#,
    ] {
        rig.send("msg", None, None, passed);
        let (wrote, acted) = rig.take();
        assert!(wrote.is_empty(), "{passed}");
        assert_eq!(acted, vec![Action::ToView(passed.to_owned())], "{passed}");
    }
}

#[test]
fn the_pages_messages_are_passed_unread_and_the_host_s_own_are_read_by_name_only() {
    let mut rig = Rig::new();
    rig.up_web(None);
    // A message whose body is a very large object is still no tree: it is passed as it is.
    let members: Vec<String> = (0..20_000).map(|n| format!(r#""m{n}":[{n}]"#)).collect();
    let big = format!("{{{}}}", members.join(","));
    assert!(big.len() < pencere::session::MAX_VIEW_MESSAGE);
    rig.page(&big);
    let (wrote, acted) = rig.take();
    assert!(acted.is_empty());
    // It is longer than a frame: it went out in several, as one message.
    assert_eq!(wrote.len(), 1);
    assert_eq!(wrote[0].payload, big);
}

#[test]
fn a_message_the_page_says_over_the_limit_ends_the_view_and_the_program_is_told() {
    let mut rig = Rig::new();
    rig.up_web(None);
    // The limit is on the text the page sent, byte for byte: this is exactly at it.
    let pad = "a".repeat(pencere::session::MAX_VIEW_MESSAGE - r#"{"pad":""}"#.len());
    let at = format!(r#"{{"pad":"{pad}"}}"#);
    assert_eq!(at.len(), pencere::session::MAX_VIEW_MESSAGE);
    rig.page(&at);
    let (wrote, acted) = rig.take();
    assert!(acted.is_empty());
    assert_eq!(wrote.len(), 1);
    assert_eq!(wrote[0].payload, at);
    // One byte more: nothing could answer it, so the view fails, as a view does that goes over a
    // limit it is held to, and the program is told in the words of that failure.
    let over = format!(r#"{{"pad":"{pad}a"}}"#);
    let ignored = rig.host.diagnostics().ignored;
    rig.page(&over);
    let (wrote, acted) = rig.take();
    assert_eq!(acted, vec![Action::DestroyView]);
    assert_eq!(wrote.len(), 1);
    assert_eq!(wrote[0].kind, "closed");
    assert_eq!(
        serde_json::from_str::<Value>(&wrote[0].payload).unwrap(),
        json!({"reason": "error", "code": "view", "message": "a message of the view was over 1 MiB"})
    );
    assert_eq!(rig.host.diagnostics().ignored, ignored + 1);
    assert_eq!(rig.host.state(), State::Idle);
    // The program's own message over the size is dropped: anyone who can write the stream can
    // send it, and ending the view for it would let them.
    let mut rig = Rig::new();
    rig.up_web(None);
    rig.send("msg", None, None, &over);
    let (wrote, acted) = rig.take();
    assert!(wrote.is_empty() && acted.is_empty());
    assert_eq!(rig.host.state(), State::Open);
}

#[test]
fn the_programs_message_is_held_to_the_limit_on_its_text_and_not_on_the_escapes_it_had_to_write() {
    // A program writes ASCII, so every `\u00e9` it writes is six bytes of the wire for the two of
    // the character: the limit is on the text, as it is for a page.
    let mut rig = Rig::new();
    rig.up_web(None);
    let push = |characters: usize| {
        format!(
            r#"{{"push":{{"id":1,"value":"{}"}}}}"#,
            "\\u00e9".repeat(characters)
        )
    };
    let within = push(500_000);
    assert!(within.len() > pencere::session::MAX_VIEW_MESSAGE);
    assert!(pencere::json::unescaped_len(&within) <= pencere::session::MAX_VIEW_MESSAGE);
    rig.send("msg", None, None, &within);
    assert_eq!(rig.take().1, vec![Action::ToView(within)]);
    // Over it, by the text: dropped, and the view goes on.
    let over = push(525_000);
    assert!(pencere::json::unescaped_len(&over) > pencere::session::MAX_VIEW_MESSAGE);
    rig.send("msg", None, None, &over);
    let (wrote, acted) = rig.take();
    assert!(wrote.is_empty() && acted.is_empty());
    assert_eq!(rig.host.state(), State::Open);
}

#[test]
fn what_the_page_said_within_the_limit_is_written_whatever_the_escapes_make_of_its_size() {
    // The escapes of the wire make a message longer, up to six times when it is all DEL, and the
    // program's limit allows for that: a message within the limit is always written.
    let mut rig = Rig::new();
    rig.up_web(None);
    let all_del = format!(
        r#"{{"call":{{"id":1,"method":"m","params":"{}"}}}}"#,
        "\u{7f}".repeat(pencere::session::MAX_VIEW_MESSAGE - 45)
    );
    assert!(all_del.len() <= pencere::session::MAX_VIEW_MESSAGE);
    for text in [
        all_del,
        // Two-byte, three-byte and four-byte characters, which grow three, two and three times.
        format!(
            r#"{{"call":{{"id":2,"method":"m","params":"{}"}}}}"#,
            "\u{e9}".repeat(500_000)
        ),
        format!(
            r#"{{"call":{{"id":3,"method":"m","params":"{}"}}}}"#,
            "\u{1f600}".repeat(250_000)
        ),
    ] {
        let unwritable = rig.host.diagnostics().unwritable;
        rig.page(&text);
        let (wrote, acted) = rig.take();
        assert!(acted.is_empty());
        assert_eq!(wrote.len(), 1);
        assert!(pencere::json::is_frame_text(&wrote[0].payload));
        assert_eq!(
            serde_json::from_str::<Value>(&wrote[0].payload).unwrap(),
            serde_json::from_str::<Value>(&text).unwrap()
        );
        assert_eq!(rig.host.diagnostics().unwritable, unwritable);
    }
    assert_eq!(rig.host.state(), State::Open);
}

#[test]
fn whitespace_between_the_tokens_of_a_message_is_accepted_and_left_out() {
    let mut rig = Rig::new();
    rig.up_web(None);
    for text in [
        "{\n  \"call\": {\r\n\t\"id\": 1, \"method\" :\t\"list\"\n}\n}\n",
        " {\"call\":{\"id\":1,\"method\":\"list\"}} ",
    ] {
        rig.page(text);
        let (wrote, acted) = rig.take();
        assert!(acted.is_empty());
        assert_eq!(wrote.len(), 1, "{text:?}");
        assert!(pencere::json::is_frame_text(&wrote[0].payload));
        assert_eq!(
            serde_json::from_str::<Value>(&wrote[0].payload).unwrap(),
            json!({"call": {"id": 1, "method": "list"}})
        );
    }
    // The host's own messages are read through the same whitespace.
    rig.page("{\n\t\"close\"\n:\r\n{}\n}");
    let (wrote, acted) = rig.take();
    assert_eq!(acted, vec![Action::DestroyView]);
    assert_eq!(wrote[0].payload, r#"{"reason":"view"}"#);
}

#[test]
fn the_program_hears_the_pages_messages_in_the_order_the_page_sent_them_and_its_close_last() {
    let mut rig = Rig::new();
    rig.up_web(None);
    let calls: Vec<String> = (1..=5)
        .map(|n| format!(r#"{{"call":{{"id":{n},"method":"m{n}"}}}}"#))
        .collect();
    for call in &calls {
        rig.host.view_message(call);
    }
    // A page that closes after its messages: the `closed` is behind every one of them.
    rig.host.view_message(r#"{"close":{}}"#);
    rig.host
        .view_message(r#"{"call":{"id":6,"method":"late"}}"#);
    let (wrote, acted) = rig.take();
    let kinds: Vec<&str> = wrote.iter().map(|m| m.kind.as_str()).collect();
    assert_eq!(kinds, ["msg", "msg", "msg", "msg", "msg", "closed"]);
    for (written, sent) in wrote.iter().zip(&calls) {
        assert_eq!(&written.payload, sent);
    }
    assert_eq!(wrote[5].payload, r#"{"reason":"view"}"#);
    assert_eq!(acted, vec![Action::DestroyView]);
}

#[test]
fn what_the_program_says_before_the_page_runs_and_after_it_is_gone_is_nobody_s() {
    let mut rig = Rig::new();
    let token = rig.hello();
    // Before: the program is not heard by a view that does not run yet.
    rig.json("open", &common::open_web(&token, None, json!({})));
    rig.take();
    rig.send("msg", None, None, r#"{"push":{"id":1,"value":1}}"#);
    assert!(rig.take().1.is_empty());
    // The page does not run yet either.
    rig.page(r#"{"call":{"id":1,"method":"m"}}"#);
    assert!(rig.take().0.is_empty());
    rig.send_files();
    let (_, acted) = rig.take();
    assert!(started(&acted).is_some());
    rig.send("msg", None, None, r#"{"push":{"id":1,"value":1}}"#);
    assert_eq!(rig.take().1.len(), 1);
    // After: whatever ended the view, nothing the page says is written and nothing is passed.
    type End = fn(&mut Rig);
    let ends: [(&str, End); 5] = [
        ("the person", |rig| rig.host.close_view()),
        ("the program's dismiss", |rig| {
            let token = rig.token.clone().unwrap_or_default();
            rig.send("dismiss", None, None, &dismiss(Some(&token)));
        }),
        ("the view's failure", |rig| rig.host.view_failed("it broke")),
        ("the program's end", |rig| rig.host.program_ended()),
        ("a restart", |rig| rig.host.restart()),
    ];
    for (name, end) in ends {
        let mut rig = Rig::new();
        let token = rig.hello();
        rig.token = Some(token);
        rig.up_web_with_token();
        end(&mut rig);
        rig.take();
        rig.page(r#"{"call":{"id":9,"method":"late"}}"#);
        rig.page(r#"{"response":{"id":9,"result":{}}}"#);
        rig.host.view_message("{");
        rig.send("msg", None, None, r#"{"push":{"id":1,"value":2}}"#);
        let (wrote, acted) = rig.take();
        assert!(
            wrote.is_empty() && acted.is_empty(),
            "{name}: {wrote:?} {acted:?}"
        );
    }
}

impl Rig {
    /// Opens the page with the token the last `hello` got, and sends its files.
    fn up_web_with_token(&mut self) {
        let token = self.token.clone().unwrap();
        self.json("open", &common::open_web(&token, None, json!({})));
        self.send_files();
        let (_, acted) = self.take();
        assert!(started(&acted).is_some());
    }
}
