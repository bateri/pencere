//! Pictures of a page: the same list makes the same bytes on every machine where no font is given
//! (a picture checked in, `src/fixtures/shot.png`, is what this one is), and a page under the host
//! with no screen is drawn as it was laid out. `PENCERE_SHOTS=write` writes the picture checked in
//! again, for a change of the drawing that is meant.

use std::path::PathBuf;
use std::sync::Arc;

use pencere_sandbox::{Fault, Frame, Runner};
use pencere_view::{
    DrawList, Event, Family, Font, Id, Laid, Paint, Point, Quad, Rect, Reply, Role, Run, Size, div,
    text,
};
use tiny_skia::Pixmap;

use crate::headless::{Error, Headless};
use crate::shot::{Fonts, Palette, png};

#[cfg(feature = "pdf")]
fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/fixtures/shot.png")
}

fn quad(rect: Rect, role: Role) -> Paint {
    Paint::Quad(Quad {
        rect,
        radii: [0.0; 4],
        fill: Some(role.solid()),
        gradient: None,
        border: None,
        shadow: None,
        clip: Rect::new(0.0, 0.0, 120.0, 60.0),
    })
}

fn run(at: Point, words: &str, role: Role, underline: bool, clip: Rect) -> Paint {
    Paint::Text(Run {
        origin: at,
        text: words.to_owned(),
        font: Font {
            family: Family::Terminal,
            size: 16.0,
            weight: 400,
            italic: false,
        },
        color: role.solid(),
        opacity: 1.0,
        grounds: Vec::new(),
        clip,
        underline,
        strike: false,
    })
}

/// A card, a bar, three words (one underlined, one cut by its box) as boxes.
fn card() -> DrawList {
    let whole = Rect::new(0.0, 0.0, 120.0, 60.0);
    DrawList {
        items: vec![
            quad(Rect::new(4.0, 4.0, 112.0, 52.0), Role::Raised(2)),
            quad(Rect::new(4.0, 4.0, 4.0, 52.0), Role::Accent),
            run(
                Point::new(12.0, 8.0),
                "pencere",
                Role::Foreground,
                false,
                whole,
            ),
            run(Point::new(12.0, 30.0), "ok", Role::Success, true, whole),
            run(
                Point::new(40.0, 30.0),
                "cut off",
                Role::Error,
                false,
                Rect::new(40.0, 30.0, 30.0, 20.0),
            ),
        ],
    }
}

/// With no font, a list is the same picture, byte for byte, wherever it is drawn: the one checked
/// in. (The bytes are those of the PNG encoder as the PDF reader's libraries set it up: a build with
/// no PDF reader compresses the same picture to other bytes, so there it is held to what it shows
/// alone.)
#[test]
fn a_list_drawn_with_no_font_is_the_same_picture_everywhere() {
    let drawn = png(&card(), (120, 60), &Fonts::none(), None, Palette::Dark).expect("a picture");
    assert_eq!(
        drawn,
        png(&card(), (120, 60), &Fonts::none(), None, Palette::Dark).expect("a picture"),
        "the same list, the same bytes"
    );
    #[cfg(feature = "pdf")]
    {
        if std::env::var("PENCERE_SHOTS").as_deref() == Ok("write") {
            std::fs::write(fixture(), &drawn).expect("the picture written");
        }
        let checked_in = std::fs::read(fixture()).expect("the picture checked in");
        assert!(
            drawn == checked_in,
            "the picture is not the one checked in (PENCERE_SHOTS=write writes it again)"
        );
    }
    // What it shows: the card's ground, the bar, a box of a word, and nothing of the word cut off
    // past its box.
    let pixmap = Pixmap::decode_png(&drawn).expect("a PNG");
    let at = |x: u32, y: u32| {
        pixmap
            .pixel(x, y)
            .map(|pixel| (pixel.red(), pixel.green(), pixel.blue()))
    };
    let ground = at(0, 0);
    assert_ne!(at(60, 50), ground, "the card");
    assert_ne!(at(5, 20), at(60, 50), "the bar");
    assert_ne!(at(14, 16), at(60, 50), "a box of `p`");
    assert_eq!(at(80, 38), at(60, 50), "past the box that cuts it, no word");
    // The light theme is another picture of the same list.
    let light = png(&card(), (120, 60), &Fonts::none(), None, Palette::Light).expect("a picture");
    assert_ne!(light, drawn);
}

/// What is in a box with a border stands inside the border, and its ground does not cover it: the
/// four sides of the border are the box's, as a box of a style sheet's.
#[test]
fn the_ground_of_what_is_in_a_bordered_box_leaves_the_four_sides_of_the_border_showing() {
    struct Mute;
    impl pencere_view::Measure for Mute {
        fn measure(&mut self, _: &Font, _: &str) -> pencere_view::Metrics {
            pencere_view::Metrics {
                width: 0.0,
                height: 0.0,
            }
        }
    }
    let root = div().child(
        div()
            .size(40.0, 30.0)
            .border(2.0, Role::Accent)
            .child(div().size_full().bg(Role::Raised(2))),
    );
    let list = pencere_view::layout(root, Size::new(60.0, 50.0), 1.0, &mut Mute).paint(&mut Mute);
    let drawn = png(&list, (60, 50), &Fonts::none(), None, Palette::Dark).expect("a picture");
    let pixmap = Pixmap::decode_png(&drawn).expect("a PNG");
    let at = |x: u32, y: u32| {
        pixmap
            .pixel(x, y)
            .map(|pixel| (pixel.red(), pixel.green(), pixel.blue()))
    };
    let ground = at(55, 45);
    let inside = at(20, 15);
    let border = at(0, 15);
    assert_ne!(border, ground, "the border is drawn");
    assert_ne!(inside, border, "and what is in the box is not the border");
    // The border all round: the pixels of its two widths at each side, then the ground of what is in it.
    for (x, y) in [
        (0, 15),
        (1, 15),
        (38, 15),
        (39, 15),
        (20, 0),
        (20, 1),
        (20, 28),
        (20, 29),
    ] {
        assert_eq!(at(x, y), border, "the border at {x}, {y}");
    }
    for (x, y) in [(2, 15), (37, 15), (20, 2), (20, 27)] {
        assert_eq!(at(x, y), inside, "what is in the box at {x}, {y}");
    }
}

/// A rounded box its clip cuts keeps its corners where they are and is cut straight at the clip.
#[test]
fn a_rounded_box_cut_by_its_clip_is_cut_straight_and_keeps_its_own_corners() {
    let list = DrawList {
        items: vec![Paint::Quad(Quad {
            rect: Rect::new(0.0, 0.0, 40.0, 40.0),
            radii: [8.0; 4],
            fill: Some(Role::Accent.solid()),
            gradient: None,
            border: None,
            shadow: None,
            clip: Rect::new(0.0, 0.0, 40.0, 20.0),
        })],
    };
    let drawn = png(&list, (40, 40), &Fonts::none(), None, Palette::Dark).expect("a picture");
    let pixmap = Pixmap::decode_png(&drawn).expect("a PNG");
    let at = |x: u32, y: u32| pixmap.pixel(x, y).map(|pixel| pixel.red());
    let ground = at(39, 39);
    assert_ne!(at(0, 19), ground, "no corner where the clip cuts it");
    assert_eq!(at(0, 0), ground, "its own corner, rounded");
    assert_eq!(at(20, 25), ground, "nothing past the clip");
}

