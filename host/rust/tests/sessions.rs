//! Every `session` entry of the fixtures' manifest: a conversation with a host
//! configured as the entry says, checked byte by byte, whether the program's
//! bytes arrive whole, cut anywhere, one at a time, or as APC bodies.

// A test fails by panicking, and indexes what it has just built.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use pencere_host::bundle::Store;
use pencere_host::sandbox::{Consent, Grant};
use pencere_host::session::{Action, Observed, Session, Size, State};
use pencere_host::token::Entropy;
use serde_json::Value;

/// What happens to the host before it answers: bytes the program writes, or something that
/// happens on the terminal's side (`event` steps of the manifest).
enum Stimulus {
    In(Vec<u8>),
    Event(Value),
}

/// A stimulus and what the host does for it: the bytes it writes, and, when the entry says, the
/// other actions it takes (`acted`) and the texts it passes to the view (`to-view`).
struct Step {
    stimulus: Stimulus,
    expected: Vec<u8>,
    acted: Option<Vec<String>>,
    to_view: Option<Vec<Vec<u8>>>,
}

fn steps(session: &Value) -> Vec<Step> {
    let list = session["steps"].as_array().unwrap();
    let mut out = Vec::new();
    let mut iter = list.iter();
    while let Some(step) = iter.next() {
        let stimulus = match step["in"].as_str() {
            Some(file) => Stimulus::In(common::file(file)),
            None => {
                assert!(step["event"].is_string(), "a step is an in or an event");
                Stimulus::Event(step.clone())
            }
        };
        let answer = iter.next().expect("an out step");
        let expected = match answer["out"].as_str() {
            Some(file) => common::file(file),
            None => Vec::new(),
        };
        let acted = answer["acted"].as_array().map(|names| {
            names
                .iter()
                .map(|name| name.as_str().unwrap().to_owned())
                .collect()
        });
        let to_view = answer["to-view"].as_array().map(|files| {
            files
                .iter()
                .map(|file| common::file(file.as_str().unwrap()))
                .collect()
        });
        out.push(Step {
            stimulus,
            expected,
            acted,
            to_view,
        });
    }
    out
}

/// How the bytes of a step reach the session.
#[derive(Clone, Copy, Debug)]
enum Feed {
    Whole,
    CutAt(usize),
    OneByOne,
    Bodies,
}

/// The name an action has in a step's `acted`.
fn name_of(action: &Action) -> Option<&'static str> {
    match action {
        Action::Write(_) | Action::WriteView(_) => None,
        Action::Consent(_) => Some("consent"),
        Action::WithdrawConsent(_) => Some("withdraw-consent"),
        Action::StartView(_) => Some("start-view"),
        Action::ToView(_) => Some("to-view"),
        Action::OpenLink { .. } => Some("open-link"),
        Action::DestroyView => Some("destroy-view"),
        other => panic!("an action the fixtures do not name: {other:?}"),
    }
}

