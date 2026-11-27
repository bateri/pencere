//! Pages with `@pencere/ui` of the kit's library, driven by the host with no screen: each test draws
//! a component on a page that writes down the actions the context gave it (the element `log`, a
//! line each), and does what a person does to it — clicks, keys, drags — then reads what the page
//! says, what it is to a screen reader and where the keyboard is. Heavy: the interpreter is compiled
//! and run (`make check-heavy`).

use pencere_sandbox::Fault;
use pencere_view::{AccessibleRole, Mods};

use crate::audit::Flaw;
use crate::headless::{Error, Focus, Headless};

/// What every test page begins with: the library, and the page's parts left for the test to give
/// (`options` for `ui.new`, `view(u)` the element the test draws, `on_action(act, u)` a reply to an
/// action).
const HEAD: &str = r#"
local ui = require("@pencere/ui")
local options, view, on_action, on_event, finish = nil, nil, nil, nil, nil
local S = {}
"#;

/// What every test page ends with: the context, the log of actions, and the page.
const TAIL: &str = r#"
local u = ui.new(options)
local log = {}
local function describe(act)
    local words = { tostring(act.from), act.kind }
    local names = {}
    for name in act do
        if name ~= "from" and name ~= "kind" and type(act[name]) ~= "table" then
            table.insert(names, name)
        end
    end
    table.sort(names)
    for _, name in names do
        table.insert(words, name .. "=" .. tostring(act[name]))
    end
    return table.concat(words, " ")
end
return {
    render = function(size, now, reduce_motion)
        u:begin(size, now, reduce_motion)
        local logged = pencere.div():id("log")
        for _, line in log do
            logged:child(pencere.text(line))
        end
        local root = pencere.div():size_full():p(8):gap(8):family("terminal")
            :child(view(u))
            :child(pencere.text("keys at " .. tostring(u:target()) .. " in " .. u:keyboard()):id("where"))
            :child(logged)
        if finish then
            finish(u, root)
        end
        return u:finish(root)
    end,
    settle = function()
        return u:settle()
    end,
    handle = function(event)
        local reply, actions = u:handle(event)
        for _, act in actions do
            table.insert(log, describe(act))
            if on_action then
                reply = pencere.merge(reply, on_action(act, u))
            end
            reply = pencere.merge(reply, true)
        end
        if on_event then
            reply = pencere.merge(reply, on_event(event, u))
        end
        return reply
    end,
}
"#;

/// A test page: `body` sets `view` (and `options`, `on_action`) between the head and the tail.
fn page_of(body: &str) -> String {
    format!("{HEAD}{body}{TAIL}")
}

fn open(body: &str) -> Result<Headless, Error> {
    Headless::script(page_of(body)).size(800.0, 500.0).open()
}

/// Why a page that is to fail failed.
fn failing(body: &str) -> String {
    match open(body).map(drop) {
        Err(Error::Ended(Fault::Failed(why))) => why,
        other => panic!("expected the page to fail, it is {other:?}"),
    }
}

/// The actions the page was given, as it wrote them down.
fn log(page: &Headless) -> Vec<String> {
    page.words_in("log")
}

/// Where the page says the keyboard is: what a key goes to, and the field or editor that has it.
fn keys_at(page: &Headless) -> String {
    page.words_in("where").join("")
}

/// Whether the words on the page, as one text, say `text`: a text of many looks is many words.
fn said(page: &Headless, text: &str) -> bool {
    page.words().concat().contains(text)
}

/// What a screen reader is told of the element named `name`.
fn accessible<'a>(page: &'a Headless, name: &str) -> &'a pencere_view::Accessible {
    let id = pencere_view::Id::named(name);
    page.outline()
        .accessible
        .iter()
        .find(|item| item.id == Some(id))
        .unwrap_or_else(|| panic!("nothing named {name} for a screen reader"))
}

// --- The small parts --------------------------------------------------------------------------------

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_button_is_pressed_by_the_pointer_and_by_the_keys_that_have_it() -> Result<(), Error> {
    let mut page = open(
        r#"
view = function(u)
    return pencere.div():row():gap(8)
        :child(u:button("save", { title = "Save", tone = "primary", keys = "cmd+s" }))
        :child(u:button("off", { title = "Off", disabled = true }))
end
"#,
    )?;
    page.click("save")?;
    page.click("off")?;
    assert_eq!(log(&page), ["save press"]);
    page.press("tab")?;
    assert_eq!(keys_at(&page), "keys at save in page");
    page.press("enter")?;
    page.press("space")?;
    assert_eq!(log(&page), ["save press", "save press", "save press"]);
    let save = accessible(&page, "save");
    assert_eq!(save.role, AccessibleRole::Button);
    assert_eq!(save.words, "Save");
    let off = accessible(&page, "off");
    assert!(off.state.disabled);
    let tips: Vec<&str> = page
        .outline()
        .tips
        .iter()
        .map(|tip| tip.words.as_str())
        .collect();
    assert!(tips.is_empty() || tips.iter().all(|tip| tip.contains("Save")));
    Ok(())
}

