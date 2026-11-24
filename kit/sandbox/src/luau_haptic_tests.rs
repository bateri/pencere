//! What a page says of the vibration of the trackpad (`haptic` in a reply): the four ways to say it,
//! what is the script's mistake and what is only told to the page's author, and how replies put
//! together keep one. The tests run the interpreter, so they are `make check-heavy`'s.

use std::time::Duration;

use pencere_view::haptic::{self, Event as Tap, Haptic, MOST_EVENTS, Pattern};
use pencere_view::{Event, Input, Key, Mods, Reply};

use crate::ModuleLimits;
use crate::luau_tests::{failed, open_script, until_seen};
use crate::test_support::{Open, SIZE};

/// A page that answers the key `k` with the reply `reply` is (a Lua expression) and a message of
/// its program with `{ haptic = message.haptic }`.
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
                    return {{ haptic = message.haptic or false }}
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

/// What a page that says `reply` to the key `k` asks of the trackpad.
fn asked(reply: &str) -> Option<Haptic> {
    answer(&mut open(reply)).haptic
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

fn play(name: &str) -> Haptic {
    Haptic::Play(haptic::preset(name).expect("a name"))
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_ready_pattern_is_asked_for_by_its_name_and_as_strong_as_the_page_says() {
    assert_eq!(asked(r#"{ haptic = "tick" }"#), Some(play("tick")));
    assert_eq!(
        asked(r#"{ haptic = { name = "thud" } }"#),
        Some(play("thud"))
    );
    let half = haptic::preset("thud").expect("a name").scaled(0.5);
    assert_eq!(
        asked(r#"{ haptic = { name = "thud", intensity = 0.5 } }"#),
        Some(Haptic::Play(half))
    );
    // The reply is the page's whole answer: the rest of it goes through.
    let reply = answer(&mut open(r#"{ redraw = true, haptic = "snap" }"#));
    assert!(reply.redraw);
    assert_eq!(reply.haptic, Some(play("snap")));
    assert!(reply.diagnostics.is_empty());
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_pattern_of_the_pages_own_is_made_of_taps_and_held_events_with_what_is_not_said_as_it_is_in_the_default()
 {
    let asked = asked(
        r#"{ haptic = { events = {
            { at = 0.2, intensity = 0.4 },
            { seconds = 0.3, intensity = 0.1, intensity_end = 0.9, sharpness = 0.2, sharpness_end = 0.8 },
            { at = 0.2, sharpness = 1 },
        } } }"#,
    );
    let pattern = Pattern::new([
        Tap::held(0.0, 0.3, (0.1, 0.9), (0.2, 0.8)),
        Tap::tap(0.2, 0.4, 0.5),
        Tap::tap(0.2, 1.0, 1.0),
    ])
    .expect("a pattern");
    assert_eq!(asked, Some(Haptic::Play(pattern)));
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn false_and_a_pattern_with_no_events_end_what_the_view_is_playing_and_nothing_is_not_an_ask() {
    assert_eq!(asked("{ haptic = false }"), Some(Haptic::Stop));
    assert_eq!(asked("{ haptic = { events = {} } }"), Some(Haptic::Stop));
    let mut page = open("{ redraw = true }");
    assert_eq!(answer(&mut page).haptic, None);
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_name_the_kit_has_no_pattern_for_is_told_to_the_author_and_the_rest_of_the_reply_goes_through()
{
    let reply = answer(&mut open(r#"{ redraw = true, haptic = "nope" }"#));
    assert!(reply.redraw);
    assert_eq!(reply.haptic, None);
    assert_eq!(reply.diagnostics.len(), 1, "{:?}", reply.diagnostics);
    assert!(
        reply.diagnostics[0].contains("\"nope\""),
        "{:?}",
        reply.diagnostics
    );
    // It says which names the page may ask for.
    assert!(
        reply.diagnostics[0].contains("heartbeat"),
        "{:?}",
        reply.diagnostics
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_reply_to_the_program_or_a_timer_may_ask_for_a_vibration_too() {
    let mut page = open("nil");
    let before = page.seen.replies.len();
    page.module.message(r#"{"haptic":"success"}"#.into());
    until_seen(&mut page, Duration::from_secs(10), |seen| {
        seen.replies.len() > before
    });
    assert_eq!(
        page.seen.replies.last().expect("an answer").reply.haptic,
        Some(play("success"))
    );
    // `false` is the end of it, and says so.
    let before = page.seen.replies.len();
    page.module.message("{}".into());
    until_seen(&mut page, Duration::from_secs(10), |seen| {
        seen.replies.len() > before
    });
    assert_eq!(
        page.seen.replies.last().expect("an answer").reply.haptic,
        Some(Haptic::Stop)
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn what_is_not_a_vibration_a_number_out_of_its_range_and_a_pattern_past_the_floors_are_the_scripts_mistake()
 {
    for (reply, why) in [
        ("{ haptic = true }", "haptic is the name of a pattern"),
        ("{ haptic = 5 }", "haptic is the name of a pattern"),
        ("{ haptic = {} }", "haptic is the name of a pattern"),
        (
            r#"{ haptic = { name = "tick", events = {} } }"#,
            "haptic is the name of a pattern",
        ),
        (
            "{ haptic = { name = 3 } }",
            "haptic is the name of a pattern",
        ),
        (
            "{ haptic = { events = 3 } }",
            "haptic is the name of a pattern",
        ),
        (
            r#"{ haptic = { name = "tick", intensity = 2 } }"#,
            "a haptic's intensity is a number from 0 to 1",
        ),
        (
            r#"{ haptic = { name = "tick", intensity = "loud" } }"#,
            "a haptic's intensity is a number from 0 to 1",
        ),
        (
            "{ haptic = { events = { 1 } } }",
            "an event of a haptic pattern is a table",
        ),
        (
            "{ haptic = { events = { { sharpness = -0.1 } } } }",
            "a haptic's sharpness is a number from 0 to 1",
        ),
        (
            "{ haptic = { events = { { intensity_end = 1.5 } } } }",
            "a haptic's intensity_end is a number from 0 to 1",
        ),
        (
            "{ haptic = { events = { { intensity = 0 / 0 } } } }",
            "a haptic's intensity is a number from 0 to 1",
        ),
        (
            "{ haptic = { events = { { at = -1 } } } }",
            "a haptic's at is a number from 0 to 60",
        ),
        (
            "{ haptic = { events = { { seconds = 61 } } } }",
            "a haptic's seconds is a number from 0 to 60",
        ),
        (
            "{ haptic = { events = { { at = 30, seconds = 31 } } } }",
            "a haptic pattern lasts at most 60 seconds",
        ),
        (
            "{ haptic = { events = (function() local list = {} for i = 1, 257 do list[i] = { at = 0 } end return list end)() } }",
            "a haptic pattern has at most 256 events",
        ),
    ] {
        let said = refused(reply);
        assert!(said.contains(why), "{reply}: {said}");
    }
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_pattern_at_the_floors_is_a_pattern() {
    let at_most = asked(&format!(
        "{{ haptic = {{ events = (function() local list = {{}} for i = 1, {MOST_EVENTS} do list[i] = {{ at = i / 10 }} end return list end)() }} }}"
    ));
    let Some(Haptic::Play(pattern)) = at_most else {
        panic!("a pattern, got {at_most:?}");
    };
    assert_eq!(pattern.events().len(), MOST_EVENTS);
    let long = asked("{ haptic = { events = { { at = 30, seconds = 30 } } } }");
    let Some(Haptic::Play(pattern)) = long else {
        panic!("a pattern, got {long:?}");
    };
    assert_eq!(pattern.seconds(), 60.0);
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn replies_put_together_keep_the_last_vibration_and_the_end_of_one_is_one() {
    assert_eq!(
        asked(r#"pencere.merge({ haptic = "tick" }, { redraw = true }, { haptic = "snap" })"#),
        Some(play("snap"))
    );
    assert_eq!(
        asked(r#"pencere.merge({ haptic = "tick" }, { haptic = false })"#),
        Some(Haptic::Stop)
    );
    assert_eq!(
        asked(r#"pencere.merge({ haptic = false }, nil, { haptic = "tap" })"#),
        Some(play("tap"))
    );
    assert_eq!(
        asked(r#"pencere.merge(true, { haptic = "bump" })"#),
        Some(play("bump"))
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn every_name_the_kit_has_is_named_by_pencere_has_and_a_name_it_has_not_is_not() {
    let names = haptic::NAMES
        .iter()
        .map(|name| format!("pencere.has(\"haptic.{name}\")"))
        .collect::<Vec<_>>()
        .join(" and ");
    let mut page = open(&format!(
        r#"{{ haptic = (pencere.has("haptic") and {names} and not pencere.has("haptic.nope") and not pencere.has("haptic.")) and "tick" or "nope" }}"#
    ));
    let reply = answer(&mut page);
    assert_eq!(reply.haptic, Some(play("tick")), "{:?}", reply.diagnostics);
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
        .split_once("for name in string.gmatch(\"")
        .expect("the names of the haptics");
    let (said, _) = rest.split_once('"').expect("the end of the names");
    assert_eq!(said.split_whitespace().collect::<Vec<_>>(), haptic::NAMES);
}