#[test]
fn a_picture_of_no_pixels_is_not_one_and_a_file_that_is_no_font_is_said_to_be_none() {
    assert_eq!(
        png(&card(), (0, 60), &Fonts::none(), None, Palette::Dark),
        Err(crate::shot::Error::Size(0, 60))
    );
    assert!(Fonts::none().terminal(b"not a font").is_err());
}

/// A page written in Rust: a raised card with a word, the size it is given.
struct Card;

impl Runner for Card {
    fn render(&mut self, size: Size, _now: f64, _reduce: bool) -> Result<Frame, Fault> {
        Ok(Frame {
            root: div()
                .size(size.w, size.h)
                .p(8.0)
                .child(div().bg(Role::Raised(2)).p(8.0).child(text("hello"))),
            animating: false,
            wake_at: None,
            wake: None,
        })
    }

    fn settle(&mut self) -> Result<bool, Fault> {
        Ok(false)
    }

    fn handle(&mut self, _event: Event) -> Result<Reply, Fault> {
        Ok(Reply::none())
    }

    fn lay_out(&mut self, _laid: Arc<Laid>) {}
}

/// A page under the host with no screen is pictured as its last frame drew it, at the view's
/// pixels: twice the points at a scale of two.
#[test]
fn a_page_with_no_screen_is_pictured_as_its_last_frame_drew_it() -> Result<(), Error> {
    let page = Headless::spawn(|| Ok(Card))
        .size(200.0, 100.0)
        .scale(2.0)
        .open()?;
    assert!(page.says("hello"));
    let drawn = page.shot(&Fonts::none()).expect("a picture");
    let pixmap = Pixmap::decode_png(&drawn).expect("a PNG");
    assert_eq!((pixmap.width(), pixmap.height()), (400, 200));
    let ground = pixmap.pixel(1, 1);
    assert_ne!(pixmap.pixel(30, 30), ground, "the card");
    assert_eq!(drawn, page.shot(&Fonts::none()).expect("a picture"));
    Ok(())
}

/// The example editor's page, pictured: its lines, coloured, the same picture twice.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_example_editor_is_pictured_with_its_lines_in_their_colours() -> Result<(), Error> {
    let page =
        Headless::app(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/editor-lua"))
            .size(500.0, 300.0)
            .open()?;
    let drawn = page.shot(&Fonts::none()).expect("a picture");
    assert_eq!(drawn, page.shot(&Fonts::none()).expect("a picture"));
    let pixmap = Pixmap::decode_png(&drawn).expect("a PNG");
    let colours: std::collections::HashSet<(u8, u8, u8)> = pixmap
        .pixels()
        .iter()
        .map(|pixel| (pixel.red(), pixel.green(), pixel.blue()))
        .collect();
    assert!(
        colours.len() > 5,
        "the ground, the editor's, its lines in their looks: {} colours",
        colours.len()
    );
    Ok(())
}

/// The gallery of the kit's components (`examples/ui-lua`), pictured in a dark theme and in a light
/// one: the same page, every colour a role, so the two pictures are of the same shapes in other
/// colours. `PENCERE_SHOTS=ui` writes them to `target/` in the terminal's font of a Mac, for an eye
/// to check what is aligned and what overlaps.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_gallery_of_components_is_pictured_in_a_dark_and_a_light_theme() -> Result<(), Error> {
    let gallery = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/ui-lua");
    let fonts = match std::env::var("PENCERE_SHOTS").as_deref() {
        Ok("ui") => Fonts::none()
            .terminal(&std::fs::read("/System/Library/Fonts/Menlo.ttc").expect("Menlo"))
            .expect("a font"),
        _ => Fonts::none(),
    };
    let page = Headless::app(&gallery)
        .measure(fonts.clone())
        .size(760.0, 1250.0)
        .scale(2.0)
        .open()?;
    let dark = page.shot_in(&fonts, Palette::Dark).expect("a picture");
    let light = page.shot_in(&fonts, Palette::Light).expect("a picture");
    assert_eq!(
        dark,
        page.shot_in(&fonts, Palette::Dark).expect("a picture")
    );
    assert_ne!(dark, light, "the same page in two themes");
    for drawn in [&dark, &light] {
        let pixmap = Pixmap::decode_png(drawn).expect("a PNG");
        let colours: std::collections::HashSet<(u8, u8, u8)> = pixmap
            .pixels()
            .iter()
            .map(|pixel| (pixel.red(), pixel.green(), pixel.blue()))
            .collect();
        assert!(colours.len() > 8, "{} colours", colours.len());
    }
    if std::env::var("PENCERE_SHOTS").as_deref() == Ok("ui") {
        let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target");
        std::fs::write(target.join("ui-gallery-dark.png"), &dark).expect("written");
        std::fs::write(target.join("ui-gallery-light.png"), &light).expect("written");
    }
    Ok(())
}

/// The gallery's read-only editor opens the library's editor menu on a right press: the standard lines,
/// Cut and Paste off for a document that cannot be changed, the page's own line after them, and Copy
/// puts the caret's line on the pasteboard.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_gallerys_editor_opens_the_editor_menu_on_a_right_press() -> Result<(), Error> {
    use pencere_view::{Button, Input, MenuEntry, Mods, Point};
    let gallery = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/ui-lua");
    let mut page = Headless::app(&gallery).size(760.0, 1250.0).open()?;
    let room = page.bounds("doc").expect("the editor");
    let at = Point::new(room.x + 6.0, room.y + 6.0);
    page.pointer(Input::Press {
        at,
        button: Button::Right,
        mods: Mods::default(),
        count: 1,
    })?;
    page.pointer(Input::Release {
        at,
        button: Button::Right,
        mods: Mods::default(),
    })?;
    let titles: Vec<String> = page
        .menu()
        .expect("a menu")
        .entries
        .iter()
        .map(|entry| match entry {
            MenuEntry::Item { title, enabled, .. } | MenuEntry::Paste { title, enabled, .. } => {
                format!("{title}{}", if *enabled { "" } else { " (off)" })
            }
            MenuEntry::Separator => "-".to_owned(),
        })
        .collect();
    assert_eq!(
        titles,
        [
            "Cut (off)",
            "Copy",
            "Paste (off)",
            "-",
            "Select All",
            "-",
            "Toggle Line Comment (off)",
            "Select All Occurrences",
            "-",
            "Command palette"
        ]
    );
    page.pick_titled("Copy")?;
    assert_eq!(page.pasteboard(), Some("A haystack of words\n"));
    Ok(())
}

