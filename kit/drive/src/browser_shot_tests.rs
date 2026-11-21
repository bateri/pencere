//! The picture of a page with a browser: a box where the room is for a host that draws a picture
//! (the engine is the host's), and, for a host that shows no browser, the box with the words that
//! say so. Heavy: the interpreter is compiled and run (`make check-heavy`).

use std::path::PathBuf;

use crate::headless::{Error, Headless};
use crate::shot::{Fonts, Palette};

const PAGE: &str = r#"
return {
	render = function()
		return pencere.div():size_full():gap(8):p(8)
			:child(pencere.text("Web page"))
			:child(pencere.browser("web"):grow(1))
	end,
}
"#;

#[allow(clippy::expect_used)]
fn fonts() -> Fonts {
    match std::env::var("PENCERE_SHOTS").as_deref() {
        Ok("browser") => Fonts::none()
            .terminal(&std::fs::read("/System/Library/Fonts/Menlo.ttc").expect("Menlo"))
            .expect("a font"),
        _ => Fonts::none(),
    }
}

/// A host that shows browsers and draws a picture has a box where the room is, and one that shows
/// none has the box and its words; the two pictures are not the same. `PENCERE_SHOTS=browser` writes
/// them to `target/browser-{room,unsupported}.png`, in the terminal's font of a Mac, for an eye to
/// check.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_room_of_a_browser_is_a_box_in_a_picture_and_says_so_where_the_host_shows_none()
-> Result<(), Error> {
    let fonts = fonts();
    let shown = Headless::script(PAGE)
        .measure(fonts.clone())
        .size(400.0, 240.0)
        .open()?;
    let none = Headless::script(PAGE)
        .measure(fonts.clone())
        .size(400.0, 240.0)
        .browsers(false)
        .open()?;
    let room = shown.shot_in(&fonts, Palette::Dark).expect("a picture");
    let unsupported = none.shot_in(&fonts, Palette::Dark).expect("a picture");
    assert_eq!(&room[1..4], b"PNG");
    assert_eq!(&unsupported[1..4], b"PNG");
    assert_ne!(room, unsupported, "the words are in the room of one only");
    if std::env::var("PENCERE_SHOTS").as_deref() == Ok("browser") {
        let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target");
        std::fs::write(target.join("browser-room.png"), &room).expect("written");
        std::fs::write(target.join("browser-unsupported.png"), &unsupported).expect("written");
    }
    Ok(())
}
