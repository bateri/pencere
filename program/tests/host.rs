//! The program is told which terminal it runs in: the `host` of the last `welcome` it acted on
//! (specification §5.2), again when a restart `welcome` names another (§10.7).
//!
//! `serve` runs on a thread of the test on the slave end of a pty; the test is the terminal.

// A test fails by panicking, and the helpers here are the test's.
#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::sync::{Arc, Mutex};

use pencere::{Host, NoViewReason, Options, Outcome, ViewControl};
use serde_json::{Value, json};

mod common;

use common::*;

fn host(name: &str, version: &str) -> Host {
    Host {
        name: name.to_owned(),
        version: version.to_owned(),
    }
}

/// What the program was told of its terminal, and what its control said of it at that moment.
#[derive(Clone, Default)]
struct Told {
    hosts: Arc<Mutex<Vec<Host>>>,
    seen: Arc<Mutex<Vec<Option<Host>>>>,
}

impl Told {
    fn hosts(&self) -> Vec<Host> {
        self.hosts.lock().expect("lock").clone()
    }
}

/// A program whose control and notice are `told`'s.
fn listening(
    told: &Told,
    control: &ViewControl,
) -> impl FnOnce(Options) -> Options + Send + 'static {
    let (told, control) = (told.clone(), control.clone());
    move |options| {
        let (hosts, seen, own) = (told.hosts, told.seen, control.clone());
        options.control(control).on_host(move |host| {
            hosts.lock().expect("lock").push(host.clone());
            seen.lock().expect("lock").push(own.host());
        })
    }
}

/// The terminal answers with a `welcome` that names `named` (nothing: no `host` member) and takes
/// the view.
fn shown_by(pane: &mut Pane, named: Option<Value>) {
    let hello = pane.expect("hello");
    let mut welcome = json!({
        "version": 1,
        "token": TOKEN,
        "features": ["dismiss"],
        "views": web_views(),
    });
    if let Some(named) = named {
        welcome["host"] = named;
    }
    pane.send("welcome", Some(hello.id), &welcome);
    let open = pane.expect("open");
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
}

/// The terminal starts the view again, naming `named`.
fn restarts(pane: &mut Pane, token: &str, named: Option<Value>) {
    let mut welcome = json!({
        "version": 1,
        "token": token,
        "features": ["dismiss"],
        "views": web_views(),
        "restart": true,
    });
    if let Some(named) = named {
        welcome["host"] = named;
    }
    pane.send("welcome", None, &welcome);
    let open = pane.expect("open");
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
}

/// The person ends the program, and the terminal sees it out.
fn ends(pane: &mut Pane, handle: std::thread::JoinHandle<Result<Outcome, pencere::Error>>) {
    interrupt();
    let bye = pane.expect("bye");
    pane.send("closed", Some(bye.id), &json!({ "reason": "program" }));
    finished(handle);
}

#[test]
fn the_first_welcome_tells_the_program_and_its_control_has_it_inside_the_notice() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let (told, control) = (Told::default(), ViewControl::new());
    let handle = serve_on(pane.terminal(), listening(&told, &control));
    assert_eq!(control.host(), None);
    shown_by(
        &mut pane,
        Some(json!({ "name": "bateri", "version": "0.9.1" })),
    );
    wait_for("the program to be told", || !told.hosts().is_empty());
    assert_eq!(told.hosts(), vec![host("bateri", "0.9.1")]);
    assert_eq!(
        *told.seen.lock().expect("lock"),
        vec![Some(host("bateri", "0.9.1"))]
    );
    assert_eq!(control.host(), Some(host("bateri", "0.9.1")));
    ends(&mut pane, handle);
    // With no `serve` there is no terminal known.
    assert_eq!(control.host(), None);
}

#[test]
fn a_welcome_that_names_no_host_or_a_bad_one_opens_the_view_and_tells_nothing() {
    let _one = one_at_a_time();
    for named in [
        None,
        Some(json!("bateri")),
        Some(json!({ "name": "bateri" })),
        Some(json!({ "name": 7, "version": "1" })),
        Some(json!({ "name": "bateri", "version": null })),
        Some(json!([])),
    ] {
        let mut pane = Pane::new();
        let (told, control) = (Told::default(), ViewControl::new());
        let handle = serve_on(pane.terminal(), listening(&told, &control));
        shown_by(&mut pane, named.clone());
        assert_eq!(control.host(), None, "{named:?}");
        ends(&mut pane, handle);
        assert_eq!(told.hosts(), vec![], "{named:?}");
    }
}

