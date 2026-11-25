//! What a page says of its sounds (`sound` in a reply): the ways to say it, what is the script's
//! mistake and what is only told to the page's author, and how replies put together keep every
//! one. The tests run the interpreter, so they are `make check-heavy`'s.

use std::time::Duration;

use pencere_view::sound::{self, Ask, MOST_ASKS, Play, Source};
use pencere_view::{Event, Input, Key, Mods, Reply};

use crate::ModuleLimits;
use crate::luau_tests::{failed, open_script, until_seen};
use crate::test_support::{Open, SIZE};

/// A page that answers the key `k` with the reply `reply` is (a Lua expression) and a message of
/// its program with `{ sound = message.sound }`.
fn open(reply: &str) -> Open {
    let mut page = open_script(
        &format!(
            r##"
            return {{
                render = function() return pencere.div() end,
                handle = function(e)
                    if e.type == "key" and e.key == "k" then
                        return {reply}
                    end
                end,
                message = function(message)
                    return {{ sound = message.sound }}
                end,
            }}
            "##
        ),
        ModuleLimits::default(),
    );
    page.module.render(SIZE, 0.0, false);
    page.until(Duration::from_secs(20), |u| {
        u.frame.is_some() || u.closed.is_some()
    });
    page
}

/// What the page answers to the key `k`.
fn answer(page: &mut Open) -> Reply {
    let before = page.seen.replies.len();
    page.module.event(Event::Input(Input::Key {
        key: Key::Character('k'),
        mods: Mods::default(),
    }));
    until_seen(page, Duration::from_secs(10), |seen| {
        seen.replies.len() > before
    });
    page.seen.replies.last().expect("an answer").reply.clone()
}

/// What a page that says `reply` to the key `k` asks of its sounds.
fn asked(reply: &str) -> Vec<Ask> {
    answer(&mut open(reply)).sound
}

/// Why a page that says `reply` to the key `k` is closed.
fn refused(reply: &str) -> String {
    let mut page = open(reply);
    page.module.event(Event::Input(Input::Key {
        key: Key::Character('k'),
        mods: Mods::default(),
    }));
    failed(&page.until(Duration::from_secs(10), |u| u.closed.is_some()))
}

fn kit(name: &str, volume: f32, pitch: f32) -> Ask {
    Ask::Play(Play {
        source: Source::Kit(name.into()),
        volume,
        pitch,
        samples: None,
    })
}

