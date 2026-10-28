//! Pages under the host with no screen, as a test of an app writes them: the example app of
//! containers (`examples/containers-lua`) with its program played by a table of calls, and short
//! scripts for what a person does and sees. Heavy: the interpreter is compiled and run
//! (`make check-heavy`).

use std::collections::HashMap;
use std::path::PathBuf;

use pencere_editor::UNDO_PAUSE;
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
                fill = { name = { text = "draft.md", select = { from = 0, to = 5 } } },
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
            Paint::Figure(figure) => match figure.paths[0].segments[0] {
                pencere_view::Segment::Move(at) => Some(at),
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