/// A page's message of exactly `size` bytes: `{"pad":"aaa…"}`.
fn padded(size: usize) -> String {
    format!(
        "{{\"pad\":\"{}\"}}",
        "a".repeat(size - r#"{"pad":""}"#.len())
    )
}

/// Something that happens on the terminal's side, as the entry's `event` names it.
fn happen<S: Store, E: Entropy>(
    host: &mut Session<S, E>,
    event: &Value,
    question: &Option<Consent>,
) {
    match event["event"].as_str().unwrap() {
        "observe" => host.observe(Observed {
            alternate_screen: event["alternate-screen"].as_bool(),
            ..Observed::default()
        }),
        "allow-opens" => host.allow_opens(event["allow"].as_bool().unwrap()),
        "consent" => {
            let asked = question.as_ref().expect("a question to answer");
            let grant = match event["grant"].as_str().unwrap() {
                "all" => asked.grant_all(),
                "none" => Grant::none(),
                other => panic!("a grant of {other}"),
            };
            host.consent(asked.id, grant);
        }
        "backlog" => {
            // The terminal asks whether it may hold that many bytes of what a view says.
            let _ = host.backlog(event["waiting"].as_u64().unwrap() as usize);
        }
        "timeout" => host.timed_out(),
        "close-view" => host.close_view(),
        "restart" => host.restart(),
        "program-ended" => host.program_ended(),
        "set-size" => host.set_size(Size {
            width: event["width"].as_u64().unwrap() as u32,
            height: event["height"].as_u64().unwrap() as u32,
        }),
        "page" => {
            let text = match event["file"].as_str() {
                Some(file) => String::from_utf8(common::file(file)).unwrap(),
                None => padded(event["pad"].as_u64().unwrap() as usize),
            };
            host.view_message(&text);
        }
        other => panic!("an event called {other}"),
    }
}

fn run(session: &Value, how: impl Fn(usize) -> Feed) {
    let manifest_session = session;
    let mut host = common::session_of(
        &manifest_session["config"],
        &manifest_session["config"]["tokens"],
    );
    let mut question: Option<Consent> = None;
    for (index, step) in steps(manifest_session).iter().enumerate() {
        match &step.stimulus {
            Stimulus::Event(event) => {
                happen(&mut host, event, &question);
            }
            Stimulus::In(input) => match how(index) {
                Feed::Whole => host.feed(input),
                Feed::CutAt(at) => {
                    let at = at.min(input.len());
                    host.feed(&input[..at]);
                    host.feed(&input[at..]);
                }
                Feed::OneByOne => {
                    for byte in input.chunks(1) {
                        host.feed(byte);
                    }
                }
                Feed::Bodies => {
                    for body in common::bodies(input) {
                        host.apc_start();
                        for piece in body.chunks(7) {
                            host.apc_put(piece);
                        }
                        host.apc_end(false);
                    }
                }
            },
        }
        let actions = host.take_actions();
        for action in &actions {
            if let Action::Consent(asked) = action {
                question = Some(asked.clone());
            }
        }
        let label = format!(
            "{} step {index} ({:?})",
            manifest_session["name"],
            how(index)
        );
        assert_eq!(
            String::from_utf8_lossy(&common::written(&actions)),
            String::from_utf8_lossy(&step.expected),
            "{label}"
        );
        if let Some(names) = &step.acted {
            let acted: Vec<&str> = actions.iter().filter_map(name_of).collect();
            assert_eq!(acted, *names, "{label}: the other actions");
        }
        if let Some(files) = &step.to_view {
            let told: Vec<Vec<u8>> = actions
                .iter()
                .filter_map(|action| match action {
                    Action::ToView(text) => Some(text.as_bytes().to_vec()),
                    _ => None,
                })
                .collect();
            assert_eq!(told, *files, "{label}: what the view is told");
        }
    }
}

#[test]
fn every_session_gives_the_bytes_the_fixtures_name() {
    let manifest = common::manifest();
    let sessions = manifest["session"].as_array().unwrap();
    assert_eq!(sessions.len(), 35);
    for session in sessions {
        run(session, |_| Feed::Whole);
        run(session, |_| Feed::OneByOne);
        run(session, |_| Feed::Bodies);
    }
}

#[test]
fn a_session_does_not_depend_on_where_the_program_bytes_are_cut() {
    let manifest = common::manifest();
    for session in manifest["session"].as_array().unwrap() {
        let steps = steps(session);
        for (index, step) in steps.iter().enumerate() {
            let Stimulus::In(input) = &step.stimulus else {
                continue;
            };
            // Every place in a short step; a long one (a manifest of a thousand files) is cut at
            // its ends and at a spread of places in between, since every place would be a run of
            // the session for each byte.
            let places: Vec<usize> = if input.len() <= 4096 {
                (0..=input.len()).collect()
            } else {
                (0..=input.len())
                    .filter(|at| *at < 16 || input.len() - *at < 16 || at % 4099 == 0)
                    .collect()
            };
            for at in places {
                run(session, |i| {
                    if i == index {
                        Feed::CutAt(at)
                    } else {
                        Feed::Whole
                    }
                });
            }
        }
    }
}

#[test]
fn the_handshake_leaves_a_token_outstanding_and_the_version_mismatch_does_not() {
    let manifest = common::manifest();
    let sessions = manifest["session"].as_array().unwrap();
    for (name, state) in [
        ("handshake", State::Offered),
        ("version-mismatch", State::Idle),
    ] {
        let session = sessions.iter().find(|s| s["name"] == name).unwrap();
        let mut host = common::session_of(&session["config"], &manifest["tokens"]["issued"]);
        let Stimulus::In(input) = &steps(session)[0].stimulus else {
            panic!("{name}: a session begins with the program's bytes");
        };
        host.feed(input);
        assert_eq!(host.state(), state, "{name}");
    }
}
