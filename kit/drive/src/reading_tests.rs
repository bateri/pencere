//! What an agent reads of a page: the name of an element, the two writers, what is waited for before
//! a reading, and the reading of pages under the host with no screen. The tests that run the
//! interpreter are heavy (`make check-heavy`).

use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pencere_sandbox::{Fault, Frame, Runner};
use pencere_view::{
    Accessible, AccessibleRole, Event, FieldKind, Id, Key, Laid, Mods, Rect, Reply, Shortcut, Size,
    State, div, text,
};
use serde_json::{Value, json};

use crate::headless::{Calls, Error, Headless};
use crate::reading::{
    CommandReading, EditorReading, FieldReading, Found, Item, LINE_LIMIT, MenuLine, Owed, Place,
    Reading, ScrollReading, Selected, WORDS_LIMIT, clean, full, keys_of, reference,
};
use crate::{FrameAt, Page, Setup};

fn example(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name)
}

fn item(role: AccessibleRole, id: Option<Id>, words: &str) -> Item {
    Item::of(&Accessible {
        role,
        id,
        rect: Rect::new(1.4, 2.0, 30.0, 10.0),
        words: words.to_owned(),
        state: State::default(),
        pressable: false,
        parent: None,
    })
}

/// Two numbers that come to the same name: found by looking at the first few thousand.
const CLASH: (u64, u64) = (2091, 3837);

#[test]
fn a_name_is_six_letters_of_the_id_mixed_and_is_the_same_each_time() {
    let id = Id::named("restart");
    let name = reference(id);
    assert_eq!(name.len(), 6);
    assert!(
        name.chars()
            .all(|c| "0123456789abcdefghjkmnpqrstvwxyz".contains(c)),
        "{name}"
    );
    assert_eq!(name, reference(Id::named("restart")));
    assert_ne!(name, reference(Id::named("remove")));
    assert_eq!(full(Id::new(255)), "00000000000000ff");
}

#[test]
fn ids_that_are_small_numbers_have_names_that_differ_from_the_first_letter() {
    // The raw low bits of 1, 2, 3 would give names that differ in their last letter only.
    let names: Vec<String> = (1..=64).map(|n| reference(Id::new(n))).collect();
    let mut firsts: Vec<char> = names
        .iter()
        .filter_map(|name| name.chars().next())
        .collect();
    firsts.sort_unstable();
    firsts.dedup();
    assert!(firsts.len() > 8, "{firsts:?}");
    let mut all = names.clone();
    all.sort();
    all.dedup();
    assert_eq!(all.len(), 64);
}

#[test]
fn two_elements_of_one_name_are_both_marked_and_each_is_found_by_its_whole_id() {
    let (a, b) = (Id::new(CLASH.0), Id::new(CLASH.1));
    assert_eq!(reference(a), reference(b), "the pair still clashes");
    let reading = Reading {
        items: vec![
            item(AccessibleRole::Button, Some(a), "First"),
            item(AccessibleRole::Button, Some(b), "Second"),
            item(AccessibleRole::Button, Some(Id::named("other")), "Third"),
            item(AccessibleRole::Text, None, "plain"),
        ],
        ..Reading::default()
    };
    let name = reference(a);
    assert_eq!(reading.find(&name), Found::Clash);
    assert_eq!(reading.find(&name.to_uppercase()), Found::Clash);
    assert_eq!(reading.find(&full(a)), Found::One(a));
    assert_eq!(reading.find(&full(b)), Found::One(b));
    assert_eq!(
        reading.find(&reference(Id::named("other"))),
        Found::One(Id::named("other"))
    );
    assert_eq!(reading.find("zzzzzz"), Found::Nothing);
    assert_eq!(reading.find(&full(Id::new(7))), Found::Nothing);

    let text = reading.text();
    let marked: Vec<&str> = text
        .lines()
        .filter(|line| line.starts_with(&format!("{name}!")))
        .collect();
    assert_eq!(marked.len(), 2, "{text}");
    assert!(
        text.lines()
            .any(|line| line.starts_with(&reference(Id::named("other"))) && !line.contains('!')),
        "{text}"
    );
    let json: Value = serde_json::from_str(&reading.json()).expect("json");
    let items = json["items"].as_array().expect("items");
    assert_eq!(items[0]["clash"], json!(true));
    assert_eq!(items[1]["clash"], json!(true));
    assert_eq!(items[2]["clash"], json!(false));
    assert_eq!(items[0]["id"], json!(full(a)));
    assert_eq!(items[3]["ref"], Value::Null, "plain text has no name");
}

#[test]
fn what_a_page_says_is_cleaned_alike_in_the_text_and_in_the_json() {
    // A control character, DEL, one past the ASCII, and a mark that turns text round are `?`; a tab is a
    // space and a line break stays.
    let said = "a\u{1}b\u{7f}c\u{85}d\u{202e}e\u{200b}f\tg\nh";
    let clean_said = "a?b?c?d?e?f g\nh";
    assert_eq!(clean(said, 100), clean_said);
    let long = "x".repeat(WORDS_LIMIT + 50);
    let cut = clean(&long, WORDS_LIMIT);
    assert_eq!(cut.chars().count(), WORDS_LIMIT + 1);
    assert!(cut.ends_with('…'));

    let reading = Reading {
        items: vec![
            item(AccessibleRole::Text, None, said),
            item(AccessibleRole::Text, None, &long),
        ],
        commands: vec![CommandReading {
            id: Id::named("go"),
            title: said.to_owned(),
            keys: vec![],
            enabled: true,
        }],
        ..Reading::default()
    };
    let json: Value = serde_json::from_str(&reading.json()).expect("json");
    assert_eq!(json["items"][0]["text"], json!(clean_said));
    assert_eq!(json["items"][1]["text"], json!(cut));
    assert_eq!(json["commands"][0]["title"], json!(clean_said));
    let text = reading.text();
    // The line break stays; what follows it is indented under its line.
    assert!(text.contains("\"a?b?c?d?e?f g\n        h\""), "{text:?}");
    assert!(
        !text.contains(['\u{1}', '\u{7f}', '\u{85}', '\u{202e}', '\u{200b}']),
        "{text:?}"
    );
}

