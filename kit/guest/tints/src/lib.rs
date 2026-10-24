//! A page of every way a colour can be said, to see that what a Rust page says crosses the
//! sandbox as the same colour a script says: the sixteen colours of the palette and the
//! selection as roles, a value with and without a form for a dark theme, each with and without an
//! opacity, in the places a colour goes (text, fill, border, shadow, a sparkline, the tint of an
//! image); and of the images themselves (`image`), said the way a script says them.
//!
//! The test sets the page beside the script that says the same (`TINTS` in the sandbox's tests):
//! the two trees have to be the same tree.

use pencere_page::{
    Color, Event, Fit, Frame, Layout, Page, Reply, Rgba, Size, Tone, div, export_page, image,
    spark, text,
};

struct Tints;

/// The colours of the palette by their names, in the order of the palette.
const PALETTE: [(&str, Tone); 16] = [
    ("black", Tone::Black),
    ("red", Tone::Red),
    ("green", Tone::Green),
    ("yellow", Tone::Yellow),
    ("blue", Tone::Blue),
    ("magenta", Tone::Magenta),
    ("cyan", Tone::Cyan),
    ("white", Tone::White),
    ("bright-black", Tone::BrightBlack),
    ("bright-red", Tone::BrightRed),
    ("bright-green", Tone::BrightGreen),
    ("bright-yellow", Tone::BrightYellow),
    ("bright-blue", Tone::BrightBlue),
    ("bright-magenta", Tone::BrightMagenta),
    ("bright-cyan", Tone::BrightCyan),
    ("bright-white", Tone::BrightWhite),
];

impl Page for Tints {
    fn new() -> Self {
        Self
    }

    fn render(&mut self, _size: Size, _now: f64, _reduce_motion: bool) -> Frame {
        let ink = Rgba::rgb(0x1a, 0x2b, 0x3c);
        let seen = Rgba::new(0x1a, 0x2b, 0x3c, 0x80);
        let both = Rgba::rgb(0x20, 0x30, 0x40).or_dark(Rgba::rgb(0xe0, 0xd0, 0xc0));
        let mut root = div().p(4.0).gap(2.0);
        for (name, tone) in PALETTE {
            root = root.child(text(name).color(tone));
        }
        root = root
            .child(text("selection").color(Tone::Selection).bg(Tone::Selection))
            .child(text("value").color(ink))
            .child(text("value with opacity").color(seen))
            .child(text("forms").color(both))
            .child(text("forms alone").color(Rgba::rgb(0xaa, 0xbb, 0xcc)))
            .child(text("role with opacity").color(Tone::BrightRed.alpha(0.5)))
            .child(text("value on top of opacity").color(Color {
                alpha: 0.25,
                ..seen.solid()
            }))
            .child(text("forms with opacity").color(Color {
                alpha: 0.75,
                ..both
            }))
            .child(
                div()
                    .w(10.0)
                    .h(10.0)
                    .bg(both)
                    .border(1.0, Tone::Cyan.alpha(0.5))
                    .shadow(0.0, 2.0, 4.0, seen),
            )
            .child(spark(&[1.0, 2.0, 3.0], Tone::BrightGreen))
            .child(spark(&[3.0, 2.0, 1.0], both))
            .child(image("icons/stop.svg", Fit::Contain))
            .child(
                image("logo.png", Fit::Cover)
                    .opacity(0.5)
                    .size(48.0, 24.0)
                    .rounded(4.0),
            )
            .child(
                image("icons/run.svg", Fit::Contain)
                    .tint(Tone::Error)
                    .w(16.0),
            )
            .child(image("icons/run.svg", Fit::Contain).tint(both))
            .child(image("icons/run.svg", Fit::Contain).tint(Tone::Accent.alpha(0.5)));
        Frame::still(root)
    }

    fn settle(&mut self, _layout: &Layout) -> bool {
        false
    }

    fn handle(&mut self, _event: Event, _layout: &Layout) -> Reply {
        Reply::none()
    }
}

export_page!(Tints);
