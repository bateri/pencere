//! Pages with `@pencere/ui` of the kit's library, driven by the host with no screen: each test draws
//! a component on a page that writes down the actions the context gave it (the element `log`, a
//! line each), and does what a person does to it — clicks, keys, drags — then reads what the page
//! says, what it is to a screen reader and where the keyboard is. Heavy: the interpreter is compiled
//! and run (`make check-heavy`).

use pencere_sandbox::Fault;
use pencere_view::AccessibleRole;

use crate::headless::{Error, Focus, Headless};

/// What every test page begins with: the library, and the page's parts left for the test to give
/// (`options` for `ui.new`, `view(u)` the element the test draws, `on_action(act, u)` a reply to an
/// action).
const HEAD: &str = r#"
local ui = require("@pencere/ui")
local options, view, on_action, finish = nil, nil, nil, nil
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

/// A component of the page's own: two buttons of the library inside it, whose presses it takes as
/// its own change, and a count it keeps by its name where the page gives it no table.
const STEPPER: &str = r#"
local Stepper = ui.component("stepper", {
    state = function(props)
        return { value = props.start or 0 }
    end,
    view = function(u, id, props, state)
        return pencere.div():row():gap(4)
            :child(u:icon_button(u:part(id, "less"), { icon = "dash", label = "Less" }))
            :child(pencere.text(id .. "=" .. state.value))
            :child(u:icon_button(u:part(id, "more"), { icon = "plus", label = "More" }))
    end,
    action = function(u, id, props, state, act)
        local part = u:part_of(act.from)
        if act.kind ~= "press" or part == nil then
            return nil
        end
        state.value += part.part == "less" and -1 or 1
        return true, { kind = "change", value = state.value }
    end,
})
"#;

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_component_takes_the_actions_of_those_inside_it_as_its_own() -> Result<(), Error> {
    let mut page = open(&format!(
        "{STEPPER}
S.mine = {{ value = 10 }}
view = function(u)
    return pencere.div():gap(4)
        :child(Stepper(u, 'kept', {{ start = 3 }}))
        :child(Stepper(u, 'owned', {{ state = S.mine }}))
end
"
    ))?;
    page.click("kept/more")?;
    page.click("kept/more")?;
    page.click("owned/less")?;
    assert!(page.says("kept=5"));
    assert!(page.says("owned=9"));
    assert_eq!(
        log(&page),
        [
            "kept change value=4",
            "kept change value=5",
            "owned change value=9"
        ]
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn what_a_component_keeps_by_its_name_is_forgotten_when_it_is_not_drawn() -> Result<(), Error> {
    let mut page = open(&format!(
        "{STEPPER}
S.shown = true
view = function(u)
    local root = pencere.div():gap(4):child(u:button('hide', {{ title = 'Hide' }}))
    if S.shown then
        root:child(Stepper(u, 'kept', {{ start = 1 }}))
    end
    return root
end
on_action = function(act)
    if act.from == 'hide' then
        S.shown = not S.shown
    end
end
"
    ))?;
    page.click("kept/more")?;
    assert!(page.says("kept=2"));
    page.click("hide")?;
    assert!(!page.says("kept="));
    page.click("hide")?;
    assert!(page.says("kept=1"), "drawn again, it starts again");
    Ok(())
}

/// A layer of the page's own: what is inside it takes the keyboard while it is in front.
const SHEET: &str = r#"
local Sheet = ui.component("sheet", {
    view = function(u, id, props)
        u:layer(id)
        return pencere.div():overlay():inset(40, 40, nil, 40):p(12):gap(8):bg("raised2")
            :child(u:button(u:part(id, "first"), { title = "First" }))
            :child(u:button(u:part(id, "second"), { title = "Second" }))
    end,
})
S.open = false
view = function(u)
    local root = pencere.div():gap(8)
        :child(u:button("open", { title = "Open" }))
        :child(u:button("after", { title = "After" }))
    if S.open then
        root:child(Sheet(u, "sheet"))
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
    assert_eq!(keys_at(&page), "keys at sheet/first in page");
    page.press("tab")?;
    assert_eq!(keys_at(&page), "keys at sheet/second in page");
    page.press("tab")?;
    assert_eq!(
        keys_at(&page),
        "keys at sheet/first in page",
        "round within the layer, never out to the page behind it"
    );
    page.press("shift+tab")?;
    assert_eq!(keys_at(&page), "keys at sheet/second in page");
    page.press("escape")?;
    page.step()?;
    assert_eq!(log(&page), ["open press", "sheet dismiss"]);
    assert!(!page.says("First"));
    assert_eq!(keys_at(&page), "keys at open in page", "back where it was");
    assert_eq!(page.focus(), Focus::Page);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_part_that_took_the_press_follows_the_pointer_until_it_comes_up() -> Result<(), Error> {
    let mut page = open(
        r#"
local Handle = ui.component("handle", {
    view = function(u, id)
        return pencere.div():row():gap(40)
            :child(pencere.div():press(u:part(id, "grip")):size(20, 20):bg("accent"))
            :child(pencere.div():id("there"):size(20, 20))
    end,
    press = function(u, id, props, state, part)
        if part == "grip" then
            return false, nil, true
        end
    end,
    drag = function(u, id, props, state, part, key, at)
        return true, { kind = "drag", dx = math.floor(at.dx) }
    end,
    release = function(u, id, props, state, part, key, at)
        return true, { kind = "done", dx = math.floor(at.dx) }
    end,
    cursor = function(u, id, props, state, part)
        if part == "grip" then
            return "resize_horizontal"
        end
    end,
})
view = function(u)
    return Handle(u, "split")
end
"#,
    )?;
    page.hover("split/grip")?;
    assert_eq!(page.cursor(), Some(pencere_view::Cursor::ResizeHorizontal));
    page.drag("split/grip", "there")?;
    let said = log(&page);
    assert_eq!(
        said.last().map(String::as_str),
        Some("split done dx=60"),
        "{said:?}"
    );
    assert!(
        said.iter().any(|line| line == "split drag dx=60"),
        "{said:?}"
    );
    assert!(said.len() > 2, "the drag is told as it goes: {said:?}");
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
    elseif act.from == "tabs" and act.kind == "menu" then
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
	return { decorate = "card", version = version, from = 0, to = #text, decorations = card and {
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
            keys = S.vim and { j = "down", k = "up" } or nil,
            count = S.count }))
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
        AccessibleRole::MenuItem
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
        Some("results menu key=k4")
    );
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
            keys = S.vim and { j = "down", k = "up", h = "left", l = "right" } or nil }))
