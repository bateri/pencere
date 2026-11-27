//! Pages under the host with no screen, as a test of an app writes them: the example app of
//! containers (`examples/containers-lua`) with its program played by a table of calls, and short
//! scripts for what a person does and sees. Heavy: the interpreter is compiled and run
//! (`make check-heavy`).

use std::collections::HashMap;
use std::path::PathBuf;

use pencere_editor::UNDO_PAUSE;
use pencere_sandbox::Fault;
use pencere_view::edit::EditCommand;
use pencere_view::{
    Button, Carry, Cursor, Event, Font, Id, Input, Key, Measure, MenuEntry, Metrics, Mods, Paint,
    Point, Rect, Reply, Size, div,
};
use serde_json::json;

use crate::headless::{Calls, Error, Failure, Files, Focus, Headless, Logged};

fn example(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name)
}

/// The program of the containers example, made up: two containers, one running, and `toggle`
/// stopping or starting one.
fn containers() -> Calls {
    let mut states: HashMap<String, &'static str> =
        HashMap::from([("c1".into(), "running"), ("c2".into(), "exited")]);
    Calls::new()
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
                    message: format!("there is no container {id}"),
                });
            };
            *state = if *state == "running" {
                "exited"
            } else {
                "running"
            };
            Ok(json!({ "id": id, "state": *state }))
        })
}

/// The pictures of the last frame that have pixels to draw.
fn pictures(page: &Headless) -> usize {
    let images = page.page().images();
    page.list()
        .items
        .iter()
        .filter(|item| match item {
            Paint::Image(picture) => images
                .and_then(|images| images.bitmap(&picture.src, picture.page, picture.full()))
                .is_some(),
            _ => false,
        })
        .count()
}

/// The example app's page, its program answering: the list asked for once, the readings subscribed
/// to and pushed, its pictures decoded from the app's files, a click on a button calling the
/// program and the answer shown.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_container_app_lists_what_its_program_says_shows_its_readings_and_toggles_a_container()
-> Result<(), Error> {
    let mut page = Headless::app(example("containers-lua"))
        .size(900.0, 500.0)
        .peer(containers())
        .open()?;
    assert_eq!(page.methods(), ["list"]);
    assert_eq!(page.peer().subscriptions().len(), 1);
    assert_eq!(page.peer().subscriptions()[0].topic, "stats");
    assert!(page.says("Containers"));
    assert!(
        page.says("api") && page.says("postgres:16"),
        "{:?}",
        page.words()
    );
    // Its pictures, read from the app's files and decoded before the page was called at rest: the
    // icon, a box for each row and the button's.
    assert!(page.diagnostics().is_empty(), "{:?}", page.diagnostics());
    assert!(pictures(&page) >= 5, "{} pictures", pictures(&page));

    // The program pushes a reading: the row shows it.
    let pushed = page.push(
        "stats",
        &json!({ "readings": [{ "name": "api", "cpu": 42.5, "memory": 130 }] }),
    )?;
    assert_eq!(pushed, 1);
    assert!(page.says("42.5%"), "{:?}", page.words_in("row c1"));

    // The button of the running one stops it: the program is called, and the row says so.
    assert_eq!(page.words_in("act c1"), ["Stop"]);
    page.click("act c1")?;
    assert_eq!(page.methods(), ["list", "toggle"]);
    assert_eq!(page.peer().calls()[1].params, json!({ "id": "c1" }));
    assert_eq!(page.words_in("act c1"), ["Start"]);
    assert!(page.words_in("row c1").iter().any(|word| word == "exited"));
    Ok(())
}

/// The kit's timers run on the host's clock, which moves only as the test moves it: every tick on
/// the way is given, at its time.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_timer_ticks_as_the_clock_is_moved_and_never_by_itself() -> Result<(), Error> {
    let mut page = Headless::script(
        r#"
local ticks = {}
pencere.time.every(1, function()
    ticks[#ticks + 1] = tostring(pencere.time.now())
    return true
end)
return {
    render = function()
        return pencere.text("ticks " .. table.concat(ticks, " "))
    end,
}
"#,
    )
    .open()?;
    assert_eq!(page.tree_words(), ["ticks "]);
    assert_eq!(page.asks_at(), Some(1.0));
    // Nothing happens while the clock stands, however often the page is looked at.
    page.settle()?;
    assert_eq!(page.tree_words(), ["ticks "]);
    page.advance(3.5)?;
    assert_eq!(page.now(), 3.5);
    assert_eq!(page.tree_words(), ["ticks 1 2 3"]);
    assert_eq!(page.asks_at(), Some(4.0));
    assert!(page.step()?);
    assert_eq!(page.now(), 4.0);
    assert_eq!(page.tree_words(), ["ticks 1 2 3 4"]);
    Ok(())
}

/// The host's clock reaches the page's module ([`pencere_sandbox::Program::now`]): a timer set for
/// five seconds runs when the clock is moved five seconds on, and no second of the machine's is
/// waited for it.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_timer_of_five_seconds_runs_on_five_seconds_of_the_hosts_clock_and_none_of_the_machines()
-> Result<(), Error> {
    let mut page = Headless::script(
        r#"
local ran = "waits"
return {
    render = function()
        return pencere.div():command("go", "Go"):child(pencere.text(ran))
    end,
    handle = function(event)
        if event.type == "command" then
            pencere.time.after(5, function()
                ran = string.format("ran at %.1f", pencere.time.now())
                return true
            end)
        end
    end,
}
"#,
    )
    .at(100.0)
    .open()?;
    page.command("go")?;
    let began = std::time::Instant::now();
    page.advance(5.0)?;
    assert_eq!(page.tree_words(), ["waits"], "a frame short");
    // Counted, as the kit counts a timer set outside a wake, from the frame that placed it: a frame
    // after the command.
    page.advance(0.1)?;
    assert_eq!(page.tree_words(), ["ran at 105.0"]);
    assert!(
        began.elapsed() < std::time::Duration::from_secs(4),
        "the machine's time is not the page's: {:?}",
        began.elapsed()
    );
    Ok(())
}

/// A page whose own frame says it moves for ever comes to rest at one time all the same, and is
/// drawn at each frame as the clock moves.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_page_that_moves_for_ever_rests_at_one_time_and_is_drawn_at_each_frame_of_the_clock()
-> Result<(), Error> {
    let mut page = Headless::script(
        r#"
return {
    render = function(size, now)
        return pencere.text(string.format("at %.3f", now)), true
    end,
}
"#,
    )
    .open()?;
    assert_eq!(page.tree_words(), ["at 0.000"]);
    page.advance(0.5)?;
    assert_eq!(page.tree_words(), ["at 0.500"]);
    Ok(())
}

/// What the page asks of the host is done as a host does it and read back: the link it asks to
/// open (one the rule of a link lets through), the text for the pasteboard, the pointer's shape
/// over what answers it, and the menu it pops up, from which the person picks.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_pages_link_copy_cursor_and_menu_are_the_hosts_to_do_and_read_back() -> Result<(), Error> {
    let mut page = Headless::script(
        r#"
local said = "nothing"
return {
    render = function()
        return pencere.div():size_full()
            :child(pencere.div():id("item"):press("item"):size(100, 30):child(pencere.text("an item")))
            :child(pencere.text(said))
    end,
    handle = function(event)
        if event.type == "key" and event.key == "l" then
            return { link = "https://example.com/a" }
        elseif event.type == "key" and event.key == "f" then
            return { link = "file:///etc/passwd" }
        elseif event.type == "key" and event.key == "c" then
            return { copy = "copied words" }
        elseif event.type == "move" then
            local over = event.y < 30 and event.x < 100
            return { cursor = over and "pointer" or "arrow" }
        elseif event.type == "press" and event.button == "right" then
            return { menu = { at = { x = event.x, y = event.y }, entries = {
                { title = "Copy" }, "-", { title = "Delete" }, { title = "Gone", enabled = false },
            } } }
        elseif event.type == "menu" then
            said = "picked " .. event.index
            return true
        end
    end,
}
"#,
    )
    .open()?;
    page.press("l")?;
    page.press("f")?;
    assert_eq!(page.links(), ["https://example.com/a"], "a file is no link");
    page.press("c")?;
    assert_eq!(page.pasteboard(), Some("copied words"));
    page.hover("item")?;
    assert_eq!(page.cursor(), Some(Cursor::Pointer));
    page.move_to(Point::new(400.0, 300.0))?;
    assert_eq!(page.cursor(), Some(Cursor::Arrow));
    // A menu takes the keyboard while it is open; an entry of no use cannot be picked.
    page.right_click("item")?;
    let menu = page.menu().expect("the menu is open");
    assert_eq!(menu.entries.len(), 4);
    assert_eq!(page.press("x"), Err(Error::MenuOpen));
    assert_eq!(page.pick_titled("Gone"), Err(Error::NoEntry("3".into())));
    page.pick_titled("Delete")?;
    assert!(page.menu().is_none());
    assert_eq!(
        page.tree_words().last().map(String::as_str),
        Some("picked 3")
    );
    assert_eq!(page.pick(0), Err(Error::NoMenu));
    // Escape closes one, and nothing is picked; so does a click elsewhere, which is not the page's,
    // its release no more than its press.
    page.right_click("item")?;
    page.press("escape")?;
    assert!(page.menu().is_none());
    page.right_click("item")?;
    let told = page.told().len();
    page.click_at(Point::new(400.0, 300.0))?;
    assert!(page.menu().is_none());
    assert_eq!(page.told().len(), told, "{:?}", &page.told()[told..]);
    Ok(())
}

/// What the page puts on the pasteboard is told with whose act it was (`Host::copy`): the host that
/// keeps the program from its pasteboard writes only the person's. A reply to a key or a press is
/// the person's, a reply to what the program said or to a wake is not, and neither is the copy an
/// editor makes of what the page asked in one: the page and its program are untrusted, and the
/// pasteboard is the person's. The editor's own copy key is.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_pasteboard_is_told_whether_the_person_asked_for_what_is_put_on_it() -> Result<(), Error> {
    let mut page = Headless::script(
        r#"
local copy_all = { editor = { e = { { load = "alpha beta" }, { command = "select_all" }, { command = "copy" } } } }
return {
    render = function()
        return pencere.div():size_full()
            :child(pencere.editor("e", { keymap = { f6 = "copy" } }):grow(1))
            :child(pencere.div():id("copy"):press("copy"):size(100, 30):child(pencere.text("copy")))
    end,
    handle = function(event)
        if event.type == "press" and event.button == "right" then
            return { copy = "by a press", menu = { at = { x = event.x, y = event.y }, entries = { { title = "Copy" } } } }
        elseif event.type == "menu" then
            return copy_all
        elseif event.type == "key" and event.key == "c" then
            return { copy = "by a key" }
        elseif event.type == "key" and event.key == "x" then
            return { copy = "by a cut" }
        elseif event.type == "key" and event.key == "e" then
            return copy_all
        elseif event.type == "wake" then
            return { copy = "by a wake", editor = copy_all.editor }
        end
    end,
    message = function(message)
        if message.say == "copy" then
            return { copy = "by the program" }
        elseif message.say == "editor" then
            return copy_all
        elseif message.say == "wake" then
            return { wake = 0 }
        end
    end,
}
"#,
    )
    .size(400.0, 300.0)
    .open()?;
    fn copies(page: &Headless) -> Vec<(&str, bool)> {
        page.copies()
            .iter()
            .map(|copied| (copied.text.as_str(), copied.by_person))
            .collect()
    }
    // Replies to the copy and the cut of the system's own keys (⌘C, ⌘X): the person's. A plain key
    // is not: the page chose what it does, and "Cmd-C" is what the person means by a copy.
    page.press("cmd+c")?;
    page.press("cmd+x")?;
    page.press("c")?;
    page.press("x")?;
    page.press("shift+cmd+c")?;
    page.press("alt+cmd+c")?;
    page.press("e")?;
    assert_eq!(
        copies(&page),
        [
            ("by a key", true),
            ("by a cut", true),
            ("by a key", false),
            ("by a cut", false),
            ("by a key", false),
            ("by a key", false),
            ("alpha beta", false),
        ],
        "only the keys of a copy and a cut are the person's"
    );
    // A press and a pick of a menu are the person's, and so is the copy of an editor a command asks.
    let from = copies(&page).len();
    page.right_click("copy")?;
    page.pick_titled("Copy")?;
    assert_eq!(
        &copies(&page)[from..],
        [("by a press", true), ("alpha beta", true)],
        "what answers a press and the pick of a menu is the person's"
    );
    // Replies to what the program said: not.
    let from = copies(&page).len();
    page.tell(&json!({ "say": "copy" }).to_string())?;
    page.tell(&json!({ "say": "editor" }).to_string())?;
    assert_eq!(
        &copies(&page)[from..],
        [("by the program", false), ("alpha beta", false)],
        "what answers the program is not"
    );
    // The reply to a wake: not, the page's copy and the editor's.
    let from = copies(&page).len();
    page.tell(&json!({ "say": "wake" }).to_string())?;
    page.advance(0.1)?;
    assert_eq!(
        &copies(&page)[from..],
        [("by a wake", false), ("alpha beta", false)],
        "what answers a wake is not"
    );
    // The editor's own copy key is the person's; a key the page chose for a copy is not.
    let from = copies(&page).len();
    page.focus_editor("e")?;
    page.press("cmd+a")?;
    page.press("cmd+c")?;
    page.press("f6")?;
    assert_eq!(
        &copies(&page)[from..],
        [("alpha beta", true), ("alpha beta", false)],
        "the copy of a key the page chose for it is not the person's"
    );
    Ok(())
}

