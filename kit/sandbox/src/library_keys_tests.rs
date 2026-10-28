//! `@pencere/keys` of the kit's library, run by the interpreter: keys written as words read as a
//! command reads them, written as a Mac and as any other platform writes them, a key event matched
//! against them as the host matches a command's, and the errors it raises for what its caller gave
//! it. Heavy: the interpreter is compiled and run (`make check-heavy`).

use crate::ModuleLimits;
use crate::luau_tests::{failed, first, words};

/// What a page shows that runs `checks`, Lua with `keys` and `check(what, got, want)` at hand
/// (lists compared by what they hold, a table of modifiers by the four of them): "ok", or each check
/// that came out otherwise.
fn checked(checks: &str) -> Vec<String> {
    let script = format!(
        r#"
local keys = require("@pencere/keys")
local failures = {{}}
local function show(value)
    if type(value) ~= "table" then
        return tostring(value)
    end
    if value.shift ~= nil or value.command ~= nil then
        local held = {{}}
        for _, name in ipairs({{ "control", "alt", "shift", "command" }}) do
            if value[name] then
                held[#held + 1] = name
            end
        end
        return "mods(" .. table.concat(held, " ") .. ")"
    end
    local parts = {{}}
    for k, item in ipairs(value) do
        parts[k] = show(item)
    end
    if value.key ~= nil then
        parts[#parts + 1] = "key=" .. show(value.key) .. " " .. show(value.mods)
    end
    return "{{" .. table.concat(parts, ", ") .. "}}"
end
local function check(what, got, want)
    if show(got) ~= show(want) then
        failures[#failures + 1] = what .. ": " .. show(got) .. ", not " .. show(want)
    end
end
local function mods(...)
    local out = {{ shift = false, control = false, alt = false, command = false }}
    for _, name in ipairs({{ ... }}) do
        out[name] = true
    end
    return out
end
local function key(name, ...)
    return {{ type = "key", key = name, mods = mods(...) }}
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
    let update = first(&script, ModuleLimits::default());
    if let Some(closed) = &update.closed {
        panic!("the page closed: {closed:?}");
    }
    words(&update.frame.expect("a page").root)
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn keys_are_read_as_a_command_reads_them_and_named_as_a_key_event_names_them() {
    let shown = checked(
        r#"
        check("letter", keys.parse("cmd+shift+k"), { key = "k", mods = mods("shift", "command") })
        check("as written", keys.parse("CMD+Shift+K"), { key = "K", mods = mods("shift", "command") })
        check("other names", keys.parse("option+control+super+x"), { key = "x", mods = mods("alt", "control", "command") })
        check("page up", keys.parse("ctrl+pageup"), { key = "page_up", mods = mods("control") })
        check("page down", keys.parse("PageDown"), { key = "page_down", mods = mods() })
        check("delete", keys.parse("alt+del"), { key = "delete", mods = mods("alt") })
        check("return", keys.parse("cmd+return"), { key = "enter", mods = mods("command") })
        check("esc", keys.parse("Esc"), { key = "escape", mods = mods() })
        check("space", keys.parse("ctrl+space"), { key = " ", mods = mods("control") })
        check("function", keys.parse("F12"), { key = "f12", mods = mods() })
        check("a mark", keys.parse("cmd+/"), { key = "/", mods = mods("command") })
        check("outside ASCII", keys.parse("alt+ş"), { key = "ş", mods = mods("alt") })
        -- What is not keys is nil and why, for a page that reads a person's settings.
        check("no key", keys.parse("cmd+"), nil)
        check("why no key", select(2, keys.parse("cmd+")), '"cmd+" says no key after its modifiers')
        check("no such key", select(2, keys.parse("cmd+launch")), '"launch" is not a key: one character, '
            .. "or up, down, left, right, page_up, page_down, home, end, enter, escape, tab, backspace, "
            .. "delete, space or f1 to f20")
        check("no such modifier", select(2, keys.parse("hyper+k")), '"hyper" is not a modifier (cmd, ctrl, alt or shift)')
        check("f21", keys.parse("f21"), nil)
        "#,
    );
    assert_eq!(shown, ["ok"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_mac_writes_the_modifiers_in_apples_order_and_the_keys_as_its_glyphs() {
    let shown = checked(
        r#"
        check("platform", keys.platform(), "mac")
        check("order", keys.label("cmd+shift+k"), "⇧⌘K")
        check("either order", keys.label("shift+cmd+k"), "⇧⌘K")
        check("all four", keys.label("cmd+shift+alt+ctrl+a"), "⌃⌥⇧⌘A")
        check("caps", keys.caps("cmd+shift+k"), { "⇧", "⌘", "K" })
        local glyphs = {
            ["cmd+enter"] = "⌘↩", ["alt+backspace"] = "⌥⌫", delete = "⌦", escape = "⎋", tab = "⇥",
            ["shift+tab"] = "⇧⇥", ["cmd+space"] = "⌘␣", up = "↑", down = "↓", left = "←", right = "→",
            page_up = "⇞", page_down = "⇟", home = "↖", ["end"] = "↘", f5 = "F5", ["cmd+f12"] = "⌘F12",
            ["cmd+/"] = "⌘/", ["cmd+,"] = "⌘,", ["ctrl+1"] = "⌃1", ["cmd+ş"] = "⌘Ş", ["cmd+ö"] = "⌘Ö",
            ["alt+ж"] = "⌥Ж", ["cmd+K"] = "⌘K",
        }
        for words, label in pairs(glyphs) do
            check(words, keys.label(words), label)
        end
        -- What keys.parse made is taken as the words are.
        check("parsed", keys.label(keys.parse("ctrl+alt+delete")), "⌃⌥⌦")
        "#,
    );
    assert_eq!(shown, ["ok"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn elsewhere_the_keys_are_named_and_joined_and_the_command_key_is_super() {
    let shown = checked(
        r#"
        local other = { platform = "other" }
        check("order", keys.label("cmd+shift+k", other), "Shift+Super+K")
        check("all four", keys.label("cmd+shift+alt+ctrl+a", other), "Ctrl+Alt+Shift+Super+A")
        check("caps", keys.caps("ctrl+shift+k", other), { "Ctrl", "Shift", "K" })
        local names = {
            ["ctrl+enter"] = "Ctrl+Enter", ["alt+backspace"] = "Alt+Backspace", delete = "Delete",
            escape = "Esc", tab = "Tab", ["ctrl+space"] = "Ctrl+Space", up = "Up", down = "Down",
            left = "Left", right = "Right", ["ctrl+page_up"] = "Ctrl+PageUp", page_down = "PageDown",
            home = "Home", ["end"] = "End", f5 = "F5", ["ctrl+/"] = "Ctrl+/", ["ctrl+ş"] = "Ctrl+Ş",
        }
        for words, label in pairs(names) do
            check(words, keys.label(words, other), label)
        end
        -- A page that knows its platform says so once; a call may still say another.
        keys.set_platform("other")
        check("set", { keys.platform(), keys.label("cmd+k") }, { "other", "Super+K" })
        check("one call", keys.label("cmd+k", { platform = "mac" }), "⌘K")
        keys.set_platform("mac")
        check("back", keys.label("cmd+k"), "⌘K")
        "#,
    );
    assert_eq!(shown, ["ok"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_key_event_is_the_shortcut_with_the_same_modifiers_and_the_key_in_either_case() {
    let shown = checked(
        r#"
        check("same", keys.matches(key("k", "command"), "cmd+k"), true)
        check("a capital", keys.matches(key("K", "command", "shift"), "cmd+shift+k"), true)
        check("either case", keys.matches(key("K", "command"), "cmd+k"), true)
        check("a modifier more", keys.matches(key("k", "command", "shift"), "cmd+k"), false)
        check("a modifier fewer", keys.matches(key("k"), "cmd+k"), false)
        check("another key", keys.matches(key("j", "command"), "cmd+k"), false)
        check("named", keys.matches(key("enter", "command"), "cmd+return"), true)
        check("page down", keys.matches(key("page_down", "control"), "ctrl+pagedown"), true)
        check("space", keys.matches(key(" "), "space"), true)
        check("function", keys.matches(key("f5"), "F5"), true)
        check("outside ASCII", keys.matches(key("Ş", "shift"), "shift+ş"), true)
        check("dotted I", keys.matches(key("İ", "shift"), "shift+i"), true)
        check("a name is no letter", keys.matches(key("Enter"), "enter"), false)
        check("key up", keys.matches({ type = "key_up", key = "k", mods = mods("command") }, "cmd+k"), true)
        check("no type", keys.matches({ key = "k", mods = { command = true } }, "cmd+k"), true)
        check("not a key", keys.matches({ type = "text", field = "f", text = "k" }, "k"), false)
        check("parsed", keys.matches(key("k", "alt"), keys.parse("option+k")), true)
        "#,
    );
    assert_eq!(shown, ["ok"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn keys_given_wrong_are_an_error_at_the_callers_line() {
    let cases = [
        (
            r#"keys.label("cmd+launch")"#,
            "\"launch\" is not a key: one character, or up, down, left, right, page_up, page_down, \
             home, end, enter, escape, tab, backspace, delete, space or f1 to f20",
        ),
        (
            r#"keys.caps("hyper+k")"#,
            r#""hyper" is not a modifier (cmd, ctrl, alt or shift)"#,
        ),
        (
            r#"keys.label(5)"#,
            r#"keys.label takes keys written as words ("cmd+k") or what keys.parse made of them, not a number"#,
        ),
        (
            r#"keys.parse(nil)"#,
            r#"keys.parse takes keys written as words ("cmd+k"), not a nil"#,
        ),
        (
            r#"keys.set_platform("linux")"#,
            r#"a platform is "mac" or "other", not linux"#,
        ),
        (
            r#"keys.caps("cmd+k", { platform = "windows" })"#,
            r#"a platform is "mac" or "other", not windows"#,
        ),
        (
            r#"keys.label("cmd+k", { os = "mac" })"#,
            "keys.label has no option os (platform)",
        ),
        (
            r#"keys.matches(nil, "cmd+k")"#,
            "keys.matches takes a key event, then keys, not a nil",
        ),
        (
            r#"keys.matches({ type = "key", key = "k", mods = {} }, "cmd+")"#,
            r#""cmd+" says no key after its modifiers"#,
        ),
    ];
    for (call, why) in cases {
        let script = format!("local keys = require('@pencere/keys')\n{call}\nreturn {{}}");
        let said = failed(&first(&script, ModuleLimits::default()));
        assert_eq!(said, format!("script.lua:2: {why}"), "{call}");
    }
}
