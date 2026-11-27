//! What the faces of a picture draw: a bold run is not the regular one, an italic run is not
//! upright, the Turkish letters, arrows and a command sign are in the faces the machine has, a face
//! with no glyph of a letter hands it to one that has, and what no face has is a hollow box. The
//! tests of the machine's own faces are `#[ignore]` (a machine of another set of faces draws
//! another picture): `cargo test -p pencere-drive --features shot --lib fonts_tests -- --ignored`.

use pencere_view::{DrawList, Family, Font, Measure, Paint, Point, Rect, Role, Run};

use crate::shot::{Fonts, Palette, Raster, raster};

/// The machine's faces: SF Mono and SF on a Mac.
fn installed() -> Fonts {
    Fonts::installed()
}

/// Whether this is a Mac with the faces the tests of the machine's own are about (SF Mono, SF, Apple
/// Color Emoji): where it is not, they have nothing to hold the layer to and say so.
fn has_the_faces_of_a_mac() -> bool {
    let has = ["SFNSMono.ttf", "SFNS.ttf", "Apple Color Emoji.ttc"]
        .iter()
        .all(|name| {
            std::path::Path::new("/System/Library/Fonts")
                .join(name)
                .exists()
        });
    if !has {
        eprintln!(
            "skipped: this machine has not the faces of a Mac (SF Mono, SF, Apple Color Emoji)"
        );
    }
    has
}

/// The faces the program carries, and no others.
fn embedded() -> Fonts {
    Fonts::carried()
}

fn font(family: Family, weight: u16, italic: bool) -> Font {
    Font {
        family,
        size: 32.0,
        weight,
        italic,
    }
}

/// `words` in `font` on a plain ground, 640 × 80 pixels. (A picture that cannot be made fails the
/// test that asked for it, as an `expect` in the test would.)
#[allow(clippy::expect_used)]
fn drawn(fonts: &Fonts, font: &Font, words: &str) -> Raster {
    let whole = Rect::new(0.0, 0.0, 640.0, 80.0);
    let list = DrawList {
        items: vec![Paint::Text(Run {
            origin: Point::new(10.0, 10.0),
            text: words.to_owned(),
            font: *font,
            color: Role::Foreground.solid(),
            opacity: 1.0,
            grounds: Vec::new(),
            clip: whole.into(),
            underline: false,
            strike: false,
        })],
    };
    raster(&list, (640, 80), fonts, None, Palette::Dark).expect("a picture")
}

/// How many pixels of the picture are not its ground.
fn ink(picture: &Raster) -> usize {
    let ground = picture.rgba().get(..4).map(<[u8]>::to_vec);
    picture
        .rgba()
        .chunks(4)
        .filter(|pixel| Some(pixel) != ground.as_deref().as_ref())
        .count()
}

/// The last column of the picture that has ink in it.
fn right_edge(picture: &Raster) -> Option<usize> {
    let ground = picture.rgba().get(..4)?.to_vec();
    let width = picture.width() as usize;
    picture
        .rgba()
        .chunks(4)
        .enumerate()
        .filter(|(_, pixel)| *pixel != ground.as_slice())
        .map(|(at, _)| at % width)
        .max()
}

/// Whether a pixel of the picture is a colour the text's own is not (a colour glyph's).
fn has_colour(picture: &Raster) -> bool {
    picture
        .rgba()
        .chunks(4)
        .any(|pixel| matches!(pixel, [r, _, b, _] if r.abs_diff(*b) > 80))
}

/// Whether the ink of the picture is the outline of a box: all of it within three pixels of the edge
/// of the rectangle it covers, the four corners of that rectangle in it, and its middle bare (what a
/// face with no glyph for a letter draws in its place, which a letter does not look like).
fn hollow(picture: &Raster) -> Option<bool> {
    let ground = picture.rgba().get(..4)?.to_vec();
    let width = picture.width() as usize;
    let inked: Vec<(usize, usize)> = picture
        .rgba()
        .chunks(4)
        .enumerate()
        .filter(|(_, pixel)| *pixel != ground.as_slice())
        .map(|(at, _)| (at % width, at / width))
        .collect();
    let left = inked.iter().map(|p| p.0).min()?;
    let right = inked.iter().map(|p| p.0).max()?;
    let top = inked.iter().map(|p| p.1).min()?;
    let bottom = inked.iter().map(|p| p.1).max()?;
    let near =
        |(x, y): (usize, usize)| x < left + 3 || x + 3 > right || y < top + 3 || y + 3 > bottom;
    let has = |at: (usize, usize)| inked.contains(&at);
    Some(
        inked.iter().all(|p| near(*p))
            && has((left, top))
            && has((right, top))
            && has((left, bottom))
            && has((right, bottom))
            && !has(((left + right) / 2, (top + bottom) / 2)),
    )
}

