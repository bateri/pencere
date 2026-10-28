//! `@pencere/fuzzy` of the kit's library, run by the interpreter: how it scores and orders what a
//! person types against texts, where it says the characters are, its lists all at once and a step
//! at a time, the parts it makes of a text for a text of many looks, and the errors it raises for
//! what its caller gave it. A search that goes on by itself, a frame at a time, is driven by the host
//! with no screen of `pencere-drive` (`kit/drive/src/library_fuzzy_tests.rs`). Heavy: the
//! interpreter is compiled and run (`make check-heavy`).

use pencere_view::wire::Kind;
use pencere_view::{Family, Role, rich, span};

use crate::ModuleLimits;
use crate::luau_tests::{failed, first, words};

/// What a page shows that runs `checks`, Lua with `fuzzy` and these at hand: `check(what, got,
/// want)` (lists compared by what they hold), `places(pattern, text, options)` and `score(...)`
/// (what `fuzzy.match` says), `ranks(pattern, texts, options)` (each text scores more than the
/// next) and `indexes(found)` (the items' indexes of what a list found). It shows "ok", or each
/// check that came out otherwise.
fn checked(checks: &str) -> Vec<String> {
    let script = format!(
        r#"
local fuzzy = require("@pencere/fuzzy")
local failures = {{}}
local function show(value)
    if type(value) ~= "table" then
        return tostring(value)
    end
    local parts = {{}}
    for k, item in ipairs(value) do
        parts[k] = show(item)
    end
    return "{{" .. table.concat(parts, ", ") .. "}}"
end
local function check(what, got, want)
    if show(got) ~= show(want) then
        failures[#failures + 1] = what .. ": " .. show(got) .. ", not " .. show(want)
    end
end
local function places(pattern, text, options)
    local _, found = fuzzy.match(pattern, text, options)
    return found
end
local function score(pattern, text, options)
    return (fuzzy.match(pattern, text, options))
end
local function ranks(pattern, texts, options)
    for k = 1, #texts - 1 do
        local a, b = score(pattern, texts[k], options), score(pattern, texts[k + 1], options)
        if a == nil or (b ~= nil and a <= b) then
            failures[#failures + 1] = pattern .. ": " .. texts[k] .. " (" .. tostring(a) .. ") is not above "
                .. texts[k + 1] .. " (" .. tostring(b) .. ")"
        end
    end
end
local function indexes(found)
    local out = {{}}
    for k, row in ipairs(found) do
        out[k] = row.index
    end
    return out
end
-- 300 words of the letters a to h, the same every time.
local many = {{}}
local seed = 7
for k = 1, 300 do
    local word = {{}}
    for c = 1, 6 + k % 7 do
        seed = (seed * 1103515245 + 12345) % 2147483648
        word[c] = string.char(97 + (seed // 65536) % 8)
    end
    many[k] = table.concat(word)
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
fn a_match_at_the_start_of_a_word_scores_above_one_in_the_middle_of_a_word() {
    let shown = checked(
        r#"
        ranks("fb", { "foo_bar", "foobar" })
        ranks("set", { "set_value", "reset" })
        -- The text's start, then after a space, a delimiter, any other mark, and none.
        ranks("bar", { "bar_foo", "foo bar", "foo/bar", "foo-bar", "foobar" })
        check("after _ . -", { score("b", "a_b"), score("b", "a.b") }, { score("b", "a-b"), score("b", "a-b") })
        check("after / \\ :", { score("b", "a/b"), score("b", "a\\b") }, { score("b", "a:b"), score("b", "a:b") })
        check("where", places("bar", "foobar foo_bar"), { 12, 13, 14 })
        -- Of two matches alike, the earlier: a first character late in the text costs.
        ranks("open", { "Open File", "File: Open" })
        ranks("x", { "x", "ax", "aax" })
        "#,
    );
    assert_eq!(shown, ["ok"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_run_of_characters_scores_above_the_same_characters_apart_and_a_longer_gap_costs_more() {
    let shown = checked(
        r#"
        ranks("abc", { "abc_def", "a_b_c" })
        ranks("abc", { "xabcx", "xaxbxc" })
        ranks("ab", { "a_b", "a__b", "a___b" })
        ranks("ord", { "orders.ts", "old_records.ts" })
        check("the run", places("abc", "a_b_c abc"), { 7, 8, 9 })
        check("a run that starts a word", places("bar", "rebar bar"), { 7, 8, 9 })
        "#,
    );
    assert_eq!(shown, ["ok"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_capital_after_a_small_letter_and_a_digit_after_a_letter_start_a_word() {
    let shown = checked(
        r#"
        ranks("gu", { "getUser", "glue" })
        ranks("fb", { "fooBar", "foobar" })
        check("camel", places("fb", "fooBar"), { 1, 4 })
        check("a digit as a capital", score("2", "abc2"), score("C", "abcC"))
        -- A capital after a capital is not a word's start.
        ranks("b", { "aB", "AB" })
        "#,
    );
    assert_eq!(shown, ["ok"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn in_a_path_a_match_in_the_files_name_scores_above_any_over_its_directories() {
    let shown = checked(
        r#"
        local path = { path = true }
        ranks("main", { "src/main.rs", "main/lib.rs" }, path)
        ranks("button", { "src/ui/Button.tsx", "src/button/index.ts" }, path)
        check("in the name", places("main", "main/src/main.rs", path), { 10, 11, 12, 13 })
        -- Characters apart in the name are above a run in a directory; as texts, the other way.
        ranks("abc", { "x/a_b_c.txt", "abc/x.txt" }, path)
        ranks("abc", { "abc/x.txt", "x/a_b_c.txt" })
        -- A path with no directory is all its name, and a separator at the end is not one.
        ranks("lib", { "lib.rs", "src/lib/x.rs" }, path)
        check("a directory's name", places("lib", "src/lib/", path), { 5, 6, 7 })
        -- A `/` and a `\` are one, and a pattern with one in it is matched against the whole path.
        check("either separator", places("src/m", "src\\main.rs", path), { 1, 2, 3, 4, 5 })
        check("not in a text", score("src/m", "src\\main.rs"), nil)
        ranks("s/m", { "src/main.rs", "lib/sub/mod.rs" }, path)
        -- Spread over the directories and the name, it is still a match.
        check("over the whole", places("slm", "src/lib/main.rs", path), { 1, 5, 9 })
        "#,
    );
    assert_eq!(shown, ["ok"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn smart_case_tells_a_capital_apart_only_where_the_pattern_has_one_in_any_script() {
    let shown = checked(
        r#"
        check("a capital asked", score("Fo", "foo"), nil)
        check("a capital found", places("Fo", "Foo"), { 1, 2 })
        check("small letters", places("fo", "FOO"), { 1, 2 })
        check("ignore", places("Fo", "foo", { case = "ignore" }), { 1, 2 })
        check("respect", score("fo", "Foo", { case = "respect" }), nil)
        check("respect found", places("fo", "Foo foo", { case = "respect" }), { 5, 6 })
        check("a capital outside ASCII", score("Çiz", "çizelge"), nil)
        check("found outside ASCII", places("Çiz", "Çizelge"), { 1, 2, 3 })
        check("Turkish", places("ölç", "ÖLÇÜM"), { 1, 2, 3 })
        check("dotted I", places("istanbul", "İSTANBUL"), { 1, 2, 3, 4, 5, 6, 7, 8 })
        check("Greek", places("ελ", "ΕΛΛΑΔΑ"), { 1, 2 })
        check("Cyrillic", places("мир", "МИР"), { 1, 2, 3 })
        check("Polish", places("łódź", "ŁÓDŹ"), { 1, 2, 3, 4 })
        -- A space in the pattern is not looked for.
        check("spaces", places("foo bar", "foobar"), { 1, 2, 3, 4, 5, 6 })
        check("only spaces", { fuzzy.match("  ", "abc") }, { 0, {} })
        "#,
    );
    assert_eq!(shown, ["ok"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn places_are_characters_from_one_in_a_text_of_any_script() {
    let shown = checked(
        r#"
        check("Turkish", places("çiz", "Ölçüm çizelgesi"), { 7, 8, 9 })
        check("a letter of two bytes", places("ğ", "dağ"), { 3 })
        check("Greek", places("γ", "αβγ"), { 3 })
        check("four bytes", places("😀b", "a😀b"), { 2, 3 })
        check("not UTF-8", places("ab", "\255ab"), { 2, 3 })
        -- A text longer than is read at once.
        local long = string.rep("é", 3000) .. "xyz"
        check("long", places("xyz", long), { 3001, 3002, 3003 })
        -- Far apart, characters score as those near the end alone: a gap long enough is worth
        -- nothing.
        local apart = string.rep("a", 2000) .. string.rep("b", 2000) .. "c"
        check("apart", places("abc", apart), { 2000, 4000, 4001 })
        -- A table of scores too large: where the pattern is first found, from its latest start.
        local spread = string.rep("a", 6000) .. string.rep("b", 6000) .. "c"
        check("spread", places("abc", spread), { 6000, 6001, 12001 })
        check("nothing", { fuzzy.match("", "abc") }, { 0, {} })
        check("none", score("abc", "xyz"), nil)
        "#,
    );
    assert_eq!(shown, ["ok"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn the_first_found_highlight_is_where_the_pattern_is_first_all_there_from_its_latest_start() {
    let shown = checked(
        r#"
        -- The rule a list of rows draws its matches by: C, B and O of CrashLoopBackOff.
        local first = { highlight = "first" }
        check("rows", places("cbo", "CrashLoopBackOff", first), { 1, 10, 14 })
        check("best", places("cbo", "CrashLoopBackOff"), { 1, 10, 14 })
        -- The best is the word's start and a gap; the first found ends earliest, starting late.
        check("best apart", places("ab", "a_xab"), { 1, 5 })
        check("first apart", places("ab", "a_xab", first), { 4, 5 })
        check("one score", score("ab", "a_xab", first), score("ab", "a_xab"))
        check("first in a path's name", places("ab", "ab/a_xab", { path = true, highlight = "first" }), { 7, 8 })
        "#,
    );
    assert_eq!(shown, ["ok"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_list_is_sorted_by_score_then_by_the_order_of_its_items_and_kept_to_its_limit() {
    let shown = checked(
        r#"
        local items = { "foobar", "foo_bar", "xyz", "foo_bar", "fb" }
        local found = fuzzy.filter("fb", items)
        check("order", indexes(found), { 5, 2, 4, 1 })
        check("first", { found[1].item, found[1].index, found[1].score, found[1].positions }, { "fb", 5, 62, { 1, 2 } })
        check("limit 2", indexes(fuzzy.filter("fb", items, { limit = 2 })), { 5, 2 })
        check("limit 3", indexes(fuzzy.filter("fb", items, { limit = 3 })), { 5, 2, 4 })
        check("first highlight", fuzzy.filter("ab", { "a_xab" }, { highlight = "first" })[1].positions, { 4, 5 })
        -- The best ten of many, kept as they are found, are the first ten of all of them.
        local all = fuzzy.filter("abc", many)
        local best = fuzzy.filter("abc", many, { limit = 10 })
        check("many", #all > 20, true)
        check("best of many", indexes(best), { table.unpack(indexes(all), 1, 10) })
        -- Items that are not strings: a field, or a function.
        local people = { { name = "Ada Lovelace" }, { name = "Alan Turing" }, { name = "Grace Hopper" } }
        check("field", indexes(fuzzy.filter("al", people, { text = "name" })), { 2, 1 })
        check("function", indexes(fuzzy.filter("al", people, { text = function(person) return person.name end })), { 2, 1 })
        check("numbers", indexes(fuzzy.filter("1", { 10, 2, 31 })), { 1, 3 })
        -- An empty pattern: every item, in order, with no places.
        local everything = fuzzy.filter("", { "b", "a", "c" })
        check("empty", indexes(everything), { 1, 2, 3 })
        check("empty scores", { everything[1].score, everything[1].positions }, { 0, {} })
        check("empty limit", indexes(fuzzy.filter(" ", { "b", "a", "c" }, { limit = 2 })), { 1, 2 })
        check("no items", #fuzzy.filter("a", {}), 0)
        "#,
    );
    assert_eq!(shown, ["ok"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_search_goes_a_step_at_a_time_and_finds_what_a_filter_finds() {
    let shown = checked(
        r#"
        local search = fuzzy.search("abc", many, { limit = 10 })
        check("not yet", { search:done(), search:progress() }, { false, 0, 300 })
        check("nothing yet", #search:results(), 0)
        local steps = 0
        while not search:step(60) do
            steps += 1
        end
        check("steps", steps > 10, true)
        check("done", { search:done(), search:progress() }, { true, 300, 300 })
        check("found", indexes(search:results()), indexes(fuzzy.filter("abc", many, { limit = 10 })))
        -- A step goes through an item at least.
        local one = fuzzy.search("abc", many)
        one:step(1)
        check("one", { one:progress() }, { 1, 300 })
        -- Cancelled, it goes no further and keeps what it found.
        one:cancel()
        check("cancelled", { one:done(), one:step(), one:progress() }, { true, true, 1, 300 })
        -- A pattern that goes on from the one before goes through what that one found.
        local wide = fuzzy.search("ab", many)
        while not wide:step() do end
        local narrow = wide:narrow("abc")
        check("the wide one stops", wide:done(), true)
        check("narrowed", select(2, narrow:progress()), #fuzzy.filter("ab", many))
        while not narrow:step() do end
        check("narrowed found", indexes(narrow:results()), indexes(fuzzy.filter("abc", many)))
        local capital = narrow:narrow("aBc")
        while not capital:step() do end
        check("a capital narrows", indexes(capital:results()), indexes(fuzzy.filter("aBc", many)))
        -- One that does not goes through all the items.
        local other = narrow:narrow("hg")
        check("not narrowed", select(2, other:progress()), 300)
        -- One narrowed before the search is done goes through the rest as well.
        local half = fuzzy.search("a", many)
        half:step(1)
        local scanned = half:progress()
        local after = half:narrow("ab")
        check("half", select(2, after:progress()), #half:results() + 300 - scanned)
        while not after:step() do end
        check("half found", indexes(after:results()), indexes(fuzzy.filter("ab", many)))
        -- An empty pattern with a limit takes the first items and is done.
        local first = fuzzy.search("", many, { limit = 3 })
        check("empty", { first:step(), indexes(first:results()) }, { true, { 1, 2, 3 } })
        "#,
    );
    assert_eq!(shown, ["ok"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn the_parts_of_a_text_mark_the_characters_at_the_places_in_the_look_given() {
    let update = first(
        r#"
        local fuzzy = require("@pencere/fuzzy")
        local look = { color = "accent", weight = 700 }
        local _, places = fuzzy.match("cbo", "CrashLoopBackOff", { highlight = "first" })
        return { render = function()
            return pencere.div()
                :child(pencere.rich(fuzzy.spans("CrashLoopBackOff", places, look)))
                :child(pencere.rich(fuzzy.spans("foo_bar", { 1, 2, 5 })))
                :child(pencere.rich(fuzzy.spans("Ölçüm çizelgesi", { 7, 8, 9 }, { bg = "raised2", italic = true })))
                :child(pencere.rich(fuzzy.spans("src/lib/main.rs", { 1, 9, 10 }, { underline = true, family = "terminal" }, 9, 15)))
                :child(pencere.rich(fuzzy.spans("plain", {}, look)))
                :child(pencere.rich(fuzzy.spans("ab", { 0, 2, 9 }, look)))
        end }
        "#,
        ModuleLimits::default(),
    );
    assert_eq!(update.closed, None, "{:?}", update.closed);
    let nodes = update.frame.expect("a page").root.fold();
    let mark = |words: &str| span(words).color(Role::Accent).weight(700);
    let expected = [
        rich([
            mark("C"),
            span("rashLoop"),
            mark("B"),
            span("ack"),
            mark("O"),
            span("ff"),
        ]),
        rich([mark("fo"), span("o_"), mark("b"), span("ar")]),
        rich([
            span("Ölçüm "),
            span("çiz").bg(Role::Raised(2)).italic(),
            span("elgesi"),
        ]),
        rich([
            span("ma").underline().family(Family::Terminal),
            span("in.rs"),
        ]),
        rich([span("plain")]),
        rich([span("a"), mark("b")]),
    ];
    for (k, rust) in expected.into_iter().enumerate() {
        let Kind::Rich(said) = &nodes[k + 1].kind else {
            panic!("a text of many looks: {:?}", nodes[k + 1].kind);
        };
        let Kind::Rich(rust) = rust.fold().remove(0).kind else {
            panic!("rich");
        };
        assert_eq!(said, &rust, "text {k}");
    }
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_text_of_more_parts_than_a_text_may_have_ends_in_one_plain_part() {
    let shown = checked(
        r#"
        local every_other = {}
        for k = 1, 3000, 2 do
            every_other[#every_other + 1] = k
        end
        local parts = fuzzy.spans(string.rep("ab", 1500), every_other)
        check("parts", #parts, 1024)
        local last = parts[#parts]
        check("the rest", #last.text, 3000 - 1023)
        check("plain", next(last.look), nil)
        -- Marks that start a part's turn on the other foot: still no more parts than a text may
        -- have, and nothing of the text left out.
        local shifted = {}
        for k = 2, 3000, 2 do
            shifted[#shifted + 1] = k
        end
        local others = fuzzy.spans(string.rep("ab", 1500), shifted)
        check("at most", #others <= 1024, true)
        local words = {}
        for k, piece in ipairs(others) do
            words[k] = piece.text
        end
        check("all of it", table.concat(words), string.rep("ab", 1500))
        "#,
    );
    assert_eq!(shown, ["ok"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn what_the_caller_gives_wrong_is_an_error_at_the_callers_line() {
    let cases = [
        (
            r#"fuzzy.match(5, "x")"#,
            "fuzzy.match takes a pattern (a string), then a text, not a number",
        ),
        (
            r#"fuzzy.match("a", nil)"#,
            "fuzzy.match takes a pattern, then a text (a string), not a nil",
        ),
        (
            r#"fuzzy.match("a", "b", { case = "loud" })"#,
            r#"the case of a fuzzy match is "smart", "ignore" or "respect", not loud"#,
        ),
        (
            r#"fuzzy.match("a", "b", { highlight = "all" })"#,
            r#"the highlight of a fuzzy match is "best" or "first", not all"#,
        ),
        (
            r#"fuzzy.match("a", "b", { colour = 1 })"#,
            "fuzzy.match has no option colour (case, path, highlight)",
        ),
        (
            r#"fuzzy.filter("a", { {} })"#,
            "item 1 is a table: the text option says how to read its text",
        ),
        (
            r#"fuzzy.filter("a", { "x" }, { limit = 0 })"#,
            "the limit of fuzzy.filter is how many items it keeps at most: a whole number, 1 or more",
        ),
        (
            r#"fuzzy.filter("a", { "x" }, { text = 5 })"#,
            "the text option of fuzzy.filter is a function of an item or the name of its field, not a number",
        ),
        (
            r#"fuzzy.filter("a", { {} }, { text = function() return nil end })"#,
            "the text of item 1 is a nil, not a string",
        ),
        (
            r#"fuzzy.search("a", "abc")"#,
            "fuzzy.search takes a pattern, then the items (a list), not a string",
        ),
        (
            r#"fuzzy.search("a", { "x", true }):step()"#,
            "item 2 is a boolean: the text option says how to read its text",
        ),
        (
            r#"fuzzy.search("a", { "x" }):run(5)"#,
            "run takes the function told after each step, not a number",
        ),
        (
            r#"fuzzy.spans("ab", { 1 }, { colour = "accent" })"#,
            "a span says no colour (color, weight, italic, underline, strike, family, bg, link)",
        ),
        (
            r#"fuzzy.spans("ab", { "1" })"#,
            "position 1 of fuzzy.spans is a character (a number), not a string",
        ),
    ];
    for (call, why) in cases {
        let script = format!("local fuzzy = require('@pencere/fuzzy')\n{call}\nreturn {{}}");
        let said = failed(&first(&script, ModuleLimits::default()));
        assert_eq!(said, format!("script.lua:2: {why}"), "{call}");
    }
}