fn file(path: &str, volume: f32, pitch: f32) -> Ask {
    Ask::Play(Play {
        source: Source::File(path.into()),
        volume,
        pitch,
        samples: None,
    })
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_sound_of_the_kit_is_asked_for_by_its_name_loud_and_fast_as_the_page_says() {
    assert_eq!(
        asked(r#"{ sound = "success" }"#),
        [kit("success", 1.0, 1.0)]
    );
    assert_eq!(
        asked(r#"{ sound = { name = "tap" } }"#),
        [kit("tap", 1.0, 1.0)]
    );
    assert_eq!(
        asked(r#"{ sound = { name = "tap", volume = 0.5, pitch = 2 } }"#),
        [kit("tap", 0.5, 2.0)]
    );
    // The two ends of what is asked are asked for.
    assert_eq!(
        asked(r#"{ sound = { name = "snap", volume = 0, pitch = 0.5 } }"#),
        [kit("snap", 0.0, 0.5)]
    );
    // The reply is the page's whole answer: the rest of it goes through.
    let reply = answer(&mut open(r#"{ redraw = true, sound = "send" }"#));
    assert!(reply.redraw);
    assert_eq!(reply.sound, [kit("send", 1.0, 1.0)]);
    assert!(reply.diagnostics.is_empty());
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_file_is_asked_for_by_a_path_a_digest_or_a_handle() {
    assert_eq!(
        asked(r#"{ sound = { file = "sounds/ding.wav", volume = 0.25, pitch = 1.5 } }"#),
        [file("sounds/ding.wav", 0.25, 1.5)]
    );
    assert_eq!(
        asked(r#"{ sound = { file = "sha256:abc" } }"#),
        [file("sha256:abc", 1.0, 1.0)]
    );
    assert_eq!(
        asked(r#"{ sound = { file = "file:7" } }"#),
        [file("file:7", 1.0, 1.0)]
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_list_is_every_sound_in_the_order_it_is_written_with_the_end_of_all_where_it_is() {
    assert_eq!(
        asked(
            r#"{ sound = { "tap", { name = "snap", volume = 0.5 }, false, { file = "a.ogg" }, "success" } }"#
        ),
        [
            kit("tap", 1.0, 1.0),
            kit("snap", 0.5, 1.0),
            Ask::Stop,
            file("a.ogg", 1.0, 1.0),
            kit("success", 1.0, 1.0),
        ]
    );
    assert_eq!(asked("{ sound = false }"), [Ask::Stop]);
    // Nothing to say is no ask.
    assert_eq!(asked("{ sound = {} }"), []);
    assert_eq!(asked("{ redraw = true }"), []);
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_name_the_kit_has_no_sound_for_is_told_to_the_author_and_the_rest_of_the_reply_goes_through() {
    let reply = answer(&mut open(
        r#"{ redraw = true, sound = { "tap", "nope", { name = "also_not", volume = 0.5 }, "snap" } }"#,
    ));
    assert!(reply.redraw);
    // The sounds the kit has are asked for, in order; the others are not.
    assert_eq!(reply.sound, [kit("tap", 1.0, 1.0), kit("snap", 1.0, 1.0)]);
    assert_eq!(reply.diagnostics.len(), 2, "{:?}", reply.diagnostics);
    assert!(reply.diagnostics[0].contains("\"nope\""));
    assert!(reply.diagnostics[1].contains("\"also_not\""));
    // It says which names the page may ask for.
    for name in sound::NAMES {
        assert!(reply.diagnostics[0].contains(name), "{name}");
    }
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_reply_to_the_program_or_a_timer_may_ask_for_a_sound_too() {
    let mut page = open("nil");
    let before = page.seen.replies.len();
    page.module
        .message(r#"{"sound":{"name":"notify","volume":0.5}}"#.into());
    until_seen(&mut page, Duration::from_secs(10), |seen| {
        seen.replies.len() > before
    });
    assert_eq!(
        page.seen.replies.last().expect("an answer").reply.sound,
        [kit("notify", 0.5, 1.0)]
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn what_is_no_sound_a_number_out_of_its_range_and_too_many_are_the_scripts_mistake() {
    let taps = |count: usize| {
        format!(
            "{{ sound = (function() local list = {{}} for i = 1, {count} do list[i] = \"tap\" end return list end)() }}"
        )
    };
    let too_many = taps(MOST_ASKS + 1);
    for (reply, why) in [
        ("{ sound = true }", "sound is the name of a sound"),
        ("{ sound = 5 }", "sound is the name of a sound"),
        (
            r#"{ sound = { name = "tap", file = "a.wav" } }"#,
            "sound is the name of a sound",
        ),
        ("{ sound = { name = 3 } }", "sound is the name of a sound"),
        ("{ sound = { file = {} } }", "sound is the name of a sound"),
        // A table that is neither a sound nor a list is a sound the page forgot to name.
        ("{ sound = { volume = 1 } }", "sound is the name of a sound"),
        // Two things said at once: a sound with entries, a list with keys.
        (
            r#"{ sound = { "tap", volume = 0.2 } }"#,
            "sound is the name of a sound",
        ),
        (
            r#"{ sound = { name = "tap", "snap" } }"#,
            "sound is the name of a sound",
        ),
        (
            r#"{ sound = { { name = "tap" }, extra = true } }"#,
            "sound is the name of a sound",
        ),
        (
            r#"{ sound = { "tap", true } }"#,
            "sound is the name of a sound",
        ),
        (
            r#"{ sound = { "tap", { "snap" } } }"#,
            "sound is the name of a sound",
        ),
        (
            r#"{ sound = { name = "tap", volume = 1.5 } }"#,
            "a sound's volume is a number from 0 to 1",
        ),
        (
            r#"{ sound = { name = "tap", volume = -0.1 } }"#,
            "a sound's volume is a number from 0 to 1",
        ),
        (
            r#"{ sound = { name = "tap", volume = "loud" } }"#,
            "a sound's volume is a number from 0 to 1",
        ),
        (
            r#"{ sound = { name = "tap", volume = 0 / 0 } }"#,
            "a sound's volume is a number from 0 to 1",
        ),
        (
            r#"{ sound = { name = "tap", pitch = 0.4 } }"#,
            "a sound's pitch is a number from 0.5 to 2",
        ),
        (
            r#"{ sound = { file = "a.wav", pitch = 2.1 } }"#,
            "a sound's pitch is a number from 0.5 to 2",
        ),
        (too_many.as_str(), "a reply asks for at most 64 sounds"),
    ] {
        let said = refused(reply);
        assert!(said.contains(why), "{reply}: {said}");
    }
    // The most the floor lets through is let through.
    assert_eq!(asked(&taps(MOST_ASKS)).len(), MOST_ASKS);
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn replies_put_together_keep_every_sound_in_order_and_the_end_of_all_where_it_was_said() {
    assert_eq!(
        asked(r#"pencere.merge({ sound = "tap" }, { redraw = true }, { sound = "snap" })"#),
        [kit("tap", 1.0, 1.0), kit("snap", 1.0, 1.0)]
    );
    assert_eq!(
        asked(r#"pencere.merge({ sound = "tap" }, { sound = false }, { sound = "snap" })"#),
        [kit("tap", 1.0, 1.0), Ask::Stop, kit("snap", 1.0, 1.0)]
    );
    // A list and a sound, a table of one and a list of two.
    assert_eq!(
        asked(
            r#"pencere.merge({ sound = { "tap", "select" } }, { sound = { name = "snap", volume = 0.5 } }, { sound = { { file = "a.wav" }, "open" } })"#
        ),
        [
            kit("tap", 1.0, 1.0),
            kit("select", 1.0, 1.0),
            kit("snap", 0.5, 1.0),
            file("a.wav", 1.0, 1.0),
            kit("open", 1.0, 1.0),
        ]
    );
    assert_eq!(
        asked(r#"pencere.merge(true, { sound = "close" })"#),
        [kit("close", 1.0, 1.0)]
    );
    assert_eq!(
        asked(r#"pencere.merge({ sound = false }, nil, { sound = "type" })"#),
        [Ask::Stop, kit("type", 1.0, 1.0)]
    );
    // What is put together is not changed: a page that keeps the reply it merged says it again.
    assert_eq!(
        asked(
            r#"(function()
                local mine = { sound = { "tap" } }
                pencere.merge(mine, { sound = "snap" })
                return pencere.merge(mine, { sound = "open" })
            end)()"#
        ),
        [kit("tap", 1.0, 1.0), kit("open", 1.0, 1.0)]
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn every_name_and_format_the_kit_has_is_named_by_pencere_has_and_one_it_has_not_is_not() {
    let names = sound::NAMES
        .iter()
        .map(|name| format!("pencere.has(\"sound.{name}\")"))
        .collect::<Vec<_>>()
        .join(" and ");
    let mut page = open(&format!(
        r#"{{ sound = (pencere.has("sound") and pencere.has("sound.wav") and pencere.has("sound.ogg") and {names}
            and not pencere.has("sound.nope") and not pencere.has("sound.mp3") and not pencere.has("sound.")) and "tap" or "nope" }}"#
    ));
    let reply = answer(&mut page);
    assert_eq!(
        reply.sound,
        [kit("tap", 1.0, 1.0)],
        "{:?}",
        reply.diagnostics
    );
}

/// The list of names the prelude knows is its own, in Lua: it is the kit's list, not one more and
/// not one fewer.
#[test]
fn the_names_the_prelude_says_are_the_names_the_kit_has() {
    let prelude = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../luau/prelude/10-element.lua"
    ))
    .expect("the prelude");
    let (_, rest) = prelude
        .split_once("HAS[\"sound.ogg\"] = true\nfor name in string.gmatch(\"")
        .expect("the names of the sounds");
    let (said, _) = rest.split_once('"').expect("the end of the names");
    assert_eq!(said.split_whitespace().collect::<Vec<_>>(), sound::NAMES);
}