const TURKISH: &str = "ğüşıöç İĞÜŞÖÇ";
const SIGNS: &str = "→ ✓ ⌘";

#[test]
#[ignore = "fonts: the machine's own faces"]
fn a_bold_run_is_not_the_regular_one() {
    if !has_the_faces_of_a_mac() {
        return;
    }
    let fonts = installed();
    for family in [Family::Terminal, Family::System] {
        let regular = drawn(&fonts, &font(family, 400, false), "Bold and ağ");
        let bold = drawn(&fonts, &font(family, 700, false), "Bold and ağ");
        assert!(
            bold.rgba() != regular.rgba(),
            "{family:?}: bold is the regular glyphs"
        );
        assert!(ink(&bold) > ink(&regular), "{family:?}: bold has more ink");
    }
}

#[test]
#[ignore = "fonts: the machine's own faces"]
fn an_italic_run_is_not_upright() {
    if !has_the_faces_of_a_mac() {
        return;
    }
    let fonts = installed();
    for family in [Family::Terminal, Family::System] {
        let regular = drawn(&fonts, &font(family, 400, false), "Italic and ağ");
        let italic = drawn(&fonts, &font(family, 400, true), "Italic and ağ");
        assert!(
            italic.rgba() != regular.rgba(),
            "{family:?}: italic is the upright glyphs"
        );
    }
}

#[test]
fn a_bold_run_of_the_carried_faces_is_not_the_regular_one() {
    let fonts = embedded();
    for family in [Family::Terminal, Family::System] {
        let regular = drawn(&fonts, &font(family, 400, false), "Bold and ağ");
        let bold = drawn(&fonts, &font(family, 700, false), "Bold and ağ");
        assert!(
            ink(&regular) > 0,
            "{family:?}: the carried faces draw the letters"
        );
        assert!(
            bold.rgba() != regular.rgba(),
            "{family:?}: bold is the regular glyphs"
        );
        assert!(ink(&bold) > ink(&regular), "{family:?}: bold has more ink");
    }
}

#[test]
fn an_italic_run_of_the_carried_faces_is_not_upright() {
    let fonts = embedded();
    for family in [Family::Terminal, Family::System] {
        let regular = drawn(&fonts, &font(family, 400, false), "Italic and ağ");
        let italic = drawn(&fonts, &font(family, 400, true), "Italic and ağ");
        assert!(
            ink(&regular) > 0,
            "{family:?}: the carried faces draw the letters"
        );
        assert!(
            italic.rgba() != regular.rgba(),
            "{family:?}: italic is the upright glyphs"
        );
    }
}

/// The carried system family is Inter, not the terminal's JetBrains Mono: the same picture as Inter's
/// own file made the system's face, in proportional letters (an `i` is narrower than a `W`), where the
/// terminal's are as wide as one another.
#[test]
fn the_carried_system_family_is_inter() {
    let carried = embedded();
    let inter = Fonts::none()
        .system(include_bytes!("../fonts/InterVariable.ttf"))
        .expect("Inter");
    for weight in [400, 700] {
        let face = font(Family::System, weight, false);
        assert!(
            drawn(&carried, &face, "Inter ağ 0123").rgba()
                == drawn(&inter, &face, "Inter ağ 0123").rgba(),
            "weight {weight}: the system family is not Inter's own face"
        );
    }
    let mono = font(Family::Terminal, 400, false);
    let sans = font(Family::System, 400, false);
    assert!(
        drawn(&carried, &sans, "Inter ağ").rgba() != drawn(&carried, &mono, "Inter ağ").rgba(),
        "the system family is drawn in the terminal's face"
    );
    let mut measure = carried.clone();
    let width = |measure: &mut Fonts, face: &Font, words: &str| measure.measure(face, words).width;
    assert!(
        (width(&mut measure, &mono, "iiii") - width(&mut measure, &mono, "WWWW")).abs() < 0.01,
        "the terminal's letters are as wide as one another"
    );
    assert!(
        width(&mut measure, &sans, "iiii") + 1.0 < width(&mut measure, &sans, "WWWW"),
        "the system's letters are proportional"
    );
}