/// The example whose gutter is the page's (`examples/gutter-lua`), pictured with a breakpoint put,
/// the pointer over an empty cell of its column, a peek open in a zone, and the overview ruler's
/// marks: the shapes of the bars, the triangle where lines were taken away, the dots and the gap, in
/// the options the page set (a wider bar, a wider ruler). `PENCERE_SHOTS=gutter` writes it to
/// `target/gutter.png` in the terminal's font of a Mac, for an eye to check.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_example_with_a_gutter_of_its_own_is_pictured_with_its_markers_and_its_zone()
-> Result<(), Error> {
    let fonts = match std::env::var("PENCERE_SHOTS").as_deref() {
        Ok("gutter") => Fonts::none()
            .terminal(&std::fs::read("/System/Library/Fonts/Menlo.ttc").expect("Menlo"))
            .expect("a font"),
        _ => Fonts::none(),
    };
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/gutter-lua");
    let mut page = Headless::app(&dir)
        .measure(fonts.clone())
        .size(560.0, 420.0)
        .scale(2.0)
        .open()?;
    page.click_gutter("code", "breakpoints", 1, pencere_view::Button::Left)?;
    page.click_gutter("code", "breakpoints", 8, pencere_view::Button::Left)?;
    page.click_gutter("code", "changes", 12, pencere_view::Button::Left)?;
    page.hover_gutter("code", "breakpoints", 15)?;
    let drawn = page.shot_in(&fonts, Palette::Dark).expect("a picture");
    let light = page.shot_in(&fonts, Palette::Light).expect("a picture");
    assert_ne!(drawn, light);
    assert_eq!(
        drawn,
        page.shot_in(&fonts, Palette::Dark).expect("a picture")
    );
    if std::env::var("PENCERE_SHOTS").as_deref() == Ok("gutter") {
        let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target");
        std::fs::write(target.join("gutter.png"), &drawn).expect("written");
        std::fs::write(target.join("gutter-light.png"), &light).expect("written");
    }
    Ok(())
}

/// What an editor of numbers, a zone and the keys' caps look like where they were wrong: a numbers
/// column that is the first of the gutter with one, two and three digits (it keeps room to its left),
/// a gap of a peek no taller than its element (no band under it), a mark on the ruler that stops at
/// its line's rows with a gap after it, and the caps of a Mac's keys written as words.
/// `PENCERE_SHOTS=defects` writes them to `target/numbers-{1,2,3}.png`, in the terminal's font of a
/// Mac, for an eye to check.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn numbers_of_one_two_and_three_digits_keep_room_and_a_gap_is_as_tall_as_its_element()
-> Result<(), Error> {
    let fonts = match std::env::var("PENCERE_SHOTS").as_deref() {
        Ok("defects") => Fonts::none()
            .terminal(&std::fs::read("/System/Library/Fonts/Menlo.ttc").expect("Menlo"))
            .expect("a font"),
        _ => Fonts::none(),
    };
    for (digits, lines) in [(1, 9), (2, 99), (3, 999)] {
        let source = format!(
            r#"
local b = pencere
local keys = require("@pencere/keys")
local text = {{}}
for n = 1, {lines} do text[n] = "line " .. n end
local joined = table.concat(text, "\n")
local function start_of(line)
	local at = 1
	for n = 1, line do at += #text[n] + 1 end
	return at
end
local function render()
	return b.div():size_full():col():gap(8)
		:child(b.editor("code", {{ gutters = {{ "numbers", {{ name = "changes", width = 6 }}, "folds" }} }}):grow(1):min_h(0)
			:child(b.div():id("peek"):zone("peek"):h(33):bg("raised2"):border(1, "border")
				:child(b.text("a peek 33 tall")))
		)
		:child(b.text(keys.label("cmd+enter") .. "  " .. keys.label("escape") .. "  " .. keys.label("cmd+shift+k")))
end
local function handle(event)
	if event.type == "editor_ready" then
		return {{ editor = {{ code = {{ {{ load = joined }} }} }} }}
	elseif event.type == "edit" then
		return {{ editor = {{ code = {{
			{{ decorate = "peek", version = event.version, from = 1, to = #joined + 1, decorations = {{
				{{ from = start_of(2), to = start_of(2), zone = "peek", height = 33 }},
				{{ from = start_of(2), to = start_of(2), ruler = "blue", lane = "full" }},
			}} }},
		}} }} }}
	end
	return nil
end
return {{ render = render, handle = handle }}
"#
        );
        let page = Headless::script(source)
            .measure(fonts.clone())
            .size(360.0, 220.0)
            .scale(2.0)
            .open()?;
        let gap = page.zone("code", "peek").expect("the gap is open");
        assert_eq!(
            gap.h, 33.0,
            "{digits} digits: the gap is as tall as its element"
        );
        let drawn = page.shot_in(&fonts, Palette::Dark).expect("a picture");
        if std::env::var("PENCERE_SHOTS").as_deref() == Ok("defects") {
            let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target");
            std::fs::write(target.join(format!("numbers-{digits}.png")), &drawn).expect("written");
        }
    }
    Ok(())
}

/// A page that makes a link of the word under the pointer while ⌘ is held, pictured with ⌘ held over
/// `alpha` (drawn as a link) and with it let go (as it was). `PENCERE_SHOTS=links` writes them to
/// `target/links-{held,free}.png`, in the terminal's font of a Mac, for an eye to check.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_word_is_a_link_while_the_modifier_is_held_and_the_pointer_is_over_it() -> Result<(), Error> {
    use pencere_view::Mods;
    let fonts = match std::env::var("PENCERE_SHOTS").as_deref() {
        Ok("links") => Fonts::none()
            .terminal(&std::fs::read("/System/Library/Fonts/Menlo.ttc").expect("Menlo"))
            .expect("a font"),
        _ => Fonts::none(),
    };
    let mut page = Headless::script(crate::luau_editor_tests::LINKS)
        .measure(fonts.clone())
        .size(360.0, 120.0)
        .scale(2.0)
        .open()?;
    page.move_to(Point::new(16.0, 14.0))?;
    page.hold(Mods {
        command: true,
        ..Mods::default()
    })?;
    page.advance(0.2)?;
    let held = page.shot_in(&fonts, Palette::Dark).expect("a picture");
    page.release()?;
    page.advance(0.2)?;
    let free = page.shot_in(&fonts, Palette::Dark).expect("a picture");
    assert_ne!(held, free, "the link is drawn only while ⌘ is held");
    if std::env::var("PENCERE_SHOTS").as_deref() == Ok("links") {
        let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target");
        std::fs::write(target.join("links-held.png"), &held).expect("written");
        std::fs::write(target.join("links-free.png"), &free).expect("written");
    }
    Ok(())
}

/// An inlay hint (with a part that is a name to press) before a word, an error at a line's end cut
/// where the room ends, and the same page with ⌃⌥ held (the hint's group is hidden then).
/// `PENCERE_SHOTS=hints` writes them to `target/hints-{shown,held}.png`.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn an_inlay_hint_and_a_line_end_error_are_pictured_in_their_lines() -> Result<(), Error> {
    use pencere_view::Mods;
    let fonts = match std::env::var("PENCERE_SHOTS").as_deref() {
        Ok("hints") => Fonts::none()
            .terminal(&std::fs::read("/System/Library/Fonts/Menlo.ttc").expect("Menlo"))
            .expect("a font"),
        _ => Fonts::none(),
    };
    let mut page = Headless::script(crate::luau_editor_tests::HINTS)
        .measure(fonts.clone())
        .size(300.0, 110.0)
        .scale(2.0)
        .open()?;
    let shown = page.shot_in(&fonts, Palette::Dark).expect("a picture");
    page.hold(Mods {
        control: true,
        alt: true,
        ..Mods::default()
    })?;
    page.advance(0.2)?;
    let held = page.shot_in(&fonts, Palette::Dark).expect("a picture");
    assert_ne!(shown, held, "the group is hidden while ⌃⌥ are held");
    if std::env::var("PENCERE_SHOTS").as_deref() == Ok("hints") {
        let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target");
        std::fs::write(target.join("hints-shown.png"), &shown).expect("written");
        std::fs::write(target.join("hints-held.png"), &held).expect("written");
    }
    Ok(())
}