/// A command of the page is told from two sources: the host's own (its menu, its palette) and a key
/// the page bound to it. The first is the person's word for the pasteboard; the second only when the
/// key is the system's copy or cut, as for a key the page is told as one: a page that binds a plain
/// key to a command that copies, or that asks its editor to copy, has not the person's word for it.
/// The attack goes by the paths a host takes: a key with no editor holding the keyboard, and the
/// same keys with one.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_command_a_key_did_is_the_persons_word_for_the_pasteboard_only_when_the_key_is_a_copy()
-> Result<(), Error> {
    let mut page = Headless::script(
        r#"
return {
    render = function()
        return pencere.div():size_full()
            :command("plain", "Plain", "z")
            :command("fn", "Fn", "f6")
            :command("via", "Via the editor", "f7")
            :command("button", "Copy", "cmd+c")
            :command("cut", "Cut", "cmd+x")
            :command("shifted", "Shifted", "shift+cmd+c")
            :command("host", "Host")
            :child(pencere.editor("e", {}):grow(1))
    end,
    handle = function(event)
        if event.type ~= "command" then
            return nil
        elseif event.id == "via" then
            return { editor = { e = { { load = "alpha beta" }, { command = "select_all" }, { command = "copy" } } } }
        end
        return { copy = "by " .. event.id }
    end,
}
"#,
    )
    .size(400.0, 300.0)
    .open()?;
    fn copies(page: &Headless) -> Vec<(&str, bool)> {
        page.copies()
            .iter()
            .map(|copied| (copied.text.as_str(), copied.by_person))
            .collect()
    }
    let keys = ["z", "f6", "f7", "cmd+c", "cmd+x", "shift+cmd+c"];
    let want = [
        ("by plain", false),
        ("by fn", false),
        ("alpha beta", false),
        ("by button", true),
        ("by cut", true),
        ("by shifted", false),
    ];
    // No editor has the keyboard: the key reaches the page.
    for key in keys {
        page.press(key)?;
    }
    assert_eq!(copies(&page), want, "a command by a key, no editor");
    // An editor has it: the key is the page's command first, told from the editor's route.
    page.focus_editor("e")?;
    let from = copies(&page).len();
    for key in keys {
        page.press(key)?;
    }
    assert_eq!(
        &copies(&page)[from..],
        want,
        "a command by a key, in an editor"
    );
    // The host's own list of commands is the person's.
    let from = copies(&page).len();
    page.command("host")?;
    assert_eq!(&copies(&page)[from..], [("by host", true)]);
    Ok(())
}

/// A menu is the person's: it opens for a press, a key, the pick of one, a command. A program that
/// opens one with a message, or a page with a wake, puts a Paste line under a pointer the person is
/// not using, and what the person picks is the pasteboard in an editor: the pasteboard is theirs to
/// read only when they ask for it. The chain is the one a program makes from its own words: the
/// page copies something, the program moves the keyboard to the editor, asks for the menu, and
/// the person's click on the line pastes.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_menu_opens_for_what_the_person_did_and_never_for_what_the_program_or_a_wake_said()
-> Result<(), Error> {
    let mut page = Headless::script(
        r#"
local got = "nothing"
local function menu() return { menu = { at = { x = 1, y = 1 }, entries = {
    { title = "Nice words", keys = "enter", paste = "e" },
} } } end
return {
    render = function()
        return pencere.div():size_full()
            :child(pencere.editor("e", {}):grow(1))
            :child(pencere.div():id("row"):press("row"):size(100, 30):child(pencere.text(got)))
    end,
    handle = function(event)
        if event.type == "key" and event.key == "s" then
            return { copy = "SECRET" }
        elseif event.type == "key" and event.key == "f5" then
            return menu()
        elseif event.type == "press" and event.button == "right" then
            return menu()
        elseif event.type == "wake" then
            return menu()
        elseif event.type == "menu" then
            got = "picked"
            return { redraw = true, copy = "EVIL-AFTER-PICK" }
        end
    end,
    message = function(m)
        if m.say == "focus" then
            return { keyboard = { field = "e" } }
        elseif m.say == "menu" then
            return menu()
        elseif m.say == "wake" then
            return { wake = 0 }
        end
    end,
}
"#,
    )
    .size(400.0, 300.0)
    .open()?;
    let editor = |page: &Headless| page.editor_text("e").unwrap_or_default();
    page.press("s")?;
    assert_eq!(page.pasteboard(), Some("SECRET"));
    // The program asks for the menu, after it moved the keyboard to the editor: no menu, and so no
    // pick that pastes.
    page.tell(&json!({ "say": "focus" }).to_string())?;
    page.tell(&json!({ "say": "menu" }).to_string())?;
    assert_eq!(page.menu(), None, "a message of the program opened a menu");
    assert_eq!(page.pick(0), Err(Error::NoMenu));
    assert_eq!(editor(&page), "", "the pasteboard went into the editor");
    // The wake the program asked for is no more the person's.
    page.tell(&json!({ "say": "wake" }).to_string())?;
    page.advance(0.1)?;
    assert_eq!(page.menu(), None, "a wake opened a menu");
    assert_eq!(page.pick(0), Err(Error::NoMenu));
    assert_eq!(editor(&page), "");
    // A key the person pressed opens it, and so does a press; the line says nothing of keys, so
    // none picks it. A key is not the person's word for the pasteboard, though: the line is there
    // and off. The right press that opens the menu is, and the person's pick of it pastes.
    page.focus_editor("e")?;
    page.press("f5")?;
    let menu = page.menu().expect("a key opens a menu").clone();
    assert_eq!(menu.entries.len(), 1);
    assert!(
        !format!("{:?}", menu.entries).contains("Enter"),
        "the line of a paste takes no keys: {:?}",
        menu.entries
    );
    assert_eq!(page.press("enter"), Err(Error::MenuOpen));
    assert_eq!(page.pick(0), Err(Error::NoEntry("0".into())));
    assert_eq!(editor(&page), "", "a key's menu pastes");
    page.press("escape")?;
    page.right_click("e")?;
    assert!(page.menu().is_some(), "a press opens a menu");
    page.pick(0)?;
    assert_eq!(editor(&page), "SECRET", "the person's pick pastes");
    page.right_click("row")?;
    assert!(page.menu().is_some(), "a press opens a menu");
    page.press("escape")?;
    assert_eq!(page.menu(), None);
    Ok(())
}

/// A Paste line reads the pasteboard when it is picked, so the page may show it only to a person who
/// asked for a menu by the one act that means a menu: a right press (a context press). Whatever else
/// opens a menu, a key, a left press (the second click of a double click too), a release, a command,
/// the pick in another menu, shows the line off (its place stays, for the page counts its entries): a
/// person who typed a letter and a space, or clicked twice, has not asked for a paste. The attack is
/// the one a program makes of its own words: it moves the keyboard to an editor, a plain key (the
/// editor lets go of it) opens a menu, and the person's next key or click picks the line.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_paste_line_of_a_menu_is_on_only_for_a_right_press() -> Result<(), Error> {
    let mut page = Headless::script(
        r#"
local function menu() return { menu = { at = { x = 1, y = 1 }, entries = {
    { title = "Other" }, { title = "Nice words", paste = "e" },
} } } end
return {
    render = function()
        return pencere.div():size_full():command("go", "Go")
            :child(pencere.editor("e", { keymap = { z = false } }):grow(1))
            :child(pencere.div():id("row"):press("row"):size(100, 30):child(pencere.text("row")))
    end,
    handle = function(event)
        if event.type == "key" and event.key == "s" then
            return { copy = "SECRET" }
        elseif event.type == "key" or event.type == "press" or event.type == "release"
            or event.type == "command" or event.type == "menu" then
            return menu()
        end
    end,
    message = function(m)
        if m.say == "focus" then return { keyboard = { field = "e" } } end
    end,
}
"#,
    )
    .size(400.0, 300.0)
    .open()?;
    let editor = |page: &Headless| page.editor_text("e").unwrap_or_default();
    // Whether the page's Paste line (the second) is on in the menu that is up.
    fn paste_on(page: &Headless) -> bool {
        matches!(
            page.menu().expect("a menu is up").entries[1],
            pencere_view::MenuEntry::Paste { enabled: true, .. }
        )
    }
    page.press("s")?;
    assert_eq!(page.pasteboard(), Some("SECRET"));
    page.tell(&json!({ "say": "focus" }).to_string())?;

    // A plain key the editor lets go of; the line is off and nothing picks it.
    page.press("z")?;
    assert!(!paste_on(&page), "a key put a Paste line under the person");
    assert_eq!(page.pick(1), Err(Error::NoEntry("1".into())));
    // The pick of the other line is the page's: its next menu has the line off, as well.
    page.pick(0)?;
    assert!(!paste_on(&page), "the pick of a menu put a Paste line up");
    page.press("escape")?;
    assert_eq!(editor(&page), "");

    // A left press (the menu the second click of a double click opens is one: the line is off for
    // the button, whatever the count), and a command.
    page.click("row")?;
    assert!(!paste_on(&page), "a left press");
    page.press("escape")?;
    page.command("go")?;
    assert!(!paste_on(&page), "a command");
    page.press("escape")?;
    assert_eq!(editor(&page), "");

    // The right press: over the editor, which has the keyboard, and the person's pick pastes.
    page.focus_editor("e")?;
    page.right_click("e")?;
    assert!(paste_on(&page), "a right press shows the line");
    page.pick(1)?;
    assert_eq!(editor(&page), "SECRET", "the person's pick of it pastes");
    Ok(())
}

