//! A Lua page's editors as the sandbox holds them: what a page may hand them in a reply and say of
//! how they look, refused where it is made. What a page with editors does when a host drives it —
//! the editors' events, their texts posted before them, its languages and timers — is driven by the
//! host with no screen of `pencere-drive` (`kit/drive/src/luau_editor_tests.rs`). Heavy: the
//! interpreter is compiled and run (`make check-heavy`).

use std::time::Duration;

use pencere_view::{Event, Id};

use crate::guests_tests::SIZE;
use crate::luau_tests::{failed, open_script};
use crate::{Fault, ModuleLimits};

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_reply_that_hands_an_editor_more_than_a_reply_may_is_not_a_reply() {
    let mut page = open_script(
        r#"
        return {
            render = function() return pencere.editor("e") end,
            handle = function()
                return { editor = { e = { load = string.rep("a", 1024 * 1024 + 1) } } }
            end,
        }
        "#,
        ModuleLimits::default(),
    );
    page.module.render(SIZE, 0.0, false);
    page.until(Duration::from_secs(20), |u| u.frame.is_some());
    page.module.event(Event::Command(Id::named("anything")));
    let update = page.until(Duration::from_secs(20), |u| u.closed.is_some());
    assert!(
        matches!(&update.closed, Some(Fault::Malformed(why)) if why.contains("bytes of text for editors")),
        "{:?}",
        update.closed
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn an_option_an_editor_has_no_such_name_for_is_a_mistake_said_where_it_is_made() {
    for (options, said) in [
        ("{ colours = {} }", "an editor has no option colours"),
        (
            "{ caret = { colour = \"accent\" } }",
            "an editor has no option caret.colour",
        ),
        (
            "{ numbers = \"roman\" }",
            "\"roman\" is not one of the words it takes",
        ),
        ("{ caret = \"block\" }", "the caret of an editor is a table"),
        ("{ colors = { selection = true } }", "colour"),
        (
            "{ indent = { increse = \"then$\" } }",
            "an editor has no option indent.increse",
        ),
        (
            "{ comment = { block = \"/*\" } }",
            "the block comment of an editor is a pair",
        ),
        (
            "{ folds = \"syntax\" }",
            "\"syntax\" is not one of the words it takes",
        ),
        (
            "{ whitespace = \"dots\" }",
            "\"dots\" is not one of the words it takes",
        ),
        (
            "{ colors = { indent_guide = \"red\" } }",
            "an editor has no option colors.indent_guide",
        ),
    ] {
        let script =
            format!("return {{ render = function() return pencere.editor(\"e\", {options}) end }}");
        let mut page = open_script(&script, ModuleLimits::default());
        page.module.render(SIZE, 0.0, false);
        let update = page.until(Duration::from_secs(30), |u| u.closed.is_some());
        let why = failed(&update);
        assert!(why.contains(said), "{options}: {why}");
    }
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_rule_of_an_editors_indent_that_is_no_pattern_fails_the_page_by_the_options_name() {
    let mut page = open_script(
        r#"return { render = function()
            return pencere.editor("e", { indent = { increase = "(then" } })
        end }"#,
        ModuleLimits::default(),
    );
    page.module.render(SIZE, 0.0, false);
    let update = page.until(Duration::from_secs(30), |u| u.closed.is_some());
    assert!(
        matches!(&update.closed, Some(Fault::Malformed(why))
            if why.contains("indent.increase") && why.contains("(then") && why.contains("unclosed group")),
        "{:?}",
        update.closed
    );
}
