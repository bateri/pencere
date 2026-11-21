//! The proof of the host's own route (specification §4.10) as the session gives it to a terminal:
//! when a `probe` is written, what token it carries, and which answer is believed. The bytes of a
//! whole conversation are the fixtures' (`sessions.rs`, `probe-route`).

// A test fails by panicking, and indexes what it has just built.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::Rig;
use pencere_host::session::{Action, MOST_TTL};
use serde_json::Value;

/// The `probe` the host wrote since the last call, as `(id, token, ttl)`.
fn probe_written(rig: &mut Rig) -> (u32, String, u64) {
    let (wrote, _) = rig.take();
    assert_eq!(wrote.len(), 1, "{wrote:?}");
    assert_eq!(wrote[0].kind, "probe");
    let body: Value = serde_json::from_str(&wrote[0].payload).unwrap();
    (
        wrote[0].id,
        body["token"].as_str().unwrap().to_owned(),
        body["ttl"].as_u64().unwrap(),
    )
}

fn is_hex_token(text: &str) -> bool {
    text.len() == 32
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[test]
fn a_probe_is_written_only_where_the_host_offers_it_while_a_view_runs_for_a_ttl_the_listener_is_asked()
 {
    let mut rig = Rig::new();
    // Not offered, and no view.
    assert_eq!(rig.host.probe(5), None);
    rig.host.offer_probe(true);
    assert_eq!(rig.host.probe(5), None, "no view runs");
    assert!(rig.take().0.is_empty());

    rig.up_web(None);
    for ttl in [0, MOST_TTL + 1, 255] {
        assert_eq!(rig.host.probe(ttl), None, "{ttl}");
    }
    assert!(rig.take().0.is_empty());

    let id = rig.host.probe(MOST_TTL).expect("a probe");
    let (written, token, ttl) = probe_written(&mut rig);
    assert_eq!(written, id);
    assert_eq!(ttl, u64::from(MOST_TTL));
    assert!(is_hex_token(&token), "{token}");
    // Not the token of the welcome, which the view was opened with.
    assert_ne!(Some(&token), rig.token.as_ref());

    // A host that stops offering it writes none.
    rig.host.offer_probe(false);
    assert_eq!(rig.host.probe(5), None);
}

#[test]
fn a_probe_that_is_asked_again_gives_up_the_one_before_and_only_the_answer_to_the_one_out_is_believed()
 {
    let mut rig = Rig::new();
    rig.host.offer_probe(true);
    rig.up_web(None);

    let first = rig.host.probe(5).expect("a probe");
    let (_, first_token, _) = probe_written(&mut rig);
    let second = rig.host.probe(5).expect("a probe");
    assert_ne!(first, second);
    let (_, second_token, _) = probe_written(&mut rig);
    assert_ne!(first_token, second_token, "a token is new for each probe");

    // The answer to the probe that was given up, and one that answers no probe at all.
    rig.send("probed", Some(first), None, r#"{"port":40001}"#);
    rig.send("probed", None, None, r#"{"port":40002}"#);
    assert!(rig.take().1.is_empty());

    rig.send("probed", Some(second), None, r#"{"port":40003}"#);
    let (_, acted) = rig.take();
    assert_eq!(
        acted,
        [Action::ProbeConnect {
            probe: second,
            port: 40003,
            token: second_token
        }]
    );
    // The first answer is the one believed.
    rig.send("probed", Some(second), None, r#"{"code":"io"}"#);
    assert!(rig.take().1.is_empty());
}

#[test]
fn a_program_that_has_no_listener_to_give_says_why_and_the_host_has_no_route() {
    let mut rig = Rig::new();
    rig.host.offer_probe(true);
    rig.up_web(None);
    let id = rig.host.probe(5).expect("a probe");
    rig.take();
    rig.send("probed", Some(id), None, r#"{"code":"busy"}"#);
    let (_, acted) = rig.take();
    assert_eq!(
        acted,
        [Action::ProbeFailed {
            probe: id,
            code: "busy".to_owned()
        }]
    );
}
