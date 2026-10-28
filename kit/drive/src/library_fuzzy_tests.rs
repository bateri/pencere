//! A Lua page with `@pencere/fuzzy` of the kit's library, driven by the host with no screen: a
//! search of a long list that goes on by itself a frame at a time, each call held to a slice, while
//! the page shows the best it found so far; and the characters a pattern found drawn in the look the
//! page gave. Heavy: the interpreter is compiled and run (`make check-heavy`).

use pencere_view::{Paint, Role};

use crate::headless::{Error, Headless};

/// A page with a field to type a pattern in and 20 000 paths of a project to look through: each
/// pattern is searched a slice at a time (`search:run`), the search of the one before narrowed to
/// what it found, and the page shows how far it went, how many paths each slice went through, and
/// the five best paths so far, the characters found in them marked.
const PICKER: &str = r#"
local fuzzy = require("@pencere/fuzzy")
local parts = { "src", "lib", "test", "docs", "assets", "view", "program", "kit", "editor", "search" }
local names = { "button", "palette", "layout", "render", "fuzzy", "keys", "syntax", "state", "theme", "rows" }
local files = {}
for k = 1, 20000 do
    local a, b, c = parts[k % 10 + 1], parts[(k // 10) % 10 + 1], names[(k // 100) % 10 + 1]
    files[k] = a .. "/" .. b .. "/" .. c .. "_" .. k .. ".lua"
end
files[13579] = "kit/view/pickerPalette.lua"
local search, shown, slices, before = nil, {}, {}, 0
local function progress(found)
    local scanned = found:progress()
    slices[#slices + 1] = scanned - before
    before = scanned
    shown = found:results()
    return true
end
return {
    render = function()
        local root = pencere.div():size_full():gap(4)
        root:child(pencere.div():text_field("find"):h(24):w(300))
        local scanned, total = 0, #files
        if search then
            scanned, total = search:progress()
        end
        root:child(pencere.text("went through " .. scanned .. " of " .. total))
        root:child(pencere.text("slices " .. table.concat(slices, " ")))
        for k = 1, math.min(5, #shown) do
            local row = shown[k]
            root:child(pencere.rich(fuzzy.spans(row.item, row.positions, { color = "accent", weight = 700 })))
        end
        return root
    end,
    handle = function(event)
        if event.type == "text" and event.field == "find" then
            slices, before = {}, 0
            if search then
                search = search:narrow(event.text)
            else
                search = fuzzy.search(event.text, files, { path = true, limit = 20 })
            end
            return search:run(progress)
        end
    end,
}
"#;

/// A page under the host with no screen, of the size the tests of the sandbox lay a page out at.
fn open(script: &str) -> Result<Headless, Error> {
    Headless::script(script).size(1000.0, 700.0).open()
}

/// How many paths each slice of the page's search went through, as it says.
fn slices(page: &Headless) -> Vec<usize> {
    let said = page
        .words()
        .into_iter()
        .find_map(|words| words.strip_prefix("slices "))
        .unwrap_or_default()
        .to_owned();
    said.split_whitespace()
        .map(|count| count.parse().expect("a count"))
        .collect()
}

/// The runs of the texts drawn, in order: their words, and whether they are in the look of a
/// match (the accent, bold).
fn runs(page: &Headless) -> Vec<(String, bool)> {
    page.list()
        .items
        .iter()
        .filter_map(|item| match item {
            Paint::Text(run) => Some((
                run.text.clone(),
                run.color == Role::Accent.solid() && run.font.weight == 700,
            )),
            _ => None,
        })
        .collect()
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_search_of_twenty_thousand_paths_goes_on_by_itself_a_slice_a_frame_and_shows_the_best()
-> Result<(), Error> {
    let mut page = open(PICKER)?;
    page.fill("find", "pickpa")?;
    // The first slice came with the reply to the typing, and the frame that drew it woke the page
    // for one more; the rest wait for the frames after it.
    assert!(page.says("went through"), "{:?}", page.words());
    assert!(!page.says("went through 20000 of 20000"));
    let first = slices(&page);
    assert!((1..=2).contains(&first.len()), "{first:?}");
    // A frame on, and the person types one more letter: the search goes on through what it found
    // and what it had not gone through yet.
    page.step()?;
    page.fill("find", "pickpal")?;
    let mut frames = 0;
    while page.step()? {
        frames += 1;
        assert!(frames < 1000, "the search never ended");
    }
    assert!(page.ended().is_none(), "{:?}", page.ended());
    let went = slices(&page);
    // Many calls, each held to a slice: none went past its time (the page is open under the
    // default limits) and none went through the whole list.
    assert!(went.len() >= 5, "{went:?}");
    assert!(
        went.iter().all(|&count| count > 0 && count <= 5000),
        "{went:?}"
    );
    let total: usize = went.iter().sum();
    assert!(
        page.says(&format!("went through {total} of {total}")),
        "{:?}",
        page.words()
    );
    assert!(
        total < 20000,
        "narrowed to what the first pattern found, and the rest: {total}"
    );
    // The best: the one path whose name has the pattern, its characters found marked.
    let drawn = runs(&page);
    let best = drawn
        .iter()
        .position(|(words, _)| words == "kit/view/")
        .expect("the best path is drawn");
    assert_eq!(
        drawn[best..best + 5],
        [
            ("kit/view/".to_owned(), false),
            ("pick".to_owned(), true),
            ("er".to_owned(), false),
            ("Pal".to_owned(), true),
            ("ette.lua".to_owned(), false),
        ],
        "{drawn:?}"
    );
    // Done, the search asks for no more frames.
    assert_eq!(page.idle()?, 0);
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_characters_a_pattern_found_are_drawn_in_the_look_the_page_gave() -> Result<(), Error> {
    let page = open(
        r#"
        local fuzzy = require("@pencere/fuzzy")
        local _, first = fuzzy.match("cbo", "CrashLoopBackOff", { highlight = "first" })
        local _, best = fuzzy.match("çiz", "Ölçüm çizelgesi")
        return { render = function()
            return pencere.div()
                :child(pencere.rich(fuzzy.spans("CrashLoopBackOff", first, { color = "accent", weight = 700 })))
                :child(pencere.rich(fuzzy.spans("Ölçüm çizelgesi", best, { color = "accent", weight = 700 })))
        end }
        "#,
    )?;
    let drawn = runs(&page);
    let marked = |words: &str| (words.to_owned(), true);
    let plain = |words: &str| (words.to_owned(), false);
    assert_eq!(
        drawn,
        [
            marked("C"),
            plain("rashLoop"),
            marked("B"),
            plain("ack"),
            marked("O"),
            plain("ff"),
            plain("Ölçüm "),
            marked("çiz"),
            plain("elgesi"),
        ]
    );
    // What is not marked is in the page's own look.
    let plain_runs = page.list().items.iter().filter_map(|item| match item {
        Paint::Text(run) if run.text == "rashLoop" => Some(run),
        _ => None,
    });
    for run in plain_runs {
        assert_ne!(run.font.weight, 700);
        assert_ne!(run.color, Role::Accent.solid());
    }
    Ok(())
}