/// A code lens: a row of buttons in a gap above the second line, the text going on under it.
/// `PENCERE_SHOTS=lens` writes it to `target/lens.png`.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_code_lens_is_pictured_in_its_gap_above_a_line() -> Result<(), Error> {
    let fonts = match std::env::var("PENCERE_SHOTS").as_deref() {
        Ok("lens") => Fonts::none()
            .terminal(&std::fs::read("/System/Library/Fonts/Menlo.ttc").expect("Menlo"))
            .expect("a font"),
        _ => Fonts::none(),
    };
    let page = Headless::script(crate::luau_editor_tests::LENS)
        .measure(fonts.clone())
        .size(300.0, 110.0)
        .scale(2.0)
        .open()?;
    let drawn = page.shot_in(&fonts, Palette::Dark).expect("a picture");
    if std::env::var("PENCERE_SHOTS").as_deref() == Ok("lens") {
        let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target");
        std::fs::write(target.join("lens.png"), &drawn).expect("written");
    }
    Ok(())
}

/// The example that answers what a server would (`examples/text-lua`), in the options it sets that
/// are not the defaults (a rest of a quarter of a second, hints a little smaller with rounder
/// corners): at rest with its hints, lens and error; with the pointer resting on a word (a card); and
/// with ⌘ held over one (a link). `PENCERE_SHOTS=text` writes them to `target/text-{rest,card,link}.png`,
/// in the terminal's font of a Mac, for an eye to check.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_example_with_a_server_less_language_is_pictured_at_rest_with_a_card_and_with_a_link()
-> Result<(), Error> {
    use pencere_view::Mods;
    let fonts = match std::env::var("PENCERE_SHOTS").as_deref() {
        Ok("text") => Fonts::none()
            .terminal(&std::fs::read("/System/Library/Fonts/Menlo.ttc").expect("Menlo"))
            .expect("a font"),
        _ => Fonts::none(),
    };
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/text-lua");
    let mut page = Headless::app(&dir)
        .measure(fonts.clone())
        .size(520.0, 230.0)
        .scale(2.0)
        .open()?;
    let rest = page.shot_in(&fonts, Palette::Dark).expect("a picture");
    let add = page
        .list()
        .items
        .iter()
        .find_map(|item| match item {
            pencere_view::Paint::Text(run) if run.text == "add" => Some(run.origin),
            _ => None,
        })
        .expect("the call");
    let at = Point::new(add.x / 2.0 + 3.0, add.y / 2.0 + 6.0);
    page.move_to(at)?;
    page.advance(0.5)?;
    let card = page.shot_in(&fonts, Palette::Dark).expect("a picture");
    page.leave()?;
    page.advance(0.3)?;
    page.move_to(at)?;
    page.hold(Mods {
        command: true,
        ..Mods::default()
    })?;
    page.advance(0.2)?;
    let link = page.shot_in(&fonts, Palette::Dark).expect("a picture");
    assert!(rest != card && card != link && rest != link);
    if std::env::var("PENCERE_SHOTS").as_deref() == Ok("text") {
        let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target");
        for (name, bytes) in [("rest", &rest), ("card", &card), ("link", &link)] {
            std::fs::write(target.join(format!("text-{name}.png")), bytes).expect("written");
        }
    }
    Ok(())
}

/// A box that zooms to the size its page says (a double tap goes to `smart = 3`) and is pinched on
/// from there is pictured with its words in a face as many times larger and its picture drawn at the
/// larger size, the box itself the size it was. `PENCERE_SHOTS=zoom` writes them to
/// `target/zoom-{rest,tapped,pinched}.png` for an eye to check.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_box_zoomed_to_the_size_its_page_says_is_pictured_with_its_words_as_much_larger()
-> Result<(), Error> {
    let fonts = Fonts::none();
    let mut page = Headless::script(
        r#"
        return {
            render = function()
                return pencere.div():size_full():p(8):child(
                    pencere.div():id("pane"):size(200, 90):scroll():zoom({ smart = 3, max = 6 }):bg("raised1"):child(
                        pencere.div():row():gap(6):child(pencere.text("pencere")):child(
                            pencere.div():size(24, 12):bg("accent")))
                )
            end,
            handle = function() return true end,
        }
        "#,
    )
    .size(240.0, 110.0)
    .open()?;
    let rest = page.shot_in(&fonts, Palette::Dark).expect("a picture");
    let face = |page: &Headless| {
        page.list()
            .items
            .iter()
            .filter_map(|item| match item {
                Paint::Text(run) => Some(run.font.size),
                _ => None,
            })
            .fold(0.0f32, f32::max)
    };
    let at_one = face(&page);
    assert!(at_one > 0.0);
    let pane = |page: &Headless| page.laid().and_then(|laid| laid.bounds(Id::named("pane")));
    let box_at_one = pane(&page).expect("the box");
    // A double tap goes where the page said, over a moment.
    page.smart_magnify("pane")?;
    page.idle()?;
    let tapped = page.shot_in(&fonts, Palette::Dark).expect("a picture");
    assert!(rest != tapped, "a double tap changes what is drawn");
    assert!((face(&page) - 3.0 * at_one).abs() < 1e-3, "{}", face(&page));
    // And a pinch goes on from there, to the end of the range the page gave, and no further.
    page.pinch("pane", 1.0)?;
    let pinched = page.shot_in(&fonts, Palette::Dark).expect("a picture");
    assert!(tapped != pinched);
    assert!((face(&page) - 6.0 * at_one).abs() < 1e-3, "{}", face(&page));
    assert_eq!(
        pane(&page),
        Some(box_at_one),
        "the box stays the size it was"
    );
    if std::env::var("PENCERE_SHOTS").as_deref() == Ok("zoom") {
        let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target");
        for (name, bytes) in [("rest", &rest), ("tapped", &tapped), ("pinched", &pinched)] {
            std::fs::write(target.join(format!("zoom-{name}.png")), bytes).expect("written");
        }
    }
    Ok(())
}

/// The same lines selected in two editors side by side, the left square as every editor's is and the
/// right with the corners of a terminal's selection (`selection.radius = 0.22`): the corners that are
/// open round, the turns inward are filled. `PENCERE_SHOTS=selection` writes it to
/// `target/selection-rounded.png`, in the terminal's font of a Mac, for an eye to check.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_selection_over_lines_is_pictured_square_and_with_round_corners() -> Result<(), Error> {
    let fonts = match std::env::var("PENCERE_SHOTS").as_deref() {
        Ok("selection") => Fonts::none()
            .terminal(&std::fs::read("/System/Library/Fonts/Menlo.ttc").expect("Menlo"))
            .expect("a font"),
        _ => Fonts::none(),
    };
    let mut page = Headless::script(
        r#"
        local text = "the quick brown fox jumps\nover the lazy dog and runs\naway\n\nfar beyond the hills, past it\nthe end"
        local function editor(id, radius)
            return pencere.editor(id, { selection = { radius = radius }, gutter = false,
                colors = { selection_blurred = "selection" } }):grow(1)
        end
        return {
            render = function()
                return pencere.div():size_full():row():gap(8)
                    :child(editor("square", 0)):child(editor("round", 0.22))
            end,
            handle = function(event)
                if event.type == "editor_ready" then
                    return { editor = { [event.editor] = { { load = text } } } }
                end
            end,
        }
        "#,
    )
    .measure(fonts.clone())
    .size(560.0, 150.0)
    .scale(2.0)
    .open()?;
    page.select("square", 10, 85)?;
    page.select("round", 10, 85)?;
    let drawn = page.shot_in(&fonts, Palette::Dark).expect("a picture");
    if std::env::var("PENCERE_SHOTS").as_deref() == Ok("selection") {
        let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target");
        std::fs::write(target.join("selection-rounded.png"), &drawn).expect("written");
    }
    // The round selection is drawn with figures, the square one with boxes alone.
    let figures = page
        .list()
        .items
        .iter()
        .filter(|item| matches!(item, Paint::Figure(_)))
        .count();
    assert!(figures > 0, "a turn inward is a figure");
    Ok(())
}

