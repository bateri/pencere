//! Pages with `@pencere/query` of the kit's library, driven by the host with no screen: each test
//! has a page ask its program for data and shows what the page reads of it as a line of text, while
//! the test stands for the program (it answers a call, fails it, pushes a topic) and moves the clock.
//! The time of a render is how the module tells one render from the next, so a test that does
//! something between two renders lets the clock go on first. Heavy: the interpreter is compiled and
//! run (`make check-heavy`).

use std::cell::Cell;
use std::rc::Rc;

use pencere_sandbox::Fault;
use pencere_wire::messages::Message;
use serde_json::json;

use crate::headless::{Calls, Error, Failure, Headless, Value};

/// What every test page begins with: the module, a context the test may make again (`q`), the
/// element it draws (`view`), the keys that do something to the page (`keys`) and what it keeps
/// (`S`), and `show`, which says what a read is in a few words.
const HEAD: &str = r#"
local query = require("@pencere/query")
local q, view, keys, S = query.new(), nil, {}, {}
local function show(r)
    local line = r.status .. " " .. (type(r.data) == "table" and tostring(r.data.v) or "-")
    line ..= r.fetching and " fetching" or " idle"
    if r.error ~= nil then
        line ..= " err=" .. r.error.code
    end
    return line
end
"#;

/// What every test page ends with: the page, which draws `view` and does a key's work.
const TAIL: &str = r#"
return {
    render = function()
        return pencere.div():size_full():child(view())
    end,
    handle = function(event)
        if event.type == "key" and keys[event.key] ~= nil then
            keys[event.key]()
            return true
        end
    end,
}
"#;

fn page_of(body: &str) -> String {
    format!("{HEAD}{body}{TAIL}")
}

/// A page whose program answers nothing until the test does.
fn open(body: &str) -> Result<Headless, Error> {
    Headless::script(page_of(body)).at(100.0).open()
}

/// A page whose program answers every `list` at once, with the number of the call as `v`; and how
/// many calls it has heard.
fn open_answering(body: &str) -> Result<(Headless, Rc<Cell<u64>>), Error> {
    let heard = Rc::new(Cell::new(0));
    let counted = Rc::clone(&heard);
    let calls = Calls::new().on("list", move |_| {
        counted.set(counted.get() + 1);
        Ok(json!({ "v": counted.get() }))
    });
    let page = Headless::script(page_of(body))
        .at(100.0)
        .peer(calls)
        .open()?;
    Ok((page, heard))
}

/// Why a page that is to fail failed, and the source it was made of.
fn failing(body: &str) -> (String, String) {
    let source = page_of(body);
    match Headless::script(source.clone()).at(100.0).open().map(drop) {
        Err(Error::Ended(Fault::Failed(why))) => (why, source),
        other => panic!("expected the page to fail, it is {other:?}"),
    }
}

/// The line of `source` that has `needle` (from 1), as a page's error names it.
fn line_of(source: &str, needle: &str) -> usize {
    source
        .lines()
        .position(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("no line has {needle}"))
        + 1
}

/// The clock goes on `seconds` and the page is made for the time it shows, as a host's frame does:
/// the page is made again by an event or by what its program says at the time of the frame before
/// (the page itself asks the host for a frame only when it wants one), so a test that has the page
/// made at a time of its own asks for the frame.
fn after(page: &mut Headless, seconds: f64) -> Result<(), Error> {
    page.advance(seconds)?;
    page.refresh()
}

/// A little time on, and the page made for it.
fn later(page: &mut Headless) -> Result<(), Error> {
    after(page, 0.5)
}

/// The program answers the call with the number `id`.
fn answer_call(page: &mut Headless, id: u64, result: Value) -> Result<(), Error> {
    page.peer_mut().say(&Message::answer(id, result).text());
    page.settle()
}

/// The number the page gave the `at`th call (from 0) of the program.
fn number(page: &Headless, at: usize) -> u64 {
    page.peer().calls()[at].id
}

