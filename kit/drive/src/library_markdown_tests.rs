//! `@pencere/markdown` on a page under the host with no screen: a document drawn and laid out as a
//! terminal lays it out, a link clicked, code coloured, a long document read over frames and
//! scrolled. Heavy: the interpreter is compiled and runs the pages (`make check-heavy`).

use pencere_view::{Color, Id, Paint, Point, Role};

use crate::headless::{Error, Headless};

/// A page under the host with no screen.
fn open(script: &str) -> Result<Headless, Error> {
    Headless::script(script).size(900.0, 600.0).open()
}

/// Where the first run of words that is `text` is drawn, and in what colour.
fn run(page: &Headless, text: &str) -> Option<(Point, Color)> {
    page.list().items.iter().find_map(|item| match item {
        Paint::Text(run) if run.text == text => Some((run.origin, run.color)),
        _ => None,
    })
}

fn x_of(page: &Headless, text: &str) -> f32 {
    run(page, text)
        .unwrap_or_else(|| panic!("{text:?} is not drawn: {:?}", page.words()))
        .0
        .x
}

fn y_of(page: &Headless, text: &str) -> f32 {
    run(page, text)
        .unwrap_or_else(|| panic!("{text:?} is not drawn: {:?}", page.words()))
        .0
        .y
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_document_shows_its_headings_lists_and_a_table_whose_cells_stand_in_columns()
-> Result<(), Error> {
    let page = open(
        r##"
        local markdown = require("@pencere/markdown")
        local tree = markdown.parse(table.concat({
            "# The title",
            "",
            "Some *words* here.",
            "",
            "- first",
            "- second",
            "",
            "3. three",
            "4. four",
            "",
            "| Name | Size |",
            "| :--- | ---: |",
            "| alpha | 1 |",
            "| beta | 22 |",
        }, "\n"))
        return { render = function()
            return pencere.div():size_full():p(16):child(markdown.render(tree))
        end }
        "##,
    )?;
    for words in ["The title", "first", "second", "•", "3.", "4.", "three"] {
        assert!(page.says(words), "{words:?} in {:?}", page.words());
    }
    // The heading is above the paragraph, larger, and the list's markers stand left of its items.
    assert!(y_of(&page, "The title") < y_of(&page, "Some "));
    assert!(x_of(&page, "•") < x_of(&page, "first"));
    assert_eq!(x_of(&page, "first"), x_of(&page, "second"));
    assert!(y_of(&page, "first") < y_of(&page, "second"));
    // The cells of a column stand one under another: the first column's at its start, the second's
    // at its end (`---:`), to the right of the first.
    assert_eq!(x_of(&page, "Name"), x_of(&page, "alpha"));
    assert_eq!(x_of(&page, "alpha"), x_of(&page, "beta"));
    assert!(x_of(&page, "Size") > x_of(&page, "Name"));
    assert!(
        x_of(&page, "1") > x_of(&page, "22"),
        "the second column is aligned to its end"
    );
    assert!(x_of(&page, "22") > x_of(&page, "beta"));
    assert_eq!(y_of(&page, "Name"), y_of(&page, "Size"));
    assert!(y_of(&page, "alpha") < y_of(&page, "beta"));
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_click_on_a_link_opens_its_address_or_answers_as_the_page_says() -> Result<(), Error> {
    let mut page = open(
        r#"
        local markdown = require("@pencere/markdown")
        local web = markdown.parse("[Example](https://example.com/a) and more words")
        local mail = markdown.parse("[mail](mailto:a@b.c)")
        local copied = markdown.parse("[copied](https://example.com/c)")
        return {
            render = function()
                return pencere.div():size_full():p(16):gap(8)
                    :child(pencere.div():id("web"):child(markdown.render(web, { id = "web" })))
                    :child(pencere.div():id("mail"):child(markdown.render(mail, { id = "mail" })))
                    :child(pencere.div():id("copied"):child(markdown.render(copied, {
                        id = "copied",
                        link = function(href) return { copy = href } end,
                    })))
            end,
            handle = function(event) return markdown.handle(event) end,
        }
        "#,
    )?;
    // A click on the words of the link opens its address in the person's browser.
    page.click_in("web", 4.0, 8.0)?;
    assert_eq!(page.links(), ["https://example.com/a"]);
    // The words after it are no link.
    page.click_in("web", 200.0, 8.0)?;
    assert_eq!(page.links().len(), 1);
    // An address that is not the web's is not opened by default.
    page.click_in("mail", 4.0, 8.0)?;
    assert_eq!(page.links().len(), 1);
    // The page's own answer.
    page.click_in("copied", 4.0, 8.0)?;
    assert_eq!(page.links().len(), 1);
    assert_eq!(page.pasteboard(), Some("https://example.com/c"));
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_block_of_code_is_coloured_by_the_language_the_page_gives() -> Result<(), Error> {
    let page = open(
        r#"
        local markdown = require("@pencere/markdown")
        local Syntax = require("@pencere/syntax")
        local toy = Syntax.define {
            name = "toy",
            start = function() return {} end,
            token = function(stream)
                if stream:eat_space() then return nil end
                if stream:match("local") then return "keyword" end
                if stream:match("%d+") then return "number" end
                stream:next()
            end,
            styles = { keyword = { color = "accent" }, number = { color = "warning" } },
        }
        local languages = {
            find = function(word) return word == "toy" and "toy" or nil end,
            load = function() return { name = "toy", syntax = toy } end,
        }
        local tree = markdown.parse("```toy\nlocal answer = 42\n```\n\n```\nlocal plain = 7\n```")
        return { render = function()
            return pencere.div():size_full():p(16):child(markdown.render(tree, { languages = languages }))
        end }
        "#,
    )?;
    let colour = |text: &str| run(&page, text).map(|(_, color)| color);
    assert_eq!(colour("local"), Some(Color::from(Role::Accent)));
    assert_eq!(colour("42"), Some(Color::from(Role::Warning)));
    // A block whose fence names no language is drawn plain, in one run.
    assert!(page.says("local plain = 7"), "{:?}", page.words());
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_block_of_code_is_coloured_by_the_kits_languages_where_the_page_gives_none() -> Result<(), Error>
{
    let page = open(
        r#"
        local markdown = require("@pencere/markdown")
        local tree = markdown.parse("```rust\nfn main() { let answer = 42; }\n```")
        return { render = function()
            return pencere.div():size_full():p(16):child(markdown.render(tree))
        end }
        "#,
    )?;
    let colour = |text: &str| run(&page, text).map(|(_, color)| color);
    // Rust's `fn` is a keyword and `42` a number in `@pencere/languages`' looks.
    assert_eq!(colour("fn"), Some(Color::from(Role::Accent)));
    assert_eq!(colour("42"), Some(Color::from(Role::Warning)));
    Ok(())
}

/// The number of the first paragraph whose words are in view in the box `preview`.
fn first_in_view(page: &Headless) -> Option<u32> {
    let top = page.bounds("preview")?.y;
    page.list()
        .items
        .iter()
        .filter_map(|item| match item {
            Paint::Text(run) if run.origin.y >= top => Some((run.origin.y, run.text.as_str())),
            _ => None,
        })
        .filter_map(|(y, words)| {
            Some((
                y,
                words
                    .strip_prefix("Paragraph ")?
                    .split(' ')
                    .next()?
                    .parse::<u32>()
                    .ok()?,
            ))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, number)| number)
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_long_document_is_read_over_frames_and_drawn_a_view_at_a_time_as_it_is_scrolled()
-> Result<(), Error> {
    let mut page = open(
        r#"
        local markdown = require("@pencere/markdown")
        local parts = {}
        for i = 1, 5000 do parts[#parts + 1] = "Paragraph " .. i .. " of the document." end
        local doc = markdown.document()
        doc:set(table.concat(parts, "\n\n"))
        return {
            render = function()
                return pencere.div():size_full():col()
                    :child(pencere.text(doc:busy() and "reading" or "read"))
                    :child(doc:list("preview"):grow(1):min_h(0))
                    :command("reveal", "Reveal", "ctrl+j")
            end,
            settle = function() return doc:settle() end,
            handle = function(event)
                if event.type == "command" then
                    -- The paragraph at line 6999 of the text, the 3500th.
                    doc:reveal(6999)
                    return true
                end
                return markdown.handle(event)
            end,
        }
        "#,
    )?;
    // The start of the text is shown while the rest is read, by the frames after.
    assert!(page.says("reading"), "{:?}", page.words());
    assert!(page.says("Paragraph 1 of the document."));
    page.idle()?;
    assert!(page.says("read"), "{:?}", page.words());
    let elements = |page: &Headless| page.root().map_or(0, |root| root.fold().len());
    assert!(elements(&page) < 1_000, "{} elements", elements(&page));
    // Half way down: the paragraphs about the middle, one after another.
    let extent = page
        .laid()
        .and_then(|laid| laid.scroll_extent(Id::named("preview")))
        .expect("the list scrolls");
    assert!(extent.content.h > 5000.0 * 10.0, "{extent:?}");
    page.scroll_to("preview", extent.content.h / 2.0)?;
    page.idle()?;
    let shown: Vec<u32> = page
        .words()
        .iter()
        .filter_map(|words| {
            words
                .strip_prefix("Paragraph ")?
                .split(' ')
                .next()?
                .parse()
                .ok()
        })
        .collect();
    assert!(shown.len() > 5, "{:?}", page.words());
    assert!(
        shown.iter().all(|number| (1500..3500).contains(number)),
        "{shown:?}"
    );
    assert!(
        shown.windows(2).all(|pair| pair[1] == pair[0] + 1),
        "{shown:?}"
    );
    assert!(elements(&page) < 1_000, "{} elements", elements(&page));
    // The page scrolls to the block of a line of its text: it is the first in view.
    page.press("ctrl+j")?;
    page.idle()?;
    assert_eq!(first_in_view(&page), Some(3500), "{:?}", page.words());
    Ok(())
}