/// The caret as the page says it looks: the default (a bar), a block with round corners and a glow
/// as a terminal's cursor has them, and a block away from the keyboard that is hollow. The same
/// line in each, and a block mid-glide over the word. `PENCERE_SHOTS=caret` writes them to
/// `target/caret-{default,block,hollow,glide}.png`, in the terminal's font of a Mac, for an eye to
/// check.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_caret_is_pictured_as_a_bar_as_a_round_block_with_a_glow_and_as_a_hollow_one()
-> Result<(), Error> {
    let fonts = match std::env::var("PENCERE_SHOTS").as_deref() {
        Ok("caret") => Fonts::none()
            .terminal(&std::fs::read("/System/Library/Fonts/Menlo.ttc").expect("Menlo"))
            .expect("a font"),
        _ => Fonts::none(),
    };
    let open = |caret: &str| {
        Headless::script(format!(
            r#"
        local text = "let total = add(3, 4);\nprintln!(total)"
        return {{
            render = function()
                return pencere.div():size_full():child(
                    pencere.editor("e", {{ gutter = false, caret = {caret} }}):grow(1))
            end,
            handle = function(event)
                if event.type == "editor_ready" then
                    return {{ keyboard = {{ field = "e" }}, editor = {{ e = {{ {{ load = text }} }} }} }}
                end
            end,
        }}
        "#
        ))
        .measure(fonts.clone())
        .size(260.0, 60.0)
        .scale(2.0)
        .open()
    };
    let mut shots = Vec::new();
    for (name, caret, away) in [
        ("default", "{}", false),
        (
            "block",
            r#"{ shape = "block", radius = 0.2, glow = 1, color = "accent" }"#,
            false,
        ),
        (
            "hollow",
            r#"{ shape = "block", radius = 0.2, glow = 1, color = "accent", unfocused = "hollow" }"#,
            true,
        ),
    ] {
        let mut page = open(caret)?;
        page.caret_to("e", 4)?;
        if away {
            page.set_active(false)?;
        }
        shots.push((
            name,
            page.shot_in(&fonts, Palette::Dark).expect("a picture"),
        ));
    }
    assert!(shots[1].1 != shots[0].1 && shots[2].1 != shots[1].1);
    // A block that glides over a word: a few frames into the spring the letters it passes over show
    // on it, in its text colour, and the box has not yet come to the end.
    let mut gliding = open(
        r#"{ shape = "block", radius = 0.2, color = "accent", blink = false, motion = { spring = 30 } }"#,
    )?;
    gliding.caret_to("e", 4)?;
    gliding.advance(1.0)?;
    gliding.caret_to("e", 15)?;
    gliding.advance(0.03)?;
    shots.push((
        "glide",
        gliding.shot_in(&fonts, Palette::Dark).expect("a picture"),
    ));
    if std::env::var("PENCERE_SHOTS").as_deref() == Ok("caret") {
        let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target");
        for (name, bytes) in &shots {
            std::fs::write(target.join(format!("caret-{name}.png")), bytes).expect("written");
        }
    }
    Ok(())
}

/// The example editor under a host that says nothing of its terminal and under one that says how
/// bateri's draws its cursor and its selection (a block with a glow, a selection with round corners):
/// the page takes it all where it says nothing (`@pencere/terminal`), and the same page is the
/// picture on both. `PENCERE_SHOTS=terminal` writes them to `target/terminal-{default,bateri}.png`,
/// in the terminal's font of a Mac, for an eye to check.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_example_editor_is_pictured_under_a_host_that_says_nothing_and_under_one_that_says_how_bateri_draws()
-> Result<(), Error> {
    use pencere_view::edit::{CaretMotion, CaretShape, CaretUnfocused};
    use pencere_view::{TerminalCaret, TerminalLook, TerminalSelection};
    let fonts = match std::env::var("PENCERE_SHOTS").as_deref() {
        Ok("terminal") => Fonts::none()
            .terminal(&std::fs::read("/System/Library/Fonts/Menlo.ttc").expect("Menlo"))
            .expect("a font"),
        _ => Fonts::none(),
    };
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/editor-lua");
    let bateri = TerminalLook {
        caret: TerminalCaret {
            shape: Some(CaretShape::Block),
            width: Some(2.0),
            radius: Some(0.1),
            glow: Some(1.0),
            blink: Some(true),
            blink_interval: Some(0.5),
            unfocused: Some(CaretUnfocused::Hollow),
            motion: Some(CaretMotion::Spring(30.0)),
        },
        selection: TerminalSelection { radius: Some(0.22) },
    };
    let mut shots = Vec::new();
    for (name, look) in [("default", TerminalLook::default()), ("bateri", bateri)] {
        let mut page = Headless::app(&dir)
            .measure(fonts.clone())
            .size(560.0, 200.0)
            .scale(2.0)
            .terminal_look(look)
            .open()?;
        page.select("sql", 92, 133)?;
        // The page reads the look again when the host says it changed.
        if name == "default" {
            page.set_terminal_look(TerminalLook::default())?;
        }
        shots.push((
            name,
            page.shot_in(&fonts, Palette::Dark).expect("a picture"),
        ));
    }
    assert_ne!(shots[0].1, shots[1].1, "the page took the look");
    if std::env::var("PENCERE_SHOTS").as_deref() == Ok("terminal") {
        let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target");
        for (name, bytes) in &shots {
            std::fs::write(target.join(format!("terminal-{name}.png")), bytes).expect("written");
        }
    }
    Ok(())
}