#[test]
fn what_stands_in_a_reading_that_a_page_did_not_say_in_words_is_cleaned_too_and_a_secret_is_never_written()
 {
    // Characters that end a line or hide text in a way a person does not see: the line and paragraph
    // separators, a soft hyphen, the tags that write text apart from what shows.
    let hidden = "a\u{2028}b\u{2029}c\u{ad}d\u{e0041}e\u{206a}f\u{fff9}g";
    assert_eq!(clean(hidden, 100), "a?b?c?d?e?f?g");
    // The keys of a command, the role of an item and the notes of a reading come from the page or a
    // host that builds a reading of its own; they are cleaned as the words are.
    let key = "ctrl+\u{1b}\u{9b}";
    let mut odd = item(AccessibleRole::Text, None, "words");
    odd.role = "but\u{1b}ton".to_owned();
    let secret = Id::named("pw");
    let reading = Reading {
        items: vec![odd],
        commands: vec![CommandReading {
            id: Id::named("go"),
            title: "Go".to_owned(),
            keys: vec![key.to_owned()],
            enabled: true,
        }],
        fields: vec![FieldReading {
            id: secret,
            kind: FieldKind::Secret,
            label: Some("Password".to_owned()),
            hint: None,
            value: Some("hunter2".to_owned()),
            covered: false,
            rect: Rect::new(0.0, 0.0, 10.0, 10.0),
        }],
        notes: vec!["a\u{1b}[2Jb".to_owned()],
        ..Reading::default()
    };
    let text = reading.text();
    let json = reading.json();
    for written in [&text, &json] {
        assert!(!written.contains(['\u{1b}', '\u{9b}']), "{written:?}");
        assert!(!written.contains("hunter2"), "{written:?}");
    }
    assert!(text.contains("note: a?[2Jb"), "{text:?}");
    assert!(text.contains("[ctrl+??]"), "{text:?}");
    let json: Value = serde_json::from_str(&json).expect("json");
    assert_eq!(json["fields"][0]["value"], Value::Null);
    assert_eq!(json["items"][0]["role"], json!("but?ton"));
}

#[test]
fn what_shows_of_an_editor_is_found_from_the_room_it_has_where_a_card_or_a_box_cuts_it() {
    use crate::reading::shown_span;
    // An editor whose room is from 100 to 400 on the page, ten places to a point: what is seen of
    // it is where `shows` says.
    let place = |shows: std::ops::RangeInclusive<f32>| {
        move |y: f32| shows.contains(&y).then_some(((y - 100.0) * 10.0) as usize)
    };
    // All of it shows: the first and the last place, near the edges of its room.
    let (top, bottom) = shown_span(place(0.0..=1000.0), 100.0, 300.0).expect("shows");
    assert!(top < 20 && bottom > 2970, "{top}-{bottom}");
    // Its top is covered up to 160 and its bottom is cut off at 250: the middle shows.
    let (top, bottom) = shown_span(place(160.0..=250.0), 100.0, 300.0).expect("shows");
    assert!((599..=620).contains(&top), "{top}");
    assert!((1490..=1500).contains(&bottom), "{bottom}");
    // Nothing of it shows.
    assert_eq!(shown_span(place(1000.0..=2000.0), 100.0, 300.0), None);
}

#[test]
fn a_frame_the_host_was_told_to_draw_and_has_not_is_not_a_page_at_rest() {
    let (mut page, _open, _sent) = held();
    // The frames a host draws until the page says it wants no more.
    let settle = |page: &mut Page| {
        for _ in 0..10 {
            assert!(page.module().flush(Duration::from_secs(5)));
            if !page.drain().redraw {
                return;
            }
            page.frame(frame());
        }
        panic!("the page never stopped asking for frames");
    };
    page.frame(frame());
    settle(&mut page);
    assert!(page.rest().resting(), "{:?}", page.rest());
    // The host is told a page is to be drawn, and is not yet at the frame that draws it.
    page.refresh();
    page.frame(frame());
    assert!(page.module().flush(Duration::from_secs(5)));
    assert!(page.drain().redraw, "the page made again is to be drawn");
    let rest = page.rest();
    assert!(rest.unsettled && !rest.resting(), "{rest:?}");
    // The frame is drawn.
    page.frame(frame());
    settle(&mut page);
    assert!(page.rest().resting(), "{:?}", page.rest());
}

/// A module whose page asks for its next frame at once for `steps` renders, as one that fades or
/// sweeps while a call is out does, and then is still. With `changing` its page is another each
/// time, as it is where what moves is what the page shows.
struct Moving {
    changing: bool,
    steps: u32,
    made: u32,
}

