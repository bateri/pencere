//! What a page reads of how its host draws its terminal's cursor and selection
//! (`pencere.terminal_look()`, `pencere.draws()`), and `@pencere/terminal` of the kit's library that
//! puts it in an editor's options where the page said nothing, run by the interpreter: a host that says
//! nothing gives `{}` and the options back as they were, a host that says some gives some, what the page
//! wrote stays and the tables of the options are merged a field at a time. Heavy: the interpreter is
//! compiled and run (`make check-heavy`).

use std::sync::Arc;
use std::time::Duration;

use pencere_view::edit::{CaretMotion, CaretShape, CaretUnfocused};
use pencere_view::{Drawing, TerminalCaret, TerminalLook, TerminalSelection};

use crate::luau_tests::{open_script_for, words};
use crate::{ModuleLimits, Program};

/// A host that says how its terminal draws its cursor and its selection, and what it draws.
struct Terminal {
    look: TerminalLook,
    draws: Vec<Drawing>,
}

impl Program for Terminal {
    fn may_say(&self, _bytes: usize) -> bool {
        true
    }

    fn taken(&self, _bytes: usize) {}

    fn terminal_look(&self) -> TerminalLook {
        self.look
    }

    fn draws(&self) -> Vec<Drawing> {
        self.draws.clone()
    }
}

/// The look of bateri's terminal's cursor, as a host with such settings says it: a block that is not
/// quite square, with a light, that glides, an outline away from the keyboard.
fn bateri() -> TerminalLook {
    TerminalLook {
        caret: TerminalCaret {
            shape: Some(CaretShape::Block),
            width: Some(1.5),
            radius: Some(0.125),
            glow: Some(1.0),
            blink: Some(true),
            blink_interval: Some(0.5),
            unfocused: Some(CaretUnfocused::Hollow),
            motion: Some(CaretMotion::Spring(30.0)),
        },
        selection: TerminalSelection { radius: Some(0.25) },
    }
}

/// What a page shows that runs `checks` on a host that says `look` and `draws`, Lua with `terminal`
/// (`@pencere/terminal`), `check(what, got, want)` (values and tables compared by what they hold) at
/// hand: "ok", or each check that came out otherwise.
fn checked(look: TerminalLook, draws: Vec<Drawing>, checks: &str) -> Vec<String> {
    let script = format!(
        r#"
local terminal = require("@pencere/terminal")
local failures = {{}}
local function show(value)
    if type(value) ~= "table" then
        return tostring(value)
    end
    local keys = {{}}
    for key in value do
        keys[#keys + 1] = tostring(key)
    end
    table.sort(keys)
    local parts = {{}}
    for _, key in ipairs(keys) do
        parts[#parts + 1] = key .. "=" .. show(value[key] ~= nil and value[key] or value[tonumber(key)])
    end
    return "{{" .. table.concat(parts, ", ") .. "}}"
end
local function check(what, got, want)
    if show(got) ~= show(want) then
        failures[#failures + 1] = what .. ": " .. show(got) .. ", not " .. show(want)
    end
end
{checks}
return {{ render = function()
    local root = pencere.div()
    if #failures == 0 then
        root:child(pencere.text("ok"))
    end
    for _, failure in ipairs(failures) do
        root:child(pencere.text(failure))
    end
    return root
end }}
"#
    );
    let mut page = open_script_for(
        &script,
        ModuleLimits::default(),
        Arc::new(Terminal { look, draws }),
    );
    page.module.render(crate::guests_tests::SIZE, 0.0, false);
    let update = page.until(Duration::from_secs(20), |u| {
        u.frame.is_some() || u.closed.is_some()
    });
    if let Some(closed) = &update.closed {
        panic!("the page closed: {closed:?}");
    }
    words(&update.frame.expect("a page").root)
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_host_that_says_nothing_gives_no_look_and_no_drawing_and_the_options_back_as_they_were() {
    let shown = checked(
        TerminalLook::default(),
        Vec::new(),
        r#"
        check("look", pencere.terminal_look(), {})
        check("draws", pencere.draws(), {})
        local options = { gutter = false, caret = { shape = "bar" }, selection = { radius = 0.3 } }
        check("same", terminal.editor(options), options)
        check("no options", terminal.editor(), {})
        check("a copy", terminal.editor(options) ~= options, true)
        "#,
    );
    assert_eq!(shown, ["ok"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_host_that_says_some_gives_what_it_says_and_what_it_draws() {
    let shown = checked(
        bateri(),
        vec![Drawing::Shadow],
        r#"
        local look = pencere.terminal_look()
        check("caret", look.caret, {
            shape = "block", width = 1.5, radius = 0.125, glow = 1, blink = true, blink_interval = 0.5,
            unfocused = "hollow", motion = { spring = 30 },
        })
        check("selection", look.selection, { radius = 0.25 })
        check("draws", pencere.draws(), { shadow = true })
        "#,
    );
    assert_eq!(shown, ["ok"]);
    // A host that says one field says that field alone.
    let some = TerminalLook {
        caret: TerminalCaret {
            glow: Some(2.0),
            motion: Some(CaretMotion::Snap),
            ..TerminalCaret::default()
        },
        ..TerminalLook::default()
    };
    let shown = checked(
        some,
        Vec::new(),
        r#"
        check("look", pencere.terminal_look(), { caret = { glow = 2, motion = "snap" } })
        check("draws", pencere.draws(), {})
        "#,
    );
    assert_eq!(shown, ["ok"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn the_helper_puts_the_terminals_look_where_the_page_said_nothing_a_field_at_a_time() {
    let shown = checked(
        bateri(),
        vec![Drawing::Shadow],
        r#"
        local all = {
            gutter = false,
            caret = { shape = "block", width = 1.5, radius = 0.125, glow = 1, blink = true,
                blink_interval = 0.5, unfocused = "hollow", motion = { spring = 30 } },
            selection = { radius = 0.25 },
        }
        check("all of it", terminal.editor({ gutter = false }), all)
        -- A page that always wants a bar keeps its bar and takes the rest, field by field: a table
        -- of one field does not lose the terminal's others.
        local bar = terminal.editor({ caret = { shape = "bar" } })
        check("bar", bar.caret, { shape = "bar", width = 1.5, radius = 0.125, glow = 1, blink = true,
            blink_interval = 0.5, unfocused = "hollow", motion = { spring = 30 } })
        -- What the page wrote stays, false and 0 included.
        local own = terminal.editor({ caret = { blink = false, glow = 0 }, selection = { radius = 0 } })
        check("false", own.caret.blink, false)
        check("zero", own.caret.glow, 0)
        check("zero selection", own.selection.radius, 0)
        -- The options it was given are not changed.
        local given = { caret = { shape = "underline" } }
        terminal.editor(given)
        check("given", given, { caret = { shape = "underline" } })
        -- A look the page gives is the look: a settings view's own, not the host's.
        check("look given", terminal.editor({}, { caret = { glow = 3 } }), { caret = { glow = 3 } })
        check("empty look", terminal.editor({ gutter = false }, {}), { gutter = false })
        "#,
    );
    assert_eq!(shown, ["ok"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn the_helper_is_a_mistake_for_what_is_not_options_or_a_look() {
    let shown = checked(
        TerminalLook::default(),
        Vec::new(),
        r#"
        check("options", pcall(terminal.editor, 3), false)
        check("look", pcall(terminal.editor, {}, "bar"), false)
        check("caret", pcall(terminal.editor, { caret = "block" }, { caret = { glow = 1 } }), false)
        "#,
    );
    assert_eq!(shown, ["ok"]);
}