/// Inter has an optical size axis, set to the size of the text in points: the same text drawn for a
/// page at two pixels to the point is not drawn as at one (it is measured so too), where JetBrains
/// Mono, which has no such axis, is.
#[test]
fn a_face_with_an_optical_size_is_set_to_the_size_of_the_text_in_points() {
    let (one, two) = (Fonts::carried(), Fonts::carried().scaled(2.0));
    let sans = font(Family::System, 400, false);
    let mono = font(Family::Terminal, 400, false);
    let (mut measure_one, mut measure_two) = (one.clone(), two.clone());
    assert!(
        drawn(&one, &sans, "Optical size").rgba() != drawn(&two, &sans, "Optical size").rgba(),
        "the system family does not follow the optical size"
    );
    assert!(
        (measure_one.measure(&sans, "Optical size").width
            - measure_two.measure(&sans, "Optical size").width)
            .abs()
            > 0.01,
        "a run is laid out as at the size it is not drawn at"
    );
    assert!(
        drawn(&one, &mono, "Optical size").rgba() == drawn(&two, &mono, "Optical size").rgba(),
        "a face with no such axis follows the scale"
    );
    assert!(
        measure_one.measure(&mono, "Optical size").width
            == measure_two.measure(&mono, "Optical size").width
    );
    // What is drawn is what was measured, at each scale.
    for (fonts, measure) in [(&one, &mut measure_one), (&two, &mut measure_two)] {
        let width = measure.measure(&sans, "Optical size").width;
        let edge = right_edge(&drawn(fonts, &sans, "Optical size")).expect("ink");
        assert!(
            edge as f32 <= 10.0 + width + 1.0 && edge as f32 >= 10.0 + width * 0.7,
            "the ink ({edge}) is not in the measure ({width})"
        );
    }
}

#[test]
#[ignore = "fonts: the machine's own faces"]
fn the_turkish_letters_arrows_and_signs_are_drawn_in_the_machines_faces() {
    if !has_the_faces_of_a_mac() {
        return;
    }
    let fonts = installed();
    for family in [Family::Terminal, Family::System] {
        for c in TURKISH.chars().chain(SIGNS.chars()).filter(|c| *c != ' ') {
            let face = font(family, 400, false);
            assert_eq!(
                fonts.covers(&face, c),
                Some(true),
                "{family:?}: {c} has a glyph"
            );
            let picture = drawn(&fonts, &face, &c.to_string());
            assert!(ink(&picture) > 0, "{family:?}: {c} draws");
            assert_eq!(
                hollow(&picture),
                Some(false),
                "{family:?}: {c} is not a box"
            );
        }
    }
}

#[test]
#[ignore = "fonts: the machine's own faces"]
fn a_colour_emoji_and_two_ideographs_come_from_a_face_that_has_them() {
    if !has_the_faces_of_a_mac() {
        return;
    }
    let fonts = installed();
    let face = font(Family::Terminal, 400, false);
    for c in ['😀', '漢', '字'] {
        assert_eq!(
            fonts.covers(&face, c),
            Some(true),
            "{c} has a glyph in some face"
        );
        let picture = drawn(&fonts, &face, &c.to_string());
        assert!(ink(&picture) > 0, "{c} draws");
        assert_ne!(hollow(&picture), Some(true), "{c} is not a box");
    }
    assert!(
        has_colour(&drawn(&fonts, &face, "😀")),
        "the emoji is in its own colours"
    );
    assert!(
        !has_colour(&drawn(&fonts, &face, "漢字")),
        "an ideograph is in the text's colour"
    );
    // A mixed run is measured and drawn char by char, where the glyph is.
    let mixed = "a😀b漢c";
    let mut measure = fonts.clone();
    let width = measure.measure(&face, mixed).width;
    let apart: f32 = mixed
        .chars()
        .map(|c| measure.measure(&face, &c.to_string()).width)
        .sum();
    assert!(
        (width - apart).abs() < 0.01,
        "a run is as wide as its letters"
    );
    // What is drawn is what was measured: the ink ends inside the width the run was laid out in.
    let edge = right_edge(&drawn(&fonts, &face, mixed)).expect("ink");
    assert!(
        edge as f32 <= 10.0 + width + 1.0,
        "the ink ({edge}) is past the measure ({width})"
    );
    assert!(
        edge as f32 >= 10.0 + width * 0.7,
        "the ink ({edge}) falls far short of it ({width})"
    );
}