/// The example that moves files with its view (`examples/files-lua`), pictured after the person
/// dropped a picture on it: the picture its program attached at the top, drawn by its digest, and
/// the dropped one in its row, drawn by its handle. `PENCERE_SHOTS=files` writes it to `target/` in
/// the terminal's font of a Mac, for an eye to check.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_files_example_is_pictured_with_the_picture_it_attached_and_a_file_it_received()
-> Result<(), Error> {
    use crate::headless::Calls;
    use serde_json::json;
    let example = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/files-lua");
    let picture = std::fs::read(example.join("src/assets/picture.png")).expect("the picture");
    let name = format!("sha256:{}", pencere_wire::bundle::Digest::of(&picture));
    let fonts = match std::env::var("PENCERE_SHOTS").as_deref() {
        Ok("files") => Fonts::none()
            .terminal(&std::fs::read("/System/Library/Fonts/Menlo.ttc").expect("Menlo"))
            .expect("a font"),
        _ => Fonts::none(),
    };
    let calls = Calls::new()
        .on("picture", move |_| Ok(json!({ "name": name })))
        .on("receive", |_| Ok(json!({ "size": 4096 })));
    let mut page = Headless::app(&example)
        .measure(fonts.clone())
        .attached(&picture)
        .size(700.0, 320.0)
        .scale(2.0)
        .peer(calls)
        .open()?;
    page.give_files(
        "drop",
        pencere_view::FileSource::Drop,
        &[("photo.png", &picture)],
    )?;
    assert!(page.says("Received"), "{:?}", page.words());
    let drawn = page.shot(&fonts).expect("a picture");
    let images = page.page().images();
    let pictures = page
        .list()
        .items
        .iter()
        .filter(|item| match item {
            pencere_view::Paint::Image(picture) => images
                .and_then(|images| images.bitmap(&picture.src, picture.page, picture.full()))
                .is_some(),
            _ => false,
        })
        .count();
    assert_eq!(pictures, 2, "the attached picture and the received one");
    if std::env::var("PENCERE_SHOTS").as_deref() == Ok("files") {
        let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target");
        std::fs::write(target.join("files-example.png"), &drawn).expect("written");
    }
    Ok(())
}

// --- Rows ---------------------------------------------------------------------------------------

/// Items that draw past their boxes in every way the drawing has: a soft shadow, a stroke, a corner
/// cut by a clip, words with accents above and tails below, a large size, a line under and through
/// them, and the characters a face has no glyph for (a colour emoji, an ideograph, a box).
fn items_that_reach() -> Vec<Paint> {
    let world = Rect::new(0.0, 0.0, 400.0, 300.0);
    let words = |y: f32, size: f32, text: &str, underline: bool, strike: bool| {
        Paint::Text(Run {
            origin: Point::new(12.0, y),
            text: text.to_owned(),
            font: Font {
                family: Family::Terminal,
                size,
                weight: 400,
                italic: false,
            },
            color: Role::Foreground.solid(),
            opacity: 1.0,
            grounds: Vec::new(),
            clip: world,
            underline,
            strike,
        })
    };
    let boxed = |rect: Rect, shadow: Option<pencere_view::Shadow>, clip: Rect| {
        Paint::Quad(Quad {
            rect,
            radii: [8.0; 4],
            fill: Some(Role::Raised(3).solid()),
            gradient: None,
            border: Some(pencere_view::Border {
                width: 2.0,
                color: Role::Accent.solid(),
            }),
            shadow,
            clip,
        })
    };
    let shadow = |dy: f32, blur: f32| {
        Some(pencere_view::Shadow {
            dx: 0.0,
            dy,
            blur,
            color: Role::Shadow.alpha(0.6),
        })
    };
    vec![
        boxed(
            Rect::new(40.0, 100.0, 120.0, 60.0),
            shadow(6.0, 16.0),
            world,
        ),
        boxed(
            Rect::new(40.0, 100.0, 120.0, 60.0),
            shadow(-9.0, 30.0),
            world,
        ),
        boxed(
            Rect::new(40.0, 100.0, 120.0, 60.0),
            None,
            Rect::new(60.0, 110.0, 50.0, 30.0),
        ),
        boxed(
            Rect::new(40.0, 100.0, 120.0, 60.0),
            shadow(4.0, 12.0),
            Rect::new(0.0, 90.0, 400.0, 50.0),
        ),
        words(100.0, 16.0, "ĞÜŞİÖÇ ğüşıöç Ǻ ÁÉÍÓÚ gjpqy", false, false),
        words(100.0, 16.0, "underline and strike", true, true),
        words(100.0, 64.0, "Hgj Ç", false, false),
        words(100.0, 12.5, "→ ✓ ⌘ • …", false, false),
        words(100.0, 16.0, "😀 🎉 👍🏽 漢字 かな 한", false, false),
        words(100.0, 16.0, "\u{10FFFD} \u{E000} ꙮ", false, false),
        words(100.0, 100.0, "😀", false, false),
    ]
}

/// The rows of a picture that are not the ground, the first and the last.
fn inked_rows(drawn: &crate::shot::Raster) -> Option<(u32, u32)> {
    let ground = Palette::Dark
        .role(Role::Background)
        .map(|c| (c * 255.0).round() as u8);
    let width = drawn.width() as usize;
    let mut rows = drawn
        .rgba()
        .chunks(width * 4)
        .enumerate()
        .filter(|(_, row)| row.chunks(4).any(|px| px.get(..3) != ground.get(..3)))
        .map(|(y, _)| y as u32);
    let first = rows.next()?;
    Some((first, rows.next_back().unwrap_or(first)))
}

/// Nothing an item draws is drawn outside the rows `reach` says: a row that no item reaches is the
/// ground, and a row whose reaching items are the same is the same.
#[test]
fn what_an_item_draws_is_in_the_rows_it_reaches() {
    for (which, fonts) in [
        ("carried", Fonts::carried()),
        ("installed", Fonts::installed()),
    ] {
        for (n, item) in items_that_reach().into_iter().enumerate() {
            let list = DrawList {
                items: vec![item.clone()],
            };
            let drawn = crate::shot::raster(&list, (400, 300), &fonts, None, Palette::Dark)
                .expect("a picture");
            let Some((first, last)) = inked_rows(&drawn) else {
                continue;
            };
            let (top, bottom) = crate::shot::reach(&item, &fonts).unwrap_or_else(|| {
                panic!("{which} item {n} draws rows {first}..={last} and reaches none")
            });
            assert!(
                f64::from(top) <= f64::from(first) && f64::from(bottom) >= f64::from(last) + 1.0,
                "{which} item {n}: draws rows {first}..={last}, reaches {top}..{bottom}: {item:?}"
            );
        }
    }
}

/// The rows of a picture drawn alone are the rows of the picture drawn whole: the same pixels, to
/// the byte, wherever the band falls (across a shadow, a stroke, a clip, or a line of words).
#[test]
fn rows_drawn_alone_are_the_rows_of_the_picture_drawn_whole() {
    let mut items = vec![Paint::Quad(Quad {
        rect: Rect::new(0.0, 0.0, 400.0, 300.0),
        radii: [0.0; 4],
        fill: Some(Role::Background.solid()),
        gradient: None,
        border: None,
        shadow: None,
        clip: Rect::new(0.0, 0.0, 400.0, 300.0),
    })];
    items.extend(items_that_reach());
    let list = DrawList { items };
    for (which, fonts) in [
        ("carried", Fonts::carried()),
        ("installed", Fonts::installed()),
    ] {
        let whole =
            crate::shot::raster(&list, (400, 300), &fonts, None, Palette::Dark).expect("a picture");
        for (top, rows) in [
            (0, 20),
            (80, 20),
            (90, 13),
            (100, 34),
            (118, 34),
            (151, 17),
            (160, 40),
            (190, 110),
            (0, 300),
            (299, 1),
        ] {
            let band = crate::shot::raster_rows(
                &list,
                (400, 300),
                &fonts,
                None,
                Palette::Dark,
                (top, rows),
            )
            .expect("a band");
            let want = whole.strip(top, rows).expect("rows");
            let wrong = band
                .rgba()
                .chunks(4)
                .zip(want.rgba().chunks(4))
                .filter(|(a, b)| a != b)
                .count();
            assert_eq!(
                (band.width(), band.height()),
                (400, rows),
                "{which}: the band's size"
            );
            assert_eq!(
                wrong, 0,
                "{which}: rows {top}+{rows} differ from the whole in {wrong} pixels"
            );
        }
    }
}