/// A menu asked for while another is open is not shown (a pane's pop-up is one at a time), and the
/// page is not told: what the person picks is what they saw, so the page is handed the entry of
/// the menu shown, not the line the same place holds in the one it asked for last.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_pick_from_the_menu_shown_is_the_entry_of_that_menu_when_a_second_was_refused()
-> Result<(), Error> {
    let mut page = Headless::script(
        r#"
local said = "nothing"
return {
    render = function()
        return pencere.div():size_full()
            :child(pencere.div():id("item"):press("item"):size(100, 30):child(pencere.text("an item")))
            :child(pencere.text(said))
    end,
    handle = function(event)
        if event.type == "press" and event.button == "right" then
            return { menu = { at = { x = event.x, y = event.y }, entries = {
                { title = "Copy", run = "copy" }, "-", { title = "Delete", run = "delete" },
            } } }
        elseif event.type == "menu" then
            said = "ran " .. tostring(event.entry and event.entry.run) .. " at " .. event.index
            return true
        end
    end,
    message = function()
        return { menu = { at = { x = 1, y = 1 }, entries = {
            { title = "Rename", run = "rename" }, "-", { title = "Wipe everything", run = "wipe" },
        } } }
    end,
}
"#,
    )
    .open()?;
    page.right_click("item")?;
    // The page asks for a second menu while the first is open: the host does not show it.
    page.tell(r#"{"ask":true}"#)?;
    let shown = page.menu().expect("the first menu is still up");
    assert!(
        matches!(&shown.entries[2], MenuEntry::Item { title, .. } if title == "Delete"),
        "{shown:?}"
    );
    page.pick_titled("Delete")?;
    assert_eq!(
        page.tree_words().last().map(String::as_str),
        Some("ran delete at 3")
    );
    Ok(())
}

/// Where replies put together (`pencere.merge`) both ask for a menu, the second's stands, and it is
/// the one numbered and kept: a pick of it is told with its entry.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn of_two_menus_put_together_the_seconds_is_shown_and_its_entries_are_what_a_pick_is_told()
-> Result<(), Error> {
    let mut page = Headless::script(
        r#"
local said = "nothing"
return {
    render = function()
        return pencere.div():size_full()
            :child(pencere.div():id("item"):press("item"):size(100, 30):child(pencere.text("an item")))
            :child(pencere.text(said))
    end,
    handle = function(event)
        if event.type == "press" and event.button == "right" then
            return pencere.merge(
                { menu = { at = { x = 1, y = 1 }, entries = { { title = "First", run = "first" } } } },
                { menu = { at = { x = 2, y = 2 }, entries = { { title = "Second", run = "second" } } } }
            )
        elseif event.type == "menu" then
            said = "ran " .. tostring(event.entry and event.entry.run)
            return true
        end
    end,
}
"#,
    )
    .open()?;
    page.right_click("item")?;
    page.pick_titled("Second")?;
    assert_eq!(
        page.tree_words().last().map(String::as_str),
        Some("ran second")
    );
    Ok(())
}

/// A text field is the host's: a click in it gives it the keyboard, what the person types is told
/// the page as the text it holds, a reply fills it, and a click on the page takes the keyboard
/// back.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_text_field_is_the_hosts_typed_into_filled_and_left() -> Result<(), Error> {
    let mut page = Headless::script(
        r#"
local said = {}
return {
    render = function()
        return pencere.div():size_full()
            :child(pencere.div():id("q"):text_field("q"):h(24):w(200):child(pencere.text("Search")))
            :child(pencere.div():id("clear"):press("clear"):size(60, 24):child(pencere.text("Clear")))
            :child(pencere.text(table.concat(said, "|")))
    end,
    handle = function(event)
        if event.type == "text" then
            said[#said + 1] = event.field .. "=" .. event.text
            return true
        elseif event.type == "focus" then
            said[#said + 1] = "focus " .. tostring(event.field)
            return true
        elseif event.type == "release" then
            said[#said + 1] = "cleared"
            return { redraw = true, fill = { q = "" } }
        end
    end,
}
"#,
    )
    .open()?;
    assert_eq!(page.outline().fields.len(), 1);
    page.click("q")?;
    assert_eq!(page.focus(), Focus::Field(Id::named("q")));
    page.type_text("ab")?;
    page.press("backspace")?;
    page.type_text("c")?;
    assert_eq!(page.field("q"), Some("ac"));
    page.click("clear")?;
    assert_eq!(page.focus(), Focus::Page);
    assert_eq!(page.field("q"), Some(""));
    page.fill("q", "whole")?;
    assert_eq!(
        page.tree_words().last().map(String::as_str),
        Some("focus q|q=a|q=ab|q=a|q=ac|focus nil|cleared|focus q|q=whole")
    );
    Ok(())
}

/// A page says how many lines a field of lines is as tall as and the host's line, the padding and the
/// border make its height; the field's text stands inside them.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_field_of_rows_is_as_tall_as_its_lines_and_its_padding_and_keeps_its_text_inside()
-> Result<(), Error> {
    let page = Headless::script(
        r#"
return {
    render = function()
        return pencere.div():size_full()
            :child(pencere.div():id("one"):text_field("one", "lines", { rows = 1 }):w(200):py(6))
            :child(pencere.div():id("three"):text_field("three", "lines", { rows = 3 }):w(200):py(6):px(10))
            :child(pencere.div():id("open"):text_field("open", "lines"):w(200):h(33))
    end,
}
"#,
    )
    .open()?;
    // The headless host's line is 16 points.
    let high = |name: &str| page.bounds(name).map(|room| room.h);
    assert_eq!(high("one"), Some(16.0 + 12.0));
    assert_eq!(high("three"), Some(48.0 + 12.0));
    assert_eq!(
        high("open"),
        Some(33.0),
        "a field with no rows has the room its style gives it"
    );
    let fields = &page.outline().fields;
    let text = |name: &str| {
        fields
            .iter()
            .find(|field| field.id == Id::named(name))
            .map(|field| field.text)
    };
    // The first field's room is 28 high: the second's text starts under it, inside its padding.
    assert_eq!(
        text("three"),
        Some(Rect::new(10.0, 28.0 + 6.0, 180.0, 48.0))
    );
    Ok(())
}

/// The host that draws the fields says how tall its line is, and it is that line, not the one of the
/// page's own measure, that a field of rows is as tall as: the answer reaches the layout through
/// what the drive and the editors wrap the measure in.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_field_of_rows_is_as_tall_as_the_line_the_host_says_for_its_fields() -> Result<(), Error> {
    struct Hosts;
    impl Measure for Hosts {
        fn measure(&mut self, _: &Font, text: &str) -> Metrics {
            #[allow(clippy::cast_precision_loss)]
            let width = text.chars().count() as f32 * 7.0;
            Metrics {
                width,
                height: 16.0,
            }
        }

        fn field_line(&mut self, _: &Font) -> Option<f32> {
            Some(20.0)
        }
    }
    let page = Headless::script(
        r#"return { render = function()
            return pencere.div():size_full()
                :child(pencere.div():id("two"):text_field("two", "lines", { rows = 2 }):w(100))
        end }"#,
    )
    .measure(Hosts)
    .open()?;
    assert_eq!(page.bounds("two").map(|room| room.h), Some(40.0));
    Ok(())
}

/// A host's line is not in proportion to its size (a face at 25 points is 31 tall, not 32): the page
/// zoomed to twice its size asks the host for the field's font at that size and is as tall as that
/// many of what it says.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_field_of_rows_at_a_zoom_is_as_tall_as_the_line_of_the_hosts_face_at_that_size()
-> Result<(), Error> {
    struct Hosts;
    impl Measure for Hosts {
        fn measure(&mut self, font: &Font, text: &str) -> Metrics {
            #[allow(clippy::cast_precision_loss)]
            let width = text.chars().count() as f32 * font.size / 2.0;
            Metrics {
                width,
                height: if font.size > 20.0 { 31.0 } else { 16.0 },
            }
        }
    }
    let mut page = Headless::script(
        r#"return { render = function()
            return pencere.div():size_full()
                :child(pencere.div():text_field("two", "lines", { rows = 2 }):w(100):py(4))
        end }"#,
    )
    .measure(Hosts)
    .open()?;
    let two = |page: &Headless| {
        let fields = &page.outline().fields;
        (fields[0].rect.h, fields[0].text.h)
    };
    assert_eq!(two(&page), (32.0 + 8.0, 32.0));
    page.zoom(2.0)?;
    // In the host's points: two lines of 31, and the padding of 4 points as 8.
    assert_eq!(two(&page), (62.0 + 16.0, 62.0));
    Ok(())
}

/// What is not a count of lines, a field that is not of lines, and a height besides the lines are
/// the page's fault, said where the page said it, whichever it said first.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn rows_of_a_field_that_are_not_a_count_of_lines_or_come_with_a_height_are_the_pages_fault() {
    let cases = [
        (
            r#"pencere.div():text_field("q", "line", { rows = 2 })"#,
            "only a field of \"lines\"",
        ),
        (
            r#"pencere.div():text_field("q", "secret", { rows = 2 })"#,
            "only a field of \"lines\"",
        ),
        (
            r#"pencere.div():text_field("q", "lines", { rows = 0 })"#,
            "from 1 to 100, not 0",
        ),
        (
            r#"pencere.div():text_field("q", "lines", { rows = 101 })"#,
            "from 1 to 100, not 101",
        ),
        (
            r#"pencere.div():text_field("q", "lines", { rows = 2.5 })"#,
            "from 1 to 100, not 2.5",
        ),
        (
            r#"pencere.div():text_field("q", "lines", { rows = "3" })"#,
            "from 1 to 100, not 3",
        ),
        (
            r#"pencere.div():text_field("q", "lines", { row = 3 })"#,
            "takes the option rows, not row",
        ),
        (
            r#"pencere.div():text_field("q", "lines", 3)"#,
            "are a table",
        ),
        (
            r#"pencere.div():h(28):text_field("q", "lines", { rows = 3 })"#,
            "rows or a height",
        ),
        (
            r#"pencere.div():h_pct(50):text_field("q", "lines", { rows = 3 })"#,
            "rows or a height",
        ),
        (
            r#"pencere.div():text_field("q", "lines", { rows = 3 }):h(28)"#,
            "rows or a height",
        ),
        (
            r#"pencere.div():text_field("q", "lines", { rows = 3 }):size(100, 28)"#,
            "rows or a height",
        ),
        (
            r#"pencere.div():text_field("q", "lines", { rows = 3 }):h_pct(50)"#,
            "rows or a height",
        ),
    ];
    for (line, said) in cases {
        let source =
            format!("local field = {line}\nreturn {{ render = function() return field end }}");
        let failed = Headless::script(source).open().map(drop);
        assert!(
            matches!(&failed, Err(Error::Ended(Fault::Failed(why))) if why.starts_with("page.lua:1:") && why.contains(said)),
            "{line}: {failed:?}"
        );
    }
    // A least and a most height go with rows, either way round.
    let page = Headless::script(
        r#"
return { render = function()
    return pencere.div():size_full()
        :child(pencere.div():id("a"):min_h(100):text_field("a", "lines", { rows = 2 }):w(100))
        :child(pencere.div():id("b"):text_field("b", "lines", { rows = 9 }):max_h(40):w(100))
end }
"#,
    )
    .open();
    let page = page.expect("a least and a most height are allowed");
    assert_eq!(page.bounds("a").map(|room| room.h), Some(100.0));
    assert_eq!(page.bounds("b").map(|room| room.h), Some(40.0));
}