#[test]
fn a_restart_tells_the_program_only_of_another_host() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let (told, control) = (Told::default(), ViewControl::new());
    let handle = serve_on(pane.terminal(), listening(&told, &control));
    let named = |version: &str| Some(json!({ "name": "bateri", "version": version }));
    shown_by(&mut pane, named("0.9.1"));

    // The same terminal again: nothing to tell.
    restarts(&mut pane, TOKEN_2, named("0.9.1"));
    // Updated: another host, and the control has it.
    restarts(&mut pane, TOKEN, named("0.9.2"));
    wait_for("the update to be told", || told.hosts().len() == 2);
    assert_eq!(control.host(), Some(host("bateri", "0.9.2")));
    // Another computer took the pane up.
    restarts(
        &mut pane,
        TOKEN_2,
        Some(json!({ "name": "ghostty", "version": "1.1.0" })),
    );
    wait_for("the other terminal to be told", || told.hosts().len() == 3);
    assert_eq!(
        told.hosts(),
        vec![
            host("bateri", "0.9.1"),
            host("bateri", "0.9.2"),
            host("ghostty", "1.1.0"),
        ]
    );
    assert_eq!(
        *told.seen.lock().expect("lock"),
        vec![
            Some(host("bateri", "0.9.1")),
            Some(host("bateri", "0.9.2")),
            Some(host("ghostty", "1.1.0")),
        ]
    );
    ends(&mut pane, handle);
}

#[test]
fn a_restart_that_names_no_host_leaves_it_unknown_and_the_next_that_names_one_is_told() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let (told, control) = (Told::default(), ViewControl::new());
    let handle = serve_on(pane.terminal(), listening(&told, &control));
    let bateri = || Some(json!({ "name": "bateri", "version": "0.9.1" }));
    shown_by(&mut pane, bateri());
    restarts(&mut pane, TOKEN_2, None);
    wait_for("the host to be unknown", || control.host().is_none());
    assert_eq!(told.hosts(), vec![host("bateri", "0.9.1")]);
    restarts(&mut pane, TOKEN, bateri());
    wait_for("the host to be told again", || told.hosts().len() == 2);
    assert_eq!(control.host(), Some(host("bateri", "0.9.1")));
    ends(&mut pane, handle);
}

#[test]
fn a_host_that_has_nothing_to_open_or_shares_no_version_is_told_before_the_fallback() {
    let _one = one_at_a_time();
    for supported in [false, true] {
        let mut pane = Pane::new();
        let events = Arc::new(Mutex::new(Vec::<String>::new()));
        let (noticed, fell) = (Arc::clone(&events), Arc::clone(&events));
        let handle = serve_on(pane.terminal(), move |options| {
            options
                .on_host(move |host| {
                    noticed
                        .lock()
                        .expect("lock")
                        .push(format!("host {}", host.name));
                })
                .fallback(move |_| fell.lock().expect("lock").push("fallback".to_owned()))
        });
        let hello = pane.expect("hello");
        let named = json!({ "name": "bateri", "version": "0.9.1" });
        if supported {
            pane.send(
                "welcome",
                Some(hello.id),
                &json!({ "supported": [2], "host": named }),
            );
        } else {
            pane.send(
                "welcome",
                Some(hello.id),
                &json!({ "version": 1, "token": TOKEN, "host": named, "views": [] }),
            );
        }
        pane.write_raw(REPLY);
        let Outcome::NoView(no_view) = finished(handle) else {
            panic!("no view");
        };
        assert_eq!(
            no_view.reason,
            if supported {
                NoViewReason::NoCommonVersion
            } else {
                NoViewReason::NoCommonView
            }
        );
        assert_eq!(
            *events.lock().expect("lock"),
            vec!["host bateri".to_owned(), "fallback".to_owned()]
        );
    }
}