#[test]
fn with_no_face_of_the_machine_the_turkish_letters_are_drawn_and_an_emoji_and_an_ideograph_are_boxes()
 {
    let fonts = embedded();
    for family in [Family::Terminal, Family::System] {
        let face = font(family, 400, false);
        for c in TURKISH.chars().filter(|c| *c != ' ') {
            assert_eq!(
                fonts.covers(&face, c),
                Some(true),
                "{family:?}: {c} has a glyph"
            );
            let picture = drawn(&fonts, &face, &c.to_string());
            assert!(ink(&picture) > 0, "{family:?}: {c} draws");
            assert_eq!(
                hollow(&picture),
                Some(false),
                "{family:?}: {c} is not a box"
            );
        }
        for c in ['😀', '漢'] {
            assert_eq!(
                fonts.covers(&face, c),
                Some(false),
                "{family:?}: {c} has no glyph"
            );
            let picture = drawn(&fonts, &face, &c.to_string());
            assert_eq!(
                hollow(&picture),
                Some(true),
                "{family:?}: {c} is a hollow box"
            );
        }
    }
}

/// A machine with no face of its own (a bare server: its font directories are empty or not there)
/// draws in the faces the program carries, as `carried` does: the letters, bold and italic, and a box
/// for an emoji and an ideograph.
#[test]
fn a_machine_with_no_faces_draws_in_the_carried_ones() {
    let fonts = Fonts::in_dirs(&[std::path::PathBuf::from("/no/such/directory/of/faces")]);
    let carried = Fonts::carried();
    let (upright, bold) = (
        font(Family::Terminal, 400, false),
        font(Family::Terminal, 700, false),
    );
    let sans = font(Family::System, 400, false);
    assert!(
        drawn(&fonts, &sans, "Bold ağ").rgba() == drawn(&carried, &sans, "Bold ağ").rgba(),
        "the system's family is Inter, as `carried`'s"
    );
    for c in TURKISH.chars().filter(|c| *c != ' ') {
        assert_eq!(fonts.covers(&upright, c), Some(true), "{c} has a glyph");
        let picture = drawn(&fonts, &upright, &c.to_string());
        assert!(ink(&picture) > 0, "{c} draws");
        assert_eq!(hollow(&picture), Some(false), "{c} is not a box");
    }
    for c in ['😀', '漢'] {
        assert_eq!(fonts.covers(&upright, c), Some(false), "{c} has no glyph");
        assert_eq!(
            hollow(&drawn(&fonts, &upright, &c.to_string())),
            Some(true),
            "{c} is a box"
        );
    }
    let (regular, heavy) = (
        drawn(&fonts, &upright, "Bold ağ"),
        drawn(&fonts, &bold, "Bold ağ"),
    );
    assert!(
        heavy.rgba() != regular.rgba(),
        "bold is not the regular glyphs"
    );
    assert!(
        regular.rgba() == drawn(&carried, &upright, "Bold ağ").rgba(),
        "the same faces as `carried`"
    );
}