/// A reply fills a field and selects in it (a name to rename, its stem selected), what is typed
/// takes the place of what is selected, and a key the field has no use for where its caret is (⌫
/// in an empty field, ← at its start) is the page's: a command of the page by any of its keys, or
/// a key.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_field_is_filled_and_selected_in_and_gives_the_page_the_keys_it_has_no_use_for()
-> Result<(), Error> {
    let mut page = Headless::script(
        r#"
local said = {}
return {
    render = function()
        return pencere.div():size_full()
            :command("rename", "Rename", { "f2", "ctrl+r" })
            :command("back", "Back", "cmd+[")
            :child(pencere.div():text_field("name"):h(24):w(200))
            :child(pencere.text(table.concat(said, "|")))
    end,
    handle = function(event)
        if event.type == "command" and event.id == "rename" then
            said[#said + 1] = "rename"
            return {
                redraw = true,
                keyboard = { field = "name" },
                fill = { name = { text = "draft.md", select = { from = 1, to = 6 } } },
            }
        elseif event.type == "command" then
            said[#said + 1] = event.id
            return true
        elseif event.type == "key" then
            said[#said + 1] = "key " .. event.key
            if event.key == "escape" then
                return { redraw = true, fill = { name = { select = true } } }
            end
            return true
        end
    end,
}
"#,
    )
    .open()?;
    page.press("ctrl+r")?;
    assert_eq!(page.focus(), Focus::Field(Id::named("name")));
    assert_eq!(page.field("name"), Some("draft.md"));
    assert_eq!(page.field_selection("name"), Some(0..5));
    page.type_text("notes")?;
    assert_eq!(page.field("name"), Some("notes.md"));
    assert_eq!(page.field_selection("name"), Some(5..5));
    // The chord a field has no use for is the page's command, here as anywhere.
    page.press("cmd+[")?;
    page.press("escape")?;
    assert_eq!(page.field_selection("name"), Some(0..8));
    page.press("backspace")?;
    assert_eq!(page.field("name"), Some(""));
    // Empty: backspace and the arrows that go nowhere are the page's.
    page.press("backspace")?;
    page.press("left")?;
    page.type_text("a")?;
    page.press("left")?;
    page.press("left")?;
    page.press("f2")?;
    assert_eq!(
        page.tree_words().last().map(String::as_str),
        Some("rename|back|key escape|key backspace|key left|key left|rename")
    );
    Ok(())
}

/// A reply that opens a field and, in the same reply, fills it and sends the keyboard to it (a find
/// bar, a palette): a host has the field only once the frame the reply asks for has laid it out, so
/// the texts and the keyboard come with that frame, in the order the reply asked; a name that frame
/// still has not is passed over, and what names a field that is there is done at once.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_reply_that_opens_a_field_fills_it_and_sends_the_keyboard_to_it() -> Result<(), Error> {
    let mut page = Headless::script(
        r#"
local open = false
return {
    render = function()
        local root = pencere.div():size_full()
            :command("find", "Find", "ctrl+f")
            :command("again", "Again", "ctrl+g")
            :command("nothing", "Nothing", "ctrl+n")
            :command("elsewhere", "Elsewhere", "ctrl+e")
        if open then
            root:child(pencere.div():text_field("q"):h(24):w(200))
        end
        return root
    end,
    handle = function(event)
        if event.type ~= "command" then return end
        if event.id == "find" then
            open = true
            return {
                redraw = true,
                fill = { q = { text = "needle", select = true }, ghost = "boo" },
                keyboard = { field = "q" },
            }
        elseif event.id == "again" then
            return { redraw = true, fill = { q = "hay" } }
        elseif event.id == "nothing" then
            return { redraw = true, keyboard = { field = "ghost" } }
        elseif event.id == "elsewhere" then
            return { redraw = true, keyboard = "page" }
        end
    end,
}
"#,
    )
    .open()?;
    assert_eq!(page.field("q"), None);
    page.press("ctrl+f")?;
    assert_eq!(page.outline().fields.len(), 1);
    assert_eq!(page.focus(), Focus::Field(Id::named("q")));
    assert_eq!(page.field("q"), Some("needle"));
    assert_eq!(page.field_selection("q"), Some(0..6));
    // A name the frame does not have is passed over, as a host passes over what is no field of its
    // own.
    assert_eq!(page.field("ghost"), None);
    // A field that is there is told at once, and the keyboard sent to one that never is goes to the
    // page.
    page.press("ctrl+g")?;
    assert_eq!(page.field("q"), Some("hay"));
    assert_eq!(page.focus(), Focus::Field(Id::named("q")));
    page.press("ctrl+n")?;
    assert_eq!(page.focus(), Focus::Page);
    page.press("ctrl+e")?;
    assert_eq!(page.focus(), Focus::Page);
    Ok(())
}

/// The boxes that scroll are the host's to scroll: a box taken to a place is where the wheel over
/// it took it, and the page is told where it went.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_box_is_scrolled_as_the_wheel_over_it_scrolls_it() -> Result<(), Error> {
    let mut page = Headless::script(
        r#"
local said = "nothing"
return {
    render = function()
        local list = pencere.div():id("list"):h(100):w(200):scroll()
        for n = 1, 50 do list:child(pencere.div():h(20):child(pencere.text("row " .. n))) end
        return pencere.div():size_full():child(list):child(pencere.text(said))
    end,
    handle = function(event)
        if event.type == "scrolled" then
            said = "scrolled " .. event.boxes[1].id .. " " .. event.boxes[1].y
            return true
        end
    end,
}
"#,
    )
    .open()?;
    page.scroll_to("list", 300.0)?;
    assert_eq!(
        page.laid()
            .and_then(|laid| laid.scroll_extent(Id::named("list")))
            .map(|extent| extent.offset.y),
        Some(300.0)
    );
    assert_eq!(
        page.tree_words().last().map(String::as_str),
        Some("scrolled list 300")
    );
    assert!(page.says("row 16"), "{:?}", page.words());
    Ok(())
}

/// A box that says it may be zoomed is zoomed by the fingers of a test as by a person's: the page
/// is told the scale, a double tap goes over a moment, and a box that does not zoom has none.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_box_that_zooms_is_pinched_and_the_test_says_how_far_it_went() -> Result<(), Error> {
    let mut page = Headless::script(
        r#"
local said = "nothing"
return {
    render = function()
        local list = pencere.div():id("list"):h(100):w(200):scroll():zoom({ max = 3 })
        for n = 1, 50 do list:child(pencere.div():h(20):no_shrink():child(pencere.text("row " .. n))) end
        local plain = pencere.div():id("plain"):h(40):w(200):scroll():child(pencere.div():h(400):no_shrink())
        return pencere.div():size_full():child(list):child(plain):child(pencere.text(said))
    end,
    handle = function(event)
        if event.type == "scrolled" then
            said = string.format("scrolled %s %.1f", event.boxes[1].id, event.boxes[1].zoom)
            return true
        end
    end,
}
"#,
    )
    .open()?;
    assert_eq!(page.zoom_of("list")?, 1.0);
    page.pinch("list", 0.5)?;
    assert!((page.zoom_of("list")? - 1.5).abs() < 1e-3);
    assert_eq!(
        page.tree_words().last().map(String::as_str),
        Some("scrolled list 1.5")
    );
    // Past the range the page gave is the end of it.
    page.pinch("list", 5.0)?;
    assert!((page.zoom_of("list")? - 3.0).abs() < 1e-3);
    // A double tap goes where it is going over a moment.
    page.smart_magnify("list")?;
    page.idle()?;
    assert!((page.zoom_of("list")? - 1.0).abs() < 1e-3);
    // A box that does not zoom is not zoomed, and a box that does not scroll has no scale to ask.
    page.pinch("plain", 0.5)?;
    assert_eq!(page.zoom_of("plain")?, 1.0);
    assert_eq!(
        page.zoom_of("nothing"),
        Err(Error::NoSuch("nothing".into()))
    );
    Ok(())
}

/// The size the page asks to be drawn at is the host's to take, in any reply: the headless host says
/// what it last asked, and the host's own text is not part of it.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_page_says_how_much_larger_than_the_hosts_text_it_is_to_be_drawn() -> Result<(), Error> {
    let mut page = Headless::script(
        r#"
return {
    render = function() return pencere.div():child(pencere.text("code")) end,
    handle = function(event)
        if event.type == "key" and event.key == "a" then
            return { text_size = 3 }
        elseif event.type == "key" and event.key == "b" then
            return { text_size = -1, redraw = true }
        end
    end,
}
"#,
    )
    .open()?;
    assert_eq!(page.text_size(), 0);
    page.press("a")?;
    assert_eq!(page.text_size(), 3);
    page.press("b")?;
    assert_eq!(page.text_size(), -1);
    Ok(())
}

/// What the page asks of the trackpad is kept, every ask and in order, by the host with no screen
/// (it has no finger and no device to judge them by): a name's pattern, the page's own, the end of
/// what plays, and a name the kit has not told as a diagnostic with the rest of the reply going on.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_page_asks_the_trackpad_to_vibrate_and_the_host_with_no_screen_keeps_every_ask_in_order()
-> Result<(), Error> {
    use pencere_view::haptic::{self, Event as Tap, Haptic, Pattern};
    let mut page = Headless::script(
        r#"
local seen = 0
return {
    render = function() return pencere.div():child(pencere.text("felt " .. seen)) end,
    handle = function(event)
        if event.type ~= "key" then return end
        seen = seen + 1
        if event.key == "a" then
            return { haptic = "tick" }
        elseif event.key == "b" then
            return { redraw = true, haptic = { events = { { at = 0.1, intensity = 0.5 }, { seconds = 0.2 } } } }
        elseif event.key == "c" then
            return { haptic = false }
        elseif event.key == "d" then
            return { redraw = true, haptic = "nope" }
        end
    end,
}
"#,
    )
    .open()?;
    assert_eq!(page.haptics(), []);
    page.press("a")?;
    page.press("b")?;
    page.press("c")?;
    let own = Pattern::new([
        Tap::held(0.0, 0.2, (0.3, 0.3), (0.5, 0.5)),
        Tap::tap(0.1, 0.5, 0.5),
    ])
    .expect("a pattern");
    assert_eq!(
        page.haptics(),
        [
            Haptic::Play(haptic::preset("tick").expect("a name")),
            Haptic::Play(own),
            Haptic::Stop,
        ]
    );
    assert!(page.diagnostics().is_empty());
    // A name the kit has not: nothing is asked of the trackpad, the page's author is told, and the
    // page is drawn again for the rest of the reply.
    page.press("d")?;
    assert_eq!(page.haptics().len(), 3);
    assert_eq!(page.diagnostics().len(), 1, "{:?}", page.diagnostics());
    assert!(page.diagnostics()[0].contains("\"nope\""));
    assert!(page.reading().text().contains("felt 4"));
    Ok(())
}

/// What the page asks of the sounds of its view is kept, every ask and in order, by the host with
/// no screen (it has no speaker to judge them by): the kit's own with their samples at the pitch
/// asked for, a file of the page's with its samples when it is decoded (after what was ready
/// then), the end of every sound, and what is no sound as a diagnostic with the rest of the reply
/// going on.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_page_asks_for_sounds_and_the_host_with_no_screen_keeps_every_ask_in_order() -> Result<(), Error>
{
    use pencere_sandbox::sounds::{palette, wav};
    use pencere_view::sound::{Ask, Play, Source};
    let ding = wav(&palette("success").expect("a sound")).expect("a file");
    let name = format!("sha256:{}", pencere_wire::bundle::Digest::of(&ding));
    let source = format!(
        r#"
local seen = 0
return {{
    render = function() return pencere.div():child(pencere.text("heard " .. seen)) end,
    handle = function(event)
        if event.type ~= "key" then return end
        seen = seen + 1
        if event.key == "a" then
            return {{ sound = "tap" }}
        elseif event.key == "b" then
            return {{ sound = {{ name = "success", volume = 0.5, pitch = 2 }} }}
        elseif event.key == "c" then
            return {{ sound = {{ {{ file = "{name}", volume = 0.25 }}, "snap" }} }}
        elseif event.key == "d" then
            return {{ sound = false }}
        elseif event.key == "e" then
            return {{ redraw = true, sound = "nope" }}
        elseif event.key == "f" then
            return {{ redraw = true, sound = {{ file = "missing.wav" }} }}
        end
    end,
}}
"#
    );
    let mut page = Headless::script(&source).attached(&ding).open()?;
    assert!(page.sounds().is_empty());
    let play = |ask: &Ask| -> Play {
        match ask {
            Ask::Play(play) => play.clone(),
            Ask::Stop => panic!("a sound, got the end of them"),
        }
    };
    let whole = palette("success").expect("a sound").frames();
    page.press("a")?;
    page.press("b")?;
    page.press("c")?;
    page.press("d")?;
    let asked = page.sounds();
    assert_eq!(asked.len(), 5, "{asked:?}");
    let tap = play(&asked[0]);
    assert_eq!(tap.source, Source::Kit("tap".into()));
    assert_eq!((tap.volume, tap.pitch), (1.0, 1.0));
    assert!(tap.samples.is_some());
    let success = play(&asked[1]);
    assert_eq!(success.source, Source::Kit("success".into()));
    assert_eq!((success.volume, success.pitch), (0.5, 2.0));
    let frames = success.samples.expect("samples").frames();
    assert!(frames.abs_diff(whole / 2) <= 1, "{frames} of {whole}");
    // The kit's sound is given with the reply; the file is given when it is decoded.
    assert_eq!(play(&asked[2]).source, Source::Kit("snap".into()));
    let ding = play(&asked[3]);
    assert_eq!(ding.source, Source::File(name));
    assert_eq!(ding.volume, 0.25);
    assert_eq!(ding.samples.expect("samples").frames(), whole);
    assert_eq!(asked[4], Ask::Stop);
    assert!(page.diagnostics().is_empty());
    // A name the kit has not and a file that is not there: nothing is asked, the page's author is
    // told, and the page is drawn again for the rest of the reply.
    page.press("e")?;
    page.press("f")?;
    assert_eq!(page.sounds().len(), 5);
    assert_eq!(page.diagnostics().len(), 2, "{:?}", page.diagnostics());
    assert!(page.diagnostics()[0].contains("\"nope\""));
    assert!(page.diagnostics()[1].contains("\"missing.wav\""));
    assert!(page.reading().text().contains("heard 6"));
    Ok(())
}