/// What `raster_rows_in` is to hand back for the rows `top`..`top + rows` of the picture `whole`: the
/// rows, each with the page's last column repeated `right` times, and after the last of them its last
/// row `below` times.
fn with_edge(
    whole: &crate::shot::Raster,
    top: u32,
    rows: u32,
    (right, below): (u32, u32),
) -> Vec<u8> {
    let width = whole.width() as usize;
    let mut out = Vec::new();
    let mut last = Vec::new();
    let lines = whole.rgba().chunks(width * 4);
    for line in lines.skip(top as usize).take(rows as usize) {
        last.clear();
        last.extend_from_slice(line);
        for _ in 0..right {
            if let Some(edge) = line.last_chunk::<4>() {
                last.extend_from_slice(edge);
            }
        }
        out.extend_from_slice(&last);
    }
    for _ in 0..below {
        out.extend_from_slice(&last);
    }
    out
}

/// Rows drawn one band after another in the same buffers are the rows the picture drawn whole has, with
/// the page's edge at the right and at the foot, whatever the buffers held before: the mask of a clip, the
/// rows of a band that was drawn higher, the margin. And once the buffers have been as large as the
/// largest band they are not grown again.
#[test]
fn bands_drawn_in_the_same_buffers_are_the_rows_of_the_whole_with_the_pages_edge_past_them() {
    let mut items = vec![Paint::Quad(Quad {
        rect: Rect::new(0.0, 0.0, 400.0, 300.0),
        radii: [0.0; 4],
        fill: Some(Role::Background.solid()),
        gradient: None,
        border: None,
        shadow: None,
        clip: Rect::new(0.0, 0.0, 400.0, 300.0),
    })];
    items.extend(items_that_reach());
    let list = DrawList { items };
    let bands = [
        (0, 20),
        (80, 20),
        (90, 13),
        (100, 34),
        (118, 34),
        (151, 17),
        (160, 40),
        (190, 110),
        (0, 300),
        (299, 1),
        (118, 34),
        (0, 20),
        (280, 20),
    ];
    for (which, fonts) in [
        ("carried", Fonts::carried()),
        ("installed", Fonts::installed()),
    ] {
        let whole =
            crate::shot::raster(&list, (400, 300), &fonts, None, Palette::Dark).expect("a picture");
        let mut scratch = crate::shot::Scratch::new();
        let mut grown = None;
        for pass in 0..2 {
            for (top, rows) in bands {
                let edge = (10, if top + rows >= 300 { 30 } else { 0 });
                let got = crate::shot::raster_rows_in(
                    &mut scratch,
                    &list,
                    (400, 300),
                    &fonts,
                    None,
                    Palette::Dark,
                    (top, rows),
                    edge,
                )
                .expect("a band")
                .to_vec();
                let want = with_edge(&whole, top, rows, edge);
                assert_eq!(
                    got.len(),
                    want.len(),
                    "{which}: rows {top}+{rows}: the size"
                );
                let wrong = got
                    .chunks(4)
                    .zip(want.chunks(4))
                    .filter(|(a, b)| a != b)
                    .count();
                assert_eq!(
                    wrong, 0,
                    "{which} pass {pass}: rows {top}+{rows} differ from the whole in {wrong} pixels"
                );
            }
            if pass == 0 {
                grown = Some(scratch.grown());
            }
        }
        assert_eq!(
            Some(scratch.grown()),
            grown,
            "{which}: the buffers are grown again by bands they have been as large as"
        );
    }
}

/// A clip far past the picture (more than a pixel count can say) leaves a mask that the next clip clears
/// without a sum that overflows.
#[test]
fn a_clip_far_past_the_picture_is_cleared_for_the_next_clip() {
    let boxed = |rect: Rect| {
        Paint::Quad(Quad {
            rect,
            radii: [0.0; 4],
            fill: None,
            gradient: None,
            border: Some(pencere_view::Border {
                width: 2.0,
                color: Role::Accent.solid(),
            }),
            shadow: None,
            clip: rect,
        })
    };
    let list = DrawList {
        items: vec![
            boxed(Rect::new(2.0e19, 5.0, 1.0e13, 100.0)),
            boxed(Rect::new(4.0, 4.0, 20.0, 20.0)),
        ],
    };
    let drawn = crate::shot::raster(&list, (40, 40), &Fonts::none(), None, Palette::Dark)
        .expect("a picture");
    let ground = drawn.rgba().get(39 * 160 + 39 * 4).copied();
    assert_ne!(
        drawn.rgba().get(4 * 160 + 4 * 4).copied(),
        ground,
        "the box"
    );
}

/// Words in the face `italic` or not, at `x` (not a whole number, where the test says so).
fn words_at(x: f32, y: f32, size: f32, text: &str, italic: bool, strike: bool) -> Paint {
    Paint::Text(Run {
        origin: Point::new(x, y),
        text: text.to_owned(),
        font: Font {
            family: Family::Terminal,
            size,
            weight: 400,
            italic,
        },
        color: Role::Foreground.solid(),
        opacity: 1.0,
        grounds: Vec::new(),
        clip: Rect::new(0.0, 0.0, 400.0, 300.0),
        underline: strike,
        strike,
    })
}

/// What `items_that_reach` has but for the ones with a soft shadow (which are blurred over the whole
/// width of a picture), and words at places that are not whole pixels, in two faces.
fn items_that_reach_across() -> Vec<Paint> {
    let mut items: Vec<Paint> = items_that_reach()
        .into_iter()
        .filter(|item| !matches!(item, Paint::Quad(quad) if quad.shadow.is_some()))
        .collect();
    for (n, x) in [100.0_f32, 100.25, 100.5, 100.75, 101.4999, 40.3, 7.5]
        .into_iter()
        .enumerate()
    {
        let y = 10.0 + n as f32 * 36.0;
        items.push(words_at(
            x,
            y,
            16.0,
            "fn fjord(f: &f32) -> j { f(j) }",
            n % 2 == 1,
            false,
        ));
        items.push(words_at(
            x + 0.5,
            y,
            21.0,
            "iIl1| ffj jf",
            n % 2 == 0,
            n == 3,
        ));
    }
    // Words that are wider than their size says: a letter that is three ems wide, and (where no face
    // has the letters) boxes that are as wide whatever the size.
    items.push(words_at(
        20.0,
        262.0,
        2.0,
        &"abcdefghij".repeat(7),
        false,
        false,
    ));
    items.push(words_at(
        30.0,
        278.0,
        10.0,
        "\u{2E3B}\u{2E3B}\u{2E3B}\u{FDFD}",
        false,
        false,
    ));
    items
}

/// A rounded box with a stroke whose edges are not whole pixels, and one is at the middle of one
/// (150.3 + 90.2): the corners' curves are made of the numbers as the sums that make them round.
fn box_between_pixels() -> Paint {
    Paint::Quad(Quad {
        rect: Rect::new(150.3, 30.7, 90.2, 40.5),
        radii: [5.0; 4],
        fill: Some(Role::Raised(3).solid()),
        gradient: None,
        border: Some(pencere_view::Border {
            width: 1.5,
            color: Role::Accent.solid(),
        }),
        shadow: None,
        clip: Rect::new(0.0, 0.0, 400.0, 300.0),
    })
}