/// The sheet of the characters the faces are asked for, in each family and style, 2400 × 1360
/// pixels: `PENCERE_FRAMES=<folder> PENCERE_FONTS=installed|carried`, written as `sheet-<which>.png`.
#[test]
#[ignore = "frames: writes a picture into the folder PENCERE_FRAMES names"]
fn the_character_sheet() {
    let Ok(folder) = std::env::var("PENCERE_FRAMES") else {
        return;
    };
    let which = std::env::var("PENCERE_FONTS").unwrap_or_else(|_| "installed".to_owned());
    // A page at two pixels to the point (the terminal's on a screen of the Mac's): the 36 pixel
    // text is 18 points, where a face with an optical size is set to it.
    let fonts = if which == "carried" {
        Fonts::carried()
    } else {
        Fonts::installed()
    }
    .scaled(2.0);
    let lines = [
        "The quick brown fox 0123",
        TURKISH,
        SIGNS,
        "😀 🎉 🚀 ❤",
        "漢字 かな 한글",
        "no glyph: \u{0378} \u{10FFFD}",
    ];
    let whole = Rect::new(0.0, 0.0, 2400.0, 1360.0);
    let mut items = Vec::new();
    let styles = [
        ("normal", 400, false),
        ("bold", 700, false),
        ("italic", 400, true),
        ("bold italic", 700, true),
    ];
    for (column, family) in [Family::Terminal, Family::System].into_iter().enumerate() {
        for (row, (label, weight, italic)) in styles.iter().enumerate() {
            let top = 20.0 + 335.0 * row as f32;
            let left = 30.0 + 1200.0 * column as f32;
            let mut put = |at: f32, words: &str, face: Font, role: Role| {
                items.push(Paint::Text(Run {
                    origin: Point::new(left, at),
                    text: words.to_owned(),
                    font: face,
                    color: role.solid(),
                    opacity: 1.0,
                    grounds: Vec::new(),
                    clip: whole.into(),
                    underline: false,
                    strike: false,
                }));
            };
            put(
                top,
                &format!("{family:?}, {label}"),
                Font {
                    family: Family::Terminal,
                    size: 24.0,
                    weight: 400,
                    italic: false,
                },
                Role::Dim,
            );
            for (n, words) in lines.iter().enumerate() {
                put(
                    top + 34.0 + 48.0 * n as f32,
                    words,
                    Font {
                        family,
                        size: 36.0,
                        weight: *weight,
                        italic: *italic,
                    },
                    Role::Foreground,
                );
            }
        }
    }
    let picture = raster(
        &DrawList { items },
        (2400, 1360),
        &fonts,
        None,
        Palette::Dark,
    )
    .expect("a picture");
    std::fs::write(
        std::path::Path::new(&folder).join(format!("sheet-{which}.png")),
        picture.png().expect("a PNG"),
    )
    .expect("written");
}

/// Where each place of a line is, measured as the editor asks (`Measure::offsets`): what measuring
/// the start of the line up to it comes to, to the bit, for every place and every face; a place that
/// is not between two characters, or past the end, is the whole line, as the measure of the kit says.
#[test]
fn the_places_of_a_line_are_where_its_starts_measure() {
    let lines = [
        "fn main() -> ExitCode {",
        "çğış öü ÇĞİŞÖÜ — ←→ ⌘",
        "a\u{200B}b\u{FE0F}c\u{0301}d",
        "日本語 and 😀 boxes",
        "",
    ];
    let faces = [
        (embedded(), font(Family::Terminal, 400, false)),
        (embedded(), font(Family::System, 700, true)),
        (Fonts::none(), font(Family::Terminal, 400, false)),
    ];
    for (mut fonts, font) in faces {
        for line in lines {
            let mut places: Vec<usize> = (0..=line.len()).collect();
            places.push(line.len() + 3);
            let offsets = fonts.offsets(&font, line, &places);
            for (place, offset) in places.iter().zip(&offsets) {
                let expected = match line.get(..*place) {
                    Some("") => 0.0,
                    Some(start) => fonts.measure(&font, start).width,
                    None => fonts.measure(&font, line).width,
                };
                assert!(
                    offset.to_bits() == expected.to_bits(),
                    "{line:?} at {place}: {offset} where its start measures {expected}"
                );
            }
        }
    }
}

/// The places of a line are found from the line set down once, not from each start of it set down
/// apart: an editor asks for every place of the line typed in at each key, and the starts of a line
/// are as many as its characters.
#[test]
fn the_places_of_a_line_set_it_down_once() {
    let mut fonts = embedded();
    let font = font(Family::Terminal, 400, false);
    let line = "    let shaped = self.shape_line(buffer, line, measure);";
    let places: Vec<usize> = (1..=line.len()).collect();
    let before = fonts.set_downs();
    let _ = fonts.offsets(&font, line, &places);
    assert_eq!(
        fonts.set_downs() - before,
        1,
        "set down once for {} places",
        places.len()
    );
}