/// A file that is asked for in a reply that asks for no frame (a message of the program, here) is
/// waited for as one in a reply that does: the page is settled when the sound is given.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_file_asked_for_in_a_reply_that_draws_nothing_is_waited_for_by_the_host_with_no_screen()
-> Result<(), Error> {
    use pencere_sandbox::sounds::{palette, wav};
    use pencere_view::sound::{Ask, Source};
    let ding = wav(&palette("success").expect("a sound")).expect("a file");
    let name = format!("sha256:{}", pencere_wire::bundle::Digest::of(&ding));
    let mut page = Headless::script(
        r#"
return {
    render = function() return pencere.div() end,
    message = function(message)
        return { sound = { file = message.file } }
    end,
}
"#,
    )
    .attached(&ding)
    .open()?;
    page.tell(&json!({ "file": name }).to_string())?;
    let asked = page.sounds();
    assert_eq!(asked.len(), 1, "{asked:?}");
    let Ask::Play(play) = &asked[0] else {
        panic!("a sound, got {:?}", asked[0]);
    };
    assert_eq!(play.source, Source::File(name));
    assert!(play.samples.is_some());
    Ok(())
}

/// A page whose boxes say what they carry out of the view, and that counts what the pointer did to
/// it: one that only carries, one that is no carrier, and one the page also drags.
const CARRIERS: &str = r#"
local ui = pencere.Ui.new()
local n = { press = 0, move = 0, release = 0 }
local last = "-"
return {
    render = function()
        return pencere.div():row()
            :child(pencere.div():id("tab"):size(60, 30):carry({ text = "hello", url = "https://example.com/a" }))
            :child(pencere.div():id("plain"):size(60, 30))
            :child(pencere.div():draggable("both"):size(60, 30):carry({ path = "/tmp/a b" }))
            :child(pencere.text(string.format("press:%d move:%d release:%d %s %s",
                n.press, n.move, n.release, ui:dragging() and "dragging" or "free", last)))
    end,
    handle = function(event)
        local at = event.x and { x = event.x, y = event.y }
        if event.type == "press" then
            n.press = n.press + 1
            ui:press(pencere.layout, at)
        elseif event.type == "move" then
            n.move = n.move + 1
            ui:point(pencere.layout, at)
        elseif event.type == "release" then
            n.release = n.release + 1
            ui:release(pencere.layout, at)
        elseif event.type == "carried" then
            last = "carried " .. event.id .. " " .. tostring(event.done)
        end
        return true
    end,
}
"#;

/// What the page was told: the line it draws of what the pointer did to it.
fn heard(page: &Headless) -> String {
    page.words()
        .into_iter()
        .find(|words| words.starts_with("press:"))
        .expect("the page's line")
        .to_owned()
}

fn left(at: Point) -> Input {
    Input::Press {
        at,
        button: Button::Left,
        mods: Mods::default(),
        count: 1,
    }
}

fn moved(at: Point) -> Input {
    Input::Move {
        at,
        mods: Mods::default(),
    }
}

/// The person presses on an element that carries and moves the pointer away: once it has moved
/// `DRAG_START` points the host is given the drag (what is carried, where the pointer is, the
/// element's room), and the page is told none of the pointer from then on, its release included,
/// until the system's drag is over, and then it is told so and hears the pointer again.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn an_element_that_carries_is_given_to_the_system_once_the_pointer_has_moved_far_enough()
-> Result<(), Error> {
    let mut page = Headless::script(CARRIERS).open()?;
    let room = page.bounds("tab").expect("the element's room");
    let middle = Point::new(room.x + room.w / 2.0, room.y + room.h / 2.0);
    page.pointer(left(middle))?;
    // Short of the distance, it is the page's pointer.
    page.pointer(moved(Point::new(middle.x + 2.0, middle.y)))?;
    assert!(page.carries().is_empty());
    assert_eq!(heard(&page), "press:1 move:1 release:0 free -");
    let past = Point::new(middle.x + 6.0, middle.y);
    page.pointer(moved(past))?;
    let [carry] = page.carries() else {
        panic!("one drag out of the view: {:?}", page.carries());
    };
    assert_eq!(carry.id, Id::named("tab"));
    assert_eq!(
        carry.carry,
        Carry::text("hello").url("https://example.com/a")
    );
    assert_eq!((carry.at, carry.rect), (past, room));
    // The page hears no more of it: not a move, not the release the system took.
    page.pointer(moved(Point::new(300.0, 200.0)))?;
    page.pointer(Input::Release {
        at: Point::new(300.0, 200.0),
        button: Button::Left,
        mods: Mods::default(),
    })?;
    assert_eq!(heard(&page), "press:1 move:1 release:0 free -");
    assert_eq!(page.carries().len(), 1, "told once");
    // The drag is over: the page is told, and hears the pointer again (a click is a click).
    page.finish_carry(true)?;
    assert_eq!(
        heard(&page),
        "press:1 move:1 release:0 free carried tab true"
    );
    page.finish_carry(true)?;
    page.click("plain")?;
    assert_eq!(
        heard(&page),
        "press:2 move:1 release:1 free carried tab true"
    );
    Ok(())
}

/// A click on an element that carries is a click, an element that does not carry is never given to
/// the system however far the pointer goes, and a host that did not say it carries is not asked.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_click_or_a_drag_of_what_does_not_carry_stays_the_pages_and_a_host_that_does_not_carry_is_not_asked()
-> Result<(), Error> {
    let mut page = Headless::script(CARRIERS).open()?;
    page.click("tab")?;
    assert_eq!(heard(&page), "press:1 move:0 release:1 free -");
    page.drag_out("plain", Point::new(500.0, 300.0))?;
    assert!(page.carries().is_empty());
    assert_eq!(heard(&page), "press:2 move:8 release:2 free -");
    page.page_mut().set_carry(false);
    page.drag_out("tab", Point::new(500.0, 300.0))?;
    assert!(page.carries().is_empty());
    assert_eq!(heard(&page), "press:3 move:16 release:3 free -");
    Ok(())
}

/// An element the page also drags is the page's drag while the pointer is in the view, and the
/// system's from the moment it leaves; the end of the system's drag lets go of what the page's `Ui`
/// held, though the page never heard the release.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn an_element_the_page_also_drags_is_the_systems_once_the_pointer_has_left_the_view()
-> Result<(), Error> {
    let mut page = Headless::script(CARRIERS).open()?;
    page.drag_out("both", Point::new(900.0, 100.0))?;
    let [carry] = page.carries() else {
        panic!("one drag out of the view: {:?}", page.carries());
    };
    assert_eq!(carry.carry, Carry::path("/tmp/a b"));
    assert!(carry.at.x > 800.0, "{:?}", carry.at);
    // It went on in the page while the pointer was in the view: six moves heard, a drag under way.
    assert_eq!(heard(&page), "press:1 move:6 release:0 dragging -");
    page.finish_carry(false)?;
    assert_eq!(
        heard(&page),
        "press:1 move:6 release:0 free carried both false",
        "the drag the page held is gone"
    );
    Ok(())
}

/// A host that stops carrying while a drag goes ends it: the page is told it is over, not done.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_host_that_stops_carrying_ends_the_drag_that_goes() -> Result<(), Error> {
    let mut page = Headless::script(CARRIERS).open()?;
    page.drag_out("tab", Point::new(400.0, 200.0))?;
    assert_eq!(page.carries().len(), 1);
    page.page_mut().set_carry(false);
    page.settle()?;
    assert_eq!(
        heard(&page),
        "press:1 move:0 release:0 free carried tab false"
    );
    Ok(())
}

/// A page drawn twice as large as its own points: the distance that starts a drag and the edge of
/// the view are counted in the page's points, and what the host is given is in its own.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_drag_out_is_counted_in_the_pages_points_and_handed_to_the_host_in_its_own() -> Result<(), Error>
{
    let mut page = Headless::script(CARRIERS).open()?;
    page.zoom(2.0)?;
    let room = page.bounds("tab").expect("the element's room");
    assert_eq!(room.w, 120.0, "in the host's points");
    let middle = Point::new(room.x + room.w / 2.0, room.y + room.h / 2.0);
    page.pointer(left(middle))?;
    // Five of the host's points are two and a half of the page's: not far enough.
    page.pointer(moved(Point::new(middle.x + 5.0, middle.y)))?;
    assert!(page.carries().is_empty());
    // Nine are four and a half.
    let past = Point::new(middle.x + 9.0, middle.y);
    page.pointer(moved(past))?;
    let [carry] = page.carries() else {
        panic!("one drag out of the view: {:?}", page.carries());
    };
    assert_eq!((carry.at, carry.rect), (past, room));
    page.finish_carry(false)?;
    // The view is 400 of the page's points wide, 800 of the host's: an element the page also drags
    // is the system's beyond the first.
    let room = page.bounds("both").expect("the element's room");
    let middle = Point::new(room.x + room.w / 2.0, room.y + room.h / 2.0);
    page.pointer(left(middle))?;
    page.pointer(moved(Point::new(790.0, middle.y)))?;
    assert_eq!(page.carries().len(), 1, "still in the view");
    page.pointer(moved(Point::new(810.0, middle.y)))?;
    assert_eq!(page.carries().len(), 2, "out of it");
    assert_eq!(page.carries()[1].rect, room);
    Ok(())
}

/// A press that was never let go of (the host lost the release) is not a drag out of the view when
/// the pointer comes back and moves: a move alone starts none, and the pointer's leaving ends what
/// the press began.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_move_that_follows_no_press_starts_no_drag_out_of_the_view() -> Result<(), Error> {
    let mut page = Headless::script(CARRIERS).open()?;
    let room = page.bounds("tab").expect("the element's room");
    let middle = Point::new(room.x + room.w / 2.0, room.y + room.h / 2.0);
    page.pointer(left(middle))?;
    page.pointer(Input::Leave)?;
    page.pointer(moved(Point::new(middle.x + 20.0, middle.y)))?;
    assert!(page.carries().is_empty(), "{:?}", page.carries());
    Ok(())
}

/// A page written in Rust, which the sandbox never held to the rule of what is carried: four
/// boxes that carry what no host may hand on, and one that carries a line. It counts the moves
/// it is told.
struct Careless(std::sync::Arc<std::sync::atomic::AtomicUsize>);

impl pencere_sandbox::Runner for Careless {
    fn render(
        &mut self,
        _size: Size,
        _now: f64,
        _reduce: bool,
    ) -> Result<pencere_sandbox::Frame, pencere_sandbox::Fault> {
        let carrier = |name: &str, carry: Carry| {
            div()
                .id(Id::named(name))
                .size(60.0, 30.0)
                .no_shrink()
                .carry(carry)
        };
        Ok(pencere_sandbox::Frame {
            root: div()
                .row()
                .size(800.0, 600.0)
                .child(carrier("lines", Carry::text("rm -rf ~\nls")))
                .child(carrier("escape", Carry::path("\u{1b}]0;x\u{7}")))
                .child(carrier("far", Carry::text("x").url("file:///etc/passwd")))
                .child(carrier(
                    "long",
                    Carry::text("a".repeat(pencere_wire::carry::MAX_CARRY + 1)),
                ))
                .child(carrier("fine", Carry::text("a line"))),
            animating: false,
            wake_at: None,
            wake: None,
        })
    }

    fn settle(&mut self) -> Result<bool, pencere_sandbox::Fault> {
        Ok(false)
    }