/// The params the page gave each call, in order.
fn asked(page: &Headless) -> Vec<Value> {
    page.peer()
        .calls()
        .iter()
        .map(|call| call.params.clone())
        .collect()
}

fn failure(code: &str) -> Failure {
    Failure {
        code: code.to_owned(),
        message: format!("{code} for a test"),
    }
}

// --- A read -------------------------------------------------------------------------------------

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_read_is_loading_until_the_program_answers_and_then_ready_with_the_data() -> Result<(), Error> {
    let mut page = open(
        r#"
keys.i = function() q:invalidate("list") end
view = function()
    return pencere.text("list " .. show(q:get("list", { n = 1 })))
end
"#,
    )?;
    assert!(page.says("list loading - fetching"), "{:?}", page.words());
    assert_eq!(page.methods(), ["list"]);
    assert!(page.answer("list", Ok(json!({ "v": 5 })))?);
    assert!(page.says("list ready 5 idle"), "{:?}", page.words());
    // A call that fails where there is data leaves the data and says why.
    later(&mut page)?;
    page.press("i")?;
    assert!(page.says("list ready 5 fetching"), "{:?}", page.words());
    assert!(page.answer("list", Err(failure("busy")))?);
    assert!(
        page.says("list ready 5 idle err=busy"),
        "{:?}",
        page.words()
    );
    // The next answer that is a result takes the error away.
    later(&mut page)?;
    page.press("i")?;
    assert!(page.answer("list", Ok(json!({ "v": 6 })))?);
    assert!(page.says("list ready 6 idle"), "{:?}", page.words());
    assert!(!page.says("err="), "{:?}", page.words());
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_read_that_fails_with_no_data_is_failed_until_it_is_asked_again_and_a_null_is_a_result()
-> Result<(), Error> {
    let mut page = open(
        r#"
keys.i = function() q:invalidate("list") end
view = function()
    return pencere.text("list " .. show(q:get("list")))
end
"#,
    )?;
    assert!(page.answer("list", Err(failure("busy")))?);
    assert!(
        page.says("list failed - idle err=busy"),
        "{:?}",
        page.words()
    );
    // A failed key is not asked again by the render that shows it: the page says when.
    later(&mut page)?;
    later(&mut page)?;
    assert_eq!(page.methods(), ["list"]);
    page.press("i")?;
    assert_eq!(page.methods(), ["list", "list"]);
    // It is loading again, with the last error where the page still wants it.
    assert!(
        page.says("list loading - fetching err=busy"),
        "{:?}",
        page.words()
    );
    // A JSON null is a result: there is data, and it is nothing.
    assert!(page.answer("list", Ok(Value::Null))?);
    assert!(page.says("list ready - idle"), "{:?}", page.words());
    assert!(!page.says("err="), "{:?}", page.words());
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_key_asked_twice_is_one_call_and_no_params_are_an_empty_table() -> Result<(), Error> {
    let page = open(
        r#"
view = function()
    local a = q:get("list", { n = 1, m = 2 })
    local b = q:get("list", { m = 2, n = 1 })
    local c = q:get("other")
    local d = q:get("other", {})
    return pencere.div():child(pencere.text("a " .. a.status .. " " .. b.status))
        :child(pencere.text("c " .. c.status .. " " .. d.status))
end
"#,
    )?;
    assert_eq!(page.methods(), ["list", "other"]);
    assert!(page.says("a loading loading"), "{:?}", page.words());
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn an_invalidate_while_the_call_is_out_is_written_and_the_key_stays_stale() -> Result<(), Error> {
    let mut page = open(
        r#"
keys.i = function() q:invalidate("list") end
view = function()
    return pencere.text("list " .. show(q:get("list")))
end
"#,
    )?;
    later(&mut page)?;
    page.press("i")?;
    // The call that is out is not asked over.
    assert_eq!(page.methods(), ["list"]);
    assert!(page.answer("list", Ok(json!({ "v": 1 })))?);
    // Its answer is the data, and the render it asked for asks again.
    assert_eq!(page.methods(), ["list", "list"]);
    assert!(page.says("list ready 1 fetching"), "{:?}", page.words());
    assert!(page.answer("list", Ok(json!({ "v": 2 })))?);
    assert!(page.says("list ready 2 idle"), "{:?}", page.words());
    assert_eq!(page.methods(), ["list", "list"]);
    Ok(())
}

// --- What is given up ---------------------------------------------------------------------------

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_key_let_go_of_gives_up_its_call_and_the_late_answer_changes_nothing() -> Result<(), Error> {
    let mut page = open(
        r#"
keys.f = function() q:forget("list") end
view = function()
    return pencere.text("list " .. show(q:get("list", { n = 1 })))
end
"#,
    )?;
    let first = number(&page, 0);
    later(&mut page)?;
    page.press("f")?;
    // The program is told it is given up, and the next render asks from the start.
    assert!(page.peer().calls()[0].cancelled);
    assert_eq!(page.methods(), ["list", "list"]);
    answer_call(&mut page, first, json!({ "v": 1 }))?;
    assert!(page.says("list loading - fetching"), "{:?}", page.words());
    assert!(page.answer("list", Ok(json!({ "v": 2 })))?);
    assert!(page.says("list ready 2 idle"), "{:?}", page.words());
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn forgetting_a_method_lets_go_of_every_key_of_it_and_a_key_only_the_one() -> Result<(), Error> {
    let mut page = open(
        r#"
keys.a = function() q:forget("list", { n = 1 }) end
keys.m = function() q:forget("list") end
view = function()
    return pencere.div()
        :child(pencere.text("one " .. show(q:get("list", { n = 1 }))))
        :child(pencere.text("two " .. show(q:get("list", { n = 2 }))))
        :child(pencere.text("other " .. show(q:get("other"))))
end
"#,
    )?;
    assert_eq!(page.methods(), ["list", "list", "other"]);
    later(&mut page)?;
    page.press("a")?;
    // Only the key said: its call is given up and asked again, the others are as they were.
    assert!(page.peer().calls()[0].cancelled);
    assert!(!page.peer().calls()[1].cancelled);
    assert_eq!(page.methods(), ["list", "list", "other", "list"]);
    later(&mut page)?;
    page.press("m")?;
    assert!(page.peer().calls()[1].cancelled);
    assert!(page.peer().calls()[3].cancelled);
    assert!(!page.peer().calls()[2].cancelled);
    assert_eq!(page.methods().len(), 6);
    Ok(())
}

// --- What the program pushes --------------------------------------------------------------------

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_push_and_an_answer_are_the_data_in_the_order_they_came() -> Result<(), Error> {
    let mut page = open(
        r#"
pencere.subscribe("pushed", nil, function(value)
    q:set("list", nil, value)
    return true
end)
view = function()
    return pencere.text("list " .. show(q:get("list")))
end
"#,
    )?;
    assert_eq!(page.push("pushed", &json!({ "v": 7 }))?, 1);
    // The data of a push is shown while the call that is out is awaited.
    assert!(page.says("list ready 7 fetching"), "{:?}", page.words());
    assert!(page.answer("list", Ok(json!({ "v": 3 })))?);
    assert!(page.says("list ready 3 idle"), "{:?}", page.words());
    page.push("pushed", &json!({ "v": 9 }))?;
    assert!(page.says("list ready 9 idle"), "{:?}", page.words());
    assert_eq!(page.methods(), ["list"]);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_version_lets_go_of_an_answer_and_a_push_that_are_older_than_the_data() -> Result<(), Error> {
    let mut page = open(
        r#"
pencere.subscribe("pushed", nil, function(value)
    q:set("list", nil, value)
    return true
end)
keys.i = function() q:invalidate("list") end
view = function()
    local r = q:get("list", nil, { version = function(data) return data.rev end })
    return pencere.text("list " .. show(r))
end
"#,
    )?;
    assert!(page.answer("list", Ok(json!({ "v": 1, "rev": 5 })))?);
    assert!(page.says("list ready 1 idle"), "{:?}", page.words());
    // Older: let go of.
    page.push("pushed", &json!({ "v": 2, "rev": 3 }))?;
    assert!(page.says("list ready 1 idle"), "{:?}", page.words());
    // As old as the data, or newer: the data.
    page.push("pushed", &json!({ "v": 3, "rev": 5 }))?;
    assert!(page.says("list ready 3 idle"), "{:?}", page.words());
    later(&mut page)?;
    page.press("i")?;
    page.push("pushed", &json!({ "v": 4, "rev": 8 }))?;
    assert!(page.says("list ready 4 fetching"), "{:?}", page.words());
    // The answer that was out when the push came is older than the push.
    assert!(page.answer("list", Ok(json!({ "v": 5, "rev": 6 })))?);
    assert!(page.says("list ready 4 idle"), "{:?}", page.words());
    assert_eq!(page.methods(), ["list", "list"]);
    Ok(())
}

// --- A stamp ------------------------------------------------------------------------------------

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_new_stamp_asks_again_with_the_old_data_in_view_and_one_call_is_out_at_a_time()
-> Result<(), Error> {
    let mut page = open(
        r#"
S.at = 1
keys.s = function() S.at += 1 end
view = function()
    return pencere.text("list " .. show(q:get("list", nil, { stamp = S.at })))
end
"#,
    )?;
    assert!(page.answer("list", Ok(json!({ "v": 1 })))?);
    later(&mut page)?;
    assert_eq!(page.methods(), ["list"]);
    page.press("s")?;
    assert_eq!(page.methods(), ["list", "list"]);
    assert!(page.says("list ready 1 fetching"), "{:?}", page.words());
    // The stamp moves on while the call is out: no second call, and the answer is asked over.
    later(&mut page)?;
    page.press("s")?;
    assert_eq!(page.methods(), ["list", "list"]);
    assert!(page.answer("list", Ok(json!({ "v": 2 })))?);
    assert_eq!(page.methods(), ["list", "list", "list"]);
    assert!(page.says("list ready 2 fetching"), "{:?}", page.words());
    assert!(page.answer("list", Ok(json!({ "v": 3 })))?);
    assert!(page.says("list ready 3 idle"), "{:?}", page.words());
    later(&mut page)?;
    assert_eq!(page.methods().len(), 3);
    Ok(())
}

// --- Asking again as long as it is asked for ----------------------------------------------------

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_key_asked_for_every_so_often_is_asked_again_that_long_after_each_answer() -> Result<(), Error>
{
    let (mut page, heard) = open_answering(
        r#"
view = function()
    return pencere.text("list " .. show(q:get("list", nil, { every = 10 })))
end
"#,
    )?;
    assert_eq!(heard.get(), 1);
    assert!(page.says("list ready 1 idle"), "{:?}", page.words());
    page.advance(5.0)?;
    assert_eq!(heard.get(), 1);
    for expected in 2..=4 {
        page.advance(10.5)?;
        assert_eq!(heard.get(), expected, "at {}", page.now());
        assert!(
            page.says(&format!("list ready {expected} idle")),
            "{:?}",
            page.words()
        );
    }
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_polling_stops_when_the_page_stops_asking_and_a_get_sets_it_going_again() -> Result<(), Error>
{
    let (mut page, heard) = open_answering(
        r#"
keys.h = function() S.hidden = not S.hidden end
view = function()
    if S.hidden then
        return pencere.text("nothing asked")
    end
    return pencere.text("list " .. show(q:get("list", nil, { every = 10 })))
end
"#,
    )?;
    page.advance(1.0)?;
    page.press("h")?;
    assert!(page.says("nothing asked"), "{:?}", page.words());
    // The render that asked last came after the answer: one more is asked, and no render asks for
    // what it brings.
    page.advance(60.0)?;
    assert_eq!(heard.get(), 2);
    // The first render that asks sets it going again: the time is long past, so at once.
    page.press("h")?;
    assert_eq!(heard.get(), 3);
    assert!(page.says("list ready 3 idle"), "{:?}", page.words());
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_key_that_comes_to_be_asked_for_every_so_often_is_asked_from_the_last_answer()
-> Result<(), Error> {
    let (mut page, heard) = open_answering(
        r#"
keys.e = function() S.every = 10 end
view = function()
    return pencere.text("list " .. show(q:get("list", nil, { every = S.every })))
end
"#,
    )?;
    after(&mut page, 3.0)?;
    assert_eq!(heard.get(), 1);
    page.press("e")?;
    // The answer came 3 seconds ago: the time that is left is 7.
    page.advance(5.0)?;
    assert_eq!(heard.get(), 1, "at {}", page.now());
    page.advance(3.0)?;
    assert_eq!(heard.get(), 2, "at {}", page.now());
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_call_that_takes_longer_than_the_time_between_two_does_not_stop_the_polling()
-> Result<(), Error> {
    let mut page = open(
        r#"
view = function()
    return pencere.text("list " .. show(q:get("list", nil, { every = 10 })))
end
"#,
    )?;
    // The program takes 25 seconds, and the page is drawn while it does.
    page.advance(25.0)?;
    assert_eq!(page.methods(), ["list"]);
    assert!(page.answer("list", Ok(json!({ "v": 1 })))?);
    assert!(page.says("list ready 1 idle"), "{:?}", page.words());
    page.advance(5.0)?;
    assert_eq!(page.methods(), ["list"]);
    page.advance(6.0)?;
    assert_eq!(page.methods(), ["list", "list"]);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_page_that_is_not_seen_is_made_for_what_its_program_says_and_goes_on_polling_no_faster()
-> Result<(), Error> {
    let (mut page, heard) = open_answering(
        r#"
view = function()
    return pencere.text("list " .. show(q:get("list", nil, { every = 10 })))
end
"#,
    )?;
    // The module makes the page again for every answer that asks a frame, seen or not, so the key is
    // asked for as long as the page asks; the host holds a page's wakes to one a second while it is
    // not seen, and that is all that slows the polling down.
    page.hide(35.0)?;
    assert!((3..=4).contains(&heard.get()), "{}", heard.get());
    // Seen again, the page shows what the last answer said.
    after(&mut page, 0.5)?;
    assert!(
        page.says(&format!("list ready {} idle", heard.get())),
        "{:?}",
        page.words()
    );
    Ok(())
}

// --- A slot -------------------------------------------------------------------------------------

/// A page whose one slot holds the key of `S.n`: `a` and `b` move it to 1 and to 2, `i` makes the
/// key stale, `o` and `p` have the page ask the first key with no slot as well, `o` after the key
/// that has the slot and `p` before it.
const SLOTTED: &str = r#"
S.n = 1
keys.a = function() S.n = 1 end
keys.b = function() S.n = 2 end
keys.i = function() q:invalidate("list") end
keys.o = function() S.plain = "after" end
keys.p = function() S.plain = "before" end
view = function()
    if S.plain == "before" then
        q:get("list", { n = 1 })
    end
    local r = q:get("list", { n = S.n }, { slot = "s" })
    if S.plain == "after" then
        q:get("list", { n = 1 })
    end
    return pencere.text("n" .. S.n .. " " .. show(r))
end
"#;

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_new_key_in_a_slot_gives_up_the_call_of_the_one_before_it() -> Result<(), Error> {
    let mut page = open(SLOTTED)?;
    let first = number(&page, 0);
    later(&mut page)?;
    page.press("b")?;
    assert_eq!(page.methods(), ["list", "list"]);
    // The call is given up at the wake after the render that left the key.
    page.advance(0.5)?;
    assert!(page.peer().calls()[0].cancelled);
    assert!(!page.peer().calls()[1].cancelled);
    assert_eq!(asked(&page), [json!({ "n": 1 }), json!({ "n": 2 })]);
    // What it says late is not heard.
    answer_call(&mut page, first, json!({ "v": 1 }))?;
    assert!(page.says("n2 loading - fetching"), "{:?}", page.words());
    assert!(page.answer("list", Ok(json!({ "v": 2 })))?);
    assert!(page.says("n2 ready 2 idle"), "{:?}", page.words());
    // It had no data: the key is asked from the start.
    later(&mut page)?;
    page.press("a")?;
    assert_eq!(page.methods().len(), 3);
    assert!(page.says("n1 loading - fetching"), "{:?}", page.words());
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_data_of_the_key_that_left_a_slot_stays_and_it_is_asked_again_when_it_comes_back()
-> Result<(), Error> {
    let mut page = open(SLOTTED)?;
    assert!(page.answer("list", Ok(json!({ "v": 1 })))?);
    later(&mut page)?;
    page.press("i")?;
    assert_eq!(page.methods().len(), 2);
    later(&mut page)?;
    page.press("b")?;
    page.advance(0.5)?;
    assert!(page.peer().calls()[1].cancelled);
    assert!(page.says("n2 loading - fetching"), "{:?}", page.words());
    later(&mut page)?;
    page.press("a")?;
    // The data it had is shown at once, and the ask that was lost is made again.
    assert!(page.says("n1 ready 1 fetching"), "{:?}", page.words());
    assert_eq!(page.methods().len(), 4);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_key_that_is_asked_with_no_slot_in_the_render_that_leaves_it_is_not_given_up()
-> Result<(), Error> {
    for plain in ["o", "p"] {
        let mut page = open(SLOTTED)?;
        later(&mut page)?;
        page.press(plain)?;
        later(&mut page)?;
        page.press("b")?;
        page.advance(0.5)?;
        assert_eq!(page.methods(), ["list", "list"], "{plain}");
        assert!(!page.peer().calls()[0].cancelled, "{plain}");
        assert!(!page.peer().calls()[1].cancelled, "{plain}");
    }
    Ok(())
}

// --- What is let go of --------------------------------------------------------------------------

/// A page that asks the key of `S.n`: `a` and `b` move it to 1 and to 2.
const KEPT: &str = r#"
q = query.new({ keep = 30 })
S.n = 1
keys.a = function() S.n = 1 end
keys.b = function() S.n = 2 end
view = function()
    return pencere.text("n" .. S.n .. " " .. show(q:get("list", { n = S.n })))
end
"#;

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn data_that_no_render_asked_for_in_keep_seconds_before_the_last_one_is_let_go() -> Result<(), Error>
{
    let mut page = open(KEPT)?;
    assert!(page.answer("list", Ok(json!({ "v": 1 })))?);
    after(&mut page, 1.0)?;
    page.press("b")?;
    assert!(page.answer("list", Ok(json!({ "v": 2 })))?);
    // The render at 141 asks for the second key: the first was asked for in the render at 101, which
    // is where keep counts from, and 40 seconds before the one at 141 is not 30 before the one at 101.
    after(&mut page, 40.0)?;
    // At 142 the render before it, at 141, is 40 seconds after the first key was asked for.
    after(&mut page, 1.0)?;
    page.press("a")?;
    assert!(page.says("n1 loading - fetching"), "{:?}", page.words());
    assert_eq!(page.methods().len(), 3);
    // What was asked for just now is there.
    later(&mut page)?;
    page.press("b")?;
    assert!(page.says("n2 ready 2 idle"), "{:?}", page.words());
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn data_asked_for_inside_keep_stays_and_a_key_let_go_of_gives_up_its_call() -> Result<(), Error> {
    let mut page = open(KEPT)?;
    assert!(page.answer("list", Ok(json!({ "v": 1 })))?);
    after(&mut page, 1.0)?;
    page.press("b")?;
    later(&mut page)?;
    page.press("a")?;
    assert!(page.says("n1 ready 1 idle"), "{:?}", page.words());
    // The second key is not asked for from here on, and its call is out: it is given up with it.
    after(&mut page, 40.0)?;
    assert!(!page.peer().calls()[1].cancelled);
    after(&mut page, 1.0)?;
    assert!(page.peer().calls()[1].cancelled);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_time_a_view_is_not_drawn_is_not_counted_against_what_it_keeps() -> Result<(), Error> {
    let mut page = open(KEPT)?;
    assert!(page.answer("list", Ok(json!({ "v": 1 })))?);
    page.hide(500.0)?;
    page.refresh()?;
    assert!(page.says("n1 ready 1 idle"), "{:?}", page.words());
    assert_eq!(page.methods().len(), 1);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_handler_that_sets_peeks_and_invalidates_while_the_view_is_hidden_lets_go_of_nothing()
-> Result<(), Error> {
    let mut page = open(
        r#"
q = query.new({ keep = 30 })
pencere.subscribe("pushed", nil, function(value)
    q:set("list", nil, value)
    S.peeked = q:peek("other") ~= nil
    q:invalidate("other")
end)
view = function()
    return pencere.div()
        :child(pencere.text("list " .. show(q:get("list"))))
        :child(pencere.text("other " .. show(q:get("other"))))
        :child(pencere.text("peeked " .. tostring(S.peeked)))
end
"#,
    )?;
    assert!(page.answer("list", Ok(json!({ "v": 1 })))?);
    assert!(page.answer("other", Ok(json!({ "v": 2 })))?);
    page.hide(500.0)?;
    page.push("pushed", &json!({ "v": 9 }))?;
    page.hide(500.0)?;
    // The first render after it has what the handler gave, and what it made stale is asked.
    page.refresh()?;
    assert!(page.says("list ready 9 idle"), "{:?}", page.words());
    assert!(page.says("other ready 2 fetching"), "{:?}", page.words());
    assert!(page.says("peeked true"), "{:?}", page.words());
    assert_eq!(page.methods(), ["list", "other", "other"]);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_handler_that_asks_for_a_frame_while_the_view_is_hidden_keeps_what_the_page_shows()
-> Result<(), Error> {
    let mut page = open(
        r#"
q = query.new({ keep = 30 })
pencere.subscribe("pushed", nil, function() return true end)
keys.t = function() S.other = not S.other end
view = function()
    return pencere.text(S.other and ("other " .. show(q:get("other"))) or ("list " .. show(q:get("list"))))
end
"#,
    )?;
    assert!(page.answer("list", Ok(json!({ "v": 1 })))?);
    page.press("t")?;
    assert!(page.answer("other", Ok(json!({ "v": 2 })))?);
    // The page is made for what the program says though it is not seen, and the time goes on in
    // those renders: what they ask for stays and the rest is let go of after `keep`.
    page.hide(500.0)?;
    page.push("pushed", &json!({}))?;
    page.hide(500.0)?;
    page.push("pushed", &json!({}))?;
    page.refresh()?;
    assert!(page.says("other ready 2 idle"), "{:?}", page.words());
    assert_eq!(page.methods(), ["list", "other"]);
    page.press("t")?;
    assert!(page.says("list loading - fetching"), "{:?}", page.words());
    assert_eq!(page.methods(), ["list", "other", "list"]);
    Ok(())
}

// --- The same data ------------------------------------------------------------------------------

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn an_answer_that_is_alike_keeps_the_table_and_still_draws_a_frame() -> Result<(), Error> {
    let mut page = open(
        r#"
keys.i = function() q:invalidate("list") end
view = function()
    local r = q:get("list")
    if S.prev ~= nil and r.data ~= nil and not rawequal(S.prev, r.data) then
        S.replaced = (S.replaced or 0) + 1
    end
    S.prev = r.data
    return pencere.text("list " .. show(r) .. " replaced=" .. tostring(S.replaced or 0))
end
"#,
    )?;
    let same = json!({ "v": 1, "items": [1, 2, { "x": [3] }] });
    assert!(page.answer("list", Ok(same.clone()))?);
    assert!(
        page.says("list ready 1 idle replaced=0"),
        "{:?}",
        page.words()
    );
    later(&mut page)?;
    page.press("i")?;
    assert!(
        page.says("list ready 1 fetching replaced=0"),
        "{:?}",
        page.words()
    );
    // Nothing changed in the data, and `fetching` did: the frame is drawn.
    assert!(page.answer("list", Ok(same))?);
    assert!(
        page.says("list ready 1 idle replaced=0"),
        "{:?}",
        page.words()
    );
    later(&mut page)?;
    page.press("i")?;
    assert!(page.answer("list", Ok(json!({ "v": 1, "items": [1, 2, { "x": [4] }] })))?);
    assert!(
        page.says("list ready 1 idle replaced=1"),
        "{:?}",
        page.words()
    );
    Ok(())
}

// --- A method's keys ----------------------------------------------------------------------------

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_method_made_stale_is_asked_for_every_key_over_the_renders_that_follow() -> Result<(), Error> {
    let mut page = open(
        r#"
keys.x = function() q:invalidate("item") end
view = function()
    local ready = 0
    local root = pencere.div()
    for i = 1, 12 do
        if q:get("item", { i = i }).status == "ready" then
            ready += 1
        end
    end
    return root:child(pencere.text("ready " .. ready))
end
"#,
    )?;
    // More calls than a page may say in one go: the rest follow on the frames after.
    page.advance(0.5)?;
    assert_eq!(page.methods().len(), 12);
    while page.answer("item", Ok(json!({ "v": 1 })))? {}
    assert!(page.says("ready 12"), "{:?}", page.words());
    later(&mut page)?;
    page.press("x")?;
    page.advance(0.5)?;
    assert_eq!(page.methods().len(), 24);
    let mut again: Vec<i64> = asked(&page)[12..]
        .iter()
        .map(|params| params["i"].as_i64().unwrap_or(0))
        .collect();
    again.sort_unstable();
    assert_eq!(again, (1..=12).collect::<Vec<i64>>());
    Ok(())
}

// --- What a page gives wrong --------------------------------------------------------------------

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn what_a_page_gives_wrong_is_an_error_at_its_own_line() {
    let cases = [
        ("q:get(5)", "takes the method to ask for, a string"),
        ("q:get(\"list\", 5)", "takes params that are nil or a table"),
        ("q:get(\"list\", nil, 5)", "takes a table of options"),
        (
            "q:get(\"list\", nil, { bogus = 1 })",
            "no option called bogus",
        ),
        (
            "q:get(\"list\", nil, { every = 0 })",
            "every as a number of seconds",
        ),
        ("q:get(\"list\", nil, { slot = 1 })", "slot as a name"),
        (
            "q:get(\"list\", nil, { version = 1 })",
            "version as a function",
        ),
        ("q:peek(nil)", "takes the method to ask for"),
        (
            "q:set(\"list\", 5, {})",
            "takes params that are nil or a table",
        ),
        ("q:invalidate(7)", "takes the method to ask for"),
        ("q:forget({})", "takes the method to ask for"),
        ("q.get(\"list\")", "is called on a context"),
        ("query.new(5)", "takes a table of options"),
        ("query.new({ keep = -1 })", "keep as a number of seconds"),
        ("query.new({ every = 1 })", "no option called every"),
    ];
    for (call, why_in_words) in cases {
        let (why, source) = failing(&format!(
            "view = function() {call} return pencere.text('x') end\n"
        ));
        assert!(why.contains(why_in_words), "{call}: {why}");
        let line = line_of(&source, call);
        assert!(
            why.contains(&format!(":{line}:")),
            "{call}: the line of the page: {why}"
        );
    }
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_version_that_is_no_number_fails_the_page() -> Result<(), Error> {
    let mut page = open(
        r#"
view = function()
    local r = q:get("list", nil, { version = function(data) return "new" end })
    return pencere.text("list " .. show(r))
end
"#,
    )?;
    match page.answer("list", Ok(json!({ "v": 1 }))) {
        Err(Error::Ended(Fault::Failed(why))) => {
            assert!(
                why.contains("the version of data is a number or nil"),
                "{why}"
            );
        }
        other => panic!("expected the page to fail, it is {other:?}"),
    }
    Ok(())
}