impl Runner for Moving {
    fn render(&mut self, _size: Size, _now: f64, _reduce: bool) -> Result<Frame, Fault> {
        self.made += 1;
        let animating = self.made < self.steps;
        let words = if self.changing && animating {
            format!("step {}", self.made)
        } else {
            "step".to_owned()
        };
        Ok(Frame {
            root: div().child(text(words)),
            animating,
            wake_at: None,
            wake: None,
        })
    }

    fn settle(&mut self) -> Result<bool, Fault> {
        Ok(false)
    }

    fn handle(&mut self, _event: Event) -> Result<Reply, Fault> {
        Ok(Reply::none())
    }

    fn lay_out(&mut self, _laid: Arc<Laid>) {}

    fn take_sent(&mut self) -> Vec<String> {
        Vec::new()
    }
}

#[test]
fn a_page_whose_program_asks_for_each_frame_is_not_at_rest_while_it_does_and_is_when_it_stops() {
    for changing in [false, true] {
        let mut page = Page::spawn(
            move || {
                Ok(Moving {
                    changing,
                    steps: 12,
                    made: 0,
                })
            },
            Setup::default(),
            || {},
        );
        let mut at = 1.0;
        let mut rested_at = None;
        for step in 0..40 {
            assert!(page.module().flush(Duration::from_secs(5)));
            page.drain();
            let rest = page.rest();
            if rest.resting() && rested_at.is_none() {
                rested_at = Some(step);
            }
            at += 0.016;
            page.frame(FrameAt { now: at, ..frame() });
        }
        // The page's own animation is waited for (it is the page taking in what it makes) and not
        // for ever: when the program stops asking, the page comes to rest.
        let rested = rested_at.unwrap_or_else(|| panic!("changing {changing}: never at rest"));
        assert!(
            rested >= 10,
            "changing {changing}: at rest at {rested}, while it still asked for frames"
        );
        assert!(!page.rest().moving, "{:?}", page.rest());
    }
}

#[test]
fn a_reading_is_written_for_a_person_and_for_a_program_from_the_same_parts() {
    let go = Id::named("go");
    let reading = Reading {
        focus: crate::reading::Focus::Field(Id::named("q")),
        rest: crate::reading::Rest {
            calls: 2,
            moving: true,
            ..Default::default()
        },
        items: vec![Item::of(&Accessible {
            role: AccessibleRole::Checkbox,
            id: Some(go),
            rect: Rect::new(0.0, 0.0, 10.0, 10.0),
            words: "Auto start".to_owned(),
            state: State {
                checked: Some(true),
                disabled: true,
                ..State::default()
            },
            pressable: false,
            parent: None,
        })],
        fields: vec![FieldReading {
            id: Id::named("q"),
            kind: FieldKind::Line,
            label: None,
            hint: Some("Search".to_owned()),
            value: Some("api".to_owned()),
            covered: false,
            rect: Rect::new(0.0, 20.0, 100.0, 20.0),
        }],
        editors: vec![EditorReading {
            id: Id::named("sql"),
            label: Some("Query".to_owned()),
            focused: true,
            first: 9,
            lines: vec!["select 1".to_owned(), "from t".to_owned()],
            total: 40,
            caret: Place {
                line: 10,
                column: 3,
            },
            selection: Some(Selected {
                from: Place {
                    line: 10,
                    column: 3,
                },
                to: Place {
                    line: 10,
                    column: 8,
                },
                text: "lect ".to_owned(),
            }),
            rect: Rect::new(0.0, 40.0, 100.0, 40.0),
        }],
        scrolls: vec![ScrollReading {
            id: Id::named("list"),
            percent: 56,
            above: true,
            below: true,
            rect: Rect::new(0.0, 80.0, 100.0, 60.0),
        }],
        commands: vec![CommandReading {
            id: Id::named("rename"),
            title: "Rename".to_owned(),
            keys: vec!["f2".to_owned(), "ctrl+r".to_owned()],
            enabled: false,
        }],
        menu: Some(vec![MenuLine {
            title: "Copy".to_owned(),
            enabled: true,
        }]),
        notes: vec!["no commands here".to_owned()],
        behind: 0,
    };
    let text = reading.text();
    let q = reference(Id::named("q"));
    assert!(text.starts_with(&format!("focus: field {q}\n")), "{text}");
    assert!(
        text.contains("rest: 2 calls are waiting for an answer; motion\n"),
        "{text}"
    );
    assert!(text.contains("note: no commands here\n"), "{text}");
    assert!(
        text.contains(&format!(
            "{}  checkbox \"Auto start\" [checked, disabled]",
            reference(go)
        )),
        "{text}"
    );
    assert!(
        text.contains(&format!("{q}  field \"Search\" = \"api\"")),
        "{text}"
    );
    assert!(
        text.contains(
            "editor \"Query\" lines 9-10 of 40, caret 10:3, selected 10:3-10:8 \"lect \" (keyboard)"
        ),
        "{text}"
    );
    assert!(text.contains("   9| select 1\n  10| from t\n"), "{text}");
    assert!(text.contains("scroll 56% (more above and below)"), "{text}");
    assert!(
        text.contains("\"Rename\" [f2, ctrl+r] (disabled)"),
        "{text}"
    );
    assert!(text.contains("menu:\n  \"Copy\"\n"), "{text}");

    let json: Value = serde_json::from_str(&reading.json()).expect("json");
    assert_eq!(json["focus"]["kind"], json!("field"));
    assert_eq!(json["rest"]["calls"], json!(2));
    assert_eq!(json["rest"]["resting"], json!(false));
    assert_eq!(
        json["items"][0]["state"],
        json!({ "checked": true, "disabled": true })
    );
    assert_eq!(json["items"][0]["box"], json!([0.0, 0.0, 10.0, 10.0]));
    assert_eq!(json["fields"][0]["value"], json!("api"));
    assert_eq!(json["fields"][0]["kind"], json!("line"));
    assert_eq!(json["editors"][0]["lines"]["from"], json!(9));
    assert_eq!(
        json["editors"][0]["selection"]["to"],
        json!({ "line": 10, "column": 8 })
    );
    assert_eq!(json["scrolls"][0]["percent"], json!(56));
    assert_eq!(json["commands"][0]["keys"], json!(["f2", "ctrl+r"]));
    assert_eq!(json["menu"][0]["title"], json!("Copy"));
    assert_eq!(json["notes"][0], json!("no commands here"));
}