    fn handle(&mut self, event: Event) -> Result<Reply, pencere_sandbox::Fault> {
        if matches!(
            event,
            Event::Input(Input::Move { .. })
                | Event::Pointer {
                    input: Input::Move { .. },
                    ..
                }
        ) {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        Ok(Reply::none())
    }

    fn lay_out(&mut self, _laid: std::sync::Arc<pencere_view::Laid>) {}
}

/// What a page written in Rust says to carry is held to the rule again where the pointer begins the
/// drag: none of what no host may hand on is given to the host, and the page is told the whole
/// of that drag as of any other.
#[test]
fn a_carry_that_breaks_the_rule_is_never_given_to_the_host_whatever_made_the_page()
-> Result<(), Error> {
    let moves = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let heard = std::sync::Arc::clone(&moves);
    let mut page = Headless::spawn(move || Ok(Careless(heard))).open()?;
    for name in ["lines", "escape", "far", "long"] {
        let before = moves.load(std::sync::atomic::Ordering::SeqCst);
        page.drag_out(name, Point::new(500.0, 400.0))?;
        assert!(page.carries().is_empty(), "{name}: {:?}", page.carries());
        assert!(
            moves.load(std::sync::atomic::Ordering::SeqCst) >= before + 8,
            "{name}: the page heard the drag"
        );
    }
    page.drag_out("fine", Point::new(500.0, 400.0))?;
    assert_eq!(page.carries().len(), 1);
    Ok(())
}

/// A press while the drive thinks the system's drag is going says the host lost its end: the page
/// is told it is over (it was not done), and the press is the page's.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_press_while_a_drag_out_is_going_ends_it_for_a_host_that_did_not_say_so() -> Result<(), Error> {
    let mut page = Headless::script(CARRIERS).open()?;
    page.drag_out("tab", Point::new(400.0, 200.0))?;
    assert_eq!(page.carries().len(), 1);
    let plain = page.bounds("plain").expect("the element's room");
    page.pointer(left(Point::new(plain.x + 5.0, plain.y + 5.0)))?;
    assert_eq!(
        heard(&page),
        "press:2 move:0 release:0 free carried tab false"
    );
    Ok(())
}

/// A test says what went wrong in words: a page that fails, an element that is not there, keys no
/// person can press, a command the page does not offer.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn what_cannot_be_done_is_an_error_that_says_why() -> Result<(), Error> {
    let failed = Headless::script("local x = nil\nreturn x.y")
        .open()
        .map(drop);
    assert!(
        matches!(&failed, Err(Error::Ended(Fault::Failed(why))) if why.starts_with("page.lua:2:")),
        "{failed:?}"
    );
    let mut page = Headless::script(
        r#"return { render = function()
            return pencere.div():command("go", "Go")
        end, handle = function() return true end }"#,
    )
    .open()?;
    assert_eq!(page.click("nothing"), Err(Error::NoSuch("nothing".into())));
    assert_eq!(page.press("hyper+k"), Err(Error::NoKey("hyper+k".into())));
    assert_eq!(page.command("stay"), Err(Error::NoCommand("stay".into())));
    page.command("go")?;
    assert!(matches!(page.told().last(), Some(Event::Command(id)) if *id == Id::named("go")));
    assert_eq!(page.advance(-1.0), Err(Error::NoTime(-1.0)));
    let missing = Headless::bundle(Files::dir(example("nowhere")), "page.lua")
        .open()
        .map(drop);
    assert_eq!(missing, Err(Error::NoFile("page.lua".into())));
    Ok(())
}

/// A key the page binds to a command is told as the command: the page says so, laid out, before
/// the key is pressed (a host that sends keys without waiting asks, to know which are a command).
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_page_says_which_keys_it_binds_to_a_command() -> Result<(), Error> {
    let mut page = Headless::script(
        r#"return { render = function()
            return pencere.div():command("go", "Go", { "down" })
        end, handle = function() return true end }"#,
    )
    .open()?;
    page.advance(0.1)?;
    assert!(page.page().binds(Key::Down, Mods::default()));
    assert!(!page.page().binds(Key::Up, Mods::default()));
    assert!(!page.page().binds(
        Key::Down,
        Mods {
            shift: true,
            ..Mods::default()
        }
    ));
    Ok(())
}

/// An editor's selection and its own commands, as a person makes them without the pointer: a range
/// selected, the caret put, a region folded and unfolded, what was typed undone.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn an_editors_selection_and_commands_are_the_persons_without_the_pointer() -> Result<(), Error> {
    let mut page = Headless::script(
        r#"
local loaded = false
return {
    render = function()
        return pencere.editor("e", { folds = "indent" }):size_full()
    end,
    handle = function(event)
        if event.type == "viewport" and not loaded then
            loaded = true
            return { editor = { e = { load = "if a then\n    b\n    c\nend" } } }
        end
    end,
}
"#,
    )
    .open()?;
    assert_eq!(page.edit(EditCommand::Undo), Err(Error::NoEditor));
    page.select("e", 3, 4)?;
    assert_eq!(page.focus(), Focus::Editor(Id::named("e")));
    let buffer = page.editor("e").expect("the editor");
    assert_eq!((buffer.main().start(), buffer.main().end()), (3, 4));
    page.insert_text("x")?;
    assert_eq!(
        page.editor_text("e").as_deref(),
        Some("if x then\n    b\n    c\nend")
    );
    page.edit(EditCommand::Undo)?;
    assert_eq!(
        page.editor_text("e").as_deref(),
        Some("if a then\n    b\n    c\nend")
    );
    page.caret_to("e", 0)?;
    page.edit(EditCommand::ToggleFold)?;
    assert_eq!(
        page.editor("e").expect("the editor").fold_lines(0, 3),
        vec![0]
    );
    assert_eq!(
        page.select("nothing", 0, 0),
        Err(Error::NoSuch("nothing".into()))
    );
    Ok(())
}

/// What is undone together follows the host's clock, as every time of the page does: what is typed
/// with no time between is one group, and what is typed after a pause longer than the editor's
/// (`UNDO_PAUSE`) on that clock is one of its own — without a moment of the machine's waited.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn what_is_typed_is_undone_in_groups_by_the_hosts_clock() -> Result<(), Error> {
    let mut page = Headless::script(
        r#"return { render = function() return pencere.editor("e"):size_full() end }"#,
    )
    .open()?;
    page.focus_editor("e")?;
    page.type_text("ab")?;
    page.type_text("cd")?;
    page.advance(UNDO_PAUSE + 0.5)?;
    page.type_text("ef")?;
    page.edit(EditCommand::Undo)?;
    assert_eq!(page.editor_text("e").as_deref(), Some("abcd"));
    page.edit(EditCommand::Undo)?;
    assert_eq!(page.editor_text("e").as_deref(), Some(""));
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_page_reads_what_its_ui_keeps_of_the_pointer_and_the_keyboard() -> Result<(), Error> {
    let mut page = Headless::script(
        r#"
        local ui = pencere.Ui.new()
        local layout = pencere.layout
        local function button(id)
            return pencere.div():id(id):press(id):focusable():p(8):child(pencere.text(id))
        end
        return {
            render = function()
                return pencere.div():p(10):gap(10)
                    :child(button("a"))
                    :child(button("b"))
                    :child(pencere.text(string.format("hover %s pressed %s focus %s",
                        tostring(ui.hover), tostring(ui.pressed), tostring(ui.focus))))
            end,
            handle = function(event)
                if event.type == "move" then
                    return ui:point(layout, event)
                elseif event.type == "leave" then
                    return ui:point(layout, nil)
                elseif event.type == "press" then
                    ui:press(layout, event)
                    return true
                elseif event.type == "release" then
                    ui:release(layout, event)
                    return true
                elseif event.type == "key" and event.key == "tab" then
                    ui:focus_step(layout, event.mods.shift)
                    return true
                end
            end,
        }
        "#,
    )
    .size(400.0, 300.0)
    .open()?;
    let shown = |page: &Headless| {
        page.tree_words()
            .into_iter()
            .find(|words| words.starts_with("hover "))
            .expect("the page says what its Ui keeps")
    };
    assert_eq!(shown(&page), "hover nil pressed nil focus nil");
    page.hover("a")?;
    assert_eq!(shown(&page), "hover a pressed nil focus nil");
    let middle = |page: &Headless, name: &str| {
        let bounds = page.bounds(name).expect("the button is laid out");
        Point::new(bounds.x + bounds.w / 2.0, bounds.y + bounds.h / 2.0)
    };
    let at = middle(&page, "b");
    page.move_to(at)?;
    page.pointer(pencere_view::Input::Press {
        at,
        button: pencere_view::Button::Left,
        mods: pencere_view::Mods::default(),
        count: 1,
    })?;
    assert_eq!(shown(&page), "hover b pressed b focus b");
    page.pointer(pencere_view::Input::Release {
        at,
        button: pencere_view::Button::Left,
        mods: Mods::default(),
    })?;
    assert_eq!(shown(&page), "hover b pressed nil focus b");
    page.press("tab")?;
    assert_eq!(shown(&page), "hover b pressed nil focus a");
    page.leave()?;
    assert_eq!(shown(&page), "hover nil pressed nil focus a");
    Ok(())
}

/// A member the program sent as `null` holds nothing, as one it left out does, and the page tells
/// the two apart where they mean different things: `git: null` is a repository gone, no `git`
/// nothing changed. An object read is written again with its `null`s.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_page_tells_a_member_that_is_null_from_one_that_is_not_there() -> Result<(), Error> {
    let mut page = Headless::script(
        r#"
local said = {}
local json = pencere.json
return {
    render = function()
        return pencere.div():child(pencere.text(table.concat(said, "|")))
    end,
    message = function(message)
        local changes = message.changes
        said[#said + 1] = string.format("%s %s %s %s", tostring(changes.git ~= nil),
            tostring(json.has(changes, "git")), tostring(json.is_null(changes, "git")),
            json.encode(changes))
        return true
    end,
}
"#,
    )
    .open()?;
    page.tell(r#"{"changes":{"git":null,"dirs":["a"]}}"#)?;
    page.tell(r#"{"changes":{"dirs":[]}}"#)?;
    page.tell(r#"{"changes":{"git":{"branch":"main"}}}"#)?;
    assert_eq!(
        page.tree_words().last().map(String::as_str),
        Some(
            r#"false true true {"dirs":["a"],"git":null}|false false false {"dirs":[]}|true true false {"git":{"branch":"main"}}"#
        )
    );
    Ok(())
}

/// What a page prints (`print`, `warn`) is a call of the reserved method `pencere.log`: the table of
/// calls answers it at once and keeps the lines, apart from the application's own calls.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn what_a_page_prints_is_kept_by_its_program_and_is_none_of_the_applications_calls()
-> Result<(), Error> {
    let mut page = Headless::script(
        r#"
local heard = 0
print("start", 1, nil)
return {
    render = function() return pencere.text("heard " .. heard) end,
    handle = function(event)
        if event.type == "key" and event.key == "enter" then
            warn("saving", event.key)
            pencere.call("save", { n = 1 })
            return true
        end
    end,
    message = function()
        heard += 1
        return true
    end,
}
"#,
    )
    .open()?;
    let line = |level: &str, text: &str| Logged {
        level: level.to_owned(),
        text: text.to_owned(),
    };
    assert_eq!(page.logged(), [line("info", "start\t1\tnil")]);
    page.press("enter")?;
    assert_eq!(
        page.logged(),
        [line("info", "start\t1\tnil"), line("warn", "saving\tenter")]
    );
    // The application's calls are the one `save`, which waits for its answer: no line is a call of
    // the page's, and none waits for an answer.
    assert_eq!(page.methods(), ["save"]);
    assert_eq!(page.peer().calls().len(), 1);
    assert_eq!(page.peer().waiting().len(), 1);
    assert!(page.peer().noise().is_empty());
    // The lines were answered `{}`, and the answers are the prelude's: the page's `message` was
    // not called for them.
    assert!(page.says("heard 0"), "{:?}", page.words());
    Ok(())
}

/// The time of day is the host's, and goes on with its clock; how the host's system writes its
/// keys is the host's too, and `mod` is its main modifier.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_page_reads_the_time_of_day_and_the_keyboard_of_the_hosts_system() -> Result<(), Error> {
    let script = r#"
local said = "nothing"
return {
    render = function()
        local seconds, offset = pencere.time.wall()
        return pencere.div():size_full():command("save", "Save", "mod+s")
            :child(pencere.text(string.format("%d %d %s %s", seconds, offset, pencere.keyboard(), said)))
    end,
    handle = function(event)
        if event.type == "command" then
            said = event.id
            return true
        end
    end,
}
"#;
    let mut page = Headless::script(script)
        .wall_clock(1_000.0, 3 * 3600)
        .keyboard(pencere_view::KeyConvention::Linux)
        .open()?;
    assert_eq!(page.tree_words(), ["1000 10800 linux nothing"]);
    page.advance(5.0)?;
    page.press("ctrl+s")?;
    assert_eq!(page.tree_words(), ["1005 10800 linux save"]);
    // A Mac's main modifier is command: the same words are other keys there.
    let mut mac = Headless::script(script).open()?;
    mac.press("ctrl+s")?;
    assert_eq!(
        mac.tree_words(),
        [format!("{} 0 mac nothing", crate::headless::WALL_ORIGIN)]
    );
    mac.press("mod+s")?;
    assert!(mac.tree_words()[0].ends_with("mac save"));
    Ok(())
}

/// A picture that spins turns with the clock, a turn in the seconds it says, and is still under
/// Reduce Motion.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_canvas_that_spins_turns_with_the_clock_and_not_under_reduce_motion() -> Result<(), Error> {
    let script = r#"
return {
    render = function()
        return pencere.div():size_full():child(
            pencere.canvas({ pencere.path():move_to(10, 0):line_to(10, 5):stroke(1, "accent") })
                :size(20, 20):motion({ spin = 2 }))
    end,
}
"#;
    let first = |page: &Headless| {
        page.list().items.iter().find_map(|item| match item {
            // The turn is in the path's transform, not in its points.
            Paint::Figure(figure) => match figure.paths[0].segments[0] {
                pencere_view::Segment::Move(at) => Some(figure.paths[0].transform.apply(at)),
                _ => None,
            },
            _ => None,
        })
    };
    let mut page = Headless::script(script).open()?;
    let start = first(&page).expect("a figure");
    page.advance(0.5)?;
    // A quarter of a turn: the point at the top of the middle is at its right.
    let quarter = first(&page).expect("a figure");
    assert!(
        (start.x - 10.0).abs() < 0.01 && start.y.abs() < 0.01,
        "{start:?}"
    );
    assert!(
        (quarter.x - 20.0).abs() < 0.01 && (quarter.y - 10.0).abs() < 0.01,
        "{quarter:?}"
    );
    let mut still = Headless::script(script).reduce_motion(true).open()?;
    still.advance(0.5)?;
    let at = first(&still).expect("a figure");
    assert!((at.x - 10.0).abs() < 0.01 && at.y.abs() < 0.01, "{at:?}");
    Ok(())
}

