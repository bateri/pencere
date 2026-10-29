//! A Lua page with an editor, driven by the host with no screen: the page laid out with the host's
//! editors (`pencere-editor`), what they tell sent to the page with the text it reads posted before
//! it, what the page asks of them done, its wakes given as the clock moves. The page is the example's
//! (`examples/editor-lua`), and some short scripts besides. Heavy: the interpreter is compiled and run
//! (`make check-heavy`).

use pencere_sandbox::Fault;
use pencere_view::edit::{
    Caret, CaretHeight, CaretShape, CommentTokens, EditorAsk, EditorColors, EditorOptions, Folds,
    IndentRules, LineHighlight, Mark, Matches, Numbers, Origin, SelectionLook, Tint, Whitespace,
};
use pencere_view::{Event, Id, Key, Mods, Paint, Point, Role};

use crate::Route;
use crate::headless::{Error, Headless};

const PAGE: &str = include_str!("../../../examples/editor-lua/src/view/page.lua");

/// A page under the host with no screen, of the size the tests of the sandbox lay a page out at.
fn open(script: &str) -> Result<Headless, Error> {
    Headless::script(script).size(1000.0, 700.0).open()
}

/// Why a page that is to fail failed, in a script's words.
fn failing(script: &str) -> String {
    failed(open(script).map(drop))
}

/// Why what was done failed the page.
fn failed(done: Result<(), Error>) -> String {
    match done {
        Err(Error::Ended(Fault::Failed(why))) => why,
        other => panic!("expected the page to fail, it is {other:?}"),
    }
}

/// The events of its editors the page was told, in order.
fn editor_events(page: &Headless) -> Vec<&Event> {
    page.told()
        .iter()
        .filter(|event| {
            matches!(
                event,
                Event::Edit { .. }
                    | Event::Selection { .. }
                    | Event::Viewport { .. }
                    | Event::EditorRefused { .. }
                    | Event::Found { .. }
            )
        })
        .collect()
}

/// The most decorations one reply of the page's handed the editors.
fn most(page: &Headless) -> usize {
    page.replies()
        .iter()
        .map(|answer| {
            answer
                .reply
                .editor
                .iter()
                .map(|(_, ask)| match ask {
                    EditorAsk::Decorate(decorate) => decorate.decorations.len(),
                    _ => 0,
                })
                .sum()
        })
        .max()
        .unwrap_or(0)
}

fn text(page: &Headless, editor: &str) -> String {
    page.editor_text(editor).expect("the editor")
}

