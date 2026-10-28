//! `@pencere/languages` on a page with editors, driven by the host with no screen: an editor told a
//! language's `config` comments, indents and closes its brackets as the language does, and coloured
//! by the language's `syntax` (with `@pencere/syntax`) shows the lines in view in the looks of
//! `languages.styles`, a page of HTML its scripts in JavaScript's. Heavy: the interpreter is
//! compiled and run (`make check-heavy`).

use pencere_view::edit::{CommentTokens, EditCommand, Folds, Look, Mark};
use pencere_view::{Id, Role, Source};

use crate::headless::{Error, Headless};

/// A page of one editor, `e`, told the language `name`'s config and coloured by its syntax, which
/// loads `document` when it is first in view.
fn page_of(name: &str, document: &str) -> String {
    format!(
        r#"
local Syntax = require("@pencere/syntax")
local languages = require("@pencere/languages")
local language = languages.load({name:?})
local syntax = Syntax.new(language.syntax, "e")
local loaded = false
return {{
    render = function()
        return pencere.div():size_full():child(pencere.editor("e", language.config):grow(1))
    end,
    handle = function(event)
        local ask = nil
        if event.type == "viewport" and not loaded then
            loaded = true
            ask = {{ editor = {{ e = {{ load = {document:?} }} }} }}
        end
        syntax:handle(event)
        return syntax:reply(ask)
    end,
}}
"#
    )
}

fn open(script: &str) -> Result<Headless, Error> {
    Headless::script(script).size(1000.0, 700.0).open()
}

/// The style of `languages.styles` a look is, by its colour (a role, or a role taken toward
/// another) and whether it is italic.
fn style_of(look: &Look) -> &'static str {
    let Some(color) = look.color else {
        return "uncoloured";
    };
    let toward = color.mix.map(|mix| mix.toward);
    match (color.source, toward, look.italic) {
        (Source::Role(Role::Accent), None, Some(true)) => "keyword.modifier",
        (Source::Role(Role::Accent), None, _) => "keyword",
        (Source::Role(Role::Accent), Some(Source::Role(Role::Info)), _) => "function",
        (Source::Role(Role::Success), None, _) => "string",
        (Source::Role(Role::Warning), None, _) => "number",
        (Source::Role(Role::Info), None, _) => "type",
        (Source::Role(Role::Error), None, _) => "tag",
        (Source::Role(Role::Dim), Some(Source::Role(Role::Quiet)), Some(true)) => "comment",
        (Source::Role(Role::Foreground), Some(Source::Role(Role::Background)), _) => "punctuation",
        (Source::Role(Role::Foreground), Some(Source::Role(Role::Info)), _) => "property",
        _ => "other",
    }
}

/// What the `syntax` layer of the editor `e` gives line `line`: each look from where to where in
/// the line (characters), by the style it is.
fn looks(page: &Headless, line: usize) -> Vec<(usize, usize, &'static str)> {
    let buffer = page.editor("e").expect("the editor");
    let Some(layer) = buffer
        .layer_names()
        .iter()
        .position(|name| name == "syntax")
    else {
        return Vec::new();
    };
    let start = buffer.line_start(line);
    buffer
        .decorations_in(start, buffer.line_end(line))
        .into_iter()
        .filter(|(index, _)| *index == layer)
        .map(|(_, decoration)| {
            let Mark::Style(look) = &decoration.mark else {
                panic!("a syntax colours, it does not mark: {decoration:?}");
            };
            (
                decoration.from - start,
                decoration.to - start,
                style_of(look),
            )
        })
        .collect()
}

