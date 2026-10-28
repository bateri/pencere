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
    for word in ["12", "draft", "⇧", "⌘", "K", "⌃↩"] {
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
            return false
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