/// What never stops (a spin) does not keep `idle` from returning, and its frames are drawn on the way
/// of what does: the page is waited for until the change that ends has ended.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn idle_waits_for_a_change_that_ends_and_not_for_a_spin_that_never_does() -> Result<(), Error> {
    let script = r#"
local on = false
return {
    render = function()
        return pencere.div():size_full()
            :child(
                pencere.canvas({ pencere.path():move_to(10, 0):line_to(10, 5):stroke(1, "accent") })
                    :size(20, 20):motion({ spin = 1 }))
            :child(pencere.div():id("chip"):size(100, 40):bg(on and "accent" or "dim")
                :motion({ seconds = 0.5, color = true }))
    end,
    handle = function(event)
        if event.type == "key" then on = not on end
        return true
    end,
}
"#;
    let top = |page: &Headless| {
        page.list().items.iter().find_map(|item| match item {
            Paint::Figure(figure) => match figure.paths[0].segments[0] {
                pencere_view::Segment::Move(at) => Some(figure.paths[0].transform.apply(at)),
                _ => None,
            },
            _ => None,
        })
    };
    let mut page = Headless::script(script).open()?;
    assert_eq!(page.idle()?, 0, "a spin alone is nothing to wait for");
    let start = top(&page).expect("a figure");
    page.press("a")?;
    let begun = page.now();
    let frames = page.idle()?;
    assert!(
        (25..=40).contains(&frames),
        "{frames} frames for half a second"
    );
    assert!(
        page.now() - begun >= 0.5,
        "it waited until the change ended"
    );
    let turned = top(&page).expect("a figure");
    assert!(
        (turned.x - start.x).abs() > 0.5 || (turned.y - start.y).abs() > 0.5,
        "the spin was drawn on the way: {start:?} {turned:?}"
    );
    assert_eq!(page.idle()?, 0, "and it is done");
    Ok(())
}

/// A loop goes by the host's clock and is no change to wait for; a spring that bounces is, and it
/// has come to rest, at the width the page said, by the time `idle` returns.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_loop_goes_by_the_clock_and_idle_waits_for_a_spring_that_bounces_and_not_for_it()
-> Result<(), Error> {
    let script = r#"
local wide = false
return {
    render = function()
        return pencere.div():size_full():row():items_start()
            :child(pencere.div():id("dot"):size(10, 10):bg("accent")
                :motion({ loop = { seconds = 1, dx = 40, ease = "linear", opacity = 0.5 } }))
            :child(pencere.div():id("bar"):h(10):w(wide and 200 or 100):bg("dim"):no_shrink()
                :motion({ seconds = 0.4, ease = "spring", bounce = 0.5, size = true }))
    end,
    handle = function(event)
        if event.type == "key" then wide = not wide end
        return true
    end,
}
"#;
    let quads = |page: &Headless| -> Vec<Rect> {
        page.list()
            .items
            .iter()
            .filter_map(|item| match item {
                Paint::Quad(quad) if quad.fill.is_some() => Some(quad.rect),
                _ => None,
            })
            .collect()
    };
    let mut page = Headless::script(script).open()?;
    assert_eq!(page.idle()?, 0, "a loop alone is nothing to wait for");
    let start = quads(&page);
    page.advance(0.25)?;
    let half = quads(&page);
    assert!(
        (half[0].x - start[0].x - 20.0).abs() < 1.5,
        "a loop of a second, there and back, is half way at a quarter of it: {start:?} {half:?}"
    );
    assert_eq!(half[1].w, 100.0, "and the bar has not moved");
    page.press("a")?;
    // A loop asks for every frame: a second of them is more than the spring needs.
    let mut widest = 0.0_f32;
    for _ in 0..60 {
        assert!(page.step()?);
        widest = widest.max(quads(&page)[1].w);
    }
    assert!(
        widest > 200.0,
        "the spring went past where it was going: {widest}"
    );
    let rest = quads(&page);
    assert_eq!(rest[1].w, 200.0, "and it came to rest at the width said");
    assert_eq!(page.idle()?, 0);
    Ok(())
}

// --- Files beside the bundle (specification §8.8 to §8.10) ------------------------------------

/// A picture two pixels wide, for the pages that draw files beside their bundle.
const PNG: [u8; 84] = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x08, 0x06, 0x00, 0x00, 0x00, 0x72, 0xb6, 0x0d,
    0x24, 0x00, 0x00, 0x00, 0x1b, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x38, 0xe1, 0xe0, 0xf0,
    0xdf, 0x61, 0x41, 0xc0, 0x7f, 0x06, 0x87, 0x84, 0x0b, 0xff, 0x1f, 0x5c, 0x98, 0xf0, 0x1f, 0x00,
    0x51, 0xbc, 0x0a, 0x25, 0x0f, 0x57, 0x03, 0xad, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44,
    0xae, 0x42, 0x60, 0x82,
];

/// A page that draws the picture its program attached, the one the person gave it last, and asks
/// for the panels: the Open panel at a press of `open`, and also at every wake (which is passed
/// over); the Save panel at a key.
const FILES_PAGE: &str = r##"
    local shown, attached, told = nil, nil, "nothing"
    return {
        render = function()
            local page = pencere.div():child(pencere.text(told))
                :child(pencere.div():id("drop"):size(100, 40))
                :child(pencere.div():press("open"):size(100, 20))
            if attached then page:child(pencere.image(attached):size(10, 10)) end
            if shown then page:child(pencere.image("file:" .. shown):size(10, 10)) end
            return page
        end,
        handle = function(e)
            if e.type == "files" then
                told = e.source .. " " .. #e.files
                if e.files[1] then
                    shown = e.files[1].handle
                    told = told .. " " .. e.files[1].name .. " " .. e.files[1].size .. " " .. e.files[1].type
                end
                return true
            elseif e.type == "files_over" then
                told = "over " .. e.count
                return true
            elseif e.type == "files_left" then
                told = "left"
                return true
            elseif e.type == "link_dropped" then
                told = "link " .. e.url .. " " .. tostring(e.x > 0 and e.y > 0)
                return true
            elseif e.type == "saving" then
                told = "saving " .. tostring(e.name)
                return true
            elseif e.type == "press" then
                return { redraw = true, pick = { multiple = true } }
            elseif e.type == "key" then
                return { save = { name = "notes.txt", type = "text/plain" } }
            elseif e.type == "wake" then
                return { pick = true, copy = "woken" }
            end
        end,
        message = function(m)
            attached = m.show
            return { redraw = true, wake = 0 }
        end,
    }
"##;

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_file_the_person_gives_is_drawn_by_its_handle_and_an_attached_one_by_its_digest()
-> Result<(), Error> {
    let mut page = Headless::script(FILES_PAGE).size(300.0, 200.0).open()?;
    page.drag_files_over("drop", 2)?;
    assert!(page.says("over 2"), "{:?}", page.words());
    page.drag_files_away()?;
    assert!(page.says("left"), "{:?}", page.words());
    let handles = page.give_files("drop", pencere_view::FileSource::Drop, &[("dot.png", &PNG)])?;
    assert_eq!(handles, [format!("{:032x}", 1)]);
    assert!(
        page.says("drop 1 dot.png 84 image/png"),
        "{:?}",
        page.words()
    );
    assert_eq!(pictures(&page), 1);
    // A digest the program did not attach is no file of the page's.
    let unattached = format!("sha256:{}", pencere_wire::bundle::Digest::of(b"nothing"));
    page.tell(&json!({ "show": unattached }).to_string())?;
    assert_eq!(pictures(&page), 1);
    let name = page.attach(&PNG);
    page.tell(&json!({ "show": name }).to_string())?;
    assert_eq!(pictures(&page), 2);
    // Let go of: forgotten, and drawn no more.
    page.detach(&name)?;
    assert_eq!(pictures(&page), 1);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn an_address_dropped_on_the_page_reaches_it_with_where_it_fell() -> Result<(), Error> {
    let mut page = Headless::script(FILES_PAGE).size(300.0, 200.0).open()?;
    page.drop_link("drop", "https://example.com/a?b=c")?;
    assert!(
        page.says("link https://example.com/a?b=c true"),
        "{:?}",
        page.words()
    );
    // What a page may not open as a link is not told it: not a file, not a script, not a line with a
    // space or a break in it.
    for bad in [
        "file:///etc/passwd",
        "javascript:alert(1)",
        "https://example.com/a b",
        "https://example.com/\n",
    ] {
        page.drop_link("drop", bad)?;
        assert!(!page.says("link file"), "{bad}");
        assert!(!page.says("link javascript"), "{bad}");
        assert!(page.says("link https://example.com/a?b=c true"), "{bad}");
    }
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_panel_is_shown_for_what_the_person_did_and_one_at_a_time() -> Result<(), Error> {
    let mut page = Headless::script(FILES_PAGE).size(300.0, 200.0).open()?;
    // Asked for in the answer to a wake: passed over.
    page.tell(&json!({}).to_string())?;
    page.advance(0.1)?;
    // Woken, and its reply taken: but not the panel in it.
    assert_eq!(page.pasteboard(), Some("woken"));
    assert_eq!(page.panel(), None);
    // Asked for at a press: shown.
    page.click("open")?;
    assert_eq!(
        page.panel(),
        Some(&pencere_view::Panel::Pick { multiple: true })
    );
    // A second while it is up: dropped.
    page.key(pencere_view::Key::Enter, pencere_view::Mods::default())?;
    assert_eq!(page.dropped_panels(), 1);
    let handles = page.choose_files(&[("a.txt", b"hello"), ("b.txt", b"!")])?;
    assert_eq!(handles.len(), 2);
    assert!(page.says("pick 2 a.txt 5 text/plain"), "{:?}", page.words());
    // The Save panel, at a key, and the place the person chose.
    page.key(pencere_view::Key::Enter, pencere_view::Mods::default())?;
    assert_eq!(
        page.panel(),
        Some(&pencere_view::Panel::Save {
            name: "notes.txt".into(),
            media_type: Some("text/plain".into()),
        })
    );
    let handle = page.choose_place(Some("notes 2.txt"))?;
    assert_eq!(handle.as_deref().map(str::len), Some(32));
    assert!(page.says("saving notes 2.txt"), "{:?}", page.words());
    Ok(())
}

// --- The pages of a long PDF, on a display with two pixels to the point ------------------------------

/// A PDF whose pages are `(width, height, [red, green, blue])`, points, each a box of its colour
/// over the whole page.
#[cfg(feature = "pdf")]
fn pdf_of_colours(pages: &[(u32, u32, [f32; 3])]) -> Vec<u8> {
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    let mut object = |out: &mut Vec<u8>, body: &str| {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n{body}\nendobj\n", offsets.len()).bytes());
    };
    object(&mut out, "<< /Type /Catalog /Pages 2 0 R >>");
    let kids: Vec<String> = (0..pages.len())
        .map(|at| format!("{} 0 R", 3 + 2 * at))
        .collect();
    object(
        &mut out,
        &format!(
            "<< /Type /Pages /Count {} /Kids [{}] >>",
            pages.len(),
            kids.join(" ")
        ),
    );
    for (at, (width, height, [r, g, b])) in pages.iter().enumerate() {
        let content = format!("{r} {g} {b} rg 0 0 {width} {height} re f\n");
        object(
            &mut out,
            &format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width} {height}] /Contents {} 0 R >>",
                4 + 2 * at
            ),
        );
        object(
            &mut out,
            &format!(
                "<< /Length {} >>\nstream\n{content}endstream",
                content.len()
            ),
        );
    }
    let table = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).bytes());
    for offset in &offsets {
        out.extend(format!("{offset:010} 00000 n \n").bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{table}\n%%EOF\n",
            offsets.len() + 1
        )
        .bytes(),
    );
    out
}