/// A control is as large as the theme says and its words and padding make it, whatever its border
/// takes: the ring of a button is a border, which takes room in a box, and the padding is less by it.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_control_is_as_large_as_its_theme_says_whatever_its_border_takes() -> Result<(), Error> {
    let mut page = open(
        r#"
view = function(u)
    return pencere.div():gap(8):items_start()
        :child(pencere.text("Save"):weight(500):id("words"))
        :child(u:button("save", { title = "Save" }))
        :child(u:input("q", { hint = "Search", label = "Search" }))
        :child(u:toggle("t", { on = false, glyph = "Aa", label = "Match case" }))
end
"#,
    )?;
    let sizes = |page: &Headless| {
        let size = |name: &str| {
            let rect = page.bounds(name).unwrap_or_else(|| panic!("no {name}"));
            (rect.w, rect.h)
        };
        (size("words"), size("save"), size("q"), size("t"))
    };
    let (words, button, input, toggle) = sizes(&page);
    // The regular theme: a control is 26 points tall, 10 of padding each side of a button's words,
    // and a toggle 4 points less tall than a control.
    assert_eq!(button, (words.0 + 20.0, 26.0));
    assert_eq!(input.1, 26.0);
    assert_eq!(toggle.1, 22.0);
    // The ring a key gives the button changes the colour of its border, not what it takes.
    page.press("tab")?;
    assert_eq!(sizes(&page), (words, button, input, toggle));
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_button_says_its_keys_in_its_tip_as_the_platform_writes_them() -> Result<(), Error> {
    let page = open(
        r#"
view = function(u)
    return u:button("save", { title = "Save", keys = "cmd+s" })
end
"#,
    )?;
    let tips: Vec<&str> = page
        .outline()
        .tips
        .iter()
        .map(|tip| tip.words.as_str())
        .collect();
    assert!(
        tips.iter()
            .any(|tip| tip.contains("Save") && tip.contains("⌘S")),
        "{tips:?}"
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn an_icon_button_is_named_in_words_and_one_that_stays_down_is_a_switch() -> Result<(), Error> {
    let why = failing(
        r#"
view = function(u)
    return u:icon_button("close", { icon = "close" })
end
"#,
    );
    assert!(why.contains("says what it does in words"), "{why}");
    let mut page = open(
        r#"
S.shown = false
view = function(u)
    return pencere.div():row()
        :child(u:icon_button("close", { icon = "close", label = "Close" }))
        :child(u:icon_button("preview", { icon = "dot", label = "Preview", on = S.shown, keys = "cmd+k" }))
end
on_action = function(act)
    if act.from == "preview" then
        S.shown = not S.shown
    end
end
"#,
    )?;
    assert_eq!(accessible(&page, "close").role, AccessibleRole::Button);
    assert_eq!(accessible(&page, "close").words, "Close");
    assert_eq!(accessible(&page, "preview").role, AccessibleRole::Switch);
    assert_eq!(accessible(&page, "preview").state.checked, Some(false));
    page.click("preview")?;
    assert_eq!(accessible(&page, "preview").state.checked, Some(true));
    page.click("close")?;
    assert_eq!(log(&page), ["preview press", "close press"]);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_toggle_says_what_it_is_to_become_and_is_checked_as_the_page_keeps_it() -> Result<(), Error> {
    let mut page = open(
        r#"
S.case = false
view = function(u)
    return u:toggle("case", { on = S.case, glyph = "Aa", label = "Match case", keys = "alt+cmd+c" })
end
on_action = function(act)
    S.case = act.on
end
"#,
    )?;
    let case = accessible(&page, "case");
    assert_eq!(case.role, AccessibleRole::Checkbox);
    assert_eq!(case.state.checked, Some(false));
    assert!(page.says("Aa"));
    page.click("case")?;
    assert_eq!(accessible(&page, "case").state.checked, Some(true));
    page.press("tab")?;
    page.press("space")?;
    assert_eq!(log(&page), ["case toggle on=true", "case toggle on=false"]);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_banner_answers_with_the_key_of_its_button_and_is_dismissed() -> Result<(), Error> {
    let mut page = open(
        r#"
view = function(u)
    return u:banner("conflict", {
        tone = "warn",
        icon = "warning",
        words = "a.lua changed on disk",
        actions = { { key = "reload", title = "Reload", primary = true }, { key = "keep", title = "Keep mine" } },
        dismiss = true,
    })
end
"#,
    )?;
    assert!(page.says("a.lua changed on disk"));
    page.click("conflict/action/reload")?;
    page.click("conflict/action/keep")?;
    page.click("conflict/dismiss")?;
    assert_eq!(
        log(&page),
        [
            "conflict press key=reload",
            "conflict press key=keep",
            "conflict dismiss"
        ]
    );
    assert_eq!(accessible(&page, "conflict/dismiss").words, "Dismiss");
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_badge_the_caps_of_a_shortcut_and_a_rule_are_drawn_and_answer_nothing() -> Result<(), Error> {
    let page = open(
        r#"
view = function(u)
    return pencere.div():row():gap(6)
        :child(u:badge(12, { tone = "info", round = true }))
        :child(u:badge("draft"))
        :child(u:rule("y"))
        :child(u:kbd("cmd+shift+k"))
        :child(u:kbd("ctrl+enter", { quiet = true }))
end
"#,
    )?;
    let words = page.words();
    for word in ["12", "draft", "⇧", "⌘", "K", "⌃Enter"] {
        assert!(words.contains(&word), "{word} in {words:?}");
    }
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn what_a_component_is_given_wrong_is_an_error_at_the_line_of_the_page() {
    let tone = failing(
        r#"
view = function(u)
    return u:button("save", { title = "Save", tone = "loud" })
end
"#,
    );
    assert!(tone.contains("a button's tone is"), "{tone}");
    assert!(tone.contains(":7:"), "the line of the page: {tone}");
    let named = failing("view = function(u) return u:button(5, { title = \"Five\" }) end\n");
    assert!(named.contains("a button is named by a string"), "{named}");
    let twice = failing(
        "view = function(u) return pencere.div():child(u:button('a', { title = 'A' })):child(u:button('a', { title = 'B' })) end\n",
    );
    assert!(
        twice.contains("two components on the page are named a"),
        "{twice}"
    );
    let glyph =
        failing("view = function(u) return u:button('a', { title = 'A', icon = 'sparkle' }) end\n");
    assert!(glyph.contains("there is no glyph sparkle"), "{glyph}");
    let theme = failing(
        "options = { theme = { loud = 'red' } }\nview = function(u) return pencere.div() end\n",
    );
    assert!(theme.contains("a theme has no token loud"), "{theme}");
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_component_is_drawn_in_render_alone() -> Result<(), Error> {
    let mut page = open(
        r#"
view = function(u)
    return u:button("go", { title = "Go" })
end
on_action = function(act, u)
    u:button("late", { title = "Late" })
end
"#,
    )?;
    match page.click("go") {
        Err(Error::Ended(Fault::Failed(why))) => {
            assert!(why.contains("drawn in render"), "{why}");
        }
        other => panic!("expected the page to fail, it is {other:?}"),
    }
    Ok(())
}

// --- The context ----------------------------------------------------------------------------------

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_library_does_not_hand_a_page_the_way_it_makes_its_components() -> Result<(), Error> {
    let page = open(
        r#"
view = function(u)
    return pencere.text("component is " .. tostring(ui.component))
end
"#,
    )?;
    assert!(page.says("component is nil"), "{:?}", page.words());
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn what_a_component_keeps_by_its_name_is_forgotten_when_it_is_not_drawn() -> Result<(), Error> {
    let mut page = open(
        r#"
S.shown = true
S.items = { { key = "a", title = "Item a" }, { key = "b", title = "Item b" } }
view = function(u)
    local root = pencere.div():gap(4):child(u:button("hide", { title = "Hide" }))
    if S.shown then
        root:child(u:list("results", { items = S.items, height = 24 }))
    end
    return root
end
on_action = function(act)
    if act.from == "hide" then
        S.shown = not S.shown
    end
end
"#,
    )?;
    page.click("results/row/b")?;
    assert!(accessible(&page, "results/row/b").state.selected);
    page.click("hide")?;
    assert!(!page.says("Item b"));
    page.click("hide")?;
    assert!(
        !accessible(&page, "results/row/b").state.selected,
        "drawn again, it starts again"
    );
    Ok(())
}

/// A layer: what is inside it takes the keyboard while it is in front.
const SHEET: &str = r#"
S.open = false
view = function(u)
    local root = pencere.div():gap(8)
        :child(u:button("open", { title = "Open" }))
        :child(u:button("after", { title = "After" }))
    if S.open then
        root:child(u:modal("sheet", { label = "Sheet", width = 300 }, pencere.div():p(12):gap(8)
            :child(u:button("first", { title = "First" }))
            :child(u:button("second", { title = "Second" }))))
    end
    return root
end
on_action = function(act)
    if act.from == "open" then
        S.open = true
    elseif act.from == "sheet" and act.kind == "dismiss" then
        S.open = false
    end
end
"#;

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_layer_takes_the_keyboard_holds_it_and_gives_it_back_when_it_closes() -> Result<(), Error> {
    let mut page = open(SHEET)?;
    page.press("tab")?;
    assert_eq!(keys_at(&page), "keys at open in page");
    page.press("enter")?;
    // A render has no reply: the keyboard goes in with the kit's timer, at the next frame.
    page.step()?;
    assert_eq!(keys_at(&page), "keys at first in page");
    page.press("tab")?;
    assert_eq!(keys_at(&page), "keys at second in page");
    page.press("tab")?;
    assert_eq!(
        keys_at(&page),
        "keys at first in page",
        "round within the layer, never out to the page behind it"
    );
    page.press("shift+tab")?;
    assert_eq!(keys_at(&page), "keys at second in page");
    page.press("escape")?;
    page.step()?;
    assert_eq!(log(&page), ["open press", "sheet dismiss"]);
    assert!(!page.says("First"));
    assert_eq!(keys_at(&page), "keys at open in page", "back where it was");
    assert_eq!(page.focus(), Focus::Page);
    Ok(())
}

// --- The controls -----------------------------------------------------------------------------------

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn an_input_is_a_labelled_field_that_tells_what_is_typed_and_holds_what_the_page_says()
-> Result<(), Error> {
    let why = failing(
        r#"
view = function(u)
    return u:input("q", { hint = "Search" })
end
"#,
    );
    assert!(why.contains("says what it is in words"), "{why}");
    let mut page = open(
        r#"
S.q = { value = "start" }
view = function(u)
    return pencere.div():gap(6)
        :child(u:input("q", { state = S.q, hint = "Search", label = "Search in files",
            trail = { u:toggle("q:case", { on = false, glyph = "Aa", label = "Match case" }) } }))
        :child(u:button("clear", { title = "Clear" }))
end
on_action = function(act, u)
    if act.from == "clear" then
        S.q.value = ""
    elseif act.from == "q" and act.kind == "submit" then
        S.sent = act.text
    end
end
"#,
    )?;
    assert_eq!(
        page.field("q/field"),
        Some("start"),
        "the page's text is in the field"
    );
    let fields = &page.outline().fields;
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].label.as_deref(), Some("Search in files"));
    assert_eq!(fields[0].hint.as_deref(), Some("Search"));
    page.click("q/field")?;
    assert_eq!(keys_at(&page), "keys at q/field in q/field");
    page.type_text("!")?;
    assert_eq!(page.field("q/field"), Some("start!"));
    assert!(
        log(&page).contains(&"q change text=start!".to_string()),
        "{:?}",
        log(&page)
    );
    page.press("enter")?;
    assert_eq!(
        log(&page).last().map(String::as_str),
        Some("q submit text=start!")
    );
    page.click("clear")?;
    // A render has no reply: what puts the text in the field is the kit's timer, at the next frame.
    page.step()?;
    assert_eq!(
        page.field("q/field"),
        Some(""),
        "what the page says again is put in the field"
    );
    Ok(())
}

/// An input of lines is the field itself in a box of the theme: its ground, border and padding are the
/// field element's own (so that the host scrolls the text in the padding), and with `rows` it is as tall
/// as that many of the host's lines, its padding and its border.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn an_input_of_lines_is_as_tall_as_its_rows_and_its_field_is_the_box() -> Result<(), Error> {
    let page = open(
        r#"
view = function(u)
    return pencere.div():row():gap(8):items_stretch():w(400)
        :child(pencere.div():col():gap(8):w(190)
            :child(u:input("one", { kind = "lines", rows = 1, label = "One" }))
            :child(u:input("three", { kind = "lines", rows = 3, label = "Three" }))
            :child(u:input("plain", { kind = "lines", label = "Plain" }))
            :child(u:input("line", { label = "Line" })))
        :child(pencere.div():col():gap(8):w(190)
            :child(u:input("lead", { kind = "lines", rows = 2, label = "Lead", lead = pencere.text("@") }))
            :child(pencere.div():h(150)))
end
"#,
    )?;
    let fields = &page.outline().fields;
    let field = |name: &str| {
        fields
            .iter()
            .find(|field| field.id == pencere_view::Id::named(name))
            .unwrap_or_else(|| panic!("no field {name}"))
    };
    let (one, three) = (field("one/field"), field("three/field"));
    // The text box is the lines and nothing else: three lines are three times one.
    let line = one.text.h;
    assert!(line > 0.0);
    assert_eq!(three.text.h, 3.0 * line);
    // The padding (4 above and below, 7 at the sides) and the border (1) are the field's own: the
    // padding box holds the text box and the border is outside it.
    assert_eq!(
        (
            one.text.x - one.padding_box.x,
            one.text.y - one.padding_box.y
        ),
        (7.0, 4.0)
    );
    assert_eq!(one.padding_box.h, line + 8.0);
    assert_eq!(one.rect.h, line + 10.0);
    assert_eq!(three.rect.h, 3.0 * line + 10.0);
    // The element that carries the name is as large as the field, whatever is around it.
    for name in ["one", "three"] {
        assert_eq!(
            page.bounds(name),
            Some(field(&format!("{name}/field")).rect)
        );
    }
    // Without rows the box is as tall as a control, as the field of a line is.
    assert_eq!(field("plain/field").rect.h, 26.0);
    assert_eq!(page.bounds("line").map(|rect| rect.h), Some(26.0));
    // With a lead the box is around the field, and rows are the field's: no least height.
    assert_eq!(field("lead/field").text.h, 2.0 * line);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn rows_are_for_an_input_of_lines() {
    let why = failing(
        r#"
view = function(u)
    return u:input("q", { label = "Search", rows = 3 })
end
"#,
    );
    assert!(
        why.contains("only an input of kind \"lines\" has rows"),
        "{why}"
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn segments_choose_one_and_move_with_the_arrow_keys_as_radio_buttons_do() -> Result<(), Error> {
    let mut page = open(
        r#"
S.view = "list"
view = function(u)
    return u:segments("view", { value = S.view, items = {
        { key = "list", title = "List" }, { key = "grid", title = "Grid" }, { key = "tree", title = "Tree" } } })
end
on_action = function(act)
    if act.kind == "select" then
        S.view = act.key
    end
end
"#,
    )?;
    assert_eq!(
        accessible(&page, "view/item/list").role,
        AccessibleRole::Radio
    );
    assert_eq!(
        accessible(&page, "view/item/list").state.checked,
        Some(true)
    );
    assert_eq!(
        accessible(&page, "view/item/grid").state.checked,
        Some(false)
    );
    page.click("view/item/tree")?;
    assert_eq!(log(&page), ["view select key=tree"]);
    assert_eq!(
        accessible(&page, "view/item/tree").state.checked,
        Some(true)
    );
    page.press("left")?;
    page.press("left")?;
    assert_eq!(
        log(&page)[1..],
        ["view select key=grid", "view select key=list"]
    );
    assert_eq!(keys_at(&page), "keys at view/item/list in page");
    page.press("left")?;
    assert_eq!(log(&page).len(), 3, "no change at the end");
    page.click("view/item/list")?;
    assert_eq!(
        log(&page).len(),
        3,
        "the chosen one chosen again is no change"
    );
    Ok(())
}

const TABS: &str = r#"
S.tabs = {
    { key = "a.lua", title = "a.lua", closable = true },
    { key = "b.lua", title = "b.lua", closable = true, dirty = true },
    { key = "c.lua", title = "c.lua", closable = true },
}
S.active = "a.lua"
view = function(u)
    return u:tabs("tabs", { items = S.tabs, active = S.active, reorder = true })
end
on_action = function(act, u)
    if act.kind == "select" then
        S.active = act.key
    elseif act.kind == "close" then
        for index, tab in S.tabs do
            if tab.key == act.key then
                table.remove(S.tabs, index)
                break
            end
        end
    elseif act.kind == "move" then
        local from
        for index, tab in S.tabs do
            if tab.key == act.key then
                from = index
            end
        end
        local tab = table.remove(S.tabs, from)
        local to = #S.tabs + 1
        for index, other in S.tabs do
            if other.key == act.before then
                to = index
            end
        end
        table.insert(S.tabs, to, tab)
    elseif act.from == "tabs" and act.kind == "context" then
        return u:menu("tab-menu", { { key = "close", title = "Close" }, "-", { key = "rename", title = "Rename" } },
            { at = act.at })
    end
end
"#;

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn tabs_choose_close_move_and_ask_for_a_menu() -> Result<(), Error> {
    let mut page = open(TABS)?;
    assert_eq!(
        accessible(&page, "tabs/tab/a.lua").role,
        AccessibleRole::Tab
    );
    assert!(accessible(&page, "tabs/tab/a.lua").state.selected);
    assert!(!accessible(&page, "tabs/tab/c.lua").state.selected);
    page.click("tabs/tab/c.lua")?;
    assert_eq!(log(&page), ["tabs select key=c.lua"]);
    assert!(accessible(&page, "tabs/tab/c.lua").state.selected);
    // Where the pointer rests a tab shows its cross; with changes unsaved a dot stands in its place.
    page.hover("tabs/tab/c.lua")?;
    page.click("tabs/close/c.lua")?;
    assert_eq!(log(&page)[1], "tabs close key=c.lua");
    assert!(page.bounds("tabs/tab/c.lua").is_none());
    page.drag("tabs/tab/a.lua", "tabs/tab/b.lua")?;
    assert!(
        log(&page).contains(&"tabs move key=a.lua".to_string()),
        "{:?}",
        log(&page)
    );
    page.right_click("tabs/tab/b.lua")?;
    assert!(page.menu().is_some());
    page.pick_titled("Rename")?;
    assert_eq!(
        log(&page).last().map(String::as_str),
        Some("tab-menu menu entry=rename")
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_tab_that_carries_is_handed_to_the_systems_drag_when_it_leaves_the_view() -> Result<(), Error> {
    use pencere_view::Point;
    let mut page = open(
        &TABS.replace(
            r#"{ key = "a.lua", title = "a.lua", closable = true },"#,
            r#"{ key = "a.lua", title = "a.lua", closable = true, carry = { text = "https://example.com/a", url = "https://example.com/a" } },"#,
        ),
    )?;
    page.drag_out("tabs/tab/b.lua", Point::new(5000.0, 400.0))?;
    assert!(
        page.carries().is_empty(),
        "a tab with no carry hands nothing out"
    );
    page.drag_out("tabs/tab/a.lua", Point::new(5000.0, 400.0))?;
    let [carry] = page.carries() else {
        panic!("one drag out of the view: {:?}", page.carries());
    };
    assert_eq!(
        carry.carry,
        pencere_view::Carry::text("https://example.com/a").url("https://example.com/a")
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn tabs_move_among_themselves_with_the_arrow_keys() -> Result<(), Error> {
    let mut page = open(TABS)?;
    page.press("tab")?;
    assert_eq!(keys_at(&page), "keys at tabs/tab/a.lua in page");
    page.press("right")?;
    assert_eq!(log(&page), ["tabs select key=b.lua"]);
    assert_eq!(keys_at(&page), "keys at tabs/tab/b.lua in page");
    page.press("end")?;
    assert_eq!(log(&page)[1], "tabs select key=c.lua");
    Ok(())
}

// --- The layers ---------------------------------------------------------------------------------------

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_modal_with_more_than_the_page_is_tall_stays_on_the_page_and_its_list_gives_way()
-> Result<(), Error> {
    // A card of a title and a list that scrolls, whose rows come to more than the page: the page
    // has not worked out how tall the list may be.
    let mut page = open(
        r#"
view = function(u)
    local list = pencere.div():gap(2)
    for index = 1, 60 do
        list:child(pencere.text("row " .. index):h(18):no_shrink())
    end
    return pencere.div():child(u:modal("long", { width = 400, label = "Long", top = 40 },
        pencere.div():p(16):gap(8)
            :child(pencere.text("A long list"))
            :child(pencere.div():id("rows"):scroll():child(list))))
end
"#,
    )?;
    // The card has come in.
    page.advance(0.5)?;
    let card = page.bounds("long/card").expect("the card");
    assert_eq!(card.y, 40.0);
    assert!(
        card.y + card.h <= 500.0,
        "the card ends at {} on a page of 500",
        card.y + card.h
    );
    let rows = page.bounds("rows").expect("the list");
    assert!(
        rows.y + rows.h <= card.y + card.h,
        "the list ends at {} and the card at {}",
        rows.y + rows.h,
        card.y + card.h
    );
    assert_eq!(page.spills(), [], "nothing stands past the card");
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_modal_is_closed_by_its_scrim_and_by_escape_and_not_by_a_click_on_its_card() -> Result<(), Error>
{
    let mut page = open(
        r#"
S.open = false
view = function(u)
    local root = pencere.div():gap(8):child(u:button("open", { title = "Open" }))
    if S.open then
        root:child(u:modal("settings", { width = 400, label = "Settings" },
            pencere.div():p(16):gap(8)
                :child(pencere.text("Settings"))
                :child(u:input("name", { label = "Name" }))
                :child(u:button("ok", { title = "OK" }))))
    end
    return root
end
on_action = function(act)
    if act.from == "open" then
        S.open = true
    elseif act.from == "settings" and act.kind == "dismiss" then
        S.open = false
    end
end
"#,
    )?;
    page.click("open")?;
    page.step()?;
    assert!(page.says("Settings"));
    assert_eq!(
        page.focus(),
        Focus::Field(pencere_view::Id::named("name/field")),
        "the keyboard goes into the first thing in it that takes it"
    );
    page.click("settings/card")?;
    assert_eq!(
        log(&page),
        ["open press"],
        "a click on the card is not one on the scrim"
    );
    page.click("settings/scrim")?;
    page.step()?;
    assert_eq!(log(&page), ["open press", "settings dismiss"]);
    assert!(!page.says("Settings"));
    assert_eq!(page.focus(), Focus::Page, "back where it was");
    page.click("open")?;
    page.step()?;
    page.press("escape")?;
    page.step()?;
    assert_eq!(
        log(&page).last().map(String::as_str),
        Some("settings dismiss")
    );
    Ok(())
}

const DIALOG: &str = r#"
S.asking = false
view = function(u)
    local root = pencere.div():gap(8):child(u:button("open", { title = "Open" }))
    if S.asking then
        root:child(u:dialog("unsaved", {
            title = "Save changes to a.lua?", message = "Your changes will be lost.",
            input = S.named and { value = "a.lua", hint = "File name", label = "File name" } or nil,
            actions = { { key = "drop", title = "Don't Save", danger = true }, { key = "save", title = "Save", primary = true } } }))
    end
    return root
end
finish = function(u, root)
    S.cmds:write(u, root)
end
S.cmds = ui.commands({
    { id = "save", title = "Save", keys = "mod+s", group = "File" },
    { id = "help", title = "Help", keys = "f1", global = true },
})
on_action = function(act)
    if act.from == "open" then
        S.asking = true
    elseif act.from == "unsaved" and act.kind == "answer" then
        S.asking = false
    elseif act.from == "commands" and act.id == "save" then
        S.saved = (S.saved or 0) + 1
    end
end
"#;

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_dialog_is_answered_by_a_button_by_enter_and_by_giving_up() -> Result<(), Error> {
    let mut page = open(DIALOG)?;
    page.click("open")?;
    page.step()?;
    assert_eq!(keys_at(&page), "keys at unsaved/action/save in page");
    page.press("enter")?;
    page.step()?;
    assert_eq!(log(&page), ["open press", "unsaved answer key=save"]);
    assert!(!page.says("Don't Save"));
    page.click("open")?;
    page.step()?;
    page.click("unsaved/action/drop")?;
    assert_eq!(log(&page)[2..], ["open press", "unsaved answer key=drop"]);
    page.click("open")?;
    page.step()?;
    page.press("escape")?;
    assert_eq!(
        log(&page).last().map(String::as_str),
        Some("unsaved answer")
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_layer_lists_its_way_out_and_is_given_up_by_it_as_by_escape() -> Result<(), Error> {
    // The dialog lists "Close" (what a screen reader's actions and `bateri view command` do); doing it on
    // the card gives the dialog up as escape does.
    let mut page = open(DIALOG)?;
    page.click("open")?;
    page.step()?;
    let text = page.reading().text();
    assert!(
        text.contains("alertdialog \"Save changes to a.lua?\" [action \"Close\"]"),
        "{text}"
    );
    page.command_on("unsaved/card", "ui.dismiss")?;
    page.step()?;
    assert_eq!(
        log(&page).last().map(String::as_str),
        Some("unsaved answer")
    );
    assert!(!page.says("Don't Save"), "the dialog is gone");
    // The same "Close", done with no element named, as a terminal's list of commands does it where the keyboard
    // is in the card: it gives up the layer in front.
    let mut kept = open(
        r#"
S.asking = false
view = function(u)
    local root = pencere.div():child(u:button("open", { title = "Open" }))
    if S.asking then
        root:child(u:modal("m", { label = "Pick" }, pencere.div():focused(true):child(pencere.text("Inside"))))
    end
    return root
end
on_action = function(act)
    if act.from == "open" then
        S.asking = true
    elseif act.from == "m" and act.kind == "dismiss" then
        S.asking = false
    end
end
"#,
    )?;
    kept.click("open")?;
    kept.step()?;
    assert!(kept.says("Inside"));
    kept.command("ui.dismiss")?;
    kept.step()?;
    assert!(!kept.says("Inside"), "the layer is given up");
    // A modal that cannot be given up lists no way out.
    let mut modal = open(
        r#"
view = function(u)
    return pencere.div():child(u:button("open", { title = "Open" }))
        :child(u:modal("m", { label = "Pick", dismiss = false }, pencere.div():child(pencere.text("Inside"))))
end
"#,
    )?;
    modal.step()?;
    let text = modal.reading().text();
    assert!(
        text.contains("dialog \"Pick\"") && !text.contains("action"),
        "{text}"
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_dialog_and_a_modal_take_the_width_of_their_card() -> Result<(), Error> {
    let sized = DIALOG.replace(
        "title = \"Save changes to a.lua?\",",
        "width = 300, title = \"Save changes to a.lua?\",",
    );
    let mut page = open(&sized)?;
    page.click("open")?;
    page.step()?;
    let card = page.bounds("unsaved/card").expect("the card");
    assert_eq!(card.w, 300.0, "{card:?}");
    // A modal takes the width as a dialog does.
    let mut modal = open(
        r#"
S.on = true
view = function(u)
    local root = pencere.div():child(u:button("open", { title = "Open" }))
    if S.on then
        root:child(u:modal("m", { label = "Pick", width = 260 },
            pencere.div():child(pencere.text("Inside"))))
    end
    return root
end
"#,
    )?;
    modal.step()?;
    let card = modal.bounds("m/card").expect("the card");
    assert_eq!(card.w, 260.0, "{card:?}");
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_dialog_with_an_input_answers_with_what_is_typed() -> Result<(), Error> {
    let mut page = open(&DIALOG.replace("S.asking = false", "S.asking = false\nS.named = true"))?;
    page.click("open")?;
    page.step()?;
    assert_eq!(
        page.focus(),
        Focus::Field(pencere_view::Id::named("unsaved/input/field"))
    );
    assert_eq!(page.field("unsaved/input/field"), Some("a.lua"));
    page.type_text("x")?;
    page.press("enter")?;
    assert_eq!(
        log(&page).last().map(String::as_str),
        Some("unsaved answer key=save value=a.luax")
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_commands_of_the_page_are_not_done_while_a_modal_is_in_front_unless_they_are_global()
-> Result<(), Error> {
    let mut page = open(DIALOG)?;
    let enabled = |page: &Headless| -> Vec<String> {
        page.outline()
            .commands
            .iter()
            .filter(|command| command.enabled)
            .map(|command| command.title.clone())
            .collect()
    };
    assert_eq!(enabled(&page), ["Save", "Help"]);
    page.command("save")?;
    assert_eq!(log(&page), ["commands command id=save"]);
    page.click("open")?;
    page.step()?;
    assert_eq!(
        enabled(&page),
        ["Help"],
        "the dialog is in front: only what is global"
    );
    assert!(page.command("save").is_err());
    page.press("escape")?;
    page.step()?;
    assert_eq!(enabled(&page), ["Save", "Help"]);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn toasts_are_shown_for_their_time_and_closed_with_their_cross() -> Result<(), Error> {
    let mut page = open(
        r#"
S.items = {}
view = function(u)
    ui.toasts_prune(S.items, u.now)
    return pencere.div():gap(8)
        :child(u:button("say", { title = "Say" }))
        :child(u:toasts("toasts", { items = S.items }))
end
on_action = function(act)
    if act.from == "say" then
        ui.toast(S.items, "Saved a.lua", "ok", 2)
        ui.toast(S.items, "Disk is full", "bad", 0)
    elseif act.kind == "close" then
        for index, item in S.items do
            if item.key == act.key then
                table.remove(S.items, index)
                break
            end
        end
    end
end
"#,
    )?;
    page.click("say")?;
    page.advance(0.3)?;
    assert!(page.says("Saved a.lua") && page.says("Disk is full"));
    page.advance(2.5)?;
    assert!(!page.says("Saved a.lua"), "it ran out");
    assert!(page.says("Disk is full"), "one with no time stays");
    page.click("toasts/close/2")?;
    assert!(!page.says("Disk is full"));
    assert_eq!(
        log(&page).last().map(String::as_str),
        Some("toasts close key=2")
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_menu_is_asked_of_the_host_at_an_element_and_what_is_picked_comes_back() -> Result<(), Error> {
    let mut page = open(
        r#"
view = function(u)
    return u:button("more", { title = "More" })
end
on_action = function(act, u)
    if act.from == "more" then
        return u:menu("more-menu", { { key = "copy", title = "Copy", keys = "cmd+c" }, "-",
            { key = "paste", title = "Paste", enabled = false }, { key = "delete", title = "Delete" } },
            { near = "more" })
    end
end
"#,
    )?;
    page.click("more")?;
    let menu = page.menu().expect("a menu");
    assert_eq!(menu.entries.len(), 4);
    page.pick_titled("Delete")?;
    assert_eq!(log(&page), ["more press", "more-menu menu entry=delete"]);
    Ok(())
}

/// A menu by an element that is not laid out opens at the place the page gave, and at none when it
/// gave none; with neither it is a mistake.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_menu_by_an_element_that_is_not_laid_out_opens_at_its_place_or_not_at_all() -> Result<(), Error>
{
    let script = |options: &str| {
        format!(
            r#"
view = function(u)
    return u:button("more", {{ title = "More" }})
end
on_action = function(act, u)
    if act.from == "more" then
        return u:menu("more-menu", {{ {{ key = "copy", title = "Copy" }} }}, {options})
    end
end
"#
        )
    };
    let mut page = open(&script(r#"{ near = "gone", at = { x = 30, y = 40 } }"#))?;
    page.click("more")?;
    assert!(page.menu().is_some(), "it opens at the place");
    let mut page = open(&script(r#"{ near = "gone" }"#))?;
    page.click("more")?;
    assert!(page.menu().is_none(), "there is no place for it");
    assert_eq!(log(&page), ["more press"], "and the page goes on");
    let mut page = open(&script("{}"))?;
    match page.click("more") {
        Err(Error::Ended(Fault::Failed(why))) => {
            assert!(why.contains("a menu is at an element"), "{why}");
        }
        other => panic!("expected the page to fail, it is {other:?}"),
    }
    Ok(())
}

/// The host shows one menu at a time and does not show one asked for while another is up: what is
/// picked is a line of the menu shown, whichever menu the page asked for last.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_pick_of_a_menu_is_a_line_of_that_menu_when_the_page_asked_for_another_since()
-> Result<(), Error> {
    let mut page = open(
        r#"
view = function(u)
    return u:button("more", { title = "More" })
end
on_action = function(act, u)
    if act.from == "more" then
        pencere.time.after(1, function()
            return u:menu("late-menu", { { key = "wipe", title = "Wipe" }, "-", { key = "rename", title = "Rename" },
                { key = "burn", title = "Burn" }, { key = "bury", title = "Bury" } }, { at = { x = 1, y = 1 } })
        end)
        return u:menu("more-menu", { { key = "copy", title = "Copy" }, "-", { key = "paste", title = "Paste" },
            { key = "delete", title = "Delete" } }, { near = "more" })
    end
end
"#,
    )?;
    page.click("more")?;
    page.advance(2.0)?;
    let shown = page.menu().expect("the first menu is up");
    assert_eq!(shown.entries.len(), 4, "the second was not shown");
    page.pick_titled("Delete")?;
    assert_eq!(
        log(&page).last().map(String::as_str),
        Some("more-menu menu entry=delete")
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_popover_stands_by_its_anchor_and_a_press_outside_it_closes_it() -> Result<(), Error> {
    let mut page = open(
        r#"
S.open = false
view = function(u)
    local root = pencere.div():gap(8):p(20)
        :child(u:button("anchor", { title = "Anchor" }))
        :child(u:button("elsewhere", { title = "Elsewhere" }))
    if S.open then
        root:child(u:popover("card", { anchor = "anchor", side = "below" }, pencere.div():p(8):child(pencere.text("Hello"))))
    end
    return root
end
on_action = function(act)
    if act.from == "anchor" then
        S.open = not S.open
    elseif act.from == "card" and act.kind == "dismiss" then
        S.open = false
    end
end
"#,
    )?;
    page.click("anchor")?;
    page.advance(0.3)?;
    assert!(page.says("Hello"));
    let card = page.bounds("card").expect("the card");
    let anchor = page.bounds("anchor").expect("the anchor");
    assert!(card.y >= anchor.y + anchor.h, "below its anchor");
    assert_eq!(page.focus(), Focus::Page, "the keyboard stays where it was");
    page.click("elsewhere")?;
    assert!(!page.says("Hello"));
    assert_eq!(
        log(&page),
        ["anchor press", "card dismiss", "elsewhere press"]
    );
    page.click("anchor")?;
    page.press("escape")?;
    assert!(!page.says("Hello"));
    page.click("anchor")?;
    page.click("anchor")?;
    assert!(
        !page.says("Hello"),
        "the anchor's own press toggles it, and does not close then open it"
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_popover_by_a_field_stays_at_a_click_into_that_field_and_goes_at_one_into_another()
-> Result<(), Error> {
    let mut page = open(
        r#"
S.open = true
view = function(u)
    local root = pencere.div():gap(8):p(20)
        :child(u:input("search", { label = "Search" }))
        :child(u:input("other", { label = "Other" }))
    if S.open then
        root:child(u:popover("card", { anchor = "search", side = "right" }, pencere.div():p(8):child(pencere.text("Hello"))))
    end
    return root
end
on_action = function(act)
    if act.from == "card" and act.kind == "dismiss" then
        S.open = false
    end
end
"#,
    )?;
    page.advance(0.3)?;
    assert!(page.says("Hello"));
    page.click("search/field")?;
    assert!(
        page.says("Hello"),
        "a click into its own field is on its anchor"
    );
    page.click("other/field")?;
    assert!(
        !page.says("Hello"),
        "a click into another field is outside it"
    );
    assert_eq!(log(&page), ["card dismiss"]);
    Ok(())
}

/// A page with an editor and a card by the word the pointer rests on: the card is a popover that stands
/// by the range the page named, goes at what its `dismiss` lists, and waits `hide_delay` after the
/// pointer left the word.
const CARD: &str = r#"
local ui = require("@pencere/ui")
local text = "alpha beta\ngamma delta"
local u = ui.new()
local card = nil
local version = 0
local function layer()
	return { decorate = "card", version = version, from = 1, to = #text + 1, decorations = card and {
		{ from = card.from, to = card.to, anchor = "card" } } or {} }
end
local function render(size, now, reduce_motion)
	u:begin(size, now, reduce_motion)
	local root = pencere.div():size_full():child(
		pencere.editor("code", { gutter = false, events = { "hover" }, hover = { delay = 0.2 } }):size_full())
	if card then
		root:child(u:popover("card", { anchor = pencere.range_of("code", "card"), focus = true,
			dismiss = card.dismiss }, pencere.div():p(8):child(pencere.text("About " .. card.word))))
	end
	return u:finish(root)
end
local function handle(event)
	local reply, actions = u:handle(event)
	if event.type == "editor_ready" then
		return pencere.merge(reply, { editor = { code = { { load = text } } } })
	elseif event.type == "edit" then
		version = event.version
	elseif event.type == "hover" and event.kind == "dwell" then
		card = { from = event.from, to = event.to, dismiss = DISMISS,
			word = pencere.layout.editor_text("code", event.from, event.to) }
		return pencere.merge(reply, { redraw = true, editor = { code = { layer() } } })
	end
	for _, act in actions do
		if act.from == "card" and act.kind == "dismiss" then
			card = nil
			reply = pencere.merge(reply, { redraw = true, editor = { code = { layer() } } })
		end
	end
	return reply
end
return { render = render, handle = handle, settle = function() return u:settle() end }
"#;

fn card_page(dismiss: &str) -> Result<Headless, Error> {
    Headless::script(format!("DISMISS = {dismiss}\n{CARD}"))
        .size(800.0, 500.0)
        .open()
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_popover_by_a_range_of_an_editor_waits_for_the_pointer_to_come_back_and_goes_when_the_text_changes()
-> Result<(), Error> {
    let mut page = card_page("nil")?;
    page.dwell("code", 12.0, 12.0)?;
    page.advance(0.3)?;
    assert!(page.says("About alpha"), "{:?}", page.words());
    let word = page.anchor_rect("code", "card").expect("the range");
    let card = page.bounds("card").expect("the card");
    assert_eq!(card.x, word.x);
    assert!(card.y >= word.bottom(), "{card:?} {word:?}");
    // The pointer goes off the word: the card stays the delay.
    page.move_to(pencere_view::Point::new(600.0, 400.0))?;
    page.advance(0.2)?;
    assert!(page.says("About alpha"), "the card waits");
    page.advance(0.2)?;
    assert!(!page.says("About alpha"), "and goes: {:?}", page.words());
    // The pointer goes onto the card in that time: it stays.
    page.dwell("code", 12.0, 12.0)?;
    page.advance(0.3)?;
    assert!(page.says("About alpha"));
    page.move_to(pencere_view::Point::new(600.0, 400.0))?;
    page.advance(0.1)?;
    page.hover("card")?;
    page.advance(0.5)?;
    assert!(page.says("About alpha"), "the pointer is on the card");
    // And goes off the card: then it goes after the delay... the card's own leaving is the page's
    // to tell; the text changing under it is the library's.
    page.focus_editor("code")?;
    page.insert_text("x")?;
    assert!(
        !page.says("About alpha"),
        "an edit closes it: {:?}",
        page.words()
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_popover_that_lists_what_dismisses_it_is_dismissed_by_that_alone() -> Result<(), Error> {
    let mut page = card_page(r#"{ "escape" }"#)?;
    page.dwell("code", 12.0, 12.0)?;
    page.advance(0.3)?;
    assert!(page.says("About alpha"));
    page.move_to(pencere_view::Point::new(600.0, 400.0))?;
    page.advance(1.0)?;
    assert!(page.says("About alpha"), "leaving does not close it");
    page.focus_editor("code")?;
    page.insert_text("x")?;
    assert!(page.says("About alpha"), "nor does an edit");
    let why = failing(
        r#"
view = function(u)
    return u:popover("card", { anchor = pencere.range_of("code", "x"), dismiss = { "never" } }, pencere.text("x"))
end
"#,
    );
    assert!(why.contains("is dismissed by"), "{why}");
    Ok(())
}

// --- The lists ----------------------------------------------------------------------------------------

const LIST: &str = r#"
S.items = {}
for index = 1, 200 do
    table.insert(S.items, { key = "k" .. index, title = "Item " .. index })
end
S.state = { cursor = nil, marks = {} }
view = function(u)
    return pencere.div():size_full():gap(4)
        :child(u:button("before", { title = "Before" }))
        :child(u:list("results", { items = S.items, state = S.state, height = 24, multi = true,
            key_map = S.vim and { j = "down", k = "up" } or nil,
            count = S.count }))
        :child(pencere.text(S.marked or "none marked"))
end
on_action = function(act)
    if act.kind == "mark" then
        S.marked = #act.marked .. " marked"
    end
end
"#;

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_list_builds_the_rows_in_view_and_is_chosen_and_opened_with_the_pointer() -> Result<(), Error> {
    let mut page = open(LIST)?;
    assert!(page.bounds("results/row/k1").is_some());
    assert!(
        page.bounds("results/row/k150").is_none(),
        "only the rows near what is in view are built"
    );
    assert_eq!(
        accessible(&page, "results/row/k1").role,
        AccessibleRole::ListOption
    );
    page.click("results/row/k2")?;
    assert_eq!(log(&page), ["results select how=pointer key=k2"]);
    assert!(accessible(&page, "results/row/k2").state.selected);
    page.double_click("results/row/k3")?;
    assert!(
        log(&page).contains(&"results open how=pointer key=k3".to_string()),
        "{:?}",
        log(&page)
    );
    page.right_click("results/row/k4")?;
    assert_eq!(
        log(&page).last().map(String::as_str),
        Some("results context key=k4")
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_list_is_asked_to_show_a_row_that_is_not_built_and_builds_it_and_moves_its_cursor_there()
-> Result<(), Error> {
    let mut page = open(LIST)?;
    assert!(page.bounds("results/row/k150").is_none());
    // The row 150 of the 200: not built, and not where the pointer or the keys went.
    page.reveal("results", Some(150))?;
    assert!(
        page.bounds("results/row/k150").is_some(),
        "the row asked for is built and in view"
    );
    assert!(accessible(&page, "results/row/k150").state.selected);
    assert_eq!(
        accessible(&page, "results/row/k150").state.posinset,
        Some(150)
    );
    assert_eq!(
        accessible(&page, "results/row/k150").state.setsize,
        Some(200)
    );
    assert!(
        log(&page).contains(&"results select how=reveal key=k150".to_string()),
        "{:?}",
        log(&page)
    );
    // A row there is none of is not an error: nothing moves.
    page.reveal("results", Some(500))?;
    assert!(accessible(&page, "results/row/k150").state.selected);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_list_is_named_by_its_fallback_and_not_by_the_rows_built_past_the_edge_of_what_it_shows()
-> Result<(), Error> {
    let mut page = open(LIST)?;
    let reading = page.reading();
    let boxes: Vec<_> = reading
        .items
        .iter()
        .filter(|item| item.role == "listbox")
        .collect();
    assert_eq!(boxes.len(), 1, "{}", reading.text());
    assert_eq!(boxes[0].words, "List", "{}", reading.text());
    assert!(reading.audit().is_empty(), "{:?}", reading.audit());
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_list_is_shown_the_row_that_is_the_target_of_a_reveal_and_a_grid_its_row() -> Result<(), Error>
{
    let mut page = open(LIST)?;
    // The row itself is asked for, as a screen reader's move to it is: by its name.
    page.reveal("results/row/k2", None)?;
    assert!(
        log(&page).contains(&"results select how=reveal key=k2".to_string()),
        "{:?}",
        log(&page)
    );
    // A grid shows the row, in the column the cursor is in, by its place and by its row.
    let mut page = open(GRID)?;
    assert!(page.bounds("g/row/250").is_none());
    page.reveal("g", Some(250))?;
    assert!(
        page.bounds("g/row/250").is_some(),
        "the row asked for is built"
    );
    assert_eq!(log(&page), ["g select column=1 how=reveal row=250"]);
    // A row that is built is a target by its name too.
    page.reveal("g/row/248", None)?;
    assert_eq!(
        log(&page).last().map(String::as_str),
        Some("g select column=1 how=reveal row=248")
    );
    page.reveal("g", Some(9999))?;
    assert_eq!(log(&page).len(), 2, "a row there is none of moves nothing");
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_command_done_on_an_element_reaches_the_page_with_the_element() -> Result<(), Error> {
    let mut page = open(
        r#"
view = function(u)
    return pencere.div():gap(8)
        :child(u:button("stop", { title = "Stop" }):command("kill", "Kill", {}))
end
"#,
    )?;
    page.command_on("stop", "kill")?;
    assert_eq!(log(&page), ["commands command id=kill target=stop"]);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_list_with_the_keyboard_goes_by_the_arrow_keys_and_keeps_the_row_in_view() -> Result<(), Error>
{
    let mut page = open(LIST)?;
    page.press("tab")?;
    page.press("tab")?;
    assert_eq!(keys_at(&page), "keys at results in page");
    page.press("down")?;
    page.press("down")?;
    assert_eq!(
        log(&page),
        [
            "results select how=key key=k1",
            "results select how=key key=k2"
        ]
    );
    page.press("enter")?;
    assert_eq!(
        log(&page).last().map(String::as_str),
        Some("results open how=key key=k2")
    );
    page.press("end")?;
    page.settle()?;
    let list = page.bounds("results").expect("the list");
    let last = page
        .bounds("results/row/k200")
        .expect("the last row is built when it is the cursor");
    assert!(
        last.y >= list.y && last.y + last.h <= list.y + list.h + 0.5,
        "{last:?} in {list:?}"
    );
    page.press("home")?;
    page.settle()?;
    assert!(page.bounds("results/row/k1").is_some());
    page.press("page_down")?;
    let moved = log(&page).last().cloned().unwrap();
    assert!(
        moved.starts_with("results select how=key key=k") && !moved.ends_with("k2"),
        "{moved}"
    );
    page.press("space")?;
    assert!(
        log(&page).last().unwrap().starts_with("results mark"),
        "{:?}",
        log(&page).last()
    );
    assert!(page.says("1 marked"), "the keys marked are in `marked`");
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_list_is_given_the_keys_a_page_maps_and_asks_for_rows_it_has_not_read() -> Result<(), Error> {
    let mut page = open(&LIST.replace("S.items = {}", "S.vim = true\nS.items = {}"))?;
    page.press("tab")?;
    page.press("tab")?;
    page.press("j")?;
    page.press("j")?;
    page.press("k")?;
    assert_eq!(
        log(&page).last().map(String::as_str),
        Some("results select how=key key=k1")
    );
    let mut lazy = open(
        &LIST
            .replace("for index = 1, 200 do", "for index = 1, 30 do")
            .replace("S.state =", "S.count = 1000\nS.state ="),
    )?;
    lazy.scroll_to("results", 2000.0)?;
    let asked = log(&lazy)
        .into_iter()
        .find(|line| line.starts_with("results need"))
        .expect("a need");
    assert!(
        asked.contains("first=") && asked.contains("last="),
        "{asked}"
    );
    Ok(())
}

// --- A palette ----------------------------------------------------------------------------------------

const PALETTE: &str = r#"
S.open = false
S.items = {
    { key = "save", title = "Save File", hint = "File", keys = "mod+s" },
    { key = "save_all", title = "Save All Files", hint = "File" },
    { key = "close", title = "Close Tab", hint = "View", keys = "mod+w" },
    { key = "find", title = "Find in Files", hint = "Search", keys = "mod+shift+f" },
    { key = "off", title = "Turned Off", enabled = false },
}
S.state = { query = "", cursor = nil }
view = function(u)
    local root = pencere.div():gap(8):child(u:button("open", { title = "Open" }))
    if S.open then
        root:child(u:palette("palette", { items = S.items, state = S.state, label = "Command palette",
            hint = "Type a command", footer = { { keys = "enter", title = "Run" } } }))
    end
    return root
end
on_action = function(act)
    if act.from == "open" then
        S.open = true
    elseif act.from == "palette" and (act.kind == "pick" or act.kind == "dismiss") then
        S.open = false
    end
end
"#;

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_palette_finds_by_what_is_typed_moves_with_the_arrows_and_picks_with_enter() -> Result<(), Error>
{
    let mut page = open(PALETTE)?;
    page.click("open")?;
    page.step()?;
    page.advance(0.3)?;
    assert_eq!(
        page.focus(),
        Focus::Field(pencere_view::Id::named("palette/input/field"))
    );
    assert_eq!(
        page.outline().fields[0].label.as_deref(),
        Some("Command palette")
    );
    assert!(said(&page, "Save File") && said(&page, "Close Tab"));
    assert!(
        accessible(&page, "palette/list/row/save").state.selected,
        "the first is chosen"
    );
    page.type_text("sf")?;
    page.advance(0.3)?;
    assert!(said(&page, "Save File") && said(&page, "Save All Files"));
    assert!(
        !page.says("Close Tab"),
        "what the query is not in is not shown"
    );
    page.press("down")?;
    assert!(
        accessible(&page, "palette/list/row/save_all")
            .state
            .selected
    );
    page.press("enter")?;
    page.step()?;
    assert_eq!(
        log(&page)
            .iter()
            .filter(|line| line.starts_with("palette pick"))
            .count(),
        1,
        "{:?}",
        log(&page)
    );
    assert!(!page.says("Command palette"));
    assert_eq!(page.focus(), Focus::Page);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_palette_row_the_pointer_comes_to_is_chosen_where_it_is_and_the_list_does_not_scroll()
-> Result<(), Error> {
    let many = PALETTE.replace(
        "S.state = { query = \"\", cursor = nil }",
        "for index = 1, 40 do\n    table.insert(S.items, { key = \"k\" .. index, title = \"Command \" .. index })\nend\nS.state = { query = \"\", cursor = nil }",
    );
    let mut page = open(&many)?;
    page.click("open")?;
    page.step()?;
    page.advance(0.3)?;
    let list = page.bounds("palette/list").expect("the palette's list");
    let first = page
        .bounds("palette/list/row/save")
        .expect("the first row")
        .y;
    // The pointer comes to the foot of the list, where the last row in view is cut by its edge, and
    // goes a point at a time down it, as a hand does.
    for step in 0..6 {
        let at = pencere_view::Point::new(list.x + 40.0, list.y + list.h - 6.0 + step as f32);
        page.move_to(at)?;
        page.advance(0.05)?;
    }
    assert_eq!(
        page.bounds("palette/list/row/save").map(|row| row.y),
        Some(first),
        "the list stays where it is under the pointer"
    );
    let chosen = page
        .outline()
        .accessible
        .iter()
        .filter(|item| item.state.selected)
        .count();
    assert_eq!(chosen, 1, "the row under the pointer is chosen");
    // The keys still bring the row they choose into view.
    for _ in 0..20 {
        page.press("down")?;
    }
    page.advance(0.1)?;
    assert_ne!(
        page.bounds("palette/list/row/save").map(|row| row.y),
        Some(first),
        "the arrows scroll to the row they choose"
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_palette_picks_with_a_click_gives_up_with_escape_and_says_back_in_an_empty_field()
-> Result<(), Error> {
    let mut page = open(PALETTE)?;
    page.click("open")?;
    page.step()?;
    page.advance(0.3)?;
    page.press("backspace")?;
    assert_eq!(log(&page).last().map(String::as_str), Some("palette back"));
    page.type_text("c")?;
    page.press("backspace")?;
    assert_eq!(
        log(&page).last().map(String::as_str),
        Some("palette query text="),
        "{:?}",
        log(&page)
    );
    page.click("palette/list/row/find")?;
    assert!(
        log(&page)
            .iter()
            .any(|line| line.starts_with("palette pick")),
        "{:?}",
        log(&page)
    );
    page.click("open")?;
    page.step()?;
    page.advance(0.3)?;
    page.click_in("palette/card", 4.0, 4.0)?;
    page.press("escape")?;
    page.step()?;
    assert_eq!(
        log(&page).last().map(String::as_str),
        Some("palette dismiss")
    );
    page.click("open")?;
    page.step()?;
    page.advance(0.3)?;
    page.type_text("zzzz")?;
    page.advance(0.3)?;
    assert!(page.says("No results"));
    page.press("enter")?;
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_palette_picks_the_row_the_pointer_rests_on_with_one_click() -> Result<(), Error> {
    // The page writes down the key of the item it is given, which the log leaves out (a table).
    let source = PALETTE.replace(
        "        S.open = false\n    end\nend",
        "        S.open = false\n        if act.kind == \"pick\" then\n            S.picked = act.item.key\n        end\n    end\nend\nfinish = function(_, root)\n    root:child(pencere.text(\"picked \" .. tostring(S.picked)):id(\"picked\"))\nend",
    );
    assert_ne!(source, PALETTE, "the page writes down what it picks");
    let mut page = open(&source)?;
    let picks = |page: &Headless| {
        log(page)
            .iter()
            .filter(|line| line.starts_with("palette pick"))
            .count()
    };
    let picked = |page: &Headless| page.words_in("picked").join("");
    // The first row is chosen as the palette opens: one click on it picks it.
    page.click("open")?;
    page.step()?;
    page.advance(0.3)?;
    page.click("palette/list/row/save")?;
    page.step()?;
    assert_eq!(picks(&page), 1, "{:?}", log(&page));
    assert_eq!(picked(&page), "picked save");
    assert!(page.bounds("palette/card").is_none(), "the palette is gone");
    // The pointer that comes to a row chooses it, as a hand does before it clicks: the click then
    // picks it, and not only a second one.
    page.click("open")?;
    page.step()?;
    page.advance(0.3)?;
    page.hover("palette/list/row/close")?;
    assert!(accessible(&page, "palette/list/row/close").state.selected);
    page.click("palette/list/row/close")?;
    page.step()?;
    assert_eq!(picks(&page), 2, "{:?}", log(&page));
    assert_eq!(picked(&page), "picked close");
    assert!(page.bounds("palette/card").is_none(), "the palette is gone");
    // A double click picks once: the first press picks and the palette is gone at the second.
    page.click("open")?;
    page.step()?;
    page.advance(0.3)?;
    page.hover("palette/list/row/find")?;
    page.double_click("palette/list/row/find")?;
    page.step()?;
    assert_eq!(picks(&page), 3, "{:?}", log(&page));
    assert_eq!(picked(&page), "picked find");
    // A row that cannot be done now is not picked by the pointer either.
    page.click("open")?;
    page.step()?;
    page.advance(0.3)?;
    page.hover("palette/list/row/off")?;
    page.click("palette/list/row/off")?;
    page.step()?;
    assert_eq!(picks(&page), 3, "{:?}", log(&page));
    assert!(page.bounds("palette/card").is_some(), "the palette stays");
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_palette_searches_a_long_list_a_step_at_a_frame() -> Result<(), Error> {
    let mut page = open(
        &PALETTE
            .replace(
                "S.state = { query = \"\", cursor = nil }",
                "for index = 1, 6000 do table.insert(S.items, { key = \"i\" .. index, title = \"Entry number \" .. index }) end\nS.state = { query = \"\", cursor = nil }",
            ),
    )?;
    page.click("open")?;
    page.step()?;
    page.advance(0.3)?;
    page.type_text("number 5999")?;
    page.advance(1.0)?;
    assert!(
        said(&page, "Entry ") && said(&page, "5999"),
        "{:?}",
        page.words()
    );
    Ok(())
}

// --- A picker -----------------------------------------------------------------------------------------

/// A page with a control that opens a picker of labels by it: the page holds what is on, a toggle
/// turns it, and a pick or a dismissal closes the card (`MULTI` and `SEARCH` say which picker).
const PICKER: &str = r#"
S.open = false
S.items = {
    { key = "bug", title = "Bug", on = true },
    { key = "api", title = "API" },
    { key = "ui", title = "Interface" },
    { key = "off", title = "Turned Off", enabled = false },
}
S.state = { query = "" }
view = function(u)
    local root = pencere.div():gap(8):p(20)
        :child(u:button("labels", { title = "Labels" }))
        :child(u:button("elsewhere", { title = "Elsewhere" }))
        :child(u:input("other", { label = "Other" }))
    if S.open then
        root:child(u:picker("picker", { anchor = "labels", items = S.items, multi = MULTI, search = SEARCH,
            state = S.state, label = "Labels", hint = "Change labels" }))
    end
    return root
end
on_action = function(act)
    if act.from == "labels" then
        S.open = not S.open
        S.state = { query = "" }
    elseif act.from == "picker" and act.kind == "toggle" then
        for _, item in S.items do
            if item.key == act.item.key then
                item.on = act.on
            end
        end
    elseif act.from == "picker" and (act.kind == "pick" or act.kind == "dismiss") then
        if act.kind == "pick" then
            for _, item in S.items do
                item.on = item.key == act.item.key
            end
        end
        S.open = false
    end
end
"#;

fn picker(multi: bool, search: bool) -> Result<Headless, Error> {
    open(&format!("MULTI = {multi}\nSEARCH = {search}\n{PICKER}"))
}

/// The lines of the log that are the picker's.
fn picked(page: &Headless) -> Vec<String> {
    log(page)
        .into_iter()
        .filter(|line| line.starts_with("picker"))
        .collect()
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_picker_of_several_turns_each_on_and_off_and_stays_open() -> Result<(), Error> {
    let mut page = picker(true, true)?;
    page.click("labels")?;
    page.step()?;
    page.advance(0.3)?;
    let card = page.bounds("picker").expect("the card");
    let anchor = page.bounds("labels").expect("the anchor");
    assert!(card.y >= anchor.y + anchor.h, "below its anchor");
    assert_eq!(
        page.focus(),
        Focus::Field(pencere_view::Id::named("picker/input/field")),
        "the keyboard is in its field"
    );
    assert_eq!(
        accessible(&page, "picker/list/row/bug").state.checked,
        Some(true)
    );
    assert_eq!(
        accessible(&page, "picker/list/row/api").state.checked,
        Some(false)
    );
    assert!(
        accessible(&page, "picker/list/row/bug").state.selected,
        "the first is chosen"
    );
    page.click("picker/list/row/api")?;
    page.click("picker/list/row/bug")?;
    assert_eq!(
        picked(&page),
        ["picker toggle on=true", "picker toggle on=false"]
    );
    assert!(page.says("Interface"), "the card stays");
    assert_eq!(
        accessible(&page, "picker/list/row/api").state.checked,
        Some(true)
    );
    assert_eq!(
        accessible(&page, "picker/list/row/bug").state.checked,
        Some(false)
    );
    assert_eq!(page.bounds("picker"), Some(card), "it does not move");
    assert_eq!(
        page.focus(),
        Focus::Field(pencere_view::Id::named("picker/input/field")),
        "the keyboard stays in its field"
    );
    // What is typed narrows the rows, and enter turns the one the cursor is on.
    page.type_text("int")?;
    page.advance(0.3)?;
    assert!(!page.says("API"), "what the query is not in is not shown");
    page.press("enter")?;
    assert_eq!(
        picked(&page).last().map(String::as_str),
        Some("picker toggle on=true")
    );
    assert_eq!(
        accessible(&page, "picker/list/row/ui").state.checked,
        Some(true)
    );
    // What was typed is selected after enter: the next word replaces it. An item that is off is not
    // chosen.
    page.type_text("turned")?;
    page.advance(0.3)?;
    assert_eq!(
        picked(&page).last().map(String::as_str),
        Some("picker query text=turned")
    );
    page.press("enter")?;
    page.click("picker/list/row/off")?;
    let toggles = |page: &Headless| {
        picked(page)
            .into_iter()
            .filter(|line| line.starts_with("picker toggle"))
            .count()
    };
    assert_eq!(toggles(&page), 3, "{:?}", picked(&page));
    page.press("escape")?;
    assert_eq!(
        picked(&page).last().map(String::as_str),
        Some("picker dismiss")
    );
    assert!(page.bounds("picker").is_none(), "the card is gone");
    page.step()?;
    assert_eq!(page.focus(), Focus::Page, "the keyboard is back");
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_picker_goes_at_a_press_outside_it_and_its_anchor_is_the_pages() -> Result<(), Error> {
    let mut page = picker(true, true)?;
    page.click("labels")?;
    page.advance(0.3)?;
    page.click("elsewhere")?;
    assert!(page.bounds("picker").is_none());
    assert_eq!(
        log(&page),
        ["labels press", "picker dismiss", "elsewhere press"]
    );
    page.click("labels")?;
    page.advance(0.3)?;
    page.click("labels")?;
    assert!(
        page.bounds("picker").is_none(),
        "the anchor's own press closes it, and does not close then open it"
    );
    assert_eq!(picked(&page), ["picker dismiss"]);
    // A click into a field outside it is a press outside it, which the host keeps.
    page.click("labels")?;
    page.advance(0.3)?;
    page.click("other/field")?;
    assert!(page.bounds("picker").is_none(), "the card is gone");
    assert_eq!(picked(&page), ["picker dismiss", "picker dismiss"]);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_picker_of_one_marks_the_one_on_and_a_pick_closes_it() -> Result<(), Error> {
    let mut page = picker(false, true)?;
    page.click("labels")?;
    page.advance(0.3)?;
    assert_eq!(
        accessible(&page, "picker/list/row/bug").state.checked,
        None,
        "a row of one is no box"
    );
    page.press("down")?;
    page.press("enter")?;
    assert_eq!(picked(&page), ["picker pick"]);
    assert!(page.bounds("picker").is_none());
    page.click("labels")?;
    page.advance(0.3)?;
    page.click("picker/list/row/ui")?;
    assert_eq!(picked(&page), ["picker pick", "picker pick"]);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_picker_with_no_field_gives_its_list_the_keyboard_and_the_space_bar_chooses()
-> Result<(), Error> {
    let mut page = picker(true, false)?;
    page.click("labels")?;
    page.step()?;
    page.advance(0.3)?;
    assert!(page.bounds("picker/input").is_none(), "no field of its own");
    assert_eq!(keys_at(&page), "keys at picker/list in page");
    page.press("down")?;
    page.press("space")?;
    page.press("enter")?;
    assert_eq!(
        picked(&page),
        ["picker toggle on=true", "picker toggle on=false"]
    );
    Ok(())
}

/// The ground of the row chosen stands inside the card on every side, inside its border and the rows'
/// inset, its corners the card's less the border and the inset, so that it does not stand out of the
/// card's round corners.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_row_chosen_in_a_picker_stands_inside_the_round_corners_of_its_card() -> Result<(), Error> {
    let mut page = picker(false, false)?;
    page.click("labels")?;
    page.step()?;
    page.advance(0.3)?;
    let card = page.bounds("picker").expect("the card");
    let row = page.bounds("picker/list/row/bug").expect("the first row");
    assert!(row.y >= card.y + 5.0, "{row:?} in {card:?}");
    assert!(
        row.x >= card.x + 5.0 && row.x + row.w <= card.x + card.w - 5.0,
        "{row:?} in {card:?}"
    );
    let ground = page.list().items.iter().find_map(|item| match item {
        pencere_view::Paint::Quad(quad) if quad.rect == row && quad.fill.is_some() => Some(quad),
        _ => None,
    });
    assert_eq!(
        ground.expect("the ground of the row chosen").radii,
        [5.0; 4]
    );
    Ok(())
}

// --- Completions --------------------------------------------------------------------------------------

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn completions_stand_by_the_caret_and_take_their_keys_only_while_they_are_open() -> Result<(), Error>
{
    let mut page = open(
        r#"
S.items = {}
view = function(u)
    return pencere.div():size_full():gap(6)
        :child(u:button("show", { title = "Show" }))
        :child(pencere.editor("doc", {}):w_full():h(120))
        :child(u:completions("complete", { editor = "doc", items = S.items }))
end
on_action = function(act)
    if act.from == "show" then
        S.items = { { key = "print", title = "print" }, { key = "pairs", title = "pairs" }, { key = "pcall", title = "pcall" } }
    elseif act.from == "complete" then
        S.items = {}
    end
end
"#,
    )?;
    let keys_of = |page: &Headless| -> Vec<String> {
        page.outline()
            .commands
            .iter()
            .map(|command| command.title.clone())
            .collect()
    };
    assert!(
        keys_of(&page).is_empty(),
        "no keys of its own while there is nothing to complete"
    );
    page.click("show")?;
    page.focus_editor("doc")?;
    page.advance(0.3)?;
    assert!(page.says("pairs"));
    assert!(
        keys_of(&page)
            .iter()
            .any(|title| title == "Next completion")
    );
    page.press("down")?;
    assert!(accessible(&page, "complete/list/row/pairs").state.selected);
    page.press("enter")?;
    assert_eq!(log(&page).last().map(String::as_str), Some("complete pick"));
    assert!(!page.says("pairs"));
    assert!(keys_of(&page).is_empty());
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_completion_the_pointer_rests_on_is_picked_with_one_click() -> Result<(), Error> {
    let mut page = open(
        r#"
S.items = {}
view = function(u)
    return pencere.div():size_full():gap(6)
        :child(u:button("show", { title = "Show" }))
        :child(pencere.editor("doc", {}):w_full():h(120))
        :child(u:completions("complete", { editor = "doc", items = S.items }))
        :child(pencere.text("picked " .. tostring(S.picked)):id("picked"))
end
on_action = function(act)
    if act.from == "show" then
        S.items = { { key = "print", title = "print" }, { key = "pairs", title = "pairs" }, { key = "pcall", title = "pcall" } }
    elseif act.from == "complete" then
        if act.kind == "pick" then
            S.picked = act.item.key
        end
        S.items = {}
    end
end
"#,
    )?;
    let picks = |page: &Headless| {
        log(page)
            .iter()
            .filter(|line| line.starts_with("complete pick"))
            .count()
    };
    let picked = |page: &Headless| page.words_in("picked").join("");
    // The first is chosen as the card comes: one click on it picks it.
    page.click("show")?;
    page.focus_editor("doc")?;
    page.advance(0.3)?;
    page.click("complete/list/row/print")?;
    assert_eq!(picks(&page), 1, "{:?}", log(&page));
    assert_eq!(picked(&page), "picked print");
    assert!(!page.says("pairs"));
    // The pointer that comes to one chooses it; the click then picks it.
    page.click("show")?;
    page.focus_editor("doc")?;
    page.advance(0.3)?;
    page.hover("complete/list/row/pcall")?;
    assert!(accessible(&page, "complete/list/row/pcall").state.selected);
    page.click("complete/list/row/pcall")?;
    assert_eq!(picks(&page), 2, "{:?}", log(&page));
    assert_eq!(picked(&page), "picked pcall");
    assert!(!page.says("pairs"));
    Ok(())
}

// --- Tree, split, status ------------------------------------------------------------------------------

const TREE: &str = r#"
S.kids = {
    src = { "src/a.lua", "src/lib", "src/z.lua" },
    ["src/lib"] = { "src/lib/x.lua", "src/lib/y.lua" },
}
S.state = { expanded = {}, cursor = nil }
view = function(u)
    return pencere.div():size_full()
        :child(u:tree("files", { roots = { "src" }, state = S.state, height = 24,
            children = function(key) return S.kids[key] end,
            item = function(key) return { title = string.match(key, "[^/]+$"), badge = key == "src/z.lua" and "M" or nil } end,
            key_map = S.vim and { j = "down", k = "up", h = "left", l = "right" } or nil }))
end
"#;

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_tree_opens_and_closes_folders_with_the_pointer_and_the_arrow_keys() -> Result<(), Error> {
    let mut page = open(TREE)?;
    assert!(page.says("src") && !page.says("a.lua"));
    assert_eq!(
        accessible(&page, "files/list/row/src").role,
        AccessibleRole::TreeItem
    );
    assert_eq!(
        accessible(&page, "files/list/row/src").state.expanded,
        Some(false)
    );
    page.click("files/chevron/src")?;
    assert_eq!(log(&page), ["files expand key=src open=true"]);
    assert!(page.says("a.lua") && page.says("lib"));
    assert_eq!(
        accessible(&page, "files/list/row/src").state.expanded,
        Some(true)
    );
    assert_eq!(
        accessible(&page, "files/list/row/src/a.lua").state.expanded,
        None
    );
    // A press on a file chooses it, a second opens it.
    page.click("files/list/row/src/a.lua")?;
    page.double_click("files/list/row/src/z.lua")?;
    assert!(
        log(&page).contains(&"files select how=pointer key=src/a.lua".to_string()),
        "{:?}",
        log(&page)
    );
    assert!(
        log(&page).contains(&"files open how=pointer key=src/z.lua".to_string()),
        "{:?}",
        log(&page)
    );
    // A press on a folder opens it.
    page.click("files/list/row/src/lib")?;
    assert!(page.says("x.lua"));
    page.press("left")?;
    assert_eq!(
        log(&page).last().map(String::as_str),
        Some("files expand key=src/lib open=false")
    );
    assert!(!page.says("x.lua"));
    page.press("left")?;
    assert_eq!(
        log(&page).last().map(String::as_str),
        Some("files select how=key key=src")
    );
    page.press("right")?;
    assert_eq!(
        log(&page).last().map(String::as_str),
        Some("files select how=key key=src/a.lua"),
        "right on an open folder goes into it"
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_tree_is_moved_with_the_keys_a_page_maps() -> Result<(), Error> {
    let mut page = open(&TREE.replace("S.state =", "S.vim = true\nS.state ="))?;
    page.click("files/list/row/src")?;
    assert!(page.says("a.lua") && !page.says("x.lua"));
    page.press("j")?;
    page.press("j")?;
    assert_eq!(
        log(&page).last().map(String::as_str),
        Some("files select how=key key=src/lib")
    );
    page.press("l")?;
    assert!(page.says("x.lua"));
    page.press("h")?;
    assert!(!page.says("x.lua"));
    page.press("k")?;
    assert_eq!(
        log(&page).last().map(String::as_str),
        Some("files select how=key key=src/a.lua")
    );
    Ok(())
}

const SPLIT: &str = r#"
S.side = { size = 240 }
S.drawer = false
view = function(u)
    S.side.drawer = S.drawer
    return pencere.div():size_full():gap(0)
        :child(u:button("toggle", { title = "Toggle" }))
        :child(pencere.div():grow(1):min_h(0):child(
            u:split("side", { state = S.side, min = 160, max = 400, narrow_below = S.narrow, label = "Sidebar width",
                side = S.edge,
                first = pencere.div():id("first"):size_full():child(pencere.text("Sidebar")),
                second = pencere.div():id("second"):size_full():child(pencere.text("Main")) })))
end
on_action = function(act)
    if act.from == "toggle" then
        S.drawer = not S.drawer
    elseif act.from == "side" and act.kind == "drawer" then
        S.drawer = act.open
    end
end
"#;

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_split_is_dragged_held_between_its_limits_and_sized_with_the_arrow_keys() -> Result<(), Error> {
    let mut page = open(SPLIT)?;
    assert_eq!(page.bounds("first").expect("pane").w, 240.0);
    let handle = accessible(&page, "side/handle").clone();
    assert_eq!(handle.role, AccessibleRole::Slider);
    assert_eq!(handle.words, "Sidebar width");
    assert_eq!(handle.state.value.as_deref(), Some("240 pt"));
    page.drag_between(
        pencere_view::Point::new(handle.rect.x + 3.0, 200.0),
        pencere_view::Point::new(handle.rect.x + 63.0, 200.0),
    )?;
    assert_eq!(page.bounds("first").expect("pane").w, 300.0);
    let said = log(&page);
    assert!(
        said.iter()
            .any(|line| line == "side resize final=false size=300"),
        "{said:?}"
    );
    assert_eq!(
        said.last().map(String::as_str),
        Some("side resize final=true size=300"),
        "let go, it says it is the last"
    );
    page.drag_between(
        pencere_view::Point::new(handle.rect.x + 63.0, 200.0),
        pencere_view::Point::new(handle.rect.x + 900.0, 200.0),
    )?;
    assert_eq!(
        page.bounds("first").expect("pane").w,
        400.0,
        "held at its limit"
    );
    let told = log(&page).len();
    page.click("side/handle")?;
    assert_eq!(
        log(&page).len(),
        told,
        "a press on the edge that does not move it says nothing"
    );
    page.press("left")?;
    assert_eq!(page.bounds("first").expect("pane").w, 384.0);
    assert_eq!(
        log(&page).last().map(String::as_str),
        Some("side resize final=true size=384")
    );
    Ok(())
}

/// Every reply to the pointer says its shape, and one that says none is the arrow: the shape a
/// component asks for is said at each move over it, at a press and a release there and at a turn of
/// the wheel, not only when the pointer comes onto it.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_shape_a_component_asks_for_is_said_at_each_input_of_the_pointer_over_it() -> Result<(), Error>
{
    use pencere_view::{Button, Cursor, Input, Mods, Point};
    let mut page = open(SPLIT)?;
    let handle = accessible(&page, "side/handle").rect;
    let at = |y: f32| Point::new(handle.x + handle.w / 2.0, y);
    for (step, y) in [180.0, 200.0, 220.0].into_iter().enumerate() {
        page.move_to(at(y))?;
        assert_eq!(page.cursor(), Some(Cursor::ResizeHorizontal), "move {step}");
    }
    page.pointer(Input::Press {
        at: at(220.0),
        button: Button::Left,
        mods: Mods::default(),
        count: 1,
    })?;
    assert_eq!(page.cursor(), Some(Cursor::ResizeHorizontal), "pressed");
    page.pointer(Input::Release {
        at: at(220.0),
        button: Button::Left,
        mods: Mods::default(),
    })?;
    assert_eq!(page.cursor(), Some(Cursor::ResizeHorizontal), "let go");
    page.wheel_at(at(220.0), 0.0, -1.0)?;
    assert_eq!(
        page.cursor(),
        Some(Cursor::ResizeHorizontal),
        "the wheel turned"
    );
    // Off it, the arrow.
    page.move_to(Point::new(handle.x + 120.0, 220.0))?;
    assert_eq!(page.cursor(), Some(Cursor::Arrow), "off the handle");
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_split_that_is_narrow_gives_its_sized_pane_as_a_drawer_over_the_other() -> Result<(), Error> {
    let mut page = open(&SPLIT.replace("S.drawer = false", "S.drawer = false\nS.narrow = 1000"))?;
    assert!(page.says("Main") && !page.says("Sidebar"));
    assert!(page.bounds("side/handle").is_none());
    page.click("toggle")?;
    page.advance(0.3)?;
    assert!(page.says("Sidebar"));
    page.click("side/scrim")?;
    assert_eq!(
        log(&page).last().map(String::as_str),
        Some("side drawer open=false")
    );
    page.advance(0.3)?;
    assert!(!page.says("Sidebar"));
    page.click("toggle")?;
    page.advance(0.3)?;
    page.press("escape")?;
    assert_eq!(
        log(&page).last().map(String::as_str),
        Some("side drawer open=false")
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_split_sized_at_its_end_grows_the_other_way() -> Result<(), Error> {
    let mut page = open(&SPLIT.replace("S.drawer = false", "S.drawer = false\nS.edge = \"end\""))?;
    assert_eq!(page.bounds("second").expect("pane").w, 240.0);
    let handle = accessible(&page, "side/handle").clone();
    page.drag_between(
        pencere_view::Point::new(handle.rect.x + 3.0, 200.0),
        pencere_view::Point::new(handle.rect.x - 37.0, 200.0),
    )?;
    assert_eq!(page.bounds("second").expect("pane").w, 280.0);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_status_bar_answers_its_items_and_drops_the_least_important_where_it_is_narrow()
-> Result<(), Error> {
    let source = r#"
view = function(u)
    return u:status("status", {
        start = { { key = "branch", words = "main", icon = "dot", press = true, priority = 9 } },
        center = { { key = "saved", words = "Saved", priority = 1 } },
        finish = {
            { key = "pos", words = "Ln 12, Col 4", press = true, priority = 6, tip = "Go to line", keys = "ctrl+g" },
            { key = "lang", words = "Lua", priority = 3 },
            { key = "enc", words = "UTF-8 with a very long name", priority = 2 },
        },
    })
end
"#;
    let mut page = open(source)?;
    assert!(page.says("main") && page.says("Saved") && page.says("Lua"));
    page.click("status/item/pos")?;
    assert_eq!(log(&page), ["status press key=pos"]);
    assert_eq!(
        accessible(&page, "status/item/branch").role,
        AccessibleRole::Button
    );
    let narrow = Headless::script(page_of(source))
        .size(320.0, 200.0)
        .open()?;
    assert!(
        narrow.says("main") && narrow.says("Ln 12, Col 4"),
        "{:?}",
        narrow.words()
    );
    assert!(
        !narrow.says("Saved") && !narrow.says("UTF-8 with a very long name"),
        "{:?}",
        narrow.words()
    );
    Ok(())
}

// --- The find bar and the grid ------------------------------------------------------------------------

const FIND: &str = r#"
S.find = nil
S.open = false
view = function(u)
    local root = pencere.div():size_full():gap(0)
        :child(u:button("open", { title = "Find" }))
    if S.open then
        root:child(u:find_bar("find", { editor = "doc", state = S.find, read_only = S.read_only }))
    end
    return root:child(pencere.editor("doc", {}):w_full():h(160):id("doc"))
end
on_action = function(act, u)
    if act.from == "open" then
        S.open = true
        S.find = S.find or { query = "", replacement = "" }
        S.find.query = "foo"
        return u:focus("find/query")
    elseif act.from == "find" and act.kind == "close" then
        S.open = false
    end
end
"#;

fn opened_find(source: &str) -> Result<Headless, Error> {
    let mut page = open(source)?;
    page.focus_editor("doc")?;
    page.insert_text("foo bar Foo\nfoo baz foofoo\n")?;
    page.click("open")?;
    page.step()?;
    page.advance(0.3)?;
    Ok(page)
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_find_bar_drives_the_editor_and_says_how_many_it_found() -> Result<(), Error> {
    let mut page = opened_find(FIND)?;
    assert_eq!(
        page.focus(),
        Focus::Field(pencere_view::Id::named("find/query/field"))
    );
    assert_eq!(
        page.field("find/query/field"),
        Some("foo"),
        "what the page seeds it with"
    );
    assert!(said(&page, "1 / 5"), "{:?}", page.words());
    page.press("enter")?;
    assert!(said(&page, "2 / 5"), "{:?}", page.words());
    page.press("shift+enter")?;
    assert!(said(&page, "1 / 5"));
    page.click("find/case")?;
    assert!(said(&page, "/ 4"), "{:?}", page.words());
    assert_eq!(accessible(&page, "find/case").state.checked, Some(true));
    page.click("find/regex")?;
    page.click("find/query/field")?;
    page.type_text("(")?;
    page.advance(0.3)?;
    assert!(said(&page, "Invalid pattern"), "{:?}", page.words());
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_find_bar_replaces_and_stops_the_search_when_it_closes() -> Result<(), Error> {
    let mut page = opened_find(FIND)?;
    page.click("find/replacing")?;
    page.click("find/replace/field")?;
    page.type_text("qux")?;
    page.click("find/replace_one")?;
    assert!(
        page.editor_text("doc").unwrap().starts_with("qux bar Foo"),
        "{:?}",
        page.editor_text("doc")
    );
    page.click("find/replace_all")?;
    assert_eq!(
        page.editor_text("doc").unwrap(),
        "qux bar qux\nqux baz quxqux\n"
    );
    page.click("find/query/field")?;
    page.press("escape")?;
    page.step()?;
    assert!(log(&page).iter().any(|line| line == "find close"));
    assert!(page.bounds("find").is_none());
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_find_bar_of_a_read_only_editor_does_not_replace() -> Result<(), Error> {
    let mut page =
        opened_find(&FIND.replace("S.open = false", "S.open = false\nS.read_only = true"))?;
    page.click("find/replacing")?;
    assert!(accessible(&page, "find/replace_one").state.disabled);
    assert!(accessible(&page, "find/replace_all").state.disabled);
    Ok(())
}

const GRID: &str = r#"
S.rows = {}
for row = 1, (S.loaded or 300) do
    S.rows[row] = { id = row, name = "name " .. row, qty = row * 7 }
end
S.state = { cursor = nil, widths = {}, sort = nil }
view = function(u)
    local columns = {
        { key = "id", title = "id", type = "int", align = "end" },
        { key = "name", title = "name", type = "text" },
        { key = "qty", title = "qty", type = "int", align = "end" },
    }
    for index = 1, (S.wide or 0) do
        table.insert(columns, { key = "c" .. index, title = "column " .. index })
    end
    return pencere.div():size_full():child(u:grid("g", { columns = columns, count = S.count or 300, state = S.state,
        numbers = S.numbers, select = S.select,
        cell = function(row, column)
            local values = S.rows[row]
            if values == nil then return nil end
            return values[column.key] ~= nil and tostring(values[column.key]) or ("v" .. row)
        end }))
end
on_action = function(act)
    if act.kind == "sort" then
        S.state.sort = { column = act.column, descending = false }
    end
end
"#;

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_grid_builds_the_rows_in_view_and_chooses_a_cell_with_the_pointer_and_the_keys()
-> Result<(), Error> {
    let mut page = open(GRID)?;
    assert!(said(&page, "name 1") && said(&page, "int"));
    assert!(
        page.bounds("g/row/250").is_none(),
        "a long grid builds the rows near what is in view"
    );
    let row = page.bounds("g/row/3").expect("a row");
    let id = page.bounds("g/head/id").expect("a column's head");
    assert!(row.x <= id.x, "the rows and the header start together");
    page.click_in("g/row/3", 160.0, 5.0)?;
    assert_eq!(log(&page), ["g select column=2 row=3"]);
    page.right_click("g/row/5")?;
    assert!(
        log(&page).last().unwrap().starts_with("g context column="),
        "{:?}",
        log(&page).last()
    );
    page.double_click("g/row/4")?;
    assert!(
        log(&page).iter().any(|line| line.starts_with("g open")),
        "{:?}",
        log(&page)
    );
    page.press("tab")?;
    page.press("down")?;
    page.press("right")?;
    let last = log(&page).last().cloned().unwrap();
    assert!(last.starts_with("g select column=3 row="), "{last}");
    page.press("home")?;
    assert!(log(&page).last().unwrap().contains("column=1"));
    page.press("page_down")?;
    page.settle()?;
    let after = log(&page).last().cloned().unwrap();
    assert!(after.starts_with("g select column=1 row="), "{after}");
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_grid_that_chooses_rows_goes_from_row_to_row_and_leaves_left_and_right_to_the_page()
-> Result<(), Error> {
    let mut page = open(&GRID.replace("S.rows = {}", "S.select = \"row\"\nS.rows = {}"))?;
    page.click_in("g/row/3", 160.0, 5.0)?;
    assert_eq!(log(&page), ["g select column=2 row=3"]);
    page.press("right")?;
    page.press("left")?;
    assert_eq!(
        log(&page).len(),
        1,
        "left and right choose nothing: {:?}",
        log(&page)
    );
    page.press("down")?;
    assert_eq!(
        log(&page).last().map(String::as_str),
        Some("g select column=2 row=4")
    );
    page.press("end")?;
    page.settle()?;
    assert_eq!(
        log(&page).last().map(String::as_str),
        Some("g select column=2 row=300")
    );
    page.press("home")?;
    page.settle()?;
    assert_eq!(
        log(&page).last().map(String::as_str),
        Some("g select column=2 row=1")
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_grid_row_is_read_by_the_words_of_its_cells_and_the_controls_in_it_apart() -> Result<(), Error>
{
    // A row with words only is said by them (its number first), not by "Row 3".
    let mut page = open(GRID)?;
    let row = accessible(&page, "g/row/3");
    assert_eq!(row.role, pencere_view::AccessibleRole::Row);
    assert_eq!(row.state.rowindex, Some(3));
    assert_eq!(row.words, "3 3 name 3 21");
    assert!(!row.state.selected);
    page.click_in("g/row/3", 40.0, 5.0)?;
    assert!(accessible(&page, "g/row/3").state.selected);
    assert!(!accessible(&page, "g/row/4").state.selected);

    // A row that holds a mark and a fold button is said by the words that are in neither, and
    // each of the two is read apart, with its state.
    let controls = GRID.replace(
        "return values[column.key] ~= nil and tostring(values[column.key]) or (\"v\" .. row)",
        "if column.key == \"id\" then
                return pencere.div():id(\"mark/\" .. row):press(\"mark/\" .. row):role(\"checkbox\")
                    :checked(row == 2):label(\"Marked\"):size(12, 12)
            elseif column.key == \"qty\" then
                return pencere.div():id(\"fold/\" .. row):press(\"fold/\" .. row):role(\"button\")
                    :expanded(false):label(\"Fold\"):size(12, 12)
            end
            return tostring(values[column.key])",
    );
    let page = open(&controls)?;
    assert_eq!(accessible(&page, "g/row/3").words, "3 name 3");
    let mark = accessible(&page, "mark/2");
    assert_eq!(mark.role, pencere_view::AccessibleRole::Checkbox);
    assert_eq!(
        (mark.words.as_str(), mark.state.checked),
        ("Marked", Some(true))
    );
    assert_eq!(accessible(&page, "mark/3").state.checked, Some(false));
    let fold = accessible(&page, "fold/3");
    assert_eq!(
        (fold.words.as_str(), fold.state.expanded),
        ("Fold", Some(false))
    );

    // A name the page gives a row wins.
    let named = open(&GRID.replace(
        "numbers = S.numbers,",
        "numbers = S.numbers, row_label = function(row) return \"Item \" .. row end,",
    ))?;
    assert_eq!(accessible(&named, "g/row/3").words, "Item 3");
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_grid_without_numbers_puts_each_head_over_its_column() -> Result<(), Error> {
    let page = open(&GRID.replace("S.rows = {}", "S.numbers = false\nS.rows = {}"))?;
    let row = page.bounds("g/row/3").expect("a row");
    let id = page.bounds("g/head/id").expect("a column's head");
    let name = page.bounds("g/head/name").expect("a column's head");
    assert_eq!(
        id.x, row.x,
        "the first head starts where the first cell does"
    );
    assert_eq!(
        name.x - id.x,
        id.w,
        "each head starts where the one before it ends"
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_grid_sorts_by_a_press_on_a_head_and_sizes_a_column_by_its_edge() -> Result<(), Error> {
    let mut page = open(GRID)?;
    page.click("g/head/name")?;
    assert_eq!(log(&page), ["g sort column=name"]);
    let head = accessible(&page, "g/head/name");
    assert_eq!(head.role, pencere_view::AccessibleRole::ColumnHeader);
    assert_eq!(head.words, "name");
    assert_eq!(head.state.sort, Some(pencere_view::Sort::Ascending));
    let before = page.bounds("g/head/qty").expect("head").x;
    let edge = page.bounds("g/edge/id").expect("an edge");
    page.drag_between(
        pencere_view::Point::new(edge.x + 3.0, edge.y + 8.0),
        pencere_view::Point::new(edge.x + 53.0, edge.y + 8.0),
    )?;
    let after = page.bounds("g/head/qty").expect("head").x;
    assert_eq!(after - before, 50.0, "the columns after it move over");
    assert!(
        log(&page)
            .iter()
            .any(|line| line.starts_with("g resize column=id final=false width=")),
        "{:?}",
        log(&page)
    );
    assert!(
        log(&page)
            .last()
            .unwrap()
            .starts_with("g resize column=id final=true width="),
        "{:?}",
        log(&page).last()
    );
    let told = log(&page).len();
    page.click("g/edge/id")?;
    assert_eq!(
        log(&page).len(),
        told,
        "a press on the edge that does not move it says nothing"
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_grid_asks_for_the_rows_it_has_not_read_and_builds_the_columns_as_it_goes_across()
-> Result<(), Error> {
    let mut lazy =
        open(&GRID.replace("S.rows = {}", "S.loaded = 40\nS.count = 4000\nS.rows = {}"))?;
    lazy.scroll_to("g/body", 6000.0)?;
    let asked = log(&lazy)
        .into_iter()
        .find(|line| line.starts_with("g need"))
        .expect("a need");
    assert!(
        asked.contains("first=") && asked.contains("last="),
        "{asked}"
    );
    let mut wide = open(&GRID.replace("S.rows = {}", "S.wide = 40\nS.rows = {}"))?;
    assert!(wide.bounds("g/head/c1").is_some());
    assert!(
        wide.bounds("g/head/c40").is_none(),
        "only the columns near what is in view"
    );
    wide.wheel("g", 6000.0, 0.0)?;
    wide.settle()?;
    assert!(
        wide.bounds("g/head/c40").is_some(),
        "{:?}",
        wide.bounds("g/head/c1")
    );
    // What a frame costs is the cells it makes, a row made at each of the columns made, and the
    // wall clock of the machine is no measure of it: the columns made are a view of them each way
    // of what is in view, however many the grid has.
    let made = (1..=40)
        .filter(|column| wide.bounds(&format!("g/head/c{column}")).is_some())
        .count();
    assert!(
        (10..=30).contains(&made),
        "{made} columns made with an 800 point view and columns 100 points wide"
    );
    Ok(())
}

/// A page whose editor opens the library's editor menu on a right press and on the command that asks
/// for it at the caret; `%OPTIONS%` is the editor's options besides the gesture, `%MENU%` the menu's.
fn editor_menu_page(options: &str, menu: &str) -> String {
    r#"
S.text = "alpha beta\ngamma delta"
local cmds = ui.commands({ ui.EDITOR_MENU_COMMAND })
view = function(u)
    return pencere.editor("code", { gutter = false, %OPTIONS% gestures = { ["right+press"] = "context" } }):grow(1)
end
finish = function(u, root)
    cmds:write(u, root)
end
on_event = function(event, u)
    if event.type == "editor_ready" then
        return { editor = { code = { { load = S.text } } } }
    elseif event.type == "press" and event.button == "right" and event.place then
        return u:editor_menu("code-menu", { editor = event.place.editor, at = { x = event.x, y = event.y }, %MENU% })
    end
end
on_action = function(act, u)
    if act.from == "commands" and act.id == "editor_menu" then
        return u:editor_menu("code-menu", { editor = "code", %MENU% })
    elseif act.from == "code-menu" then
        return u:editor_menu_pick(act)
    end
end
"#
    .replace("%OPTIONS%", options)
    .replace("%MENU%", menu)
}

fn right_press_in(page: &mut Headless, editor: &str) -> Result<(), Error> {
    let room = page.bounds(editor).expect("the editor");
    let at = pencere_view::Point::new(room.x + 6.0, room.y + 6.0);
    page.pointer(pencere_view::Input::Press {
        at,
        button: pencere_view::Button::Right,
        mods: pencere_view::Mods::default(),
        count: 1,
    })?;
    page.pointer(pencere_view::Input::Release {
        at,
        button: pencere_view::Button::Right,
        mods: Mods::default(),
    })
}

fn menu_titles(page: &Headless) -> Vec<String> {
    use pencere_view::MenuEntry;
    page.menu()
        .expect("a menu")
        .entries
        .iter()
        .map(|entry| match entry {
            MenuEntry::Item { title, enabled, .. } | MenuEntry::Paste { title, enabled, .. } => {
                format!("{title}{}", if *enabled { "" } else { " (off)" })
            }
            MenuEntry::Separator => "-".to_owned(),
        })
        .collect()
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn an_editors_menu_has_the_standard_lines_with_the_editors_keys_and_the_pages_own_around_them()
-> Result<(), Error> {
    use pencere_view::MenuEntry;
    let mut page = open(&editor_menu_page(
        "",
        r#"before = { { key = "definition", title = "Go to Definition", keys = "f12" } },
            after = { { key = "palette", title = "Command Palette" } }"#,
    ))?;
    right_press_in(&mut page, "code")?;
    assert_eq!(
        menu_titles(&page),
        [
            "Go to Definition",
            "-",
            "Cut",
            "Copy",
            "Paste",
            "-",
            "Select All",
            "-",
            "Toggle Line Comment",
            "Select All Occurrences",
            "-",
            "Command Palette"
        ]
    );
    // The keys are the editor's now, written as a command's are: ⌘ and a letter, not the word Enter.
    // The host's line of a paste has none: no key picks it.
    let keys: Vec<Option<String>> = page
        .menu()
        .expect("a menu")
        .entries
        .iter()
        .filter_map(|entry| match entry {
            MenuEntry::Item { title, keys, .. } => {
                Some((title.clone(), keys.as_ref().map(|k| format!("{k:?}"))))
            }
            MenuEntry::Paste { .. } | MenuEntry::Separator => None,
        })
        .filter(|(title, _)| ["Cut", "Copy", "Select All"].contains(&title.as_str()))
        .map(|(_, keys)| keys)
        .collect();
    assert_eq!(keys.len(), 3);
    assert!(keys.iter().all(Option::is_some), "{keys:?}");
    // Paste is the host's line; the rest are the page's.
    assert!(matches!(
        page.menu().expect("a menu").entries[4],
        MenuEntry::Paste { enabled: true, .. }
    ));
    // The page's own line comes back to the page, and the helper has nothing to ask for it.
    page.pick_titled("Go to Definition")?;
    assert_eq!(log(&page), ["code-menu menu editor=code entry=definition"]);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_line_of_the_menu_asks_the_editor_and_paste_is_the_hosts_with_its_pasteboard()
-> Result<(), Error> {
    let mut page = open(&editor_menu_page("", ""))?;
    page.select("code", 0, 5)?;
    right_press_in(&mut page, "code")?;
    page.pick_titled("Copy")?;
    assert_eq!(page.pasteboard(), Some("alpha"));
    right_press_in(&mut page, "code")?;
    page.pick_titled("Select All Occurrences")?;
    right_press_in(&mut page, "code")?;
    page.pick_titled("Select All")?;
    assert_eq!(
        page.editor("code").map(|b| b.ranges().to_vec()),
        Some(vec![pencere_view::edit::Range::new(0, 22)])
    );
    right_press_in(&mut page, "code")?;
    page.pick_titled("Cut")?;
    assert_eq!(page.editor("code").map(|b| b.text()), Some(String::new()));
    assert_eq!(page.pasteboard(), Some("alpha beta\ngamma delta"));
    // Paste: the host reads its pasteboard, and the text goes in as the paste key's does.
    right_press_in(&mut page, "code")?;
    page.pick_titled("Paste")?;
    assert_eq!(
        page.editor("code").map(|b| b.text()),
        Some("alpha beta\ngamma delta".to_owned())
    );
    right_press_in(&mut page, "code")?;
    page.pick_titled("Toggle Line Comment")?;
    Ok(())
}

/// The command that asks for the editor's menu at the caret (Shift F10) is a key's, and a key is not
/// the person's word to read the pasteboard: the menu has the Paste line, off, so that the lines
/// after it keep their places. The right press the page asked its editor for is, and its line pastes.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_editors_menu_a_key_asks_for_has_no_paste_to_pick_and_the_one_a_right_press_opens_has()
-> Result<(), Error> {
    let mut page = open(&editor_menu_page("", ""))?;
    page.select("code", 0, 5)?;
    right_press_in(&mut page, "code")?;
    page.pick_titled("Copy")?;
    assert_eq!(page.pasteboard(), Some("alpha"));
    page.select("code", 0, 0)?;
    page.focus_editor("code")?;
    page.press("shift+f10")?;
    let shown = menu_titles(&page);
    assert!(shown.contains(&"Paste (off)".to_owned()), "{shown:?}");
    assert!(
        matches!(page.pick_titled("Paste"), Err(Error::NoEntry(_))),
        "the menu a key asked for pastes"
    );
    assert_eq!(
        page.editor("code").map(|b| b.text()),
        Some("alpha beta\ngamma delta".to_owned())
    );
    // The lines after Paste are still the ones the person sees: Select All is picked as itself.
    page.pick_titled("Select All")?;
    assert_eq!(
        page.editor("code").map(|b| b.ranges().to_vec()),
        Some(vec![pencere_view::edit::Range::new(0, 22)])
    );
    right_press_in(&mut page, "code")?;
    assert!(menu_titles(&page).contains(&"Paste".to_owned()));
    page.press("escape")?;
    // A Mac's ⌃ with the main button in the editor is the right button the page is told (the drive
    // turns it into one before the page hears it), so it is a context press as well.
    let room = page.bounds("code").expect("the editor");
    let at = pencere_view::Point::new(room.x + 6.0, room.y + 6.0);
    page.pointer(pencere_view::Input::Press {
        at,
        button: pencere_view::Button::Left,
        mods: pencere_view::Mods {
            control: true,
            ..pencere_view::Mods::default()
        },
        count: 1,
    })?;
    assert!(
        menu_titles(&page).contains(&"Paste".to_owned()),
        "{:?}",
        menu_titles(&page)
    );
    Ok(())
}

/// The editor a line of the menu asks is the editor of the menu the person was shown, not of the last
/// menu the page asked for under the same name.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_line_of_an_editors_menu_asks_the_editor_it_was_shown_for_when_another_was_asked_under_its_name()
-> Result<(), Error> {
    let mut page = open(
        r#"
view = function(u)
    return pencere.div():row():size_full()
        :child(pencere.editor("one", { gutter = false, gestures = { ["right+press"] = "context" } }):grow(1))
        :child(pencere.editor("two", { gutter = false, gestures = { ["right+press"] = "context" } }):grow(1))
end
on_event = function(event, u)
    if event.type == "editor_ready" then
        return { editor = { [event.editor] = { { load = event.editor == "one" and "alpha" or "beta" } } } }
    elseif event.type == "press" and event.button == "right" and event.place then
        pencere.time.after(1, function()
            return u:editor_menu("shared", { editor = "two", at = { x = 5, y = 5 } })
        end)
        return u:editor_menu("shared", { editor = event.place.editor, at = { x = event.x, y = event.y } })
    end
end
on_action = function(act, u)
    if act.from == "shared" then
        return u:editor_menu_pick(act)
    end
end
"#,
    )?;
    page.select("one", 0, 5)?;
    right_press_in(&mut page, "one")?;
    page.advance(2.0)?;
    page.pick_titled("Copy")?;
    assert_eq!(page.pasteboard(), Some("alpha"));
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_read_only_editors_menu_greys_what_changes_the_text_and_not_what_the_keys_leave_on()
-> Result<(), Error> {
    let mut page = open(&editor_menu_page("read_only = true,", "read_only = true"))?;
    right_press_in(&mut page, "code")?;
    let titles = menu_titles(&page);
    assert!(titles.contains(&"Cut (off)".to_owned()), "{titles:?}");
    assert!(titles.contains(&"Paste (off)".to_owned()), "{titles:?}");
    assert!(
        titles.contains(&"Toggle Line Comment (off)".to_owned()),
        "{titles:?}"
    );
    // Nothing is greyed for an empty selection: the keys copy and cut the caret's line.
    assert!(titles.contains(&"Copy".to_owned()), "{titles:?}");
    assert!(titles.contains(&"Select All".to_owned()), "{titles:?}");
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_keys_an_editors_menu_shows_are_those_the_page_left_it_and_the_keyboard_opens_it_at_the_caret()
-> Result<(), Error> {
    let mut page = open(&editor_menu_page(
        r#"keymap = { ["mod+shift+c"] = "copy", ["mod+c"] = false, ["mod+k mod+c"] = "toggle_comment" },"#,
        "",
    ))?;
    page.focus_editor("code")?;
    page.press("shift+f10")?;
    let menu = page.menu().expect("a menu at the caret");
    assert!(menu.at.x.is_finite() && menu.at.y > 0.0, "{:?}", menu.at);
    let copy = menu
        .entries
        .iter()
        .find_map(|entry| match entry {
            pencere_view::MenuEntry::Item { title, keys, .. } if title == "Copy" => *keys,
            _ => None,
        })
        .expect("Copy has keys");
    assert_eq!(
        copy,
        pencere_view::Shortcut::parse("cmd+shift+c").expect("keys")
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_page_cannot_name_a_line_of_its_own_as_one_of_the_menus() -> Result<(), Error> {
    let mut page = open(
        r#"
view = function(u)
    return u:button("more", { title = "More" })
end
on_action = function(act, u)
    return u:editor_menu("m", { editor = "code", at = { x = 1, y = 1 }, before = { { key = "copy", title = "Mine" } } })
end
"#,
    )?;
    match page.click("more") {
        Err(Error::Ended(Fault::Failed(why))) => {
            assert!(
                why.contains("copy") && why.contains("the menu's own"),
                "{why}"
            );
        }
        other => panic!("expected the page to fail, it is {other:?}"),
    }
    Ok(())
}

// --- What a screen reader is told of a component, whole -----------------------------------------------
//
// A golden reading: the whole of what a component says to a screen reader and to an agent, item by item,
// set against the text it is expected to be, which is a file of `src/goldens/` (`golden_tests`). The
// parts' own tests look at one item each; what they miss is an item that is gone, twice there, or
// nameless, which is only seen in the whole. A change of what is read is a change of those files, in
// the diff, where it is reviewed; `PENCERE_BLESS=1` writes them again.

/// What a screen reader is told of the page, a line for each item: its role, its words and its state.
/// The page's own lines (where the keyboard is, what the log says), which every test page has, are
/// left out.
fn reading_of(page: &Headless) -> Vec<String> {
    let logged = log(page);
    page.outline()
        .accessible
        .iter()
        .filter(|item| !item.words.starts_with("keys at ") && !logged.contains(&item.words))
        .map(|item| {
            let mut line = format!("{:?} {:?}", item.role, item.words);
            let state = &item.state;
            let flags = [
                state.selected.then(|| "selected".to_owned()),
                state
                    .checked
                    .map(|on| if on { "checked" } else { "unchecked" }.to_owned()),
                state
                    .expanded
                    .map(|on| if on { "expanded" } else { "collapsed" }.to_owned()),
                state.disabled.then(|| "disabled".to_owned()),
                state.value.as_ref().map(|value| format!("value {value}")),
            ];
            for flag in flags.into_iter().flatten() {
                line.push_str(&format!(" [{flag}]"));
            }
            line
        })
        .collect()
}

/// Sets what the page says of itself, whole, against the golden `name`: what a screen reader is told,
/// item by item, and what an agent reads, line by line. The page's own lines (where the keyboard is,
/// what the log says) are left out of both.
fn golden(name: &str, page: &mut Headless) {
    let logged = log(page);
    let agent = page.reading().text();
    let mut text = String::from("== screen reader\n");
    for line in reading_of(page) {
        text.push_str(&line);
        text.push('\n');
    }
    text.push_str("== agent\n");
    for line in agent.lines() {
        let own = line.contains("\"keys at ")
            || logged
                .iter()
                .any(|entry| line.contains(&format!("\"{entry}\"")));
        if !own {
            text.push_str(line);
            text.push('\n');
        }
    }
    // The host holds a menu the page asked for, and tells its lines.
    if page.menu().is_some() {
        text.push_str("== menu\n");
        for title in menu_titles(page) {
            text.push_str(&format!("{title}\n"));
        }
    }
    crate::golden_tests::check(name, &text);
}

/// A grid of three rows of words, with widths, a sort and the cursor on a cell, for a golden reading.
const GRID_READ: &str = r#"
S.state = { cursor = { row = 2, column = 2 }, widths = {}, sort = { column = "name", descending = false } }
view = function(u)
    local columns = {
        { key = "id", title = "id", type = "int", align = "end", width = 60 },
        { key = "name", title = "name", width = 100 },
        { key = "qty", title = "qty", width = 70 },
    }
    return pencere.div():size_full():child(u:grid("g", { columns = columns, count = 3, state = S.state,
        numbers = S.numbers,
        cell = function(row, column)
            if S.loading then return nil end
            return ({ id = row, name = "name " .. row, qty = row * 7 })[column.key]
        end }))
end
"#;

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_grid_row_is_named_by_the_text_its_cells_show_and_not_by_the_data_behind_them()
-> Result<(), Error> {
    // The page's `cell` gives what is shown ("101.0%", "110 MiB"); the raw data is never the name.
    let page = open(
        r#"
S.state = { cursor = nil, widths = {}, sort = nil }
S.data = { { name = "bateri", cpu = 101.0, mem = 115343360 }, { name = "toppen", cpu = 0.5, mem = 2048 } }
view = function(u)
    local columns = {
        { key = "name", title = "name", width = 80 },
        { key = "cpu", title = "cpu", width = 70 },
        { key = "mem", title = "mem", width = 90 },
        { key = "rows", title = "rows", width = 60 },
    }
    return pencere.div():size_full():child(u:grid("g", { columns = columns, count = 2, state = S.state, numbers = false,
        cell = function(row, column)
            local data = S.data[row]
            if column.key == "cpu" then return string.format("%.1f%%", data.cpu) end
            if column.key == "mem" then return string.format("%d MiB", data.mem / 1048576) end
            if column.key == "rows" then return data.mem % 7 end
            return data.name
        end }))
end
"#,
    )?;
    let row = accessible(&page, "g/row/1").words.clone();
    assert_eq!(row, "bateri 101.0% 110 MiB 6");
    // What it says is what the cells draw, word for word.
    let drawn = page.words();
    for word in ["bateri", "101.0%", "110 MiB", "6"] {
        assert!(drawn.contains(&word), "{word} is drawn: {drawn:?}");
    }
    assert!(!row.contains("101 ") && !row.contains("115343360"), "{row}");
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_grid_row_label_that_is_not_a_function_is_refused() {
    let why = failing(&GRID_READ.replace("numbers = S.numbers,", "row_label = \"Item\","));
    assert!(why.contains("row_label is a function"), "{why}");
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_grid_is_read_whole_by_its_heads_edges_and_rows() -> Result<(), Error> {
    // What the label does not say (the type under the title) is read apart; the title is not.
    golden("grid", &mut open(GRID_READ)?);
    // Without the numbers the rows are as they were, less the number.
    golden(
        "grid-plain",
        &mut open(&GRID_READ.replace("numbers = S.numbers,", "numbers = false,"))?,
    );
    // A row the page has not read is said so, and a name the page gives wins.
    golden(
        "grid-loading",
        &mut open(&GRID_READ.replace("S.state =", "S.loading = true\nS.state ="))?,
    );
    golden(
        "grid-labelled",
        &mut open(&GRID_READ.replace(
            "numbers = S.numbers,",
            "row_label = function(row) return \"Item \" .. row end,",
        ))?,
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_grid_with_a_control_in_every_cell_and_no_numbers_is_read_by_its_rows_and_its_controls()
-> Result<(), Error> {
    // Each row is there, with what it is and whether it is chosen, though every cell in it is a control
    // and it has no words of its own; each control is there apart.
    golden(
        "grid-controls",
        &mut open(&GRID_READ.replace(
            r#"            if S.loading then return nil end
            return ({ id = row, name = "name " .. row, qty = row * 7 })[column.key]"#,
            r#"            local name = "c/" .. row .. "/" .. column.key
            return pencere.div():id(name):press(name):role("button"):label(column.title .. " " .. row):size(12, 12)"#,
        ).replace("numbers = S.numbers,", "numbers = false,"))?,
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_tree_is_read_whole_by_its_rows_and_its_arrows() -> Result<(), Error> {
    let mut page = open(TREE)?;
    golden("tree-collapsed", &mut page);
    page.click("files/chevron/src")?;
    golden("tree-expanded", &mut page);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn tabs_are_read_whole_by_their_titles_and_their_close_buttons() -> Result<(), Error> {
    let mut page = open(TABS)?;
    golden("tabs", &mut page);
    page.click("tabs/tab/b.lua")?;
    golden("tabs-second", &mut page);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_list_is_read_whole_with_a_button_in_a_row_and_the_row_not_said_twice() -> Result<(), Error> {
    let mut page = open(
        r#"
S.items = {
    { key = "a", title = "Alpha", hint = "first" },
    { key = "b", title = "Beta", expanded = false },
    { key = "c", title = "Gamma", checked = true },
}
S.state = { cursor = "b", marks = {} }
view = function(u)
    return pencere.div():size_full():child(u:list("things", { items = S.items, state = S.state, height = 24,
        row = function(item)
            local row = pencere.div():row():items_center():gap(8):w_full()
                :child(pencere.text(item.title):grow(1))
            if item.key == "c" then
                row:child(u:button("open/" .. item.key, { title = "Open" }))
            end
            return row
        end }))
end
"#,
    )?;
    // The row's title is not said again by the words in it, and a button in it is apart.
    golden("list", &mut page);
    Ok(())
}

/// A modal over a page, for a golden reading.
const MODAL_READ: &str = r#"
view = function(u)
    return pencere.div():gap(8):child(u:button("open", { title = "Open" }))
        :child(u:modal("settings", { width = 400, label = "Settings" },
            pencere.div():p(16):gap(8)
                :child(pencere.text("Settings"))
                :child(u:input("name", { label = "Name" }))
                :child(u:button("ok", { title = "OK" }))))
end
"#;

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_modal_is_read_whole_by_its_scrim_its_words_its_field_and_its_button() -> Result<(), Error> {
    // The scrim says what a press on it does; the card is no element, so its words are words.
    golden("modal", &mut open(MODAL_READ)?);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_dialog_is_read_whole_by_its_title_its_message_its_field_and_its_buttons() -> Result<(), Error>
{
    let mut page = open(&DIALOG.replace("S.asking = false", "S.asking = true\nS.named = true"))?;
    golden("dialog", &mut page);
    // Closed, the commands the page asks for are what is read besides the button.
    golden("commands", &mut open(DIALOG)?);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_popover_toasts_and_a_menu_are_read_whole() -> Result<(), Error> {
    golden(
        "popover",
        &mut open(
            r#"
view = function(u)
    return pencere.div():gap(8):p(20)
        :child(u:button("anchor", { title = "Anchor" }))
        :child(u:button("elsewhere", { title = "Elsewhere" }))
        :child(u:popover("card", { anchor = "anchor", side = "below" }, pencere.div():p(8):child(pencere.text("Hello"))))
end
"#,
        )?,
    );
    // A toast of each tone, each with its own button to close it.
    golden(
        "toasts",
        &mut open(
            r#"
S.items = {}
ui.toast(S.items, "Saved a.lua", "ok", 0)
ui.toast(S.items, "Disk is full", "bad", 0)
view = function(u)
    ui.toasts_prune(S.items, u.now)
    return pencere.div():gap(8)
        :child(u:button("say", { title = "Say" }))
        :child(u:toasts("toasts", { items = S.items }))
end
"#,
        )?,
    );
    // The page's side of a menu is the button that opened it; the host holds the menu, whose lines
    // are told after.
    let mut page = open(
        r#"
view = function(u)
    return u:button("more", { title = "More" })
end
on_action = function(act, u)
    if act.from == "more" then
        return u:menu("more-menu", { { key = "copy", title = "Copy", keys = "cmd+c" }, "-",
            { key = "paste", title = "Paste", enabled = false }, { key = "delete", title = "Delete" } },
            { near = "more" })
    end
end
"#,
    )?;
    page.click("more")?;
    golden("menu", &mut page);
    assert_eq!(menu_titles(&page), ["Copy", "-", "Paste (off)", "Delete"]);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_palette_the_controls_and_the_bars_are_read_whole() -> Result<(), Error> {
    let mut page = open(PALETTE)?;
    page.click("open")?;
    golden("palette", &mut page);
    golden(
        "controls",
        &mut open(
            r#"
S.view = "list"
S.case = true
view = function(u)
    return pencere.div():gap(8):p(8)
        :child(u:segments("view", { value = S.view, items = {
            { key = "list", title = "List" }, { key = "grid", title = "Grid" }, { key = "tree", title = "Tree" } } }))
        :child(u:toggle("case", { on = S.case, glyph = "Aa", label = "Match case", keys = "alt+cmd+c" }))
        :child(u:input("name", { label = "Name", value = "a.lua" }))
        :child(u:banner("conflict", {
            tone = "warn", icon = "warning", words = "a.lua changed on disk",
            actions = { { key = "reload", title = "Reload", primary = true }, { key = "keep", title = "Keep mine" } },
            dismiss = true }))
        :child(pencere.div():row():gap(6)
            :child(u:badge(12, { tone = "info", round = true }))
            :child(u:badge("draft"))
            :child(u:kbd("cmd+shift+k"))
            :child(u:kbd("ctrl+enter", { quiet = true })))
end
"#,
        )?,
    );
    golden(
        "status",
        &mut open(
            r#"
view = function(u)
    return u:status("status", {
        start = { { key = "branch", words = "main", icon = "dot", press = true, priority = 9 } },
        center = { { key = "saved", words = "Saved", priority = 1 } },
        finish = {
            { key = "pos", words = "Ln 12, Col 4", press = true, priority = 6, tip = "Go to line", keys = "ctrl+g" },
            { key = "lang", words = "Lua", priority = 3 },
        },
    })
end
"#,
        )?,
    );
    golden("split", &mut open(SPLIT)?);
    let mut page = open(FIND)?;
    page.click("open")?;
    golden("find", &mut page);
    let mut page = open(&editor_menu_page("", ""))?;
    right_press_in(&mut page, "code")?;
    golden("editor-menu", &mut page);
    Ok(())
}

/// A modal that holds a button that opens a popover and one that opens a menu, an icon button that
/// has a tip and no words, and a button that opens a dialog; the page behind it has a button and a
/// toast. What is open is `S.open` (`popover`, `dialog`) and `S.toast`.
const LAYERS: &str = r#"
S.open = nil
S.items = {}
view = function(u)
    ui.toasts_prune(S.items, u.now)
    local root = pencere.div():gap(8):p(8)
        :child(u:button("behind", { title = "Behind" }))
        :child(u:modal("sheet", { label = "Sheet", width = 300 }, pencere.div():p(12):gap(8)
            :child(pencere.text("Sheet"))
            :child(u:button("inside/popover", { title = "Hint" }))
            :child(u:button("inside/menu", { title = "Menu" }))
            :child(u:button("inside/tip", { title = "" }):child(pencere.div():size(12, 12)):tip("Search"))
            :child(u:button("inside/dialog", { title = "Delete" }))
            :child(S.open == "popover"
                and u:popover("hint", { anchor = "inside/popover", side = "below" }, pencere.div():p(8):child(pencere.text("Help")))
                or pencere.div())))
    if S.open == "dialog" then
        root:child(u:dialog("sure", { title = "Delete it?", message = "It cannot be undone.",
            actions = { { key = "no", title = "Keep" }, { key = "yes", title = "Delete", danger = true } } }))
    end
    return root:child(u:toasts("toasts", { items = S.items }))
end
on_action = function(act, u)
    if act.from == "inside/popover" then
        S.open = "popover"
    elseif act.from == "inside/menu" then
        return u:menu("sheet-menu", { { key = "copy", title = "Copy" }, { key = "paste", title = "Paste" } },
            { near = "inside/menu" })
    elseif act.from == "inside/dialog" then
        S.open = "dialog"
    elseif act.from == "behind" then
        ui.toast(S.items, "Saved", "ok", 0)
    end
end
"#;

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn what_is_read_of_a_page_under_a_modal_and_of_what_opens_over_it_is_pinned() -> Result<(), Error> {
    // A modal in front of a page: the page behind it is read today, which phase 2 of the model
    // takes out (the golden is where that shows).
    let mut page = open(LAYERS)?;
    golden("layer-modal", &mut page);
    // A popover, a menu and a tip opened from inside the modal are in it.
    page.click("inside/popover")?;
    golden("layer-modal-popover", &mut page);
    page.click("inside/menu")?;
    golden("layer-modal-menu", &mut page);
    assert_eq!(menu_titles(&page), ["Copy", "Paste"]);
    // A toast that comes up while the modal is open is a layer of its own, over both.
    let mut page = open(&LAYERS.replace(
        "S.items = {}",
        "S.items = {}\nui.toast(S.items, \"Saved\", \"ok\", 0)",
    ))?;
    golden("layer-modal-toast", &mut page);
    // A dialog over a sheet is the front layer: the sheet and the page are behind it.
    let mut page = open(LAYERS)?;
    page.click("inside/dialog")?;
    golden("layer-sheet-dialog", &mut page);
    Ok(())
}

/// A layer as the pens write one by hand: a scrim that answers the pointer over the whole page and holds a
/// card that answers it too (so that a press on the card is not one on the scrim), a button behind. What is
/// marked is what a test puts in `CARD_MARKS` and `SCRIM_MARKS`.
const PEN_LAYER: &str = r#"
view = function(u)
    local card = pencere.div():id("card"):press("card"):col():w(240):p(12):gap(8)
        :child(pencere.text("Delete it?"))
        :child(pencere.div():id("yes"):press("yes"):label("Delete"):size(80, 24))
        :child(pencere.div():id("no"):press("no"):label("Keep"):size(80, 24))
    CARD_MARKS
    local scrim = pencere.div():id("scrim"):press("scrim"):overlay():inset(0, 0, 0, 0):col():items_center():pt(40)
        :child(card)
    SCRIM_MARKS
    local root = pencere.div():gap(8):p(8):child(u:button("behind", { title = "Behind" }))
    if not S.shut then
        root:child(scrim)
    end
    return root
end
-- A press on the scrim gives the layer up: the page's own, as a pen's is.
on_event = function(event)
    if event.type == "release" and pencere.layout.hit({ x = event.x, y = event.y }) == "scrim" then
        S.shut = true
        return true
    end
end
"#;

/// [`PEN_LAYER`] with `card` and `scrim` marked as the statements say (Lua, one per line).
fn pen_layer(card: &str, scrim: &str) -> String {
    PEN_LAYER
        .replace("CARD_MARKS", card)
        .replace("SCRIM_MARKS", scrim)
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_layer_a_pen_marks_by_hand_with_three_marks_is_read_as_a_dialog_and_one_it_forgot_is_found()
-> Result<(), Error> {
    // The three marks: the scrim is a layer and is not read, the card is the dialog. The page behind
    // is out of the reading, and the reading has nothing to be found fault with.
    let mut page = open(&pen_layer(
        "card:role(\"dialog\"):label(\"Delete it?\")",
        "scrim:role(\"none\"):layer(true)",
    ))?;
    golden("layer-pen-dialog", &mut page);
    assert_eq!(page.audit(), [], "{}", crate::audit::report(&page.audit()));
    // The card's press holds a press on it: the card is no scrim.
    page.click("card")?;
    assert!(
        page.bounds("scrim").is_some(),
        "a press on the card does not close the layer"
    );
    // A press on the scrim, which the card does not cover, does.
    page.click("scrim")?;
    assert!(
        page.bounds("scrim").is_none(),
        "a press on the scrim gives the layer up"
    );
    // Forgotten: no mark at all. The page behind is read in full, under a scrim that lies over it, and the
    // audit finds what is read and cannot be reached.
    let mut page = open(&pen_layer("card:label(\"Delete it?\")", ""))?;
    golden("layer-pen-unmarked", &mut page);
    let found = page.audit();
    assert!(
        found
            .iter()
            .any(|finding| finding.flaw == Flaw::Covered && finding.words == "Behind"),
        "{}",
        crate::audit::report(&found)
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_layer_marked_the_wrong_way_has_a_reading_of_its_own_that_is_pinned() -> Result<(), Error> {
    // A dialog that is no layer: the page behind it is read in full, and the audit finds it covered.
    let mut page = open(&pen_layer(
        "card:role(\"dialog\"):label(\"Delete it?\")",
        "",
    ))?;
    golden("layer-dialog-no-layer", &mut page);
    assert!(
        page.audit()
            .iter()
            .any(|finding| finding.flaw == Flaw::Covered && finding.words == "Behind"),
        "{}",
        crate::audit::report(&page.audit())
    );
    // A layer with no dialog in it: the page behind is out, and what is in front is read as it is.
    let mut page = open(&pen_layer("", "scrim:role(\"none\"):layer(true)"))?;
    golden("layer-no-dialog", &mut page);
    // `none` on an element that has a label: the label is read, and the element is the button it is with no
    // role at all (what is in the card is not under it).
    let mut page = open(&pen_layer(
        "card:role(\"dialog\"):label(\"Delete it?\")",
        "scrim:role(\"none\"):layer(true):label(\"Close dialog\")",
    ))?;
    golden("none-with-label", &mut page);
    // `none` with a label and a command: read as with no role at all, with the action it lists.
    let mut page = open(&pen_layer(
        "card:role(\"dialog\"):label(\"Delete it?\")",
        "scrim:role(\"none\"):layer(true):label(\"Close dialog\"):command(\"close\", \"Close\")",
    ))?;
    golden("none-with-label-and-command", &mut page);
    // `none` on an element that lists a command and has no label: it is not read, and the command it lists
    // is not listed with it (a command is listed on what is read: the card).
    let mut page = open(&pen_layer(
        "card:role(\"dialog\"):label(\"Delete it?\")",
        "scrim:role(\"none\"):layer(true):command(\"close\", \"Close\", \"escape\")",
    ))?;
    golden("none-with-command", &mut page);
    // Two modal layers: the one drawn last is in front, the other is behind it with the page.
    let mut page = open(
        &pen_layer(
            "card:role(\"dialog\"):label(\"Delete it?\")",
            "scrim:role(\"none\"):layer(true)",
        )
        .replace(
            "        root:child(scrim)\n    end\n",
            "        root:child(scrim)\n    end\n    root:child(pencere.div():id(\"second\"):press(\"second\"):overlay():inset(0, 0, 0, 0):layer(true):role(\"none\")\n        :child(pencere.div():id(\"other\"):role(\"dialog\"):label(\"Other\"):press(\"other\"):w(120):h(60)\n            :child(pencere.div():id(\"ok\"):press(\"ok\"):label(\"OK\"):size(40, 24))))\n",
        ),
    )?;
    golden("layer-two-modals", &mut page);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_link_in_a_text_is_found_covered_as_a_control_is() -> Result<(), Error> {
    // A link is read with the name of its part of the text, not of a box: it is tried where it stands in its
    // text, and a scrim that was not marked, over the page it stands in, covers it.
    let with_link = |card: &str, scrim: &str| {
        pen_layer(card, scrim).replace(
            ":child(u:button(\"behind\", { title = \"Behind\" }))",
            ":child(u:button(\"behind\", { title = \"Behind\" })):child(pencere.rich({ pencere.span(\"see \"), pencere.span(\"the docs\"):link(\"docs\") }))",
        )
    };
    let mut page = open(&with_link("card:label(\"Delete it?\")", ""))?;
    let found = page.audit();
    assert!(
        found
            .iter()
            .any(|finding| finding.flaw == Flaw::Covered && finding.words == "the docs"),
        "{}",
        crate::audit::report(&found)
    );
    // Marked, the page behind is out of the reading and so is the link.
    let mut page = open(&with_link(
        "card:role(\"dialog\"):label(\"Delete it?\")",
        "scrim:role(\"none\"):layer(true)",
    ))?;
    assert_eq!(page.audit(), [], "{}", crate::audit::report(&page.audit()));
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_layer_of_the_kit_is_never_found_covered_by_the_scrim_it_stands_in() -> Result<(), Error> {
    // The card of a modal is over its scrim (a press on the scrim is one on the scrim only where the card is
    // not): what is in the card is not covered by it, whatever it is (a field, a text, a disabled button).
    let covered = |page: &mut Headless| {
        page.audit()
            .into_iter()
            .filter(|finding| finding.flaw == Flaw::Covered)
            .map(|finding| format!("{} {}", finding.role, finding.words))
            .collect::<Vec<_>>()
    };
    let mut page = open(&DIALOG.replace("S.asking = false", "S.asking = false\nS.named = true"))?;
    page.click("open")?;
    page.step()?;
    assert_eq!(
        covered(&mut page),
        Vec::<String>::new(),
        "a dialog with a field"
    );
    let mut modal = open(
        r#"
view = function(u)
    return pencere.div():child(u:button("open", { title = "Open" }))
        :child(u:modal("m", { label = "Pick" }, pencere.div():child(pencere.text("Inside"))))
end
"#,
    )?;
    modal.step()?;
    assert_eq!(
        covered(&mut modal),
        Vec::<String>::new(),
        "a modal with only text"
    );
    // A layer behind another is not listed for doing, so a reference that is old does nothing.
    let mut two = open(
        r#"
view = function(u)
    return pencere.div():child(u:button("open", { title = "Open" }))
        :child(u:modal("a", { label = "First" }, pencere.div():child(pencere.text("One"))))
        :child(u:modal("b", { label = "Second" }, pencere.div():child(pencere.text("Two"))))
end
"#,
    )?;
    two.step()?;
    let read = two.reading().text();
    assert!(
        read.contains("dialog \"Second\"") && !read.contains("dialog \"First\""),
        "the layer in front is read: {read}"
    );
    assert!(
        two.command_on("a/card", "ui.dismiss").is_err(),
        "a layer behind another has no way out listed"
    );
    assert!(
        two.reading().text().contains("dialog \"Second\""),
        "and the one in front is not given up"
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_labelled_container_is_a_group_with_no_control_in_it_and_an_item_is_named_by_what_is_in_it()
-> Result<(), Error> {
    // As on the web: a dialog, a group or a list with a label is its name and what is written in it is read
    // under it (apart from what the label says already, in the nearest group: the label of a dialog is not
    // said of a group of its own in it); a heading that names it is read after it, as on the web; a row, a tab
    // or an option with no control in it is named by its label or else by what is in it, and is a leaf; a
    // labelled container is a group in a dialog that has no label as well.
    let mut page = open(
        r#"
view = function(u)
    local jobs = pencere.div():id("jobs"):role("group"):label("Background jobs"):col()
        :child(pencere.text("Pull redis"))
        :child(pencere.text("abc: Pull complete"))
        :child(pencere.text("Background jobs"))
    local note = pencere.div():id("note"):role("dialog"):label("Heads up"):col()
        :child(pencere.text("Heads up"):role("heading", 2))
        :child(pencere.text("The disk is nearly full."))
        :child(pencere.div():id("inner"):role("group"):label("Details"):col()
            :child(pencere.text("Heads up"))
            :child(pencere.text("Free: 1 GB")))
    local bare = pencere.div():id("bare"):role("dialog"):col()
        :child(pencere.div():id("ok"):press("ok"):label("OK"):p(4):child(pencere.text("OK")))
        :child(pencere.div():id("opts"):role("group"):label("Options"):col()
            :child(pencere.text("Free: 2 GB")))
    local branch = pencere.div():id("branch"):role("dialog"):label("Delete branch: feature-x"):col()
        :child(pencere.div():id("which"):role("group"):label("Branch"):col()
            :child(pencere.text("feature-x")))
    local plain = pencere.div():id("plain"):role("group"):col()
        :child(pencere.text("Unnamed"))
        :child(pencere.text("group"))
    local list = pencere.div():id("list"):role("listbox"):label("Commands"):col()
        :child(pencere.div():id("one"):role("option"):child(pencere.text("Stop")))
        :child(pencere.div():id("two"):role("option"):child(pencere.text("Start")))
    local row = pencere.div():id("row"):role("row"):label("Row one"):col()
        :child(pencere.text("cell a"))
        :child(pencere.text("cell b"))
    return pencere.div():col():gap(8):child(jobs):child(note):child(bare):child(branch):child(plain):child(list):child(row)
end
"#,
    )?;
    golden("group-labelled-no-control", &mut page);
    assert_eq!(
        page.audit()
            .into_iter()
            .filter(|finding| finding.flaw != Flaw::NoId)
            .count(),
        0,
        "{}",
        crate::audit::report(&page.audit())
    );
    Ok(())
}

// --- Where the parts of a component stand: what is aligned in a style sheet is aligned here -----------

/// Where the line of words `text` is drawn: its top-left corner, and as wide and as tall as a line of
/// that many characters is where text is measured with no fonts (7 across a character, a line 16
/// high).
fn words_at(page: &Headless, text: &str) -> pencere_view::Rect {
    page.list()
        .items
        .iter()
        .find_map(|item| match item {
            pencere_view::Paint::Text(run) if run.text == text => {
                #[allow(clippy::cast_precision_loss)]
                let width = text.chars().count() as f32 * 7.0;
                Some(pencere_view::Rect::new(
                    run.origin.x,
                    run.origin.y,
                    width,
                    16.0,
                ))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("no words {text:?} drawn: {:?}", page.words()))
}

fn bounds_of(page: &Headless, name: &str) -> pencere_view::Rect {
    page.bounds(name)
        .unwrap_or_else(|| panic!("nothing named {name} is laid out"))
}

fn middle(rect: pencere_view::Rect) -> (f32, f32) {
    (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0)
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_replace_row_of_a_find_bar_stands_under_the_find_row_at_both_edges() -> Result<(), Error> {
    let page = open(
        r#"
S.find = { query = "foo", replacement = "", replacing = true }
view = function(u)
    return pencere.div():col():gap(8)
        :child(u:find_bar("find", { editor = "doc", state = S.find }))
        :child(pencere.editor("doc", {}):w_full():h(60))
end
"#,
    )?;
    let (find, replace) = (
        bounds_of(&page, "find/query"),
        bounds_of(&page, "find/replace"),
    );
    // The fields are one column: the same left edge and the same right edge, one above the other.
    assert_eq!(find.x, replace.x, "{find:?} {replace:?}");
    assert_eq!(find.right(), replace.right(), "{find:?} {replace:?}");
    assert!(replace.y > find.bottom());
    // The buttons are one column too, and stand at its end: the cross and the last button end together.
    let (close, all, one) = (
        bounds_of(&page, "find/close"),
        bounds_of(&page, "find/replace_all"),
        bounds_of(&page, "find/replace_one"),
    );
    assert_eq!(close.right(), all.right());
    assert!(one.right() <= all.x, "{one:?} {all:?}");
    // The toggle of the replace row is the first column: the fields start after it, with a gap.
    let toggle = bounds_of(&page, "find/replacing");
    assert!(find.x > toggle.right() && find.x - toggle.right() <= 8.0);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_number_of_a_row_of_a_grid_is_in_the_middle_of_it_and_a_column_that_ends_at_the_end_ends_there()
-> Result<(), Error> {
    let page = open(
        r#"
S.state = { cursor = nil, widths = {}, sort = nil }
view = function(u)
    local columns = {
        { key = "name", title = "name" },
        { key = "qty", title = "quantity", type = "int", align = "end" },
    }
    return pencere.div():size_full():child(u:grid("g", { columns = columns, count = 3, state = S.state,
        numbers = true,
        cell = function(row, column)
            if column.key == "name" then return "item" .. row end
            return ({ "7", "14", "300" })[row]
        end }))
end
"#,
    )?;
    // The head and the rows put the number in the middle of their height, as their cells' words are.
    let hash = words_at(&page, "#");
    assert_eq!(middle(hash).1, middle(words_at(&page, "name")).1);
    for (number, cell) in [("1", "item1"), ("2", "item2"), ("3", "item3")] {
        assert_eq!(
            middle(words_at(&page, number)).1,
            middle(words_at(&page, cell)).1,
            "row {number}"
        );
        // And at its right, whatever is on its left: all the same right edge as the head's.
        assert_eq!(words_at(&page, number).right(), hash.right());
    }
    // The values of a column that ends at the end end together, and so do the words of its head.
    let (seven, fourteen, three_hundred) = (
        words_at(&page, "7"),
        words_at(&page, "14"),
        words_at(&page, "300"),
    );
    assert_eq!(seven.right(), fourteen.right());
    assert_eq!(seven.right(), three_hundred.right());
    assert_eq!(
        words_at(&page, "quantity").right(),
        words_at(&page, "int").right()
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_row_chosen_in_a_list_is_inside_its_inset_with_round_corners_and_with_no_inset_it_is_as_wide_as_the_list()
-> Result<(), Error> {
    let page = open(
        r#"
S.items = { { key = "a", title = "alpha" }, { key = "b", title = "beta" } }
view = function(u)
    return pencere.div():row():gap(8):size_full()
        :child(pencere.div():w(160):h(120):child(u:list("plain", { items = S.items, state = { cursor = "a" }, inset = 4 })))
        :child(pencere.div():w(160):h(120):child(u:list("round", { items = S.items, state = { cursor = "a" }, inset = 4, radius = 3 })))
        :child(pencere.div():w(160):h(120):child(u:list("flat", { items = S.items, state = { cursor = "a" } })))
end
"#,
    )?;
    let ground = |name: &str| {
        let row = bounds_of(&page, name);
        let quad = page.list().items.iter().find_map(|item| match item {
            pencere_view::Paint::Quad(quad) if quad.rect == row && quad.fill.is_some() => {
                Some(quad)
            }
            _ => None,
        });
        (
            row,
            quad.unwrap_or_else(|| panic!("no ground of {name}")).radii,
        )
    };
    // A list with an inset keeps the ground of its rows inside it on every side, with the corners of the
    // theme's chip (4) unless the page says another.
    for (name, radius) in [("plain", 4.0), ("round", 3.0)] {
        let list = bounds_of(&page, name);
        let (row, radii) = ground(&format!("{name}/row/a"));
        assert_eq!(row.x, list.x + 4.0, "{name}");
        assert_eq!(row.right(), list.right() - 4.0, "{name}");
        assert_eq!(row.y, list.y + 4.0, "{name}");
        assert_eq!(radii, [radius; 4], "{name}");
    }
    // A list with none is as wide as it is and its rows are square.
    let list = bounds_of(&page, "flat");
    let (row, radii) = ground("flat/row/a");
    assert_eq!((row.x, row.w, row.y), (list.x, list.w, list.y));
    assert_eq!(radii, [0.0; 4]);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_list_row_with_nothing_said_of_it_draws_its_icon_title_and_detail() -> Result<(), Error> {
    let page = open(
        r#"
S.items = { { key = "a", title = "button", detail = "(id, props)", icon = "dot" } }
view = function(u)
    return pencere.div():w(300):h(60):child(u:list("l", { items = S.items, state = { cursor = "a" } }))
end
"#,
    )?;
    let (title, detail) = (words_at(&page, "button"), words_at(&page, "(id, props)"));
    assert!(detail.x > title.right(), "{title:?} {detail:?}");
    // The icon is before the title, in the row's padding.
    let row = bounds_of(&page, "l/row/a");
    assert!(title.x >= row.x + 8.0 + 15.0, "{title:?} {row:?}");
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_edge_of_a_split_is_a_line_the_panes_touch_with_its_grip_laid_over_it() -> Result<(), Error> {
    let page = open(SPLIT)?;
    let first = bounds_of(&page, "first");
    let second = bounds_of(&page, "second");
    let handle = bounds_of(&page, "side/handle");
    // Between the panes there is one point, the line, and no room beside it: the ground of the pane
    // touches it, and so does the other's.
    assert_eq!(second.x, first.right() + 1.0);
    // The grip is as wide as a finger and stands on the line, over both panes, taking no room.
    assert_eq!(handle.w, 7.0);
    assert_eq!(middle(handle).0, first.right() + 0.5);
    assert_eq!(handle.y, first.y);
    assert_eq!(handle.h, first.h);
    // The same when the sized pane is the last.
    let at_end = open(&format!("S.edge = \"end\"\n{SPLIT}"))?;
    let (first, second, handle) = (
        bounds_of(&at_end, "first"),
        bounds_of(&at_end, "second"),
        bounds_of(&at_end, "side/handle"),
    );
    // (The pane that is sized is the second, at the end: the line is on its near side.)
    assert_eq!(second.x, first.right() + 1.0);
    assert_eq!(middle(handle).0, second.x - 0.5);
    // And the grip takes a press at either side of the line: the first of its seven points is one.
    let mut page = at_end;
    page.drag_between(
        pencere_view::Point::new(handle.x + 0.5, 200.0),
        pencere_view::Point::new(handle.x - 19.5, 200.0),
    )?;
    assert_eq!(bounds_of(&page, "second").w, 260.0);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_vertical_rule_in_a_row_is_as_tall_as_the_row() -> Result<(), Error> {
    let page = open(
        r#"
view = function(u)
    return pencere.div():row():items_center():gap(10):h(24)
        :child(pencere.text("main"))
        :child(u:rule("y"):id("down"))
        :child(pencere.text("3 changed"))
end
"#,
    )?;
    let rule = bounds_of(&page, "down");
    assert_eq!((rule.w, rule.h), (1.0, 24.0));
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_middle_of_a_status_bar_is_the_middle_of_the_bar_whatever_stands_at_its_sides()
-> Result<(), Error> {
    let page = open(
        r#"
view = function(u)
    return u:status("status", {
        start = { { key = "branch", words = "main", press = true } },
        center = { { key = "saved", words = "Saved", press = true } },
        finish = {
            { key = "pos", words = "Ln 12, Col 4", press = true },
            { key = "lang", words = "Lua", press = true },
        },
    })
end
"#,
    )?;
    let bar = bounds_of(&page, "status");
    let saved = bounds_of(&page, "status/item/saved");
    // (A box is laid out on whole pixels: half a point is the least the middle can be off.)
    assert!(
        (middle(saved).0 - middle(bar).0).abs() <= 0.5,
        "{bar:?} {saved:?}"
    );
    // The first group is at the start and the last at the end of the bar (inside its padding of 4).
    let (branch, lang) = (
        bounds_of(&page, "status/item/branch"),
        bounds_of(&page, "status/item/lang"),
    );
    assert_eq!(branch.x, bar.x + 4.0);
    assert_eq!(lang.right(), bar.right() - 4.0);
    // Each is as tall as the bar, as the items of a bar are.
    assert_eq!(saved.h, bar.h);
    Ok(())
}

/// The first two lines of the guides of a level of a tree go through the middle of the arrow of the
/// node above: the guide of level k is a point wide in the middle of the arrow cell that starts there.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_guide_of_a_tree_goes_through_the_middle_of_the_arrow_of_the_node_above() -> Result<(), Error> {
    let page = open(
        r#"
S.kids = {
    src = { "src/a.lua", "src/lib" },
    ["src/lib"] = { "src/lib/x.lua" },
}
S.state = { expanded = { src = true, ["src/lib"] = true }, cursor = nil }
view = function(u)
    return pencere.div():size_full()
        :child(u:tree("files", { roots = { "src" }, state = S.state, height = 24,
            children = function(key) return S.kids[key] end,
            item = function(key) return { title = string.match(key, "[^/]+$") } end }))
end
"#,
    )?;
    let row = bounds_of(&page, "files/list/row/src/lib/x.lua");
    // What the row's own padding leaves, from where its levels start.
    let start = row.x + 8.0;
    let mut guides: Vec<f32> = page
        .list()
        .items
        .iter()
        .filter_map(|item| match item {
            pencere_view::Paint::Quad(quad)
                if quad.rect.w == 1.0 && quad.rect.y == row.y && quad.rect.h == row.h =>
            {
                Some(quad.rect.x + 0.5)
            }
            _ => None,
        })
        .collect();
    guides.sort_by(f32::total_cmp);
    // A node of depth 2 has two guides: through the arrows (16 across, the middle 8 in) of the nodes of
    // depths 0 and 1, which stand a level (12) apart. (A box is laid out on whole pixels: at a scale of 1
    // the point's line is half a point past the middle at the most.)
    assert_eq!(guides.len(), 2, "{guides:?}");
    for (guide, middle) in guides.iter().zip([start + 8.0, start + 12.0 + 8.0]) {
        assert!((guide - middle).abs() <= 0.5, "{guides:?} from {start}");
    }
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_button_with_an_icon_has_as_much_of_its_padding_seen_on_the_left_as_on_the_right()
-> Result<(), Error> {
    let page = open(
        r#"
view = function(u)
    return pencere.div():row():gap(8)
        :child(u:button("with", { title = "Revert", icon = "chevron_left" }))
        :child(u:button("without", { title = "Revert" }))
end
"#,
    )?;
    let (with, without) = (bounds_of(&page, "with"), bounds_of(&page, "without"));
    // The icon (15), the gap (6) and the glyph's empty left side (5.25 of 16 of 15) are what the icon
    // adds: a chevron draws from a third of its box in, and the button's padding takes that off, so
    // that what is seen of it on the left is the padding the right has.
    let added = with.w - without.w;
    let expected = 15.0 + 6.0 - 5.25 * 15.0 / 16.0;
    // (A width is whole pixels.)
    assert!((added - expected).abs() <= 0.5, "{added} {expected}");
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_rows_of_a_palette_and_a_picker_start_their_words_where_the_words_of_the_field_start()
-> Result<(), Error> {
    let field_text = |page: &Headless, name: &str| {
        page.outline()
            .fields
            .iter()
            .find(|field| field.id == pencere_view::Id::named(name))
            .unwrap_or_else(|| panic!("no field {name}"))
            .text
            .x
    };
    let mut palette = open(
        r#"
S.items = { { key = "save", title = "Save File" }, { key = "close", title = "Close Tab" } }
S.state = { query = "", cursor = nil }
view = function(u)
    return u:palette("palette", { items = S.items, state = S.state, label = "Command palette", hint = "Type a command" })
end
"#,
    )?;
    palette.advance(0.3)?;
    assert_eq!(
        words_at(&palette, "Save File").x,
        field_text(&palette, "palette/input/field")
    );
    let mut picker = open(
        r#"
S.items = { { key = "bug", title = "Bug" }, { key = "api", title = "API" } }
S.state = { query = "" }
view = function(u)
    return pencere.div():p(20):gap(8)
        :child(u:button("labels", { title = "Labels" }))
        :child(u:picker("picker", { anchor = "labels", items = S.items, state = S.state, label = "Labels", hint = "Change labels" }))
end
"#,
    )?;
    picker.advance(0.3)?;
    assert_eq!(
        words_at(&picker, "Bug").x,
        field_text(&picker, "picker/input/field")
    );
    Ok(())
}
