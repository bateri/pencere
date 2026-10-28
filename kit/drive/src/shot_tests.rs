//! Pictures of a page: the same list makes the same bytes on every machine where no font is given
//! (a picture checked in, `src/fixtures/shot.png`, is what this one is), and a page under the host
//! with no screen is drawn as it was laid out. `PENCERE_SHOTS=write` writes the picture checked in
//! again, for a change of the drawing that is meant.

use std::path::PathBuf;
use std::sync::Arc;

use pencere_sandbox::{Fault, Frame, Runner};
use pencere_view::{
    DrawList, Event, Family, Font, Laid, Paint, Point, Quad, Rect, Reply, Role, Run, Size, div,
    text,
};
use tiny_skia::Pixmap;

use crate::headless::{Error, Headless};
use crate::shot::{Fonts, Palette, png};

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
/// in.
#[test]
fn a_list_drawn_with_no_font_is_the_same_picture_everywhere() {
    let drawn = png(&card(), (120, 60), &Fonts::none(), None, Palette::Dark).expect("a picture");
    assert_eq!(
        drawn,
        png(&card(), (120, 60), &Fonts::none(), None, Palette::Dark).expect("a picture"),
        "the same list, the same bytes"
    );
    if std::env::var("PENCERE_SHOTS").as_deref() == Ok("write") {
        std::fs::write(fixture(), &drawn).expect("the picture written");
    }
    let checked_in = std::fs::read(fixture()).expect("the picture checked in");
    assert!(
        drawn == checked_in,
        "the picture is not the one checked in (PENCERE_SHOTS=write writes it again)"
    );
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
	local at = 0
	for n = 1, line do at += #text[n] + 1 end
	return at
end
local function render()
	return b.div():size_full():col():gap(8)
		:child(b.editor("code", {{ gutters = {{ "numbers", {{ name = "changes", width = 6 }}, "folds" }} }}):grow(1):min_h(0)
			:child(b.div():id("peek"):zone("peek"):h(33):bg("raised2"):border(1, "border")
				:child(b.text("a peek 33 tall")))
		)
		:child(b.text(keys.label("cmd+enter") .. "  " .. keys.label("esc") .. "  " .. keys.label("cmd+shift+k")))
end
local function handle(event)
	if event.type == "editor_ready" then
		return {{ editor = {{ code = {{ {{ load = joined }} }} }} }}
	elseif event.type == "edit" then
		return {{ editor = {{ code = {{
			{{ decorate = "peek", version = event.version, from = 0, to = #joined, decorations = {{
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