/// Whether the pixels the host holds for `picture` are as sharp as it is shown: as many to a pixel of
/// the page as the page has.
#[cfg(feature = "pdf")]
fn is_sharp(images: &pencere_sandbox::Images, picture: &pencere_view::Picture) -> bool {
    let full = picture.full();
    images.picture(picture).is_some_and(|bitmap| {
        // The pixels to a pixel of the page: the bitmap's width over the width of the part it holds.
        bitmap.width as f32 >= 0.99 * bitmap.part.w * full.w
    })
}

/// The pictures of the draw list that are in view, and whether each is sharp.
#[cfg(feature = "pdf")]
fn in_view(page: &Headless) -> Vec<(u32, bool)> {
    let images = page.page().images().expect("the page's images");
    page.list()
        .items
        .iter()
        .filter_map(|item| match item {
            Paint::Image(picture) if picture.rect.intersect(picture.clip).h > 0.0 => {
                Some((picture.page, is_sharp(images, picture)))
            }
            _ => None,
        })
        .collect()
}

/// Pages as large as a person makes them (the user's own: 1536 by 1024 points at a hundred percent),
/// a view as wide as a wide pane, a display with two pixels to the point and the images' own limits.
/// What is seen is sharp at every zoom up to four hundred percent, and a page scrolled to is sharp at
/// once, not when the pixels of the page scrolled away from have gone unasked for the time they are
/// kept.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
#[cfg(feature = "pdf")]
fn what_is_in_view_is_sharp_at_every_zoom_and_a_page_scrolled_to_is_sharp_at_once()
-> Result<(), Error> {
    let colours = [
        [0.8, 0.1, 0.1],
        [0.1, 0.6, 0.1],
        [0.1, 0.1, 0.8],
        [0.6, 0.6, 0.1],
    ];
    let pages: Vec<(u32, u32, [f32; 3])> = colours.iter().map(|&c| (1536, 1024, c)).collect();
    let pdf = pdf_of_colours(&pages);
    let name = format!("sha256:{}", pencere_wire::bundle::Digest::of(&pdf));
    // At a hundred and twenty-five percent two of these are 79 MB of pixels, past the 64 MiB a view
    // may hold; at four hundred, one is 400 MB.
    for (zoom, wait) in [(1.25_f32, 1500_u64), (4.0, 3000)] {
        let (w, h) = ((1536.0 * zoom).round(), (1024.0 * zoom).round());
        let source = format!(
            r#"return {{ render = function()
                local box = pencere.div():id("pages"):col():size(1500, 1000):scroll()
                for n = 1, 4 do
                    box:child(pencere.image("{name}"):page(n):size({w}, {h}):no_shrink())
                end
                return pencere.div():child(box)
            end }}"#
        );
        let mut page = Headless::script(source)
            .size(1500.0, 1000.0)
            .scale(2.0)
            .attached(&pdf)
            .open()?;
        let sharp = |page: &Headless| {
            let seen = in_view(page);
            !seen.is_empty() && seen.iter().all(|(_, sharp)| *sharp)
        };
        let patience = std::time::Duration::from_millis(wait);
        page.wait_for(std::time::Duration::from_secs(30), sharp)?;
        // Each page in turn, and the place two of them meet: sharp within the time of drawing it,
        // which is well under the two seconds pixels are kept.
        for at in [0.6, 1.0, 1.6, 2.0, 2.6, 3.0] {
            page.scroll_to("pages", h * at as f32 - 500.0)?;
            page.wait_for(patience, sharp).inspect_err(|_| {
                eprintln!("zoom {zoom}, scrolled to {at} pages: {:?}", in_view(&page));
            })?;
        }
        let budget = page
            .diagnostics()
            .iter()
            .any(|words| words.contains("64 MiB"));
        assert!(!budget, "{:?}", page.diagnostics());
    }
    Ok(())
}

/// A field that grows a line as it is typed in goes to its new height over its time: each line
/// that is added is sent the box on to where it is going, the words inside it are where they will
/// be, and the keyboard is still the field's.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_text_field_that_grows_as_it_is_typed_in_goes_to_its_height_and_keeps_the_keyboard()
-> Result<(), Error> {
    let mut page = Headless::script(
        r#"
local lines = 1
return {
    render = function()
        local field = pencere.div():id("q"):text_field("q"):col():w(200)
            :motion({ seconds = 0.3, auto_size = true })
        for line = 1, lines do
            field:child(pencere.text("line " .. line):id("line" .. line):h(20):no_shrink())
        end
        return pencere.div():size_full():col():child(field):child(pencere.div():id("below"):h(10))
    end,
    handle = function(event)
        if event.type == "text" then
            lines = math.floor(#event.text / 3) + 1
            return true
        end
    end,
}
"#,
    )
    .size(300.0, 300.0)
    .open()?;
    let at = |page: &Headless<Calls>, name: &str| page.bounds(name).expect(name);
    assert_eq!(at(&page, "q").h, 20.0);
    page.click("q")?;
    page.type_text("abcdef")?;
    // Three lines are asked for at once; the box is on its way to them, and it is under the
    // keyboard still.
    assert_eq!(page.focus(), Focus::Field(Id::named("q")));
    let first = at(&page, "q").h;
    assert_eq!(first, 20.0, "it starts where it was");
    page.advance(0.1)?;
    let on_the_way = at(&page, "q").h;
    assert!(on_the_way > 20.0 && on_the_way < 60.0, "{on_the_way}");
    assert_eq!(at(&page, "line3").h, 20.0, "a line is where it will be");
    assert!((at(&page, "below").y - on_the_way).abs() <= 1.0);
    // Another line in flight is sent on from where it is.
    page.type_text("ghi")?;
    page.advance(0.05)?;
    let sent = at(&page, "q").h;
    assert!(
        sent >= on_the_way && sent <= 80.0,
        "{on_the_way} then {sent}"
    );
    page.idle()?;
    assert_eq!(at(&page, "q").h, 80.0);
    assert_eq!(at(&page, "below").y, 80.0);
    assert_eq!(page.focus(), Focus::Field(Id::named("q")));
    assert_eq!(page.field("q"), Some("abcdefghi"));
    Ok(())
}

/// An editor in a box that opens is laid out for the size the box goes to: it is where it will be
/// from the first frame, the box shows what fits, and the editor is the editor's after it, as it
/// would have been had the box not moved.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn an_editor_in_a_box_that_opens_is_where_it_will_be_and_takes_the_keyboard_all_the_same()
-> Result<(), Error> {
    let mut page = Headless::script(
        r#"
local open = false
return {
    render = function()
        local panel = pencere.div():id("panel"):col():motion({ seconds = 0.4, auto_size = true })
        if open then
            panel:child(pencere.editor("e", {}):id("e"):h(100):w(200))
        end
        return pencere.div():size_full():col()
            :child(pencere.div():id("go"):press("go"):size(60, 24):child(pencere.text("Go")))
            :child(panel)
            :child(pencere.div():id("foot"):h(20))
    end,
    handle = function(event)
        if event.type == "release" then
            open = not open
            return true
        end
    end,
}
"#,
    )
    .size(300.0, 300.0)
    .open()?;
    let at = |page: &Headless<Calls>, name: &str| page.bounds(name);
    assert_eq!(at(&page, "panel").map(|r| r.h), Some(0.0));
    page.click("go")?;
    page.advance(0.1)?;
    let on_the_way = at(&page, "panel").expect("panel").h;
    assert!(on_the_way > 0.0 && on_the_way < 100.0, "{on_the_way}");
    let editor = at(&page, "e").expect("an editor");
    assert_eq!(
        (editor.y, editor.h),
        (24.0, 100.0),
        "it is where it will be"
    );
    assert!((at(&page, "foot").expect("foot").y - (24.0 + on_the_way)).abs() <= 1.0);
    // The keyboard is the editor's in the middle of the move, and what is typed is in it.
    page.focus_editor("e")?;
    page.type_text("hi")?;
    assert_eq!(page.editor_text("e").as_deref(), Some("hi"));
    page.idle()?;
    assert_eq!(at(&page, "panel").map(|r| r.h), Some(100.0));
    assert_eq!(at(&page, "e"), Some(editor));
    assert_eq!(page.editor_text("e").as_deref(), Some("hi"));
    Ok(())
}

/// In a script as in Rust, `m`, `mx` and `my` are the margins of the sides said once, and `h_pct` is
/// a share of the parent's height.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_script_says_the_margin_of_every_side_or_of_an_axis_at_once_and_a_share_of_the_height()
-> Result<(), Error> {
    let mut page = Headless::script(
        r#"
return {
    render = function()
        local function column(first, second)
            return pencere.div():w(100):child(first:h(10)):child(second:h(10))
        end
        return pencere.div():size_full():row():items_start()
            :child(column(pencere.div():id("sides"):mt(3):mr(5):mb(3):ml(5),
                pencere.div():id("each"):mt(2):mr(2):mb(2):ml(2)))
            :child(column(pencere.div():id("axes"):my(3):mx(5), pencere.div():id("every"):m(2)))
            :child(pencere.div():w(100):h(80):child(pencere.div():id("share"):h_pct(25)))
    end,
}
"#,
    )
    .size(400.0, 200.0)
    .open()?;
    page.settle()?;
    let at = |name: &str| page.bounds(name).expect(name);
    let moved = |rect: Rect| Rect::new(rect.x + 100.0, rect.y, rect.w, rect.h);
    assert_eq!(at("axes"), moved(at("sides")));
    assert_eq!(at("every"), moved(at("each")));
    assert_eq!(at("axes"), Rect::new(105.0, 3.0, 90.0, 10.0));
    assert_eq!(at("share").h, 20.0);
    Ok(())
}
