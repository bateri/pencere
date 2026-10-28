//! Pages under the host with no screen, as a test of an app writes them: the example app of
//! containers (`examples/containers-lua`) with its program played by a table of calls, and short
//! scripts for what a person does and sees. Heavy: the interpreter is compiled and run
//! (`make check-heavy`).

use std::collections::HashMap;
use std::path::PathBuf;

use pencere_sandbox::Fault;
use pencere_view::edit::EditCommand;
use pencere_view::{Cursor, Event, Id, Paint, Point};
use serde_json::json;

use crate::headless::{Calls, Error, Failure, Files, Focus, Headless};

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
                .and_then(|images| images.bitmap(&picture.src, picture.full()))
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