#[test]
fn keys_are_written_as_a_person_writes_them_and_are_read_back() {
    for words in [
        "cmd+k",
        "ctrl+shift+p",
        "alt+enter",
        "f5",
        "escape",
        "page_down",
        "cmd++",
        "ctrl+space",
    ] {
        let shortcut = Shortcut::parse(words).expect(words);
        assert_eq!(keys_of(shortcut), words, "{words}");
    }
    assert_eq!(
        keys_of(Shortcut::new(
            Key::Character('K'),
            Mods {
                command: true,
                shift: true,
                ..Mods::default()
            }
        )),
        "shift+cmd+k"
    );
}

#[test]
fn the_calls_a_page_made_wait_for_their_answers_and_nothing_else_is_waited_for() {
    let mut owed = Owed::default();
    owed.said(r#"{"call":{"id":1,"method":"list"}}"#);
    owed.said(r#"{ "call" : {"id":2,"method":"save","params":{"a":1}}}"#);
    owed.said(r#"{"subscribe":{"id":3,"topic":"stats"}}"#);
    assert_eq!(owed.count(), 2, "a subscription is not a call");
    // What the program says that is not an answer does not end a wait.
    owed.heard(r#"{"push":{"id":3,"value":{"cpu":1}}}"#);
    owed.heard(r#"{"progress":{"id":1,"done":1,"total":4}}"#);
    owed.heard("not a message");
    assert_eq!(owed.count(), 2);
    owed.heard(r#"{"response":{"id":1,"result":{}}}"#);
    assert_eq!(owed.count(), 1);
    // A call that is given up, or whose answer is an error, is not waited for.
    owed.said(r#"{"cancel":{"id":2}}"#);
    assert_eq!(owed.count(), 0);
    owed.said(r#"{"call":{"id":4,"method":"x"}}"#);
    owed.heard(r#"{"response":{"id":4,"error":{"code":"no"}}}"#);
    assert_eq!(owed.count(), 0);
    owed.said(r#"{"call":{"id":5,"method":"x"}}"#);
    owed.clear();
    assert_eq!(owed.count(), 0);
}

#[test]
fn a_wait_is_read_from_the_name_and_number_of_a_message_and_nothing_it_holds() {
    let mut owed = Owed::default();
    // What a call asks and what an answer holds are the page's and its program's: an answer the page
    // takes is the end of the wait however the rest of it reads (the page is told `result` and
    // `error` as they are), and a call is waited for whatever its method.
    owed.said(r#"{"call":{"id":1,"method":"list"}}"#);
    owed.said(r#"{"call":{"id":2,"method":"list"}}"#);
    owed.said(r#"{"call":{"id":3,"method":7}}"#);
    assert_eq!(owed.count(), 3);
    owed.heard(r#"{"response":{"id":1,"result":{},"error":{"code":"x"}}}"#);
    owed.heard(r#"{"response":{"id":2,"result":null}}"#);
    owed.heard(r#"{"response":{"id":3}}"#);
    assert_eq!(owed.count(), 0);
    // A message with no number of a call's, or not a message at all, ends no wait.
    owed.said(r#"{"call":{"id":4,"method":"x"}}"#);
    owed.heard(r#"{"response":{"id":0,"result":{}}}"#);
    owed.heard(r#"{"response":{"id":"4","result":{}}}"#);
    owed.heard(r#"{"response":{"id":4,"result":{}}"#);
    assert_eq!(owed.count(), 1);
    owed.heard(r#"{"response":{"id":4.0,"result":[]}}"#);
    assert_eq!(owed.count(), 0);
}

/// A module made by a test: its first page is made at once, and what it is told after it waits for
/// the gate. What it says to its program is what the test puts in `sent`.
struct Held {
    gate: Receiver<()>,
    sent: Arc<Mutex<Vec<String>>>,
}

impl Runner for Held {
    fn render(&mut self, _size: Size, _now: f64, _reduce: bool) -> Result<Frame, Fault> {
        Ok(Frame {
            root: div().child(text("held")),
            animating: false,
            wake_at: None,
            wake: None,
        })
    }

    fn settle(&mut self) -> Result<bool, Fault> {
        Ok(false)
    }

    fn handle(&mut self, _event: Event) -> Result<Reply, Fault> {
        let _ = self.gate.recv();
        Ok(Reply::none())
    }

    fn lay_out(&mut self, _laid: Arc<Laid>) {}

    fn take_sent(&mut self) -> Vec<String> {
        std::mem::take(&mut *self.sent.lock().unwrap())
    }
}

fn held() -> (Page, Sender<()>, Arc<Mutex<Vec<String>>>) {
    let (open, gate) = channel();
    let sent = Arc::new(Mutex::new(Vec::new()));
    let page = Page::spawn(
        {
            let sent = Arc::clone(&sent);
            move || Ok(Held { gate, sent })
        },
        Setup::default(),
        || {},
    );
    (page, open, sent)
}

fn frame() -> FrameAt {
    FrameAt {
        size: (800, 600),
        scale: 1.0,
        now: 1.0,
        reduce_motion: false,
    }
}

#[test]
fn what_the_page_was_told_is_unprocessed_until_the_module_has_done_it_and_a_call_is_counted_until_answered()
 {
    let (mut page, open, sent) = held();
    assert!(page.module().flush(Duration::from_secs(5)));
    page.frame(frame());
    assert!(page.module().flush(Duration::from_secs(5)));
    // The page is told something and the module holds it: it is not processed.
    page.focus(None);
    let rest = page.rest();
    assert!(rest.unprocessed, "{rest:?}");
    assert!(!rest.resting());
    // It says two calls and a subscription while it handles the next.
    sent.lock().unwrap().extend([
        r#"{"call":{"id":1,"method":"list"}}"#.to_owned(),
        r#"{"call":{"id":2,"method":"save"}}"#.to_owned(),
        r#"{"subscribe":{"id":3,"topic":"stats"}}"#.to_owned(),
    ]);
    open.send(()).unwrap();
    assert!(page.module().flush(Duration::from_secs(5)));
    page.drain();
    // What is told again is held again, and is let go the same way.
    page.focus(None);
    assert!(page.rest().unprocessed);
    open.send(()).unwrap();
    assert!(page.module().flush(Duration::from_secs(5)));
    let rest = page.rest();
    assert!(!rest.unprocessed, "{rest:?}");
    assert_eq!(rest.calls, 2, "a subscription is not a call: {rest:?}");
    assert!(!rest.resting());
    // The program answers one and pushes to the subscription: one call is left.
    page.message(r#"{"response":{"id":1,"result":{}}}"#);
    page.message(r#"{"push":{"id":3,"value":1}}"#);
    assert_eq!(page.rest().calls, 1);
    page.message(r#"{"response":{"id":2,"result":{}}}"#);
    assert!(page.module().flush(Duration::from_secs(5)));
    page.drain();
    page.frame(frame());
    assert!(page.module().flush(Duration::from_secs(5)));
    page.drain();
    assert!(page.rest().resting(), "{:?}", page.rest());
}

// --- Pages under the host with no screen --------------------------------------------------------

/// A page of a list that scrolls, an editor, a field of each kind, a button that makes a call, two
/// commands and a cover over a button.
const PAGE: &str = r#"
local loaded = false
local saved = "not saved"
pencere.subscribe("stats", nil, function(pushed) return true end)
return {
    render = function()
        local list = pencere.div():id("list"):h(60):w(300):scroll()
        for n = 1, 30 do list:child(pencere.div():h(20):no_shrink():child(pencere.text("row " .. n))) end
        return pencere.div():size_full()
            :command("rename", "Rename", { "f2", "ctrl+r" })
            :command("go", "Go", {})
            :child(pencere.div():id("q"):text_field("q"):h(24):w(200):child(pencere.text("Search")))
            :child(pencere.div():id("pw"):text_field("pw", "secret"):h(24):w(200):child(pencere.text("Password")))
            :child(pencere.div():id("save"):press("save"):size(80, 24):child(pencere.text("Save")))
            :child(pencere.text(saved))
            :child(pencere.editor("sql", {}):label("Query"):h(60):w(300))
            :child(list)
            --COVER--
    end,
    handle = function(event)
        if event.type == "viewport" and not loaded then
            loaded = true
            return { editor = { sql = { load = "line 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8\nline 9\nline 10\nline 11\nline 12\nline 13\nline 14\nline 15\nline 16\nline 17\nline 18\nline 19\nline 20" } } }
        elseif event.type == "release" then
            pencere.call("save", nil, function(result, error)
                saved = "saved"
                return true
            end)
            return true
        end
    end,
}
"#;

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_reading_of_the_container_app_gives_its_rows_by_names_that_stay() -> Result<(), Error> {
    use std::collections::HashMap;

    use crate::headless::Failure;

    let mut states: HashMap<String, &'static str> =
        HashMap::from([("c1".into(), "running"), ("c2".into(), "exited")]);
    let program = Calls::new()
        .on("list", |_| {
            Ok(json!({ "containers": [
                { "id": "c1", "name": "api", "image": "api:1", "state": "running", "ports": "" },
                { "id": "c2", "name": "db", "image": "postgres:16", "state": "exited", "ports": "" },
            ] }))
        })
        .on("toggle", move |params| {
            let id = params["id"].as_str().unwrap_or_default().to_owned();
            let Some(state) = states.get_mut(&id) else {
                return Err(Failure {
                    code: "not-found".into(),
                    message: id,
                });
            };
            *state = if *state == "running" { "exited" } else { "running" };
            Ok(json!({ "id": id, "state": *state }))
        });
    let mut page = Headless::app(example("containers-lua"))
        .size(900.0, 500.0)
        .peer(program)
        .open()?;
    let first = page.reading();
    let text = first.text();
    let stop = reference(Id::named("act c1"));
    assert!(text.contains(&format!("{stop}  button \"Stop\"")), "{text}");
    assert!(text.contains("\"api\""), "{text}");
    assert!(text.contains("\"postgres:16\""), "{text}");
    // The names stay from one reading to the next.
    assert_eq!(page.reading().text(), text);
    assert_eq!(first.find(&stop), Found::One(Id::named("act c1")));

    page.click("act c1")?;
    let after = page.reading().text();
    assert!(
        after.contains(&format!("{stop}  button \"Start\"")),
        "{after}"
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_reading_names_an_icon_alone_by_its_tip_and_says_which_tab_is_chosen() -> Result<(), Error> {
    let source = r#"
return {
    render = function()
        local rail = pencere.div():col():gap(4)
        for _, name in ipairs({ "containers", "images" }) do
            rail:child(pencere.div():id("sec:" .. name):press("sec:" .. name):size(24, 24)
                :role("tab"):selected(name == "images"):tip(name, "1")
                :child(pencere.div():size(12, 12)))
        end
        return pencere.div():size_full():child(rail)
            :child(pencere.div():id("plain"):press("plain"):size(24, 24):tip("Run it", "r"))
    end,
    handle = function(event) end,
}
"#;
    let mut page = Headless::script(source).size(400.0, 300.0).open()?;
    page.settle()?;
    let text = page.reading().text();
    let named = |name: &str| reference(Id::named(name));
    assert!(
        text.contains(&format!("{}  tab \"containers\"", named("sec:containers"))),
        "{text}"
    );
    assert!(
        text.contains(&format!(
            "{}  tab \"images\" [selected]",
            named("sec:images")
        )),
        "{text}"
    );
    assert!(
        text.contains(&format!("{}  button \"Run it\"", named("plain"))),
        "{text}"
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_reading_of_a_dialog_over_a_scrim_gives_its_buttons_and_a_press_on_one_reaches_it()
-> Result<(), Error> {
    let source = r#"
local ui = pencere.Ui.new()
local said = "nothing pressed"
return {
    render = function()
        local dialog = pencere.div():col():gap(4):p(8):w(200)
            :child(pencere.text("Dialog title"))
            :child(pencere.div():id("name"):text_field("name"):h(24):child(pencere.text("Name")))
            :child(pencere.div():row():gap(4)
                :child(pencere.div():id("ok"):press("ok"):h(24):child(pencere.text("OK")))
                :child(pencere.div():id("cancel"):press("cancel"):h(24):child(pencere.text("Cancel"))))
        return pencere.div():size_full():child(pencere.text(said))
            :child(pencere.div():id("scrim"):press("scrim"):label("Close dialog"):overlay():inset(0, 0, 0, 0):child(dialog))
    end,
    handle = function(event)
        if event.type == "press" then
            ui:press(pencere.layout, event)
        elseif event.type == "release" then
            if ui:release(pencere.layout, event) == "ok" then
                said = "ok pressed"
            end
            return true
        end
    end,
}
"#;
    let mut page = Headless::script(source).size(400.0, 300.0).open()?;
    page.settle()?;
    let first = page.reading();
    let text = first.text();
    let named = |name: &str| reference(Id::named(name));
    // The dialog is not one button named by its title: its buttons and its field are there.
    assert!(
        text.contains(&format!("{}  button \"OK\"", named("ok"))),
        "{text}"
    );
    assert!(
        text.contains(&format!("{}  button \"Cancel\"", named("cancel"))),
        "{text}"
    );
    assert!(
        text.contains(&format!("{}  button \"Close dialog\"", named("scrim"))),
        "{text}"
    );
    // The dialog's own words are words, not the scrim's name.
    assert!(
        first
            .items
            .iter()
            .any(|item| item.role == "text" && item.words == "Dialog title"),
        "{text}"
    );
    assert!(
        first
            .fields
            .iter()
            .any(|field| field.id == Id::named("name"))
    );
    // The ref of the button, as an agent has it, finds it and a click on it is what it does.
    assert_eq!(first.find(&named("ok")), Found::One(Id::named("ok")));
    page.click("ok")?;
    assert!(
        page.reading()
            .items
            .iter()
            .any(|item| item.words == "ok pressed"),
        "{}",
        page.reading().text()
    );
    Ok(())
}

/// The page, with a cover over all of it where `covered`.
fn page(covered: bool) -> String {
    let cover = r#":child(pencere.div():id("cover"):press("cover"):overlay():inset(0, 0, 0, 0))"#;
    PAGE.replace("--COVER--", if covered { cover } else { "" })
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_reading_gives_the_fields_the_commands_and_the_editor_and_never_a_secret_value()
-> Result<(), Error> {
    let mut page = Headless::script(page(false)).size(400.0, 600.0).open()?;
    page.settle()?;
    let reading = page.reading();
    // Commands, with their keys.
    assert!(
        reading
            .commands
            .iter()
            .any(|c| c.title == "Rename" && c.keys == ["f2", "ctrl+r"] && c.enabled),
        "{:?}",
        reading.commands
    );
    assert!(reading.commands.iter().any(|c| c.title == "Go"));
    // Fields: the hint, the kind and what each holds.
    page.fill("q", "api")?;
    page.fill("pw", "hunter2")?;
    let reading = page.reading();
    let q = reading
        .fields
        .iter()
        .find(|f| f.id == Id::named("q"))
        .expect("q");
    assert_eq!(
        (q.kind, q.hint.as_deref(), q.value.as_deref()),
        (FieldKind::Line, Some("Search"), Some("api"))
    );
    let pw = reading
        .fields
        .iter()
        .find(|f| f.id == Id::named("pw"))
        .expect("pw");
    assert_eq!((pw.kind, pw.value.as_deref()), (FieldKind::Secret, None));
    // The secret is in neither writer, and no field is among the items twice.
    for written in [reading.text(), reading.json()] {
        assert!(!written.contains("hunter2"), "{written}");
        assert!(written.contains("api"), "{written}");
    }
    assert!(
        reading
            .items
            .iter()
            .all(|item| item.role != "field" && item.role != "editor")
    );
    assert!(
        reading
            .items
            .iter()
            .any(|item| item.role == "button" && item.words == "Save")
    );
    // The page is read the same, twice.
    assert_eq!(page.reading(), reading);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_lines_an_editor_shows_and_what_is_selected_change_as_it_is_scrolled_and_selected()
-> Result<(), Error> {
    let mut page = Headless::script(page(false)).size(400.0, 600.0).open()?;
    page.settle()?;
    let reading = page.reading();
    let editor = reading
        .editors
        .iter()
        .find(|e| e.id == Id::named("sql"))
        .expect("editor");
    assert_eq!(editor.label.as_deref(), Some("Query"));
    assert_eq!(editor.total, 20);
    assert_eq!(editor.first, 1, "{editor:?}");
    assert!(
        editor.lines.len() >= 2 && editor.lines.len() < 20,
        "{:?}",
        editor.lines
    );
    assert_eq!(editor.lines.first().map(String::as_str), Some("line 1"));
    assert_eq!(editor.selection, None);
    assert!(!editor.focused);
    let shown = editor.lines.len();

    page.select("sql", 0, 6)?;
    let selected = page.reading();
    let editor = selected
        .editors
        .iter()
        .find(|e| e.id == Id::named("sql"))
        .expect("editor");
    assert!(editor.focused);
    assert_eq!(
        selected.focus,
        crate::reading::Focus::Editor(Id::named("sql"))
    );
    assert_eq!(editor.caret, Place { line: 1, column: 7 });
    assert_eq!(
        editor.selection,
        Some(Selected {
            from: Place { line: 1, column: 1 },
            to: Place { line: 1, column: 7 },
            text: "line 1".to_owned()
        })
    );

    // The wheel over it scrolls it: the lines that show are other lines.
    page.wheel("sql", 0.0, 400.0)?;
    let scrolled = page.reading();
    let editor = scrolled
        .editors
        .iter()
        .find(|e| e.id == Id::named("sql"))
        .expect("editor");
    assert!(editor.first > 1, "{editor:?}");
    assert_eq!(
        editor.lines.first().map(String::as_str),
        Some(format!("line {}", editor.first).as_str())
    );
    assert!(editor.lines.len() <= shown + 1);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_box_that_scrolls_says_where_it_is_and_what_is_above_and_below() -> Result<(), Error> {
    let mut page = Headless::script(page(false)).size(400.0, 600.0).open()?;
    page.settle()?;
    let at_top = page.reading();
    let list = at_top
        .scrolls
        .iter()
        .find(|s| s.id == Id::named("list"))
        .expect("list");
    assert_eq!((list.percent, list.above, list.below), (0, false, true));
    // The editor's own scrolling is its lines, not a box.
    assert!(at_top.scrolls.iter().all(|s| s.id != Id::named("sql")));
    page.scroll_to("list", 300.0)?;
    let middle = page.reading();
    let list = middle
        .scrolls
        .iter()
        .find(|s| s.id == Id::named("list"))
        .expect("list");
    // 300 of the most it can be: 30 rows of 20 points in 60.
    assert_eq!((list.percent, list.above, list.below), (56, true, true));
    page.scroll_to("list", 10_000.0)?;
    let end = page.reading();
    let list = end
        .scrolls
        .iter()
        .find(|s| s.id == Id::named("list"))
        .expect("list");
    assert_eq!((list.percent, list.above, list.below), (100, true, false));
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_press_reaches_an_element_unless_what_covers_its_middle_is_what_it_reaches() -> Result<(), Error>
{
    let mut page = Headless::script(page(true)).size(400.0, 600.0).open()?;
    page.settle()?;
    // The cover is over everything, the button under it included.
    assert_eq!(
        page.page().reach(Id::named("save")),
        Err(crate::Unreachable::Covered)
    );
    assert!(page.page().reach(Id::named("cover")).is_ok());
    assert_eq!(
        page.page().reach(Id::named("nothing")),
        Err(crate::Unreachable::Absent)
    );
    assert_eq!(
        page.click("save"),
        Err(Error::Unreachable("save".to_owned()))
    );
    assert_eq!(
        page.click("nothing"),
        Err(Error::NoSuch("nothing".to_owned()))
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_call_with_no_answer_is_waited_for_and_a_push_to_a_subscription_is_not() -> Result<(), Error> {
    let source = page(false);
    let mut page = Headless::script(&source).size(400.0, 600.0).open()?;
    page.settle()?;
    assert!(page.reading().rest.resting(), "{:?}", page.reading().rest);
    page.click("save")?;
    assert_eq!(page.peer().waiting().len(), 1);
    let waiting = page.reading();
    assert_eq!(waiting.rest.calls, 1, "{:?}", waiting.rest);
    assert!(!waiting.rest.resting());
    assert!(
        waiting
            .text()
            .contains("rest: 1 call is waiting for an answer"),
        "{}",
        waiting.text()
    );
    assert_eq!(
        waiting.json().parse::<Value>().expect("json")["rest"]["calls"],
        json!(1)
    );
    // A topic's push is no call.
    let pushed = page.push("stats", &json!({ "cpu": 1 }))?;
    assert_eq!(pushed, 1);
    assert_eq!(page.reading().rest.calls, 1);
    // The answer ends the wait.
    assert!(page.answer("save", Ok(json!({})))?);
    let answered = page.reading();
    assert_eq!(answered.rest.calls, 0, "{:?}", answered.rest);
    assert!(answered.rest.resting());
    assert!(
        answered.items.iter().any(|item| item.words == "saved"),
        "{:?}",
        answered.items
    );
    Ok(())
}

#[test]
fn long_lines_of_an_editor_are_cut_where_the_writers_cut_them() {
    let long = "y".repeat(LINE_LIMIT + 10);
    assert_eq!(clean(&long, LINE_LIMIT).chars().count(), LINE_LIMIT + 1);
}

#[test]
fn a_group_is_told_with_its_children_under_it_a_page_behind_a_modal_is_counted_and_the_shape_is_numbered()
 {
    let group = item(AccessibleRole::MenuItem, Some(Id::named("row")), "alpha");
    let mut child = item(AccessibleRole::Checkbox, Some(Id::named("mark")), "Mark");
    child.parent = Some(0);
    let reading = Reading {
        items: vec![group, child],
        behind: 3,
        ..Reading::default()
    };
    let text = reading.text();
    assert!(
        text.contains("note: behind the modal in front, 3 items of the page"),
        "the page behind is counted: {text}"
    );
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| line.contains("alpha") || line.contains("Mark"))
        .collect();
    assert!(lines[0].contains(" menuitem \"alpha\""), "{lines:?}");
    assert!(
        lines[1].contains("   checkbox \"Mark\""),
        "a child is two spaces in: {lines:?}"
    );
    let json: Value = serde_json::from_str(&reading.json()).expect("json");
    assert_eq!(json["format"], 2);
    assert_eq!(json["behind"], 3);
    assert_eq!(json["items"][0]["parent"], Value::Null);
    assert_eq!(
        json["items"][1]["parent"],
        json!(reference(Id::named("row")))
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_page_that_writes_a_layer_a_scrim_that_is_not_read_and_a_name_to_fall_back_on_is_read_by_the_model()
-> Result<(), Error> {
    let source = r#"
return {
    render = function()
        local scrim = pencere.div():absolute():inset(0, 0, 0, 0):press("scrim"):role("none")
        local card = pencere.div():col():child(pencere.text("Settings"))
            :child(pencere.div():id("ok"):press("ok"):h(24):w(60):child(pencere.text("OK")))
        local row = pencere.div():id("row"):press("row"):role("menu_item"):fallback_name("Row 3"):h(24):w(200)
            :child(pencere.div():id("mark"):role("checkbox"):checked(true):press("mark"):size(16, 16))
        return pencere.div():size_full()
            :child(pencere.div():id("behind"):press("behind"):h(24):w(80):child(pencere.text("Behind")))
            :child(row)
            :child(pencere.div():overlay():inset(0, 0, 0, 0):id("dialog"):layer(true):child(scrim):child(card))
            -- A role this kit does not know is left out, and the page runs.
            :child(pencere.div():role("a role of a later kit"):child(pencere.text("Later")))
    end,
}
"#;
    let mut page = Headless::script(source).size(400.0, 300.0).open()?;
    page.settle()?;
    let reading = page.reading();
    let text = reading.text();
    // The page behind the dialog is not read, and says how much of it there was; the scrim is not read
    // and what is in the dialog is.
    assert!(
        reading
            .items
            .iter()
            .all(|item| item.id != Some(Id::named("behind"))),
        "{text}"
    );
    assert!(reading.behind >= 2, "{text}");
    assert!(
        reading.items.iter().any(|item| item.words == "OK"),
        "{text}"
    );
    assert!(
        reading.items.iter().all(|item| item.role != "none"),
        "{text}"
    );
    assert!(reading.audit().is_empty(), "{:?}", reading.audit());
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_row_with_a_control_in_it_and_no_name_is_read_by_its_fallback_name_with_the_control_under_it()
-> Result<(), Error> {
    let source = r#"
return {
    render = function()
        local row = pencere.div():id("row"):press("row"):role("menu_item"):fallback_name("Row 3"):h(24):w(200)
            :child(pencere.div():id("mark"):role("checkbox"):checked(true):press("mark"):size(16, 16))
        return pencere.div():size_full():child(row):child(pencere.div():role("a role of a later kit"):child(pencere.text("Later")))
    end,
}
"#;
    let mut page = Headless::script(source).size(400.0, 300.0).open()?;
    page.settle()?;
    let reading = page.reading();
    let row = reading
        .items
        .iter()
        .position(|item| item.id == Some(Id::named("row")))
        .expect("row");
    assert_eq!(reading.items[row].words, "Row 3", "{}", reading.text());
    let mark = reading
        .items
        .iter()
        .find(|item| item.id == Some(Id::named("mark")))
        .expect("mark");
    assert_eq!(mark.parent, Some(row));
    // The text of the page that has a role the kit does not know is read as words.
    assert!(
        reading.items.iter().any(|item| item.words == "Later"),
        "{}",
        reading.text()
    );
    Ok(())
}
