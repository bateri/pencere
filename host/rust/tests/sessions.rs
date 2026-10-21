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

use pencere::session::State;
use serde_json::Value;

struct Step {
    input: Vec<u8>,
    expected: Vec<u8>,
}

fn steps(session: &Value) -> Vec<Step> {
    let list = session["steps"].as_array().unwrap();
    let mut out = Vec::new();
    let mut iter = list.iter();
    while let Some(step) = iter.next() {
        let input = common::file(step["in"].as_str().expect("an in step"));
        let expected = match iter.next().expect("an out step")["out"].as_str() {
            Some(file) => common::file(file),
            None => Vec::new(),
        };
        out.push(Step { input, expected });
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

fn run(session: &Value, how: impl Fn(usize) -> Feed) {
    let manifest_session = session;
    let mut host = common::session_of(
        &manifest_session["config"],
        &common::manifest()["tokens"]["issued"],
    );
    for (index, step) in steps(manifest_session).iter().enumerate() {
        match how(index) {
            Feed::Whole => host.feed(&step.input),
            Feed::CutAt(at) => {
                let at = at.min(step.input.len());
                host.feed(&step.input[..at]);
                host.feed(&step.input[at..]);
            }
            Feed::OneByOne => {
                for byte in step.input.chunks(1) {
                    host.feed(byte);
                }
            }
            Feed::Bodies => {
                for body in common::bodies(&step.input) {
                    host.apc_start();
                    for piece in body.chunks(7) {
                        host.apc_put(piece);
                    }
                    host.apc_end(false);
                }
            }
        }
        let actions = host.take_actions();
        assert_eq!(
            String::from_utf8_lossy(&common::written(&actions)),
            String::from_utf8_lossy(&step.expected),
            "{} step {index} ({:?})",
            manifest_session["name"],
            how(index)
        );
    }
}

#[test]
fn every_session_gives_the_bytes_the_fixtures_name() {
    let manifest = common::manifest();
    let sessions = manifest["session"].as_array().unwrap();
    assert_eq!(sessions.len(), 9);
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
            for at in 0..=step.input.len() {
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
        host.feed(&steps(session)[0].input);
        assert_eq!(host.state(), state, "{name}");
    }
}