/// The columns of a picture that are not the ground, the first and the last.
fn inked_columns(drawn: &crate::shot::Raster) -> Option<(u32, u32)> {
    let ground = Palette::Dark
        .role(Role::Background)
        .map(|c| (c * 255.0).round() as u8);
    let width = drawn.width() as usize;
    let mut columns: Vec<u32> = Vec::new();
    for row in drawn.rgba().chunks(width * 4) {
        for (x, px) in row.chunks(4).enumerate() {
            if px.get(..3) != ground.get(..3) {
                columns.push(x as u32);
            }
        }
    }
    Some((*columns.iter().min()?, *columns.iter().max()?))
}

/// Nothing an item draws is drawn outside the columns `across` says: the lean of an italic letter, the
/// rule under words, the edge of a box, a letter a face has not.
#[test]
fn what_an_item_draws_is_in_the_columns_it_reaches() {
    let mut most_lean = 0.0_f32;
    for (which, fonts) in [
        ("carried", Fonts::carried()),
        ("installed", Fonts::installed()),
        ("none", Fonts::none()),
    ] {
        let mut items = items_that_reach_across();
        items.push(box_between_pixels());
        for (n, item) in items.into_iter().enumerate() {
            let list = DrawList {
                items: vec![item.clone()],
            };
            let drawn = crate::shot::raster(&list, (400, 300), &fonts, None, Palette::Dark)
                .expect("a picture");
            let Some((first, last)) = inked_columns(&drawn) else {
                continue;
            };
            let (left, right) = crate::shot::across(&item, &fonts).unwrap_or_else(|| {
                panic!("{which} item {n} draws columns {first}..={last} and reaches none")
            });
            assert!(
                f64::from(left) <= f64::from(first) && f64::from(right) >= f64::from(last) + 1.0,
                "{which} item {n}: draws columns {first}..={last}, reaches {left}..{right}: {item:?}"
            );
            // How far the letters lean past what the words' advances say, for the notes.
            if let Paint::Text(run) = &item
                && !run.underline
                && which == "carried"
            {
                let advances = run.text.chars().count() as f32 * 0.6 * run.font.size;
                most_lean = most_lean.max(last as f32 + 1.0 - (run.origin.x + advances));
            }
        }
    }
    eprintln!("the letters lean at the most {most_lean} pixels past the advances of the words");
}

/// The columns of a picture drawn alone are the columns of the same rows drawn across the picture,
/// wherever they start and end and whatever rows they are of: the same pixels, to the byte (the rows
/// themselves are the rows of the whole to a few levels (sixteen in a rounded corner), where a box's edge is between two pixels:
/// `rows_drawn_alone_are_the_rows_of_the_picture_drawn_whole`). What is not a whole pixel is rounded
/// where it is in the whole picture: a letter in the column 100.5 is in the 101st, wherever the part
/// starts. A shadow that reaches them is not a part.
#[test]
fn columns_drawn_alone_are_the_columns_of_the_rows_drawn_across_the_picture() {
    let ground = Paint::Quad(Quad {
        rect: Rect::new(0.0, 0.0, 400.0, 300.0),
        radii: [0.0; 4],
        fill: Some(Role::Background.solid()),
        gradient: None,
        border: None,
        shadow: None,
        clip: Rect::new(0.0, 0.0, 400.0, 300.0),
    });
    let (width, height) = (400_u32, 300_u32);
    let mut worst = 0_u8;
    let mut worst_pixels = 0;
    let mut parts = 0;
    for (which, fonts, between) in [
        ("carried", Fonts::carried(), false),
        ("installed", Fonts::installed(), false),
        ("none", Fonts::none(), false),
        ("carried, a box between pixels", Fonts::carried(), true),
    ] {
        let mut items = vec![ground.clone()];
        items.extend(items_that_reach_across());
        if between {
            items.push(box_between_pixels());
        }
        let list = DrawList { items };
        let mut scratch = crate::shot::Scratch::new();
        let mut across_it = crate::shot::Scratch::new();
        for (top, rows) in [(0, 300), (20, 40), (95, 30), (270, 30), (299, 1)] {
            let edge = (10, if top + rows >= height { 25 } else { 0 });
            let base = crate::shot::raster_part_in(
                &mut across_it,
                &list,
                (width, height),
                &fonts,
                None,
                Palette::Dark,
                (top, rows),
                (0, width),
                edge,
            )
            .expect("rows")
            .to_vec();
            let line = (width + edge.0) as usize * 4;
            for (left, to) in [
                (0, 400),
                (0, 1),
                (1, 2),
                (7, 100),
                (100, 101),
                (99, 230),
                (101, 399),
                (150, 400),
                (233, 400),
                (399, 400),
                (13, 14),
            ] {
                let edge = (if to == width { edge.0 } else { 0 }, edge.1);
                let got = crate::shot::raster_part_in(
                    &mut scratch,
                    &list,
                    (width, height),
                    &fonts,
                    None,
                    Palette::Dark,
                    (top, rows),
                    (left, to),
                    edge,
                )
                .expect("a part")
                .to_vec();
                let end = if to == width { width + edge.0 } else { to };
                let mut want = Vec::new();
                for row in base.chunks(line) {
                    want.extend_from_slice(&row[left as usize * 4..end as usize * 4]);
                }
                assert_eq!(
                    got.len(),
                    want.len(),
                    "{which}: columns {left}..{to} of rows {top}+{rows}: the size"
                );
                let mut wrong = 0;
                for (a, b) in got.chunks(4).zip(want.chunks(4)) {
                    let most = a
                        .iter()
                        .zip(b)
                        .map(|(p, q)| p.abs_diff(*q))
                        .max()
                        .unwrap_or(0);
                    if most > 0 {
                        wrong += 1;
                        worst = worst.max(most);
                    }
                }
                parts += 1;
                // The words and the boxes are the same to the byte. The edges of a box that are
                // between two pixels, with corners that are curves, are a sixteenth of a pixel's
                // coverage (a level or sixteen) in a pixel or two: the supersampling of the path
                // takes a sample a part of a pixel one way or the other as the sums that make the
                // curve are rounded where they are small and where they are large.
                if between {
                    worst_pixels = worst_pixels.max(wrong);
                    assert!(
                        wrong <= 8 && worst <= 16,
                        "{which}: columns {left}..{to} of rows {top}+{rows} differ from the rows across the picture in {wrong} pixels, {worst} levels at the most"
                    );
                } else {
                    assert_eq!(
                        wrong, 0,
                        "{which}: columns {left}..{to} of rows {top}+{rows} differ from the rows across the picture in {wrong} pixels, {worst} levels at the most"
                    );
                }
            }
        }
        // A soft shadow is blurred over the whole width: a part it reaches is not drawn.
        let shadowed = DrawList {
            items: vec![items_that_reach().remove(0)],
        };
        assert!(
            crate::shot::raster_part_in(
                &mut scratch,
                &shadowed,
                (width, height),
                &fonts,
                None,
                Palette::Dark,
                (90, 40),
                (50, 100),
                (0, 0),
            )
            .is_err(),
            "{which}: a part that a shadow reaches is refused"
        );
    }
    eprintln!(
        "{parts} parts drawn alone are the rows across the picture (a box between pixels: {worst_pixels} pixels and {worst} levels at the most)"
    );
}