fn cmd() -> Mods {
    Mods {
        command: true,
        ..Mods::default()
    }
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_example_loads_its_document_in_pieces_colours_what_is_in_view_and_folds_its_statements()
-> Result<(), Error> {
    let page = open(PAGE)?;
    let text = text(&page, "sql");
    assert!(
        text.starts_with("-- A sample of SQL"),
        "{:?}",
        text.get(..40)
    );
    assert!(text.len() > 30_000);
    let loads = page
        .told()
        .iter()
        .filter(
            |event| matches!(event, Event::Edit { edited, .. } if edited.origin == Origin::Load),
        )
        .count();
    assert!(loads > 2, "the sample came in {loads} pieces");
    let buffer = page.editor("sql").expect("the editor");
    assert_eq!(buffer.layer_names(), vec!["syntax", "folds"]);
    // The first statement's words are coloured; one far out of view is not.
    let first = buffer.line_start(1);
    let coloured = |from: usize, to: usize| {
        buffer
            .decorations_in(from, to)
            .iter()
            .filter(|(_, d)| matches!(d.mark, Mark::Style(_)))
            .count()
    };
    assert!(coloured(first, first + 6) > 0, "`select` is coloured");
    let far = buffer.line_start(1500);
    assert_eq!(coloured(far, far + 40), 0, "a line out of view is not");
    assert!(buffer.folds().len() > 100, "each statement folds");
    // What a person sees: the lines in view, and the page's note below them.
    assert!(page.says("-- A sample of SQL"), "{:?}", page.words());
    assert!(page.says("runs the statement under the caret"));
    // A page that searches for nothing is told nothing of a search.
    assert!(
        !page
            .told()
            .iter()
            .any(|event| matches!(event, Event::Found { .. })),
        "nothing was searched for"
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_examples_completions_stand_by_the_caret_and_take_their_keys_only_while_open()
-> Result<(), Error> {
    let mut page = open(PAGE)?;
    let sql = Id::named("sql");
    page.focus_editor("sql")?;
    assert_eq!(page.key(Key::Up, cmd())?, Route::Done);
    // ↓ is the editor's while no popup is open.
    assert_eq!(page.key(Key::Down, Mods::default())?, Route::Done);
    assert_eq!(page.key(Key::Up, Mods::default())?, Route::Done);
    assert_eq!(page.key(Key::Character('s'), Mods::default())?, Route::Type);
    page.type_text("e")?;
    let popup = page.bounds("completions").expect("the popup is open");
    let caret = page
        .laid()
        .and_then(|laid| laid.bounds(sql.caret()))
        .expect("the caret is on the page");
    assert_eq!(popup.y, caret.bottom() + 2.0, "it stands below the caret");
    assert_eq!(popup.x, caret.x);
    assert_eq!(page.words_in("completions"), ["select", "set"]);
    // `se`: `select`, then `set`. ↓ is the page's now, and ↩ puts the chosen word in.
    assert_eq!(
        page.press("down")?,
        Route::Command(Id::named("completion-next"))
    );
    assert_eq!(
        page.press("enter")?,
        Route::Command(Id::named("completion-accept"))
    );
    assert!(
        text(&page, "sql").starts_with("set -- A sample"),
        "{:?}",
        text(&page, "sql").get(..20)
    );
    assert!(page.bounds("completions").is_none(), "it closed");
    assert_eq!(page.press("enter")?, Route::Done);
    // ⌘↩ sends the statement under the caret to the program.
    assert_eq!(page.press("cmd+enter")?, Route::Command(Id::named("run")));
    assert_eq!(page.methods(), ["run"]);
    assert!(
        page.sent()
            .iter()
            .any(|said| said.contains("\"method\":\"run\""))
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_page_reads_the_text_of_the_version_it_is_told_of_in_the_event_that_tells_it()
-> Result<(), Error> {
    let page = open(
        r#"
        local seen = "nothing"
        local asked = false
        return {
            render = function()
                return pencere.div():size_full()
                    :child(pencere.editor("e", { gutter = false }):grow(1))
                    :child(pencere.text(seen))
            end,
            handle = function(event)
                if event.type == "viewport" and not asked then
                    asked = true
                    return { editor = { e = { load = "hello\nwörld" } } }
                elseif event.type == "edit" then
                    local length = pencere.layout.editor_length("e")
                    seen = string.format("v%d=%s %s lines %d line1 %d of %s",
                        event.version, tostring(pencere.layout.editor_version("e")),
                        pencere.layout.editor_text("e", 6, length), pencere.layout.editor_lines("e"),
                        pencere.layout.line_start("e", 1), event.changes[1].insert)
                    return true
                end
            end,
        }
        "#,
    )?;
    assert_eq!(text(&page, "e"), "hello\nwörld");
    assert_eq!(
        page.tree_words(),
        vec!["v1=1 wörld lines 2 line1 6 of hello\nwörld".to_owned()]
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn an_editor_that_gives_its_keys_to_the_page_is_driven_by_the_page() -> Result<(), Error> {
    let mut page = open(
        r#"
        local keys = 0
        return {
            render = function()
                return pencere.div():size_full()
                    :child(pencere.editor("vim", { keys = "page" }):grow(1))
                    :child(pencere.text("keys " .. keys))
            end,
            handle = function(event)
                if event.type == "key" then
                    keys += 1
                    if event.key == "i" then
                        return { redraw = true, editor = { vim = { command = "insert", text = "x" } } }
                    end
                    return true
                end
            end,
        }
        "#,
    )?;
    page.focus_editor("vim")?;
    // Every key is the page's, told it as the key, and the page drives the editor.
    for key in ['i', 'i', 'j'] {
        assert_eq!(page.key(Key::Character(key), Mods::default())?, Route::Page);
    }
    assert_eq!(text(&page, "vim"), "xx");
    assert_eq!(page.tree_words(), vec!["keys 3".to_owned()]);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_change_the_editor_cannot_make_is_refused_and_the_page_told_why() -> Result<(), Error> {
    let page = open(
        r#"
        local said = "nothing"
        local asked = false
        return {
            render = function()
                return pencere.div():size_full()
                    :child(pencere.editor("e"):grow(1))
                    :child(pencere.text(said))
            end,
            handle = function(event)
                if event.type == "viewport" and not asked then
                    asked = true
                    return { editor = { e = {
                        { change = { { from = 0, to = 0, insert = "x" } }, base = 99 },
                        { change = { { from = 3, to = 1, insert = "" } }, base = 0 },
                    } } }
                elseif event.type == "editor_refused" then
                    said = said == "nothing" and event.reason or (said .. ", " .. event.reason)
                    return true
                end
            end,
        }
        "#,
    )?;
    assert_eq!(text(&page, "e"), "");
    assert_eq!(
        page.tree_words(),
        vec!["future-version, reversed".to_owned()]
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_page_is_told_what_its_search_finds_and_replaces_every_match_in_one_ask() -> Result<(), Error> {
    let page = open(
        r#"
        local said = {}
        local asked, replaced = false, false
        return {
            render = function()
                return pencere.div():size_full()
                    :child(pencere.editor("e"):grow(1))
                    :child(pencere.text(table.concat(said, " ")))
            end,
            handle = function(event)
                if event.type == "viewport" and not asked then
                    asked = true
                    return { editor = { e = {
                        { load = "çiçek bir çiçekçi, Çiçek" },
                        { command = "find", pattern = "çiçek", word = true, case = false },
                    } } }
                elseif event.type == "found" then
                    table.insert(said, string.format("v%d:%d/%s%s", event.version, event.count,
                        tostring(event.current or "-"), event.more and "+" or ""))
                    if event.count == 2 and not replaced then
                        replaced = true
                        return { redraw = true, editor = { e = { command = "replace_all", text = "gül" } } }
                    end
                    return true
                end
            end,
        }
        "#,
    )?;
    // Whole words in either case: `çiçekçi` is not one. Every match went in one change, and the
    // search, gone through the document again at once, finds none.
    assert_eq!(text(&page, "e"), "gül bir çiçekçi, gül");
    assert_eq!(page.tree_words(), vec!["v1:2/1 v2:0/-".to_owned()]);
    assert_eq!(page.editor("e").expect("the editor").version(), 2);
    let found = page
        .told()
        .iter()
        .filter(|event| matches!(event, Event::Found { .. }))
        .count();
    assert_eq!(
        found, 2,
        "a `found` for each search the editor went through"
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn how_an_editor_looks_is_the_pages_to_say_and_what_it_leaves_out_is_the_theme() -> Result<(), Error>
{
    let page = open(
        r#"
        return {
            render = function()
                return pencere.div():size_full():child(pencere.editor("vim", {
                    line_height = 1.8,
                    padding = 9,
                    scroll_margin = 3,
                    numbers = "relative",
                    current_line = "all",
                    matches = "word",
                    caret = { shape = "block", height = "text", width = 3, color = "accent", text = { "magenta", 0.5 } },
                    selection = { radius = 0.22 },
                    colors = { current_line = false, selection = "raised1", numbers = "dim", matches = "info" },
                }):grow(1))
            end,
        }
        "#,
    )?;
    let room = page
        .laid()
        .expect("laid out")
        .editors()
        .into_iter()
        .find(|room| room.id == Id::named("vim"))
        .expect("the editor's room");
    assert_eq!(
        room.options,
        EditorOptions {
            line_height: 1.8,
            padding: 9.0,
            scroll_margin: 3,
            numbers: Numbers::Relative,
            current_line: LineHighlight::All,
            matches: Matches::Word,
            caret: Caret {
                shape: CaretShape::Block,
                height: CaretHeight::Text,
                width: 3.0,
                color: Tint::Color(Role::Accent.solid()),
                text: Tint::Color(Role::Magenta.alpha(0.5)),
            },
            selection: SelectionLook { radius: 0.22 },
            colors: EditorColors {
                current_line: Tint::Off,
                selection: Tint::Color(Role::Raised(1).solid()),
                numbers: Tint::Color(Role::Dim.solid()),
                matches: Tint::Color(Role::Info.solid()),
                ..EditorColors::default()
            },
            ..EditorOptions::default()
        }
    );
    // What a page leaves out is the editor's default: the theme's colours, its rows and caret.
    let plain = open(r#"return { render = function() return pencere.editor("e") end }"#)?;
    let room = plain
        .laid()
        .expect("laid out")
        .editors()
        .into_iter()
        .next()
        .expect("the editor's room");
    assert_eq!(room.options, EditorOptions::default());
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_page_says_how_its_language_comments_indents_and_folds_and_the_editor_does_it()
-> Result<(), Error> {
    let mut page = open(
        r#"
        local asked = false
        return {
            render = function()
                return pencere.div():size_full()
                    :child(pencere.editor("lua", {
                        comment = { line = "--", block = { "--[[", "]]" } },
                        indent = { increase = "\\b(then|do|else)\\s*$", decrease = "^\\s*(end|else)\\b" },
                        folds = "indent",
                    }):grow(1))
                    :child(pencere.editor("css", { comment = { block = { open = "/*", close = "*/" } } }):grow(1))
            end,
            handle = function(event)
                if event.type == "viewport" and not asked then
                    asked = true
                    return { editor = { css = { { load = "a { }" }, { command = "toggle_comment" } } } }
                end
            end,
        }
        "#,
    )?;
    let room = |page: &Headless, name: &str| {
        page.laid()
            .expect("laid out")
            .editors()
            .into_iter()
            .find(|room| room.id == Id::named(name))
            .expect("the editor's room")
            .options
    };
    assert_eq!(
        room(&page, "lua"),
        EditorOptions {
            comment: CommentTokens {
                line: Some("--".into()),
                block: Some(("--[[".into(), "]]".into())),
            },
            indent: IndentRules {
                increase: Some(r"\b(then|do|else)\s*$".into()),
                decrease: Some(r"^\s*(end|else)\b".into()),
            },
            folds: Folds::Indent,
            ..EditorOptions::default()
        }
    );
    assert_eq!(room(&page, "css").comment, CommentTokens::block("/*", "*/"));
    // A language of block comments alone comments a line with them.
    assert_eq!(text(&page, "css"), "/* a { } */");
    // A line break after `then` indents, and `end` typed goes back out.
    page.focus_editor("lua")?;
    page.insert_text("if x then")?;
    page.press("enter")?;
    page.insert_text("y()")?;
    page.press("enter")?;
    page.insert_text("end")?;
    assert_eq!(text(&page, "lua"), "if x then\n    y()\nend");
    // The line with further-in lines after it folds by the indent.
    let buffer = page.editor("lua").expect("the editor");
    assert_eq!(buffer.fold_region(0), Some((9, 17)));
    assert_eq!(buffer.fold_lines(0, 2), vec![0]);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_page_says_which_spaces_and_tabs_its_editor_shows_and_whether_its_indent_has_guides()
-> Result<(), Error> {
    let mut page = open(
        r#"
        local asked = false
        return {
            render = function()
                return pencere.div():size_full():child(pencere.editor("code", {
                    gutter = false,
                    whitespace = "boundary",
                    indent_guides = true,
                    colors = { whitespace = "accent", indent_guides = "magenta", active_indent_guide = false },
                }):grow(1))
            end,
            handle = function(event)
                if event.type == "viewport" and not asked then
                    asked = true
                    return { editor = { code = { { load = "if a then\n\tb  c\nend" } } } }
                end
            end,
        }
        "#,
    )?;
    let code = Id::named("code");
    let room = page
        .laid()
        .expect("laid out")
        .editors()
        .into_iter()
        .find(|room| room.id == code)
        .expect("the editor's room");
    assert_eq!(
        room.options,
        EditorOptions {
            gutter: false,
            whitespace: Whitespace::Boundary,
            indent_guides: true,
            colors: EditorColors {
                whitespace: Tint::Color(Role::Accent.solid()),
                indent_guides: Tint::Color(Role::Magenta.solid()),
                active_indent_guide: Tint::Off,
                ..EditorColors::default()
            },
            ..EditorOptions::default()
        }
    );
    assert_eq!(text(&page, "code"), "if a then\n\tb  c\nend");
    // The tab and the two spaces together are shown, not a space alone between words; the line
    // further in has a guide.
    page.focus_editor("code")?;
    let painted = &page.list().items;
    let role = |color: Option<pencere_view::Color>| color.and_then(|color| color.role());
    let dots = painted
        .iter()
        .filter(|item| matches!(item, Paint::Quad(quad) if role(quad.fill) == Some(Role::Accent)))
        .count();
    let arrows = painted
        .iter()
        .filter(|item| {
            matches!(item, Paint::Figure(figure)
                if figure.paths.iter().any(|path| role(path.stroke.map(|line| line.color)) == Some(Role::Accent)))
        })
        .count();
    let guides = painted
        .iter()
        .filter(|item| matches!(item, Paint::Quad(quad) if role(quad.fill) == Some(Role::Magenta)))
        .count();
    assert_eq!((dots, arrows, guides), (2, 1, 1));
    Ok(())
}

// --- A language of the kit's library (`@pencere/syntax`) -----------------------------------------

/// A language of C's block comments, strings and numbers, for the tests of `@pencere/syntax`. It
/// counts the lines it reads (`read`) and the most one call of the page read (`most`, with
/// `counted`).
const C_LIKE: &str = r#"
local Syntax = require("@pencere/syntax")
local read, most = 0, 0
local language = Syntax.define({
    name = "c",
    start = function()
        return { comment = false }
    end,
    token = function(stream, state)
        if stream:sol() then
            read += 1
        end
        if state.comment then
            if stream:skip_to("*/") then
                stream:match("%*/")
                state.comment = false
            else
                stream:skip_to_end()
            end
            return "comment"
        end
        if stream:eat_space() then
            return nil
        end
        if stream:match("/%*") then
            state.comment = true
            return "comment"
        end
        if stream:match('"[^"]*"?') then
            return "string"
        end
        if stream:match("'[^']*'?") then
            return "string.single"
        end
        if stream:match("%d+") then
            return "number"
        end
        local word = stream:match("[%a_][%w_]*")
        if word == "if" then
            return "keyword strong"
        end
        if word == nil then
            stream:next()
        end
        return nil
    end,
    blank = function()
        read += 1
    end,
    styles = {
        comment = { color = "dim", italic = true },
        string = { weight = 600 },
        number = { weight = 700 },
        keyword = { color = "accent" },
        strong = { weight = 800 },
    },
})
local function counted(work)
    local before = read
    work()
    most = math.max(most, read - before)
end
"#;

/// A page of one editor coloured by `C_LIKE`, with the `options` of its syntax, which loads what
/// `document` (the body of a function) returns when it is first in view and shows what the language
/// read. Its commands change the document: `uncomment` and `comment` take the `/*` at the start of
/// line 20 out and put it back, `open` puts one at the start of line 100, `touch` puts a digit there
/// (no state changes), `split` breaks line 100 in two (the lines after it move down), `far` shows
/// line 15 000 and `end` the last line. The syntax goes on by itself (its timer wakes the page):
/// `render` does nothing for it.
fn coloured(document: &str, options: &str) -> String {
    coloured_by(C_LIKE, document, options)
}

/// The page of `coloured`, with another language: `language` defines `language`, `read`, `most`
/// and `counted` as `C_LIKE` does.
fn coloured_by(language: &str, document: &str, options: &str) -> String {
    let page = r#"
local syntax = Syntax.new(language, "e", OPTIONS)
-- Each call's reading counted, the calls of the syntax's own timer as well.
local work = syntax.work
function syntax:work()
    counted(function()
        work(self)
    end)
end
local loaded = false
local function document()
    DOCUMENT
end
local function change(line, take, insert)
    local at = pencere.layout.line_start("e", line)
    return { change = { { from = at, to = at + take, insert = insert } }, base = pencere.layout.editor_version("e") }
end
return {
    render = function()
        return pencere.div():size_full()
            :command("uncomment", "Uncomment")
            :command("comment", "Comment")
            :command("open", "Open a comment")
            :command("far", "Go far down")
            :command("end", "Go to the end")
            :command("touch", "Touch line 100")
            :command("split", "Split line 100")
            :command("nothing", "Do nothing")
            :child(pencere.editor("e", { gutter = false }):grow(1))
            :child(pencere.text(string.format("read %d most %d", read, most)))
    end,
    handle = function(event)
        local ask = nil
        if event.type == "viewport" and not loaded then
            loaded = true
            ask = { load = document() }
        elseif event.type == "command" then
            if event.id == "uncomment" then
                ask = change(20, 2, "")
            elseif event.id == "comment" then
                ask = change(20, 0, "/*")
            elseif event.id == "open" then
                ask = change(100, 0, "/*")
            elseif event.id == "touch" then
                ask = change(100, 0, "1")
            elseif event.id == "split" then
                ask = change(100, 0, "\n")
            elseif event.id == "far" then
                ask = { reveal = pencere.layout.line_start("e", 15000), version = pencere.layout.editor_version("e") }
            elseif event.id == "end" then
                local last = pencere.layout.editor_lines("e") - 1
                ask = { reveal = pencere.layout.line_start("e", last), version = pencere.layout.editor_version("e") }
            end
        end
        syntax:handle(event)
        return syntax:reply(ask and { editor = { e = ask } })
    end,
}
"#;
    format!(
        "{language}{}",
        page.replace("OPTIONS", options)
            .replace("DOCUMENT", document)
    )
}

/// The lines the language read, and the most one call read, as the page shows them.
fn read(page: &Headless) -> (usize, usize) {
    let shown = page.tree_words();
    let line = shown
        .iter()
        .find(|words| words.starts_with("read "))
        .expect("the page shows what it read");
    let numbers: Vec<usize> = line
        .split(' ')
        .filter_map(|word| word.parse().ok())
        .collect();
    (numbers[0], numbers[1])
}

/// What the `syntax` layer gives line `line` of the editor: each look from where to where in the
/// line (characters), by the style of `C_LIKE` it is.
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
            let style = match (look.italic, look.weight) {
                (Some(true), _) => "comment",
                (_, Some(600)) => "string",
                (_, Some(700)) => "number",
                (_, Some(800)) if look.color.is_some() => "keyword strong",
                _ => "other",
            };
            (decoration.from - start, decoration.to - start, style)
        })
        .collect()
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_language_colours_a_block_comment_over_its_lines_and_only_the_lines_in_view()
-> Result<(), Error> {
    let mut page = open(&coloured(
        r#"local lines = {}
        for i = 1, 3000 do
            lines[i] = string.format("v = %05d", i)
        end
        lines[3] = "a /* one"
        lines[4] = "two"
        lines[5] = "three */ 12"
        lines[6] = "if 'it' \"is\""
        return table.concat(lines, "\n")"#,
        "{}",
    ))?;
    page.idle()?;
    page.refresh()?;
    assert_eq!(looks(&page, 1), vec![(4, 9, "number")]);
    assert_eq!(looks(&page, 2), vec![(2, 8, "comment")]);
    assert_eq!(looks(&page, 3), vec![(0, 3, "comment")]);
    assert_eq!(
        looks(&page, 4),
        vec![(0, 8, "comment"), (9, 11, "number")],
        "the comment ends where it says"
    );
    // A token of two names has both looks; a dotted name the look of what is before its dot.
    assert_eq!(
        looks(&page, 5),
        vec![
            (0, 2, "keyword strong"),
            (3, 7, "string"),
            (8, 12, "string")
        ]
    );
    assert_eq!(looks(&page, 6), vec![(4, 9, "number")]);
    assert!(
        looks(&page, 2500).is_empty(),
        "a line far out of view is not coloured"
    );
    let (lines, most) = read(&page);
    assert!(
        lines < 200 && most < 200,
        "only what is in view is read: {lines}, {most}"
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn an_edit_reads_the_lines_again_only_until_their_state_is_what_it_was() -> Result<(), Error> {
    let mut page = open(&coloured(
        r#"local lines = {}
        for i = 1, 2000 do
            lines[i] = string.format("v = %05d", i)
        end
        lines[21] = "/* open"
        lines[26] = "close */"
        return table.concat(lines, "\n")"#,
        "{}",
    ))?;
    page.idle()?;
    page.refresh()?;
    let (before, _) = read(&page);
    assert_eq!(looks(&page, 22), vec![(0, 9, "comment")]);
    // Without its `/*`, lines 20 to 25 are read again: the state after line 25 is what it was, and
    // the lines after it are not read.
    page.command("uncomment")?;
    page.idle()?;
    page.refresh()?;
    let (after, _) = read(&page);
    assert_eq!(after - before, 6);
    assert_eq!(looks(&page, 22), vec![(4, 9, "number")]);
    assert_eq!(looks(&page, 26), vec![(4, 9, "number")]);
    page.command("comment")?;
    page.idle()?;
    page.refresh()?;
    assert_eq!(read(&page).0 - after, 6);
    assert_eq!(looks(&page, 22), vec![(0, 9, "comment")]);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_language_colours_characters_where_a_line_has_characters_of_many_bytes() -> Result<(), Error> {
    let mut page = open(&coloured(r#"return "çay \"şeker\" 12\né /* ğ */ x""#, "{}"))?;
    page.idle()?;
    page.refresh()?;
    assert_eq!(looks(&page, 0), vec![(4, 11, "string"), (12, 14, "number")]);
    assert_eq!(looks(&page, 1), vec![(2, 9, "comment")]);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_long_document_is_coloured_to_its_end_a_step_a_wake_with_no_event_of_the_persons()
-> Result<(), Error> {
    let mut page = open(&coloured(
        r#"local lines = {}
        for i = 1, 20000 do
            lines[i] = string.format("v = %05d", i)
        end
        return table.concat(lines, "\n")"#,
        "{ step = 20000 }",
    ))?;
    page.idle()?;
    assert_eq!(looks(&page, 1), vec![(4, 9, "number")]);
    // From line 100 on, everything is a comment; far down, out of view, it is not read yet. The
    // editor scrolls there when the page is laid out, which the command's frame does.
    page.command("open")?;
    page.command("far")?;
    // The lines in view are coloured at once, from a guess, and the reading goes on by itself:
    // one call reads no more than its step (20 000 bytes, of lines of 10).
    assert_eq!(looks(&page, 15000), vec![(4, 9, "number")]);
    assert!(
        page.asks_at().is_some(),
        "the page asks to be woken while there is more to read"
    );
    // Nothing of the person's comes now: only the wakes the page asks for, a frame apart, each
    // a call's reading, what it found handed to the editor with the wake's reply.
    let told = editor_events(&page).len();
    let woken = page.idle()?;
    assert_eq!(
        editor_events(&page).len(),
        told,
        "the editor told the page nothing in the wakes"
    );
    assert!(woken >= 5, "the reading went on over {woken} wakes");
    assert_eq!(page.asks_at(), None, "the reading came to an end");
    assert_eq!(
        looks(&page, 15000),
        vec![(0, 9, "comment")],
        "what the guess got wrong is coloured again"
    );
    // The lines out of view are not coloured any more.
    assert!(looks(&page, 1).is_empty());
    page.refresh()?;
    let (_, most) = read(&page);
    assert!(most <= 2001, "one call read {most} lines");
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_syntax_reads_nothing_for_an_event_that_is_not_its_editors_edit_or_view() -> Result<(), Error> {
    let mut page = open(&coloured(
        r#"local lines = {}
        for i = 1, 20000 do
            lines[i] = string.format("v = %05d", i)
        end
        return table.concat(lines, "\n")"#,
        "{ step = 2000 }",
    ))?;
    page.idle()?;
    // Far down, the reading toward the lines in view goes on for many wakes.
    page.command("far")?;
    assert!(page.asks_at().is_some(), "there is more to read");
    let (before, _) = read(&page);
    let told = editor_events(&page).len();
    // The pointer moves over the page, and the page gives the syntax the event: it is no edit and
    // no view of its editor, and it reads nothing for it.
    for x in 0..20 {
        page.move_to(Point::new(10.0 + x as f32, 40.0))?;
    }
    assert_eq!(
        editor_events(&page).len(),
        told,
        "the editor told the page nothing"
    );
    assert_eq!(read(&page).0, before, "a pointer's moves read no line");
    // What is left is its timer's.
    assert!(page.step()?);
    page.refresh()?;
    assert!(read(&page).0 > before);
    Ok(())
}

/// A language of `C_LIKE`'s block comments whose state is large and the state of no two lines the
/// same: the count of the lines read before, and a kibibyte of text of its own. A syntax that kept
/// the state after each line would hold 60 000 of them, more than the memory a page has (64 MiB).
/// With `KEEP` true the language keeps each state's text itself, as such a syntax would.
const FAT: &str = r#"
local Syntax = require("@pencere/syntax")
local read, most = 0, 0
local FILLER = string.rep("x", 1024)
local kept = KEEP and {} or nil
local language = Syntax.define({
    name = "fat",
    start = function()
        return { comment = false, n = 0, note = "" }
    end,
    copy = function(state)
        return { comment = state.comment, n = state.n, note = state.note }
    end,
    token = function(stream, state)
        if stream:sol() then
            read += 1
            state.n += 1
            state.note = state.n .. FILLER
            if kept then
                kept[#kept + 1] = state.note
            end
        end
        if state.comment then
            if stream:skip_to("*/") then
                stream:match("%*/")
                state.comment = false
            else
                stream:skip_to_end()
            end
            return "comment"
        end
        if stream:eat_space() then
            return nil
        end
        if stream:match("/%*") then
            state.comment = true
            return "comment"
        end
        if stream:match("%d+") then
            return "number"
        end
        stream:next()
        return nil
    end,
    styles = { comment = { color = "dim", italic = true }, number = { weight = 700 } },
})
local function counted(work)
    local before = read
    work()
    most = math.max(most, read - before)
end
"#;

/// 60 000 lines of `v = 00001`, a block comment over lines 59 990 to 59 995.
const SIXTY_THOUSAND: &str = r#"local lines = table.create(60000)
        for i = 1, 60000 do
            lines[i] = string.format("v = %05d", i)
        end
        lines[59991] = "/* open"
        lines[59996] = "close */ 1"
        return table.concat(lines, "\n")"#;

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_syntax_holds_a_state_a_checkpoint_and_not_a_line_and_colours_sixty_thousand_lines_to_the_end()
-> Result<(), Error> {
    // The proof that the states are large: a page that keeps each one's text does not fit.
    let mut keeping = open(&coloured_by(
        &FAT.replace("KEEP", "true"),
        SIXTY_THOUSAND,
        "{}",
    ))?;
    keeping.idle()?;
    let kept = keeping
        .command("end")
        .and_then(|()| keeping.idle().map(drop));
    assert!(
        matches!(kept, Err(Error::Ended(Fault::Memory))),
        "a page that keeps 60 000 states of a kibibyte is closed for its memory: {kept:?}"
    );
    // The syntax keeps them a checkpoint at a time: it reads to the end and the page stays open.
    let mut page = open(&coloured_by(
        &FAT.replace("KEEP", "false"),
        SIXTY_THOUSAND,
        "{}",
    ))?;
    page.idle()?;
    assert_eq!(looks(&page, 1), vec![(4, 9, "number")]);
    page.command("end")?;
    page.idle()?;
    assert_eq!(page.asks_at(), None, "the reading came to the end");
    assert!(!page.closed(), "{:?}", page.ended());
    assert_eq!(looks(&page, 59_992), vec![(0, 9, "comment")]);
    assert_eq!(
        looks(&page, 59_995),
        vec![(0, 8, "comment"), (9, 10, "number")]
    );
    assert_eq!(looks(&page, 59_999), vec![(4, 9, "number")]);
    page.refresh()?;
    let (lines, most) = read(&page);
    assert!(lines >= 60_000, "every line was read: {lines}");
    assert!(most <= 2001, "one call read {most} lines");
    // Far up again, the lines in view are read from the checkpoint before them.
    page.command("far")?;
    page.idle()?;
    page.refresh()?;
    let (again, _) = read(&page);
    assert!(
        again - lines < 300,
        "the view up again read {} lines",
        again - lines
    );
    assert_eq!(looks(&page, 15_000), vec![(4, 9, "number")]);
    assert!(!page.closed(), "{:?}", page.ended());
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn an_edit_out_of_view_reads_on_only_to_the_checkpoint_where_the_state_is_what_it_was()
-> Result<(), Error> {
    let mut page = open(&coloured(
        r#"local lines = {}
        for i = 1, 20000 do
            lines[i] = string.format("v = %05d", i)
        end
        lines[15003] = "/* open"
        lines[15005] = "close */"
        return table.concat(lines, "\n")"#,
        "{}",
    ))?;
    page.idle()?;
    page.command("far")?;
    page.idle()?;
    assert_eq!(looks(&page, 15_003), vec![(0, 9, "comment")]);
    page.refresh()?;
    let (before, _) = read(&page);
    // A digit at the start of line 100, far above the view: the reading goes from the checkpoint
    // before it (line 96) to the first kept state after it (line 128), and stops there.
    page.command("touch")?;
    page.idle()?;
    page.refresh()?;
    let (touched, _) = read(&page);
    assert!(
        touched - before <= 2 * 32,
        "an edit out of view read {} lines",
        touched - before
    );
    // A line break there moves every line after it, and the checkpoints with them: the reading
    // stops as soon, and the comment is coloured where it is now.
    page.command("split")?;
    page.idle()?;
    page.refresh()?;
    let (split, _) = read(&page);
    assert!(
        split - touched <= 2 * 32,
        "a line break out of view read {} lines",
        split - touched
    );
    assert_eq!(looks(&page, 15_002), vec![(4, 9, "number")]);
    assert_eq!(looks(&page, 15_003), vec![(0, 7, "comment")]);
    assert_eq!(looks(&page, 15_004), vec![(0, 9, "comment")]);
    assert_eq!(looks(&page, 15_006), vec![(4, 9, "number")]);
    // The lines the syntax has kept no state of, up again, are read from what it kept: right.
    page.command("end")?;
    page.idle()?;
    page.command("far")?;
    page.idle()?;
    assert_eq!(looks(&page, 15_005), vec![(0, 8, "comment")]);
    assert_eq!(looks(&page, 14_990), vec![(4, 9, "number")]);
    Ok(())
}

/// A language of letters and what is not, which gives every character of `a1a1…` a decoration of its
/// own: a line of 10 000 characters is 10 000 decorations, half what one reply may hand the editors.
const DENSE: &str = r#"
local Syntax = require("@pencere/syntax")
local dense = Syntax.define({
    name = "dense",
    start = function()
        return {}
    end,
    token = function(stream)
        if stream:match("%a") then
            return "letter"
        end
        stream:next()
        return "other"
    end,
    styles = { letter = { color = "accent" }, other = { color = "warning" } },
})
"#;

/// How many decorations the layer `layer` of the editor `editor` has on line `line`.
fn decorated(page: &Headless, editor: &str, layer: &str, line: usize) -> usize {
    let buffer = page.editor(editor).expect("the editor");
    let Some(index) = buffer.layer_names().iter().position(|name| name == layer) else {
        return 0;
    };
    let start = buffer.line_start(line);
    buffer
        .decorations_in(start, buffer.line_end(line))
        .into_iter()
        .filter(|(at, decoration)| *at == index && decoration.from >= start)
        .count()
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn two_syntaxes_and_the_page_that_ask_more_than_a_reply_may_at_one_wake_hand_it_over_two()
-> Result<(), Error> {
    let mut page = open(&format!(
        "{DENSE}{}",
        r#"
local a = Syntax.new(dense, "a")
local b = Syntax.new(dense, "b")
local loaded, marked = {}, false
local line = string.rep("a1", 5000)
local document = table.concat({ line, line, line, line }, "\n")
-- The page's own decorations of `a`'s first ten characters, in the layer `layer`.
local function marks(layer)
    local made = {}
    for k = 0, 9 do
        made[#made + 1] = { from = k, to = k + 1, background = "accent" }
    end
    return { editor = { a = { decorate = layer, from = 0, to = 10,
        version = pencere.layout.editor_version("a"), decorations = made } } }
end
return {
    render = function()
        return pencere.div():size_full()
            :child(pencere.editor("a", { gutter = false }):grow(1))
            :child(pencere.editor("b", { gutter = false }):grow(1))
    end,
    handle = function(event)
        local ask = nil
        if event.type == "viewport" and not loaded[event.editor] then
            loaded[event.editor] = true
            ask = { editor = { [event.editor] = { load = document } } }
        end
        a:handle(event)
        b:handle(event)
        if event.type == "edit" and event.editor == "b" and not marked then
            marked = true
            -- The page's own, with the line `b` colours now, and at the wake both syntaxes go on
            -- at with a line each: there, 10 more than the 20 000 a reply may hand the editors.
            ask = marks("marks")
            pencere.time.after(0, function()
                return marks("more")
            end)
        end
        return a:reply(b:reply(ask))
    end,
}
"#
    ))?;
    page.idle()?;
    assert!(most(&page) <= 20_000, "a reply handed {}", most(&page));
    for editor in ["a", "b"] {
        for line in 0..4 {
            assert_eq!(
                decorated(&page, editor, "syntax", line),
                10_000,
                "{editor}, line {line}"
            );
        }
    }
    assert_eq!(decorated(&page, "a", "marks", 0), 10);
    assert_eq!(decorated(&page, "a", "more", 0), 10);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_syntax_holds_at_most_half_a_reply_however_many_decorations_a_line_has() -> Result<(), Error> {
    let mut page = open(&format!(
        "{DENSE}{}",
        r#"
local syntax = Syntax.new(dense, "e")
local loaded = false
return {
    render = function()
        return pencere.editor("e", { gutter = false })
    end,
    handle = function(event)
        local ask = nil
        if event.type == "viewport" and not loaded then
            loaded = true
            -- Half a line, then lines of 10 000 decorations: the second does not go with the first.
            ask = { editor = { e = { load = string.rep("a1", 2500) .. "\n" .. string.rep("a1", 5000)
                .. "\n" .. string.rep("a1", 5000) } } }
        end
        syntax:handle(event)
        return syntax:reply(ask)
    end,
}
"#
    ))?;
    page.idle()?;
    assert!(most(&page) <= 10_000, "a reply handed {}", most(&page));
    assert_eq!(decorated(&page, "e", "syntax", 0), 5_000);
    assert_eq!(decorated(&page, "e", "syntax", 1), 10_000);
    assert_eq!(decorated(&page, "e", "syntax", 2), 10_000);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_token_that_reads_nothing_fails_the_page_with_its_languages_name_and_its_line() {
    let why = failing(
        r#"
        local Syntax = require("@pencere/syntax")
        local language = Syntax.define({
            name = "stuck",
            start = function() return {} end,
            token = function(stream)
                if stream:peek() == "!" then
                    return "x"
                end
                stream:next()
            end,
        })
        local syntax = Syntax.new(language, "e")
        local loaded = false
        return {
            render = function()
                return pencere.editor("e", { gutter = false })
            end,
            handle = function(event)
                local reply = nil
                if event.type == "viewport" and not loaded then
                    loaded = true
                    reply = { editor = { e = { load = "fine\nnot!" } } }
                end
                syntax:handle(event)
                return syntax:reply(reply)
            end,
        }"#,
    );
    assert!(
        why.contains("stuck: line 2: token read nothing at column 4"),
        "{why}"
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_language_reads_a_line_from_a_stream_as_codemirrors_does() -> Result<(), Error> {
    let page = open(
        r#"
        local seen = {}
        local language = require("@pencere/syntax").define({
            name = "probe",
            start = function() return { n = 0 } end,
            token = function(stream, state)
                state.n += 1
                if stream:sol() then
                    seen[#seen + 1] = stream:indentation()
                    stream:eat_space()
                    seen[#seen + 1] = stream:sol()
                    -- A pattern is read where the stream is, not further on.
                    seen[#seen + 1] = stream:match("b") or "no b"
                    seen[#seen + 1] = stream:match("(%a+)=(%d+)", false)
                    seen[#seen + 1] = stream:match("KEY", false, true)
                    seen[#seen + 1] = stream:match("[j-l]+", false, true)
                    seen[#seen + 1] = stream:eat(".") or "no dot"
                    seen[#seen + 1] = stream:eat("%a")
                    stream:backup(1)
                    stream:eat_while("[%w=]")
                    seen[#seen + 1] = stream:current()
                    stream:eat_space()
                    stream.start = stream.pos
                    seen[#seen + 1] = stream:column()
                    seen[#seen + 1] = stream:next()
                    seen[#seen + 1] = stream:skip_to("b") and stream.pos or "no b"
                    return "word"
                end
                stream:skip_to_end()
                return "rest"
            end,
        })
        local tokens, after = language:tokens("\t  Key=42 é ab")
        local said = {}
        for _, value in ipairs(seen) do
            said[#said + 1] = tostring(value)
        end
        for _, token in ipairs(tokens) do
            said[#said + 1] = token.name .. " " .. token.from .. "-" .. token.to
        end
        said[#said + 1] = "states " .. after.n
        return {
            render = function()
                return pencere.text(table.concat(said, ", "))
            end,
        }"#,
    )?;
    assert_eq!(
        page.tree_words(),
        vec![
            "6, false, no b, Key, Key, K, no dot, K, \t  Key=42, 13, é, 15, word 0-13, rest 13-14, \
             states 2"
                .to_owned()
        ]
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_language_gives_the_look_of_a_token_as_it_colours_one_for_a_text_not_in_an_editor()
-> Result<(), Error> {
    let page = open(
        r#"
        local Syntax = require("@pencere/syntax")
        local language = Syntax.define({
            name = "looks",
            start = function() return {} end,
            token = function(stream) stream:skip_to_end() return "x" end,
            styles = { keyword = { color = "accent" }, strong = { weight = 800 },
                string = { color = "success" } },
        })
        local function said(name)
            local look = language:look(name)
            if look == nil then
                return "none"
            end
            return tostring(look.color) .. "/" .. tostring(look.weight)
        end
        local copy = language:look("keyword")
        copy.color = "error"
        local ok, why = pcall(language.look, language, 5)
        return {
            render = function()
                return pencere.text(table.concat({
                    said("keyword"), said("keyword strong"), said("string.special"),
                    said("nothing"), said("keyword"), tostring(ok), why,
                }, ", "))
            end,
        }"#,
    )?;
    assert_eq!(
        page.tree_words(),
        vec![
            "accent/nil, accent/800, success/nil, none, accent/nil, false, \
             look takes the name of a token's style, not a number"
                .to_owned()
        ]
    );
    Ok(())
}

// --- Wakes and the kit's timers (`pencere.time`) -------------------------------------------------

/// The words of the page that start with `head`, the rest of the line.
fn shown(page: &Headless, head: &str) -> String {
    page.tree_words()
        .into_iter()
        .find_map(|line| line.strip_prefix(head).map(str::to_owned))
        .unwrap_or_else(|| panic!("the page shows no {head:?}: {:?}", page.tree_words()))
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn timers_run_in_the_order_of_their_times_a_stopped_one_never_and_what_they_return_is_one_reply()
-> Result<(), Error> {
    let mut page = open(
        r#"
local ran = {}
local told = {}
local stopped = nil
return {
    render = function()
        return pencere.div():size_full():command("start", "Start")
            :child(pencere.editor("e", { gutter = false }):grow(1))
            :child(pencere.text("ran " .. table.concat(ran, " ")))
            :child(pencere.text("told " .. table.concat(told, " ")))
    end,
    handle = function(event)
        told[#told + 1] = event.type
        if event.type == "command" and event.id == "start" then
            pencere.time.after(0.3, function()
                ran[#ran + 1] = "c"
                return true
            end)
            pencere.time.after(0.1, function()
                ran[#ran + 1] = "a"
                return { editor = { e = { load = "from a timer", more = true } } }
            end)
            stopped = pencere.time.after(0.2, function()
                ran[#ran + 1] = "never"
                return true
            end)
            -- At the time of the first: run after it, in the order they were made.
            pencere.time.after(0.1, function()
                ran[#ran + 1] = "b"
                ran[#ran + 1] = tostring(stopped:cancel()) .. "/" .. tostring(stopped:cancel())
                    .. "/" .. tostring(stopped:active())
                return { redraw = true, editor = { e = { load = ", and another" } } }
            end)
        end
    end,
}
"#,
    )?;
    assert_eq!(
        page.asks_at(),
        None,
        "a page with no timer asks to be woken for nothing"
    );
    page.command("start")?;
    // Set where the time was not known: the kit asks to be woken at once, to count from then.
    assert_eq!(page.asks_at(), Some(page.now()));
    let woken = page.idle()?;
    assert_eq!(woken, 3, "placed, then a and b together, then c");
    assert_eq!(shown(&page, "ran "), "a b true/false/false c");
    // What two timers returned at one wake went to the editor as one reply, in their order.
    assert_eq!(text(&page, "e"), "from a timer, and another");
    // The page's own `handle` never heard of the wakes its timers took.
    assert!(
        !shown(&page, "told ").contains("wake"),
        "{}",
        shown(&page, "told ")
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_timer_set_as_the_script_starts_ticks_with_no_event_and_the_page_hears_only_its_own_wake()
-> Result<(), Error> {
    let mut page = open(
        r#"
local ticks = 0
local own = {}
local clock
clock = pencere.time.every(0.5, function()
    ticks += 1
    if ticks == 3 then
        -- The page's own wake, asked from a timer: its `handle` is told of it.
        return { wake = 0.25 }
    end
    return true
end)
return {
    render = function()
        return pencere.div()
            :child(pencere.text("ticks " .. ticks))
            :child(pencere.text("own " .. table.concat(own, " ")))
    end,
    handle = function(event)
        if event.type == "wake" then
            own[#own + 1] = event.now == pencere.time.now() and "now" or "not now"
            clock:cancel()
            return true
        end
    end,
}
"#,
    )?;
    // The first page asked for it: nothing else could, for nothing of the person's comes. The frame
    // that draws the page is the first at that time, and placed the timer: the first tick is half
    // a second on.
    assert_eq!(page.asks_at(), Some(0.5));
    assert_eq!(shown(&page, "ticks "), "0");
    assert!(page.step()?);
    assert_eq!(shown(&page, "ticks "), "1");
    // A view out of sight for two seconds: its host wakes the page a second apart at the soonest,
    // and each wake runs the timer once, not once for each run it fell behind on.
    page.hide(2.0)?;
    // Seen again: what it shows now.
    page.refresh()?;
    assert_eq!(shown(&page, "ticks "), "3");
    assert_eq!(shown(&page, "own "), "");
    // The page's own wake, a quarter of a second after the third tick: its `handle` is told, with
    // the time it is given as `pencere.time.now()`.
    assert!(page.step()?);
    assert_eq!(shown(&page, "own "), "now");
    assert_eq!(
        page.asks_at(),
        None,
        "the clock was stopped and nothing is asked"
    );
    assert!(!page.step()?);
    assert_eq!(shown(&page, "ticks "), "3");
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_page_that_sets_no_timer_and_asks_no_wake_is_asked_nothing_new_and_told_nothing_new()
-> Result<(), Error> {
    let mut page = open(
        r#"
local told = {}
return {
    render = function(size, now)
        return pencere.div():size_full():command("go", "Go")
            :child(pencere.editor("e", { gutter = false }):grow(1))
            :child(pencere.text("told " .. table.concat(told, " ")))
            :child(pencere.text(string.format("now %s", tostring(pencere.time.now() == now))))
    end,
    handle = function(event)
        told[#told + 1] = event.type
        return true
    end,
}
"#,
    )?;
    page.focus_editor("e")?;
    page.type_text("plain words")?;
    page.command("go")?;
    page.advance(0.1)?;
    assert_eq!(page.asks_at(), None);
    assert!(!page.step()?);
    assert!(!shown(&page, "told ").contains("wake"));
    assert!(shown(&page, "told ").contains("command"));
    assert_eq!(shown(&page, "now "), "true");
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_timer_set_as_the_page_settles_is_asked_for_with_no_event() -> Result<(), Error> {
    let page = open(
        r#"
local set, ran = false, false
return {
    render = function()
        return pencere.text(ran and "ran" or "waits")
    end,
    settle = function()
        if not set then
            set = true
            pencere.time.after(0, function()
                ran = true
                return true
            end)
        end
        return false
    end,
}
"#,
    )?;
    // A settle has no reply: the kit asks for the page again, and its frame asks to be woken — at
    // once, which the frame that draws that page does, at the time the page opened.
    assert_eq!(page.tree_words(), vec!["ran".to_owned()]);
    assert_eq!(page.now(), 0.0, "no time went by for it");
    assert_eq!(page.asks_at(), None);
    assert!(
        page.told()
            .iter()
            .all(|event| !matches!(event, Event::Input(_))),
        "with no event of the person's"
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_timer_that_fails_fails_the_page_at_its_line_and_a_wake_that_is_no_time_is_a_mistake() {
    // An error in a timer is the page's, said with its line.
    let mut page = open(
        "pencere.time.after(0.1, function()\n    error(\"the timer broke\")\nend)\nreturn { render = function() return pencere.div() end }",
    )
    .expect("the page opens");
    let why = failed(page.idle().map(drop));
    assert!(why.starts_with("page.lua:2: the timer broke"), "{why}");
    assert!(page.ended().is_some());
    // Set with what is not a time, a timer is a mistake where it is set.
    for (set, what) in [
        (
            "pencere.time.after(-1, print)",
            "pencere.time.after takes a number of seconds",
        ),
        (
            "pencere.time.every(0, print)",
            "the time between two runs is more than 0 seconds",
        ),
        ("pencere.time.after(1, 2)", "the function to run"),
    ] {
        let why = failing(&format!(
            "local x = 1\n{set}\nreturn {{ render = function() return pencere.div() end }}"
        ));
        assert!(why.starts_with("page.lua:2:"), "{set}: {why}");
        assert!(why.contains(what), "{set}: {why}");
    }
    // A reply's wake that is not a number of seconds fails the page, as a reply's mistakes do.
    for wake in ["-1", "\"soon\"", "0/0"] {
        let mut page = open(&format!(
            r#"return {{
    render = function() return pencere.div():command("go", "Go") end,
    handle = function(event) if event.type == "command" then return {{ wake = {wake} }} end end,
}}"#
        ))
        .expect("the page opens");
        let why = failed(page.command("go"));
        assert!(
            why.contains("wake is a number of seconds, 0 or more"),
            "{why}"
        );
    }
}

/// An editor the page names is made when the page is laid out, shown or hidden, and the page is
/// told so with its text posted: the documents of a page of tabs are loaded as the editors come,
/// the one in front and the hidden ones alike, with no guess at when the layout came.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn an_editor_says_it_is_ready_shown_or_hidden_and_takes_a_load_then() -> Result<(), Error> {
    let page = open(
        r#"
        local said = {}
        return {
            render = function()
                local hidden = pencere.editor("back"):grow(1):hidden()
                return pencere.div():size_full()
                    :child(pencere.editor("front"):grow(1))
                    :child(hidden)
                    :child(pencere.text(table.concat(said, "|")))
            end,
            handle = function(event)
                if event.type == "editor_ready" then
                    said[#said + 1] = event.editor .. " v" .. event.version .. " read "
                        .. tostring(pencere.layout.editor_version(event.editor))
                    return { redraw = true, editor = { [event.editor] = { load = event.editor .. " text" } } }
                elseif event.type == "editor_refused" then
                    said[#said + 1] = "refused " .. event.reason
                    return true
                end
            end,
        }
        "#,
    )?;
    assert_eq!(text(&page, "front"), "front text");
    assert_eq!(text(&page, "back"), "back text");
    assert_eq!(
        page.tree_words(),
        vec!["front v0 read 0|back v0 read 0".to_owned()]
    );
    Ok(())
}

/// A change the page makes without a place in the history (a file read again from disk) is not
/// undone, what the person did before it still is, over it; and the page's name for a change is
/// told back with it.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_change_kept_out_of_the_history_leaves_what_was_done_before_it_to_undo() -> Result<(), Error> {
    let mut page = open(
        r#"
        local said = {}
        return {
            render = function()
                return pencere.div():size_full()
                    :child(pencere.editor("e"):grow(1))
                    :child(pencere.text(table.concat(said, "|")))
            end,
            handle = function(event)
                if event.type == "editor_ready" then
                    return { editor = { e = { load = "abc" } } }
                elseif event.type == "edit" then
                    said[#said + 1] = event.origin .. (event.tag and (":" .. event.tag) or "")
                    return true
                end
            end,
            message = function(message)
                local at = pencere.layout.editor_length("e")
                return { editor = { e = {
                    change = { { from = at, to = at, insert = message.insert } },
                    base = pencere.layout.editor_version("e"),
                    history = false,
                    tag = "disk",
                } } }
            end,
        }
        "#,
    )?;
    page.focus_editor("e")?;
    page.type_text("x")?;
    page.tell(r#"{"insert":"!"}"#)?;
    assert_eq!(text(&page, "e"), "xabc!");
    page.press("cmd+z")?;
    assert_eq!(text(&page, "e"), "abc!");
    page.press("cmd+shift+z")?;
    assert_eq!(text(&page, "e"), "xabc!");
    assert_eq!(
        page.tree_words(),
        vec!["load|person|page:disk|undo|redo".to_owned()]
    );
    Ok(())
}

/// A place revealed goes where the page asks in the room: as near as it takes (the default), at
/// the top, in the middle or at the bottom; and the commands of whole lines are the page's to ask.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_place_is_revealed_where_the_page_asks_and_the_commands_of_lines_are_asked_by_name()
-> Result<(), Error> {
    let mut page = open(
        r#"
        local lines = {}
        for n = 1, 400 do lines[n] = "line " .. n end
        return {
            render = function()
                return pencere.div():size_full():child(pencere.editor("e", { gutter = false }):h(200))
            end,
            handle = function(event)
                if event.type == "editor_ready" then
                    return { editor = { e = { load = table.concat(lines, "\n") } } }
                end
            end,
            message = function(message)
                local version = pencere.layout.editor_version("e")
                if message.command then
                    return { editor = { e = { command = message.command } } }
                end
                return { editor = { e = {
                    reveal = pencere.layout.line_start("e", message.line),
                    version = version,
                    align = message.align,
                } } }
            end,
        }
        "#,
    )?;
    let offset = |page: &Headless| {
        page.laid()
            .and_then(|laid| laid.scroll_extent(Id::named("e")))
            .map_or(-1.0, |extent| extent.offset.y)
    };
    let row = page
        .laid()
        .and_then(|laid| laid.scroll_extent(Id::named("e")))
        .map_or(0.0, |extent| extent.content.h / 400.0);
    page.tell(r#"{"line":200,"align":"start"}"#)?;
    let start = offset(&page);
    assert!((start - 200.0 * row).abs() <= row, "{start} {row}");
    page.tell(r#"{"line":200,"align":"center"}"#)?;
    let center = offset(&page);
    assert!((center - (200.5 * row - 100.0)).abs() <= row, "{center}");
    page.tell(r#"{"line":200,"align":"end"}"#)?;
    let end = offset(&page);
    assert!((end - (201.0 * row - 200.0)).abs() <= row, "{end}");
    // Already in view: the nearest way does not move it.
    page.tell(r#"{"line":199}"#)?;
    assert_eq!(offset(&page), end);
    page.focus_editor("e")?;
    page.caret_to("e", 0)?;
    page.tell(r#"{"command":"delete_lines"}"#)?;
    page.tell(r#"{"command":"select_line"}"#)?;
    page.tell(r#"{"command":"fold_all"}"#)?;
    assert!(text(&page, "e").starts_with("line 2\nline 3"));
    assert_eq!(
        page.editor("e").map(|buffer| buffer.ranges().to_vec()),
        Some(vec![pencere_view::edit::Range::new(0, 7)])
    );
    Ok(())
}

// --- The gutter -----------------------------------------------------------------------------------

const GUTTER: &str = include_str!("../../../examples/gutter-lua/src/view/page.lua");

fn gutter_events(page: &Headless) -> Vec<&pencere_view::edit::GutterEvent> {
    page.told()
        .iter()
        .filter_map(|event| match event {
            Event::Gutter(gutter) => Some(gutter),
            _ => None,
        })
        .collect()
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_pointer_is_the_i_beam_over_the_text_the_arrow_in_the_gutter_and_the_hand_where_it_answers()
-> Result<(), Error> {
    use pencere_view::Cursor;
    let mut page = open(GUTTER)?;
    let cell = |page: &Headless, column: &str, line: usize| {
        let rect = page
            .gutter_cell("code", column, line)
            .expect("a cell in view");
        Point::new(rect.x + rect.w / 2.0, rect.y + rect.h / 2.0)
    };
    // The column of breakpoints answers a press, the numbers and the buttons that fold (where none
    // does) do not, the text is the editor's.
    page.move_to(cell(&page, "breakpoints", 2))?;
    assert_eq!(page.cursor(), Some(Cursor::Pointer));
    page.move_to(cell(&page, "numbers", 2))?;
    assert_eq!(page.cursor(), Some(Cursor::Arrow));
    page.move_to(cell(&page, "folds", 2))?;
    assert_eq!(page.cursor(), Some(Cursor::Arrow));
    let numbers = cell(&page, "numbers", 2);
    page.move_to(Point::new(numbers.x + 300.0, numbers.y))?;
    assert_eq!(page.cursor(), Some(Cursor::Text));
    // Back over the gutter: the arrow again, not the I-beam the text had left.
    page.move_to(cell(&page, "changes", 20))?;
    assert_eq!(page.cursor(), Some(Cursor::Pointer));
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_press_in_a_column_of_the_pages_puts_a_breakpoint_where_there_is_none_and_takes_it_away()
-> Result<(), Error> {
    let mut page = open(GUTTER)?;
    let marks = |page: &Headless| {
        let buffer = page.editor("code").expect("the editor");
        buffer
            .decorations_in(0, buffer.len_chars())
            .into_iter()
            .filter(|(_, d)| matches!(&d.mark, Mark::Gutter(m) if m.column == "breakpoints"))
            .count()
    };
    assert_eq!(marks(&page), 0);
    // Over an empty cell the column shows a faint dot, the breakpoint to be put.
    page.hover_gutter("code", "breakpoints", 6)?;
    let faint = |page: &Headless| {
        page.list()
            .items
            .iter()
            .filter(|item| matches!(item, Paint::Quad(q) if q.fill.is_some_and(|f| f.role() == Some(Role::Red)) && q.radii[0] > 0.0))
            .count()
    };
    assert_eq!(faint(&page), 1);
    page.click_gutter("code", "breakpoints", 6, pencere_view::Button::Left)?;
    assert_eq!(marks(&page), 1);
    // The page was told: which column and line, where the cell was and that nothing was there.
    let event = gutter_events(&page)
        .into_iter()
        .find(|e| e.kind == pencere_view::edit::GutterKind::Press)
        .expect("a press");
    assert_eq!((event.column.as_str(), event.line), ("breakpoints", 6));
    assert!(event.marker.is_none());
    assert_eq!(page.gutter_cell("code", "breakpoints", 6), Some(event.rect));
    // The caret did not move and nothing was selected: the press was the page's.
    assert_eq!(page.editor("code").map(|b| b.ranges().len()), Some(1));
    assert_eq!(
        page.editor("code").map(|b| b.ranges()[0].is_empty()),
        Some(true)
    );
    // The marker has a label: the screen reader says it and the host shows it while the pointer rests.
    assert!(
        page.outline()
            .tips
            .iter()
            .any(|tip| tip.words == "Breakpoint on line 7")
    );
    assert!(
        page.outline()
            .accessible
            .iter()
            .any(|item| item.words == "Breakpoint on line 7")
    );
    // A press on it takes it away; another button is the menu, told and nothing else here.
    page.click_gutter("code", "breakpoints", 6, pencere_view::Button::Right)?;
    assert_eq!(marks(&page), 1);
    assert!(
        gutter_events(&page)
            .iter()
            .any(|e| e.button == pencere_view::Button::Right)
    );
    page.click_gutter("code", "breakpoints", 6, pencere_view::Button::Left)?;
    assert_eq!(marks(&page), 0);
    // The numbers are the editor's: a press selects the line.
    page.click_gutter("code", "numbers", 2, pencere_view::Button::Left)?;
    let ranges = page
        .editor("code")
        .map(|b| b.ranges().to_vec())
        .unwrap_or_default();
    assert_eq!(ranges.len(), 1);
    assert!(!ranges[0].is_empty());
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_press_on_a_bar_opens_a_zone_the_pages_element_stands_in_and_scrolls_with_the_text()
-> Result<(), Error> {
    let mut page = open(GUTTER)?;
    assert!(page.zone("code", "peek").is_none());
    assert!(page.bounds("peek").is_none());
    // The bar of the modified lines (3 to 5): a press on its second line opens the peek after the
    // last of them.
    page.click_gutter("code", "changes", 4, pencere_view::Button::Left)?;
    let gap = page.zone("code", "peek").expect("a gap is open");
    assert!(gap.h >= 60.0, "{gap:?}");
    let element = page.bounds("peek").expect("the page's element");
    assert_eq!((element.x, element.y), (gap.x, gap.y));
    assert!(
        element.h <= gap.h + 0.01,
        "the element {element:?} fits its gap {gap:?}"
    );
    // The text after the gap went down by it: line 6 is a gap below where it was.
    let after = page.gutter_cell("code", "numbers", 6).expect("line 6");
    assert!(after.y >= gap.y + gap.h - 0.01, "{after:?} {gap:?}");
    // Scrolled, the gap and the element go up together, as the text does.
    page.scroll_to("code", 60.0)?;
    let moved = page.zone("code", "peek").expect("still open");
    let element = page.bounds("peek").expect("the element");
    assert!((gap.y - moved.y - 60.0).abs() < 1.0, "{gap:?} {moved:?}");
    assert!((element.y - moved.y).abs() < 0.01, "{element:?} {moved:?}");
    // Esc closes it, and the gap with it.
    page.press("escape")?;
    assert!(page.zone("code", "peek").is_none());
    assert!(page.bounds("peek").is_none());
    Ok(())
}

// --- What the pointer rests on -----------------------------------------------------------------------

/// A page that shows a card by the word the pointer rests on, as a page of a language server's
/// client does: it hears of the rest (`hover`), names the word's range, and puts a card beside it.
const HOVER: &str = r#"
local b = pencere
local text = "alpha beta\ngamma delta"
local card = nil
local version = 0

local function layer(from, to)
	return { decorate = "card", version = version, from = 0, to = #text, decorations = card and {
		{ from = card.from, to = card.to, anchor = "card" },
	} or {} }
end

local function render()
	local root = b.div():size_full():child(
		b.editor("code", { gutter = false, events = { "hover" }, hover = { delay = 0.5 } }):size_full()
	)
	if card then
		root:child(b.div():id("card"):w(120):h(40):bg("raised2")
			:anchor(pencere.range_of("code", "card"), "below")
			:child(b.text("card " .. card.word)))
	end
	return root
end

local function handle(event)
	if event.type == "editor_ready" then
		return { editor = { code = { { load = text } } } }
	elseif event.type == "edit" then
		version = event.version
		return { editor = { code = { { decorate = "diagnostics", version = version, from = 0, to = #text,
			decorations = { { from = 0, to = 5, underline = { wavy = "error" }, id = "e1" } } } } } }
	elseif event.type == "hover" and event.kind == "dwell" then
		card = { from = event.from, to = event.to,
			word = pencere.layout.editor_text("code", event.from, event.to) .. #event.hits }
		return { redraw = true, editor = { code = { layer() } } }
	elseif event.type == "hover" and event.kind == "leave" then
		card = nil
		return { redraw = true, editor = { code = { layer() } } }
	end
	return nil
end

return { render = render, handle = handle }
"#;

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_page_that_hears_of_the_rest_puts_a_card_by_the_word_and_takes_it_away_when_the_pointer_goes()
-> Result<(), Error> {
    let mut page = open(HOVER)?;
    // The pointer is over `alpha` (4 in from the room's edge, a letter is some 7 wide).
    page.dwell("code", 12.0, 12.0)?;
    assert!(page.says("card alpha1"), "{:?}", page.words());
    let word = page
        .anchor_rect("code", "card")
        .expect("the range is a box");
    let card = page.bounds("card").expect("the card");
    assert_eq!((card.x, card.y), (word.x, word.bottom()));
    // The rest was told with the styles of an `id` that hold the place: the one that is under.
    let told = page
        .told()
        .iter()
        .find_map(|event| match event {
            Event::Hover(hover) if hover.kind == pencere_view::edit::HoverKind::Dwell => {
                Some(hover)
            }
            _ => None,
        })
        .expect("a rest");
    assert_eq!((told.from, told.to), (0, 5));
    assert_eq!(told.hits.len(), 1);
    assert_eq!(told.hits[0].id, "e1");
    // The word at a place, by the same rule the pointer's rest uses.
    // On `beta`: the card for the first goes, and one for the second comes.
    page.dwell("code", 60.0, 12.0)?;
    assert!(page.says("card beta0"), "{:?}", page.words());
    assert!(!page.says("card alpha1"));
    // Off the page: the card goes.
    page.leave()?;
    page.advance(0.1)?;
    assert!(!page.says("card beta0"), "{:?}", page.words());
    assert_eq!(page.anchor_rect("code", "card"), None);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_word_at_a_place_is_the_double_clicks_and_an_option_the_editor_has_not_is_an_error()
-> Result<(), Error> {
    let page = open(
        r#"
local b = pencere
local said = nil
return {
	render = function()
		return b.div():size_full():child(b.editor("code", { gutter = false }):size_full()):child(b.text(said or "none"))
	end,
	handle = function(event)
		if event.type == "editor_ready" then
			return { editor = { code = { { load = "foo.bar  baz" } } } }
		elseif event.type == "edit" then
			local a, z = pencere.layout.word_at("code", 5)
			local c, d = pencere.layout.word_at("code", 8)
			said = a .. "-" .. z .. " " .. c .. "-" .. d .. " " .. tostring(pencere.layout.word_at("code", 99))
			return { redraw = true }
		end
		return nil
	end,
}
"#,
    )?;
    // `bar` is 4 to 7; a place in the spaces is their run, 7 to 9; none past the end.
    assert!(page.says("4-7 7-9 nil"), "{:?}", page.words());
    let why = failing(
        r#"
return { render = function() return pencere.editor("code", { hover = 1, hovr = {} }) end }
"#,
    );
    assert!(why.contains("an editor has no option hovr"), "{why}");
    Ok(())
}

// --- A press that is the page's, links while a modifier is held, and the editor's keys -----------------

/// A page that makes a link of the word under the pointer while ⌘ is held: it hears of the word the
/// pointer came to, asks its (pretend) language server, and draws a look that is `held` and `over`
/// where the answer says; a ⌘-press on it is told with the link's name.
pub(crate) const LINKS: &str = r#"
local b = pencere
local text = "alpha beta\ngamma delta"
local version = 0
local told = {}
local function layer(links)
	local marks = {}
	for _, link in links do
		table.insert(marks, { from = link.from, to = link.to, color = "accent", underline = "accent",
			held = "mod", over = true, id = "def:" .. link.from })
	end
	return { decorate = "links", version = version, from = 0, to = #text, decorations = marks }
end
return {
	render = function()
		local says = table.concat(told, " ")
		return b.div():size_full():child(
			b.editor("code", {
				gutter = false,
				events = { "target" },
				gestures = { ["mod+press"] = "target", ["alt+press"] = "add_caret" },
				keymap = { ["mod+d"] = "select_all", ["mod+a"] = false, ["mod+k mod+u"] = "undo" },
			}):grow(1)):child(b.text(says):id("told"))
	end,
	handle = function(event)
		if event.type == "editor_ready" then
			return { editor = { code = { { load = text } } } }
		elseif event.type == "edit" then
			version = event.version
		elseif event.type == "target" then
			table.insert(told, event.kind .. ":" .. event.source .. ":" .. tostring(event.id))
			if event.kind == "enter" then
				return { redraw = true, editor = { code = { layer({ { from = event.from, to = event.to } }) } } }
			elseif event.kind == "leave" then
				return { redraw = true, editor = { code = { layer({}) } } }
			end
			return { redraw = true }
		elseif event.type == "keymap_pending" then
			table.insert(told, "chord:" .. table.concat(event.keys, ","))
			return { redraw = true }
		end
		return nil
	end,
}
"#;

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_modifier_makes_a_link_of_the_word_under_the_pointer_and_a_press_on_it_is_the_pages()
-> Result<(), Error> {
    use pencere_view::Cursor;
    let mut page = open(LINKS)?;
    let command = Mods {
        command: true,
        ..Mods::default()
    };
    page.move_to(Point::new(12.0, 12.0))?;
    assert_eq!(page.cursor(), Some(Cursor::Text));
    // ⌘ held over a word: the page is told of the word, answers with a look, and the pointer is the hand
    // though it did not move.
    page.hold(command)?;
    page.advance(0.2)?;
    assert!(page.says("enter:word:nil"), "{:?}", page.words());
    assert_eq!(page.cursor(), Some(Cursor::Pointer));
    // The look is the editor's to draw, by the modifier: let go, it is gone from the draw list and the
    // page is told the word was left.
    page.release()?;
    page.advance(0.2)?;
    assert!(
        page.says("enter:word:nil leave:word:nil"),
        "{:?}",
        page.words()
    );
    assert_eq!(page.cursor(), Some(Cursor::Text));
    // A ⌘-press: the caret stays, the page is told it with the link's name when it is on one.
    page.hold(command)?;
    page.advance(0.2)?;
    let before = page.editor("code").map(|b| b.ranges().to_vec());
    page.click_holding(Point::new(12.0, 12.0), command)?;
    assert!(page.says("press:link:def:0"), "{:?}", page.words());
    assert_eq!(page.editor("code").map(|b| b.ranges().to_vec()), before);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_keys_a_page_changes_and_the_chords_it_adds_work_and_a_bad_table_fails_where_it_is_written()
-> Result<(), Error> {
    let mut page = open(LINKS)?;
    page.focus_editor("code")?;
    page.press("cmd+d")?;
    assert_eq!(
        page.editor("code").map(|b| b.ranges()[0].is_empty()),
        Some(false),
        "⌘D selects all here"
    );
    page.press("cmd+k")?;
    assert!(page.says("chord:cmd+k"), "{:?}", page.words());
    page.press("cmd+u")?;
    assert!(page.says("chord:cmd+k chord:"), "{:?}", page.words());
    for (table, why) in [
        (
            r#"{ ["mod+q"] = "no_such_command" }"#,
            "no command of an editor",
        ),
        (r#"{ ["mod+q"] = "move" }"#, "needs more than its name"),
        (
            r#"{ ["mod+q mod+q mod+q mod+q mod+q"] = "undo" }"#,
            "one to four keys",
        ),
    ] {
        let failed = failing(&format!(
            "return {{ render = function() return pencere.editor(\"e\", {{ keymap = {table} }}) end }}"
        ));
        assert!(failed.contains(why), "{failed}");
    }
    // What the sandbox refuses (a press of the page's with no word of it in the events) it says as it
    // does a malformed reply.
    let refused = Headless::script(
        r#"return { render = function() return pencere.editor("e", { gestures = { ["mod+press"] = "target" } }) end }"#,
    )
    .size(1000.0, 700.0)
    .open()
    .map(drop);
    assert!(
        matches!(&refused, Err(Error::Ended(Fault::Malformed(why))) if why.contains("needs the editor's events")),
        "{refused:?}"
    );
    let failed = failing(
        r#"return { render = function() return pencere.editor("e", { gestures = { ["mod+tap"] = "word" } }) end }"#,
    );
    assert!(failed.contains("ends in \"press\""), "{failed}");
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_window_that_loses_the_keyboard_lets_go_of_the_modifiers_and_the_links_they_made()
-> Result<(), Error> {
    use pencere_view::Cursor;
    let mut page = open(LINKS)?;
    let command = Mods {
        command: true,
        ..Mods::default()
    };
    page.move_to(Point::new(12.0, 12.0))?;
    page.hold(command)?;
    page.advance(0.2)?;
    assert_eq!(page.cursor(), Some(Cursor::Pointer));
    page.set_active(false)?;
    page.advance(0.2)?;
    assert!(page.says("leave:word:nil"), "{:?}", page.words());
    assert_eq!(page.cursor(), Some(Cursor::Text));
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_key_that_is_a_command_of_the_page_and_the_start_of_a_chord_of_an_editor_is_said_where_it_is_written()
 {
    let refused = Headless::script(
        r#"return { render = function()
            return pencere.div():command("run", "Run", "cmd+d", true)
                :child(pencere.editor("e", { keymap = { ["mod+d mod+x"] = "select_all" } }))
        end }"#,
    )
    .size(1000.0, 700.0)
    .open()
    .map(drop);
    assert!(
        matches!(&refused, Err(Error::Ended(Fault::Malformed(why))) if why.contains("also a key of an editor's keymap")),
        "{refused:?}"
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn what_an_editors_keys_do_is_read_back_as_the_page_changed_them_and_a_double_press_is_a_line_if_it_says()
-> Result<(), Error> {
    let mut page = open(
        r#"
local b = pencere
local said = nil
return {
	render = function()
		return b.div():size_full():child(b.editor("code", { gutter = false,
			keymap = { ["mod+d"] = "undo", ["mod+a"] = false, ["mod+k mod+u"] = { command = "move", movement = "down" } },
			gestures = { ["press×2"] = "line" } }):grow(1)):child(b.text(said or "none"))
	end,
	handle = function(event)
		if event.type == "editor_ready" then
			return { editor = { code = { { load = "alpha beta\ngamma" } } } }
		elseif event.type == "edit" then
			local table_of = {}
			for _, entry in pencere.layout.keymap("code") do
				table_of[entry.keys] = entry.does
			end
			said = tostring(table_of["cmd+d"]) .. " " .. tostring(table_of["cmd+a"]) .. " "
				.. tostring(table_of["cmd+k cmd+u"] ~= nil) .. " " .. tostring(table_of["cmd+c"]) .. " "
				.. tostring(type(table_of["left"]))
			return { redraw = true }
		end
		return nil
	end,
}
"#,
    )?;
    // The page's: ⌘D is Undo now, ⌘A is none, the chord is there, ⌘C is what it was, ← a table.
    assert!(page.says("undo nil true copy table"), "{:?}", page.words());
    // Two quick presses are the line, as it says (the editor took them for a word).
    page.pointer(pencere_view::Input::Press {
        at: Point::new(12.0, 12.0),
        button: pencere_view::Button::Left,
        mods: Mods::default(),
        count: 2,
    })?;
    page.pointer(pencere_view::Input::Release {
        at: Point::new(12.0, 12.0),
        button: pencere_view::Button::Left,
    })?;
    let selected = page.editor("code").map(|b| b.ranges()[0]);
    assert_eq!(selected.map(|r| (r.start(), r.end())), Some((0, 11)));
    Ok(())
}

// --- Text that is not the document's ---------------------------------------------------------------------

/// A page that puts an inlay hint before the second word and an error at the end of the first line, the
/// hint with a name and a card standing by it, in a group that is hidden while ⌃⌥ are held.
pub(crate) const HINTS: &str = r#"
local b = pencere
local text = "alpha beta\ngamma delta"
local version = 0
local told = {}
local function layer()
	return { decorate = "hints", version = version, from = 0, to = #text, decorations = {
		{ from = 6, to = 6, widget = { "name:", { " int", id = "type:int", color = "accent" } }, side = "before",
			group = "hints", id = "hint", anchor = "hint" },
		{ from = 0, to = 0, widget = "expected `;` here, and a rather long message about it", side = "end",
			group = "errors", color = "error" },
	} }
end
return {
	render = function()
		return b.div():size_full():child(
			b.editor("code", { gutter = false, events = { "hover", "target" },
				widgets = { hints = { show = "unless_held" }, errors = { gap = 24 } } }):grow(1))
			:child(b.text(table.concat(told, " ")):id("told"))
	end,
	handle = function(event)
		if event.type == "editor_ready" then
			return { editor = { code = { { load = text } } } }
		elseif event.type == "edit" then
			version = event.version
			return { editor = { code = { layer() } } }
		elseif event.type == "target" then
			table.insert(told, event.kind .. ":" .. event.source .. ":" .. tostring(event.id))
			return { redraw = true }
		elseif event.type == "hover" then
			table.insert(told, "hover:" .. event.over .. ":" .. tostring(event.id))
			return { redraw = true }
		end
		return nil
	end,
}
"#;

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_page_puts_a_hint_in_a_line_and_one_at_its_end_and_presses_and_rests_on_the_hint_are_told()
-> Result<(), Error> {
    let mut page = open(HINTS)?;
    let hint = page.anchor_rect("code", "hint").expect("the hint's box");
    // It stands before `beta`, in the first line: the text of the line goes on after it.
    assert!(page.says("alpha "), "{:?}", page.words());
    assert!(
        page.words().iter().any(|w| w.contains("name:")),
        "{:?}",
        page.words()
    );
    assert!(
        page.words().iter().any(|w| w.contains("expected")),
        "{:?}",
        page.words()
    );
    // The text of the document is as it was.
    assert_eq!(
        page.editor("code").map(|b| b.text()),
        Some("alpha beta\ngamma delta".to_string())
    );
    // A press on it is the page's with no modifier, by the part's name, and the caret stays.
    let before = page.editor("code").map(|b| b.ranges().to_vec());
    let at = Point::new(hint.x + hint.w - 6.0, hint.y + hint.h / 2.0);
    page.click_at(at)?;
    assert!(page.says("press:widget:type:int"), "{:?}", page.words());
    assert_eq!(page.editor("code").map(|b| b.ranges().to_vec()), before);
    // A rest on it is told as a rest on a widget.
    page.move_to(Point::new(at.x - 1.0, at.y))?;
    page.advance(0.5)?;
    assert!(page.says("hover:widget:type:int"), "{:?}", page.words());
    // ⌃⌥ held: the group is hidden, and the line is as it was.
    page.hold(Mods {
        control: true,
        alt: true,
        ..Mods::default()
    })?;
    page.advance(0.2)?;
    assert!(
        page.anchor_rect("code", "hint").is_none(),
        "hidden while held"
    );
    assert!(!page.words().iter().any(|w| w.contains("name:")));
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_widget_too_long_or_in_parts_too_many_is_refused() -> Result<(), Error> {
    let mut page = Headless::script(
        r#"
return {
	render = function() return pencere.editor("code", { gutter = false }):size_full() end,
	handle = function(event)
		if event.type == "editor_ready" then
			return { editor = { code = { { load = "abc" } } } }
		elseif event.type == "edit" then
			return { editor = { code = { { decorate = "w", version = event.version, from = 0, to = 3,
				decorations = { { from = 1, to = 1, widget = string.rep("x", 201) } } } } } }
		end
		return nil
	end,
}
"#,
    )
    .size(400.0, 200.0)
    .open()
    .map(drop);
    let _ = &mut page;
    assert!(
        matches!(&page, Err(Error::Ended(Fault::Malformed(why))) if why.contains("characters")),
        "{page:?}"
    );
    Ok(())
}

// --- A gap above a line: a code lens ---------------------------------------------------------------------

/// A page with a code lens above the second line: a row of buttons the page makes (an element in a
/// zone that is above the line), which goes with the text as it scrolls.
pub(crate) const LENS: &str = r#"
local b = pencere
local text = "function one()\nfunction two()\nlocal x = 1"
local version = 0
local pressed = "none"
return {
	render = function()
		return b.div():size_full():col():child(
			b.editor("code", { gutter = false }):grow(1)
				:child(b.div():id("lens"):zone("lens"):h(20):row():gap(10):items_center():px(6)
					:child(b.text("2 references"):id("refs"):press("refs"):color("accent"))
					:child(b.text("Run Test"):id("run"):press("run"):color("accent"))))
			:child(b.text("pressed " .. pressed):id("said"))
	end,
	handle = function(event)
		if event.type == "editor_ready" then
			return { editor = { code = { { load = text } } } }
		elseif event.type == "edit" then
			version = event.version
			return { editor = { code = { { decorate = "lens", version = version, from = 0, to = #text,
				decorations = { { from = 15, to = 15, zone = "lens", height = 20, side = "above" } } } } } }
		elseif event.type == "press" and pencere.layout.hit({ x = event.x, y = event.y }) == "run" then
			pressed = "run"
			return { redraw = true }
		end
		return nil
	end,
}
"#;

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_code_lens_is_an_element_in_a_gap_above_a_line_that_the_page_presses_like_any()
-> Result<(), Error> {
    let mut page = open(LENS)?;
    let gap = page.zone("code", "lens").expect("the gap is open");
    // It is between the first line and the second, as tall as the page said.
    assert_eq!(gap.h, 20.0);
    let first = page.words().iter().position(|w| w.contains("function one"));
    let second = page.words().iter().position(|w| w.contains("function two"));
    assert!(first.is_some() && second.is_some());
    let lens = page.bounds("lens").expect("the element is there");
    assert_eq!((lens.y, lens.h), (gap.y, gap.h));
    // And it is pressed as any element is.
    page.click("run")?;
    assert!(page.says("pressed run"), "{:?}", page.words());
    Ok(())
}

// --- text-lua: the page that shows what an editor page does with a server's answers ------------------

const TEXT: &str = include_str!("../../../examples/text-lua/src/view/page.lua");

/// Where the run `text` was drawn, the host's points (the pointer is put a little in).
fn run_at(page: &Headless, text: &str) -> Point {
    let at = page
        .list()
        .items
        .iter()
        .find_map(|item| match item {
            Paint::Text(run) if run.text == text => Some(run.origin),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no run {text:?}: {:?}", page.words()));
    Point::new(at.x + 3.0, at.y + 6.0)
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_example_that_answers_what_a_server_would_shows_hints_a_lens_a_card_and_a_link()
-> Result<(), Error> {
    let mut page = Headless::app(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/text-lua"),
    )
    .size(640.0, 300.0)
    .open()?;
    let _ = TEXT;
    // The hints, the line's end error and the lens are there; the text is the document's.
    for word in ["left:", "right:", "1 reference", "Run Test"] {
        assert!(page.words().contains(&word), "{word} in {:?}", page.words());
    }
    assert!(page.words().iter().any(|w| w.contains("expected `;`")));
    assert_eq!(
        page.editor("code").map(|b| b.text().lines().count()),
        Some(8)
    );
    // The pointer rests on `add`: a card stands by it and says what the style under it is named.
    let at = run_at(&page, "add");
    page.move_to(at)?;
    page.advance(0.4)?;
    assert!(
        page.says("warning: warn:unused-result"),
        "{:?}",
        page.words()
    );
    let word = page.anchor_rect("code", "card").expect("the card's range");
    let card = page.bounds("card").expect("the card");
    assert_eq!(card.x, word.x);
    // ⌘ held over a word makes a link of it and a ⌘-press on it is the page's.
    let command = Mods {
        command: true,
        ..Mods::default()
    };
    page.hold(command)?;
    page.advance(0.2)?;
    page.click_holding(at, command)?;
    assert!(page.says("Go to the definition of"), "{:?}", page.words());
    // ⌃⌥ held: the group of hints is hidden.
    page.release()?;
    page.hold(Mods {
        control: true,
        alt: true,
        ..Mods::default()
    })?;
    page.advance(0.2)?;
    assert!(!page.words().contains(&"left:"), "{:?}", page.words());
    Ok(())
}