const RUST: &str = "// a comment\nfn main() {\n    let s = \"text\";\n    let n = 42;\n}\n";

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn an_editor_told_rusts_config_and_coloured_by_its_syntax_shows_rust_in_the_themes_looks()
-> Result<(), Error> {
    let page = open(&page_of("Rust", RUST))?;
    assert_eq!(page.editor_text("e").as_deref(), Some(RUST));
    assert_eq!(looks(&page, 0), [(0, 12, "comment")]);
    assert_eq!(
        looks(&page, 1),
        [
            (0, 2, "keyword"),
            (3, 7, "function"),
            (7, 9, "punctuation"),
            (10, 11, "punctuation")
        ]
    );
    assert_eq!(
        looks(&page, 2),
        [
            (4, 7, "keyword"),
            (10, 11, "punctuation"),
            (12, 18, "string"),
            (18, 19, "punctuation")
        ]
    );
    assert!(looks(&page, 3).contains(&(12, 14, "number")));
    // The editor took what the language tells it.
    let options = page
        .laid()
        .expect("laid out")
        .editors()
        .into_iter()
        .find(|room| room.id == Id::named("e"))
        .expect("the editor's room")
        .options;
    assert_eq!(
        options.comment,
        CommentTokens {
            line: Some("//".into()),
            block: Some(("/*".into(), "*/".into())),
        }
    );
    assert_eq!(options.indent.decrease.as_deref(), Some(r"^\s*[\}\]\)]"));
    assert_eq!(options.folds, Folds::Indent);
    assert!(options.pairs.contains(&('{', '}')));
    assert!(
        !options.pairs.contains(&('\'', '\'')),
        "a lifetime is a lone quote"
    );
    assert_eq!((options.hard_tabs, options.tab_width), (false, 4));
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn an_editor_told_a_languages_config_indents_closes_brackets_and_comments_as_the_language_does()
-> Result<(), Error> {
    let mut page = open(&page_of("Rust", ""))?;
    page.focus_editor("e")?;
    page.type_text("fn f() {\nx")?;
    assert_eq!(page.editor_text("e").as_deref(), Some("fn f() {\n    x\n}"));
    page.edit(EditCommand::ToggleComment)?;
    assert_eq!(
        page.editor_text("e").as_deref(),
        Some("fn f() {\n    // x\n}")
    );
    // Go's one formatter indents with tabs, and the language says so.
    let mut page = open(&page_of("Go", ""))?;
    page.focus_editor("e")?;
    page.type_text("func f() {\nx")?;
    assert_eq!(page.editor_text("e").as_deref(), Some("func f() {\n\tx\n}"));
    // SQL's comment is `--`, and a line break after an open bracket indents.
    let mut page = open(&page_of("PostgreSQL", ""))?;
    page.focus_editor("e")?;
    page.type_text("select (\n1")?;
    page.edit(EditCommand::ToggleComment)?;
    assert_eq!(
        page.editor_text("e").as_deref(),
        Some("select (\n    -- 1\n)")
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_page_of_html_colours_the_javascript_of_its_scripts_and_the_css_of_its_styles()
-> Result<(), Error> {
    let html = "<p>text</p>\n<script>\nconst n = 1;\n</script>\n<script>let a = \"x\";</script>\n\
                <style>p { color: red; }</style>\n";
    let page = open(&page_of("HTML", html))?;
    assert_eq!(looks(&page, 0), [(0, 3, "tag"), (7, 11, "tag")]);
    assert_eq!(looks(&page, 1), [(0, 8, "tag")]);
    let script = looks(&page, 2);
    assert!(script.contains(&(0, 5, "keyword")), "{script:?}");
    assert!(script.contains(&(10, 11, "number")), "{script:?}");
    assert_eq!(looks(&page, 3), [(0, 9, "tag")]);
    // A script on one line with its tags: the language reads to the tag that closes it.
    assert_eq!(
        looks(&page, 4),
        [
            (0, 8, "tag"),
            (8, 11, "keyword"),
            (14, 15, "punctuation"),
            (16, 19, "string"),
            (19, 20, "punctuation"),
            (20, 29, "tag")
        ]
    );
    let style = looks(&page, 5);
    assert!(style.contains(&(11, 16, "property")), "{style:?}");
    assert!(style.contains(&(24, 32, "tag")), "{style:?}");
    Ok(())
}