end
"#;

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_tree_opens_and_closes_folders_with_the_pointer_and_the_arrow_keys() -> Result<(), Error> {
    let mut page = open(TREE)?;
    assert!(page.says("src") && !page.says("a.lua"));
    assert_eq!(
        accessible(&page, "files/list/row/src").role,
        AccessibleRole::Button
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
        said.iter().any(|line| line == "side resize size=300"),
        "{said:?}"
    );
    assert_eq!(
        said.last().map(String::as_str),
        Some("side resized size=300")
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
    page.click("side/handle")?;
    page.press("left")?;
    assert_eq!(page.bounds("first").expect("pane").w, 384.0);
    assert_eq!(
        log(&page).last().map(String::as_str),
        Some("side resized size=384")
    );
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
fn a_grid_sorts_by_a_press_on_a_head_and_sizes_a_column_by_its_edge() -> Result<(), Error> {
    let mut page = open(GRID)?;
    page.click("g/head/name")?;
    assert_eq!(log(&page), ["g sort column=name"]);
    assert!(
        accessible(&page, "g/head/name")
            .words
            .contains("sorted ascending")
    );
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
            .any(|line| line.starts_with("g resize column=id")),
        "{:?}",
        log(&page)
    );
    assert!(
        log(&page)
            .last()
            .unwrap()
            .starts_with("g resized column=id")
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
