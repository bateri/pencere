//! `print` and `warn` in a page: each line is a call of the reserved method `pencere.log` that the
//! program writes to its log (kit specification, Section 2.4). The tests run the interpreter, so
//! they are `make check-heavy`'s.

use std::time::Duration;

use pencere_view::{Event, Input, Key, Mods};

use crate::ModuleLimits;
use crate::luau_tests::{open_script, until_seen, words};
use crate::test_support::{Open, SIZE};

/// One line, as the page says it to its program: the `call` of `pencere.log` numbered `id`. `text`
/// is as JSON writes it (a tab is `\t`).
fn line(id: u64, level: &str, text: &str) -> String {
    format!(
        r#"{{"call":{{"id":{id},"method":"pencere.log","params":{{"level":"{level}","text":"{text}"}}}}}}"#
    )
}

/// The level and the text of a message that is a line, as JSON wrote it; `None` for any other.
fn read_line(message: &str) -> Option<(&str, &str)> {
    let rest = message.strip_prefix(r#"{"call":{"id":"#)?;
    let (_, rest) = rest.split_once(r#","method":"pencere.log","params":{"level":""#)?;
    let (level, rest) = rest.split_once(r#"","text":""#)?;
    let text = rest.strip_suffix(r#""}}}"#)?;
    Some((level, text))
}

/// The lines of what the page has said so far, as `level` and text.
fn lines(page: &Open) -> Vec<(String, String)> {
    page.seen
        .sent
        .iter()
        .filter_map(|message| read_line(message))
        .map(|(level, text)| (level.to_owned(), text.to_owned()))
        .collect()
}

/// Draws the page until it says no more of what it holds: a call may say only so many messages,
/// and the rest go out in the frames after it.
fn drain(page: &mut Open) {
    for step in 0..60 {
        page.seen.frame = None;
        page.module.render(SIZE, f64::from(step) * 0.016, false);
        let update = page.until(Duration::from_secs(20), |u| {
            u.frame.is_some() || u.closed.is_some()
        });
        assert_eq!(update.closed, None, "{:?}", update.closed);
        if !update.frame.is_some_and(|frame| frame.animating) {
            break;
        }
    }
    // What the page said with its last frame is taken with it, but let the rest of the take settle.
    page.take(Duration::from_millis(50));
}

/// The person presses a key, and the page has said what it has to say of it.
fn press(page: &mut Open) {
    let before = page.seen.replies.len();
    page.module.event(Event::Input(Input::Key {
        key: Key::Enter,
        mods: Mods::default(),
    }));
    until_seen(page, Duration::from_secs(10), |seen| {
        seen.replies.len() > before
    });
    assert!(page.seen.replies.len() > before, "the page did not reply");
    drain(page);
}

/// The program answers the lines numbered `from` to `to` with `{}`, as its library does.
fn answer(page: &Open, from: u64, to: u64) {
    for id in from..=to {
        page.module
            .message(format!(r#"{{"response":{{"id":{id},"result":{{}}}}}}"#));
    }
}

/// Lets what was told the page be done: a call that came after it has been answered.
fn settle(page: &mut Open) {
    page.take(Duration::from_millis(300));
    page.take(Duration::from_millis(50));
}

fn open(script: &str) -> Open {
    open_script(script, ModuleLimits::default())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn print_says_its_arguments_as_text_joined_with_tabs_and_warn_says_it_as_a_warning() {
    let mut page = open(
        r##"
        print("a", 1, nil, true)
        print()
        print(nil)
        print("x", {})
        print(setmetatable({}, { __tostring = function() return "custom" end }), 1.5, "é")
        warn("careful", 2)
        local returned = select("#", print("a")) .. select("#", warn("b"))
        return { render = function() return pencere.text(returned) end }
        "##,
    );
    drain(&mut page);
    assert_eq!(page.seen.closed, None);
    let said = lines(&page);
    // Sent as the six messages are written, in order, one `call` each, numbered from 1.
    assert_eq!(page.seen.sent[0], line(1, "info", r"a\t1\tnil\ttrue"));
    assert_eq!(page.seen.sent[1], line(2, "info", ""));
    assert_eq!(page.seen.sent[2], line(3, "info", "nil"));
    let (level, table) = &said[3];
    assert_eq!(level, "info");
    assert!(
        table.starts_with(r"x\ttable: 0x") && table.len() > r"x\ttable: 0x".len(),
        "{table}"
    );
    assert_eq!(page.seen.sent[4], line(5, "info", r"custom\t1.5\té"));
    assert_eq!(page.seen.sent[5], line(6, "warn", r"careful\t2"));
    assert_eq!(page.seen.sent[6], line(7, "info", "a"));
    assert_eq!(page.seen.sent[7], line(8, "warn", "b"));
    assert_eq!(said.len(), 8);
    // Neither returns anything: `select("#", print(...))` counts no value.
    let frame = page.seen.frame.clone().expect("a page");
    assert_eq!(words(&frame.root), ["00"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_line_of_more_than_8192_bytes_is_cut_before_the_character_it_would_end_in_and_says_how_much() {
    let mut page = open(
        r#"
        -- 10001 bytes: the cut at 8192 is the first byte of a two-byte character.
        print("a" .. string.rep("é", 5000))
        -- Exactly 8192 bytes, and one more.
        print(string.rep("x", 8192))
        print(string.rep("x", 8193))
        -- Four-byte characters: 8192 is a boundary of them, and not after three bytes of one.
        print(string.rep("😀", 3000))
        print(string.rep("x", 8189) .. "😀" .. "tail")
        -- The line is what the arguments make, not each argument.
        print(string.rep("y", 5000), string.rep("z", 5000))
        return { render = function() return pencere.div() end }
        "#,
    );
    drain(&mut page);
    assert_eq!(page.seen.closed, None);
    let said = lines(&page);
    assert_eq!(said.len(), 6);
    assert!(said.iter().all(|(level, _)| level == "info"));
    let texts: Vec<&str> = said.iter().map(|(_, text)| text.as_str()).collect();
    assert_eq!(
        texts[0],
        format!("a{}… (1810 more bytes)", "é".repeat(4095))
    );
    assert_eq!(texts[1], "x".repeat(8192));
    assert_eq!(texts[2], format!("{}… (1 more bytes)", "x".repeat(8192)));
    assert_eq!(
        texts[3],
        format!("{}… (3808 more bytes)", "😀".repeat(2048))
    );
    assert_eq!(texts[4], format!("{}… (8 more bytes)", "x".repeat(8189)));
    // 5000 + a tab (escaped: the text is as JSON wrote it) + 5000 is 10001 bytes, 1809 over.
    assert_eq!(
        texts[5],
        format!(
            r"{}\t{}… (1809 more bytes)",
            "y".repeat(5000),
            "z".repeat(3191)
        )
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_hundred_lines_with_no_answer_send_64_and_a_line_that_says_how_many_were_dropped_follows() {
    let mut page = open(
        r#"
        for n = 1, 100 do print("line " .. n) end
        local presses = 0
        return {
            render = function() return pencere.div() end,
            handle = function(event)
                if event.type == "key" then
                    presses += 1
                    print("press " .. presses)
                    return true
                end
            end,
        }
        "#,
    );
    drain(&mut page);
    assert_eq!(page.seen.closed, None);
    // The program has said nothing: 64 are out, and the page says no more, however often it is
    // drawn and however much it prints.
    assert_eq!(page.seen.sent.len(), 64, "{:?}", page.seen.sent);
    assert_eq!(page.seen.sent[0], line(1, "info", "line 1"));
    assert_eq!(page.seen.sent[63], line(64, "info", "line 64"));
    drain(&mut page);
    drain(&mut page);
    press(&mut page);
    assert_eq!(page.seen.sent.len(), 64);

    // Room again: the 64 are answered, and the next line is said after one that tells how many were
    // let go (the 36 left over, and the press).
    answer(&page, 1, 64);
    settle(&mut page);
    press(&mut page);
    assert_eq!(page.seen.sent.len(), 66, "{:?}", &page.seen.sent[64..]);
    assert_eq!(
        page.seen.sent[64],
        line(65, "warn", "37 lines of print were dropped")
    );
    assert_eq!(page.seen.sent[65], line(66, "info", "press 2"));
    // And once only: the count started again.
    answer(&page, 65, 66);
    settle(&mut page);
    press(&mut page);
    assert_eq!(page.seen.sent.len(), 67);
    assert_eq!(page.seen.sent[66], line(67, "info", "press 3"));
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn the_notice_of_dropped_lines_needs_room_for_itself_and_the_line_or_the_line_is_dropped_too() {
    let mut page = open(
        r#"
        for n = 1, 66 do print("line " .. n) end
        local presses = 0
        return {
            render = function() return pencere.div() end,
            handle = function(event)
                if event.type == "key" then
                    presses += 1
                    print("press " .. presses)
                    return true
                end
            end,
        }
        "#,
    );
    drain(&mut page);
    assert_eq!(page.seen.sent.len(), 64);
    // One answered: there is room for one, and not for the notice and the line.
    answer(&page, 1, 1);
    settle(&mut page);
    press(&mut page);
    assert_eq!(page.seen.sent.len(), 64, "{:?}", &page.seen.sent[64..]);
    // Two: room for both. The count has the 2 of the loop and the press that was dropped too.
    answer(&page, 2, 2);
    settle(&mut page);
    press(&mut page);
    assert_eq!(page.seen.sent.len(), 66, "{:?}", &page.seen.sent[64..]);
    assert_eq!(
        page.seen.sent[64],
        line(65, "warn", "3 lines of print were dropped")
    );
    assert_eq!(page.seen.sent[65], line(66, "info", "press 2"));
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn the_answer_to_a_line_is_the_preludes_and_never_reaches_the_pages_message() {
    let mut page = open(
        r#"
        local got = {}
        print("hello")
        pencere.call("sum", nil, function(result, err)
            got[#got + 1] = "sum " .. tostring(result and result.total)
            return true
        end)
        return {
            render = function() return pencere.text("got:" .. table.concat(got, ",")) end,
            message = function(message)
                got[#got + 1] = "message"
                return true
            end,
        }
        "#,
    );
    drain(&mut page);
    // The line is call 1 and the page's own call is 2.
    assert_eq!(
        page.seen.sent,
        [
            line(1, "info", "hello"),
            r#"{"call":{"id":2,"method":"sum"}}"#.to_owned()
        ]
    );
    // The answer to the line is `{}`; the answer to the call is the page's. In that order: had the
    // line's gone to `message`, the page would say it first.
    page.module
        .message(r#"{"response":{"id":1,"result":{}}}"#.into());
    page.module
        .message(r#"{"response":{"id":2,"result":{"total":3}}}"#.into());
    until_seen(&mut page, Duration::from_secs(5), |seen| {
        seen.frame
            .as_ref()
            .is_some_and(|frame| words(&frame.root) == ["got:sum 3"])
    });
    let said = words(&page.seen.frame.clone().expect("a page").root);
    assert_eq!(said, ["got:sum 3"]);
    // What nobody asked for is still the page's.
    page.module
        .message(r#"{"response":{"id":99,"result":{}}}"#.into());
    until_seen(&mut page, Duration::from_secs(5), |seen| {
        seen.frame
            .as_ref()
            .is_some_and(|frame| words(&frame.root) == ["got:sum 3,message"])
    });
    let said = words(&page.seen.frame.clone().expect("a page").root);
    assert_eq!(said, ["got:sum 3,message"]);
    assert_eq!(page.seen.closed, None);
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_program_that_answers_not_found_is_sent_no_more_lines_for_the_rest_of_the_page() {
    let mut page = open(
        r#"
        for n = 1, 20 do print("line " .. n) end
        local presses = 0
        return {
            render = function() return pencere.div() end,
            handle = function(event)
                if event.type == "key" then
                    presses += 1
                    print("press " .. presses)
                    warn("warning " .. presses)
                    pencere.call("ping", { n = presses })
                    return true
                end
            end,
        }
        "#,
    );
    // One frame says eight of the twenty; the rest is waiting.
    page.module.render(SIZE, 0.0, false);
    page.until(Duration::from_secs(20), |u| u.frame.is_some());
    assert_eq!(page.seen.sent.len(), 8, "{:?}", page.seen.sent);
    // The program has no log: it says so, in the way it says it of any name it has none of.
    page.module.message(
        r#"{"response":{"id":1,"error":{"code":"not-found","message":"no such method"}}}"#.into(),
    );
    settle(&mut page);
    drain(&mut page);
    // What waited to be said was let go, and what is printed now says nothing.
    assert_eq!(page.seen.sent.len(), 8, "{:?}", &page.seen.sent[8..]);
    press(&mut page);
    press(&mut page);
    // The page's own calls go on, numbered as they were: the lines that were let go had theirs.
    assert_eq!(
        page.seen.sent[8..],
        [
            r#"{"call":{"id":21,"method":"ping","params":{"n":1}}}"#,
            r#"{"call":{"id":22,"method":"ping","params":{"n":2}}}"#,
        ]
    );
    assert_eq!(page.seen.closed, None);
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn an_error_that_is_not_not_found_leaves_the_lines_going() {
    let mut page = open(
        r#"
        print("one")
        return {
            render = function() return pencere.div() end,
            handle = function(event)
                if event.type == "key" then print("two") return true end
            end,
        }
        "#,
    );
    drain(&mut page);
    page.module.message(
        r#"{"response":{"id":1,"error":{"code":"internal","message":"it failed"}}}"#.into(),
    );
    settle(&mut page);
    press(&mut page);
    assert_eq!(page.seen.sent[1], line(2, "info", "two"));
    assert_eq!(page.seen.sent.len(), 2);
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_line_goes_in_order_with_the_calls_around_it_and_in_the_share_of_a_call() {
    // Twenty lines and a call between: the order is the order of the page's, eight to a call.
    let mut page = open(
        r#"
        pencere.call("first")
        for n = 1, 10 do print("line " .. n) end
        pencere.call("last")
        return { render = function() return pencere.div() end }
        "#,
    );
    page.module.render(SIZE, 0.0, false);
    let update = page.until(Duration::from_secs(20), |u| u.frame.is_some());
    // The share of one call, eight messages, and the page asks for the frame after it.
    assert_eq!(page.seen.sent.len(), 8, "{:?}", page.seen.sent);
    assert!(update.frame.is_some_and(|frame| frame.animating));
    drain(&mut page);
    let order: Vec<String> = page
        .seen
        .sent
        .iter()
        .map(|message| match read_line(message) {
            Some((_, text)) => text.to_owned(),
            None => message.clone(),
        })
        .collect();
    let mut expected = vec![r#"{"call":{"id":1,"method":"first"}}"#.to_owned()];
    expected.extend((1..=10).map(|n| format!("line {n}")));
    expected.push(r#"{"call":{"id":12,"method":"last"}}"#.to_owned());
    assert_eq!(order, expected);
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn print_and_warn_are_the_globals_of_a_script_and_a_script_that_assigns_its_own_keeps_its_own() {
    let mut page = open(
        r#"
        local ok_global, why = pcall(function() _G.print = function() end end)
        local mine = {}
        local theirs = print
        print = function(...) mine[#mine + 1] = table.concat({ ... }, " ") end
        print("not said")
        warn("said")
        theirs("also said")
        return { render = function()
            return pencere.text(table.concat(mine, ",") .. "|" .. tostring(ok_global))
        end }
        "#,
    );
    drain(&mut page);
    assert_eq!(page.seen.closed, None);
    // What the script's own `print` was given never left the page; the page's other two lines did.
    assert_eq!(
        page.seen.sent,
        [line(1, "warn", "said"), line(2, "info", "also said")]
    );
    let frame = page.seen.frame.clone().expect("a page");
    assert_eq!(words(&frame.root), ["not said|false"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn the_lines_a_page_printed_in_the_call_that_fails_are_said_before_it_is_closed() {
    let mut page = open(
        r##"
        return { render = function()
            print("one", 1)
            warn("two")
            local nothing = nil
            return nothing.field
        end }
        "##,
    );
    page.module.render(SIZE, 0.0, false);
    let update = page.until(Duration::from_secs(20), |u| u.closed.is_some());
    assert!(update.closed.is_some(), "the page fails");
    assert_eq!(
        lines(&page),
        [
            ("info".to_owned(), r"one\t1".to_owned()),
            ("warn".to_owned(), "two".to_owned()),
        ]
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn what_else_a_failed_call_would_have_asked_of_the_program_is_not_asked() {
    let mut page = open(
        r##"
        return {
            render = function() return pencere.text("x") end,
            handle = function(event)
                print("pressed")
                pencere.call("save", {})
                error("nope")
            end,
        }
        "##,
    );
    drain(&mut page);
    page.module.event(Event::Input(Input::Key {
        key: Key::Enter,
        mods: Mods::default(),
    }));
    let update = page.until(Duration::from_secs(20), |u| u.closed.is_some());
    assert!(update.closed.is_some(), "the page fails");
    assert_eq!(
        lines(&page),
        [("info".to_owned(), "pressed".to_owned())],
        "{:?}",
        page.seen.sent
    );
    assert!(
        page.seen
            .sent
            .iter()
            .all(|message| !message.contains("save")),
        "{:?}",
        page.seen.sent
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn of_the_lines_of_a_call_that_fails_the_last_ones_are_said() {
    let mut page = open(
        r##"
        return { render = function()
            for n = 1, 40 do
                print("line " .. n)
            end
            error("late")
        end }
        "##,
    );
    page.module.render(SIZE, 0.0, false);
    let update = page.until(Duration::from_secs(20), |u| u.closed.is_some());
    assert!(update.closed.is_some(), "the page fails");
    let said = lines(&page);
    assert_eq!(said.len(), 16, "{said:?}");
    assert_eq!(said.first().map(|(_, text)| text.as_str()), Some("line 25"));
    assert_eq!(said.last().map(|(_, text)| text.as_str()), Some("line 40"));
}
