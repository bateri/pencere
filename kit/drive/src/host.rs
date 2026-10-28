//! What a page asks of its host, and the order a host does it in.
//!
//! The drive never does the host's part itself and never waits for it: what the page said since the
//! host last looked is handed over in one piece ([`crate::Page::drain`] → [`Drained`]), and
//! [`Drained::deliver`] hands it to the host's [`Host`] in the order every host does it in. The host
//! does what only it can — its pasteboard, its pointer, its fields, its menus, its browser, its
//! program — and calls the page again only after `deliver` has come back (a menu the person picks
//! from is told with [`crate::Page::pick`] then). What a reply asked of a field that the frame it
//! asks for brings is not told with it: a host has the field only once that frame is laid out, so
//! the drive holds it and gives it with the frame ([`crate::Painted::held`]).

use pencere_sandbox::{Answer, Fault};
use pencere_view::{Cursor, Fill, Id, Keyboard, Menu};

/// What a page had besides its replies, since the host last looked ([`crate::Page::drain`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Drained {
    /// What the page said to what it was told, in order, each with whether the event it answers was
    /// the pointer's (the pointer's shape after a key is nobody's to set). A menu's place is in the
    /// host's points.
    pub replies: Vec<Answer>,
    /// The page has something new to show: a frame of it is worth drawing.
    pub redraw: bool,
    /// Images the page named that could not be shown, each once, a sentence for the page's author
    /// (`image "x" is …`). The page goes on without them.
    pub diagnostics: Vec<String>,
    /// Why the page is gone, the first time it is told: the host takes it down.
    pub ended: Option<Fault>,
    /// What the page said to its program since the last look, in order: JSON objects, as it wrote
    /// them. The host carries them to the program, and only while the page's view is open. Told
    /// before `ended`: what a page said before it went is said.
    pub sent: Vec<String>,
    /// The time the page waits to be woken at changed, and no frame took it: the host arms its clock
    /// for the time the page names now ([`crate::Page::wake_at`]), with no frame drawn for it. A
    /// page that asks nothing never says so.
    pub wake: bool,
}

/// What a reply asked of a host's field before the frame that brings the field was laid out: the
/// drive holds it ([`crate::Page::drain`]) and gives it with the frame that lays the field out
/// ([`crate::Painted::held`]), for a host has a field of its own only once the page's outline says
/// the page names it.
#[derive(Clone, Debug, PartialEq)]
pub enum Held {
    /// The text a field is to hold and what is selected in it ([`Host::fill`]).
    Fill(Id, Fill),
    /// Where the keyboard goes ([`Host::keyboard`]).
    Keyboard(Keyboard),
}

impl Held {
    /// Hands what was asked to `host`, as [`Drained::deliver`] hands it.
    pub fn deliver(self, host: &mut dyn Host) {
        match self {
            Self::Fill(field, fill) => host.fill(field, fill),
            Self::Keyboard(to) => host.keyboard(to),
        }
    }
}

/// What a host does for a page: each is something the page asked in a reply, or something the drive
/// found, told by [`Drained::deliver`]. Every method does nothing by default, so that a host says
/// only what it does: a host with no pasteboard writes no `copy`.
pub trait Host {
    /// What the page said to its program: carried to it, in order, while the page's view is open.
    fn send(&mut self, _message: String) {}
    /// The page asks to be closed. Whose a view is to close is the host's to say: a program's view
    /// is the program's to end.
    fn close(&mut self) {}
    /// A frame of the page is worth drawing.
    fn redraw(&mut self) {}
    /// The pointer's shape the page asks for, answering the pointer: told only for a reply to the
    /// pointer, and a host takes it only while the pointer is over the page.
    fn cursor(&mut self, _cursor: Cursor) {}
    /// What the host's field named `field` holds from now on (in place of what the person typed),
    /// and what is selected in it ([`Fill`]: its ends held to the text). Told only of a field the
    /// host has put over the page already: what a reply asked of one the frame it asks for brings
    /// comes with that frame ([`crate::Painted::held`]), and a name the page still does not have
    /// then is passed over.
    fn fill(&mut self, _field: Id, _fill: Fill) {}
    /// Where the keyboard goes: a field of the host's, or the page (an editor's is the drive's, and
    /// done before this is told). A field the frame the reply asks for brings is told with that
    /// frame ([`crate::Painted::held`]), after its texts; a name the page still does not have then
    /// sends the keyboard to the page.
    fn keyboard(&mut self, _to: Keyboard) {}
    /// Text for the pasteboard.
    fn copy(&mut self, _text: String) {}
    /// A link to open in the person's browser, held to the rule a web view's `link` is held to (an
    /// `http` or `https` address, the person asked first).
    fn link(&mut self, _url: String) {}
    /// A menu to show, at its place in the host's points; the entry the person picks is told the
    /// page with [`crate::Page::pick`]. Told last of a reply: a menu may hold the host until it is
    /// picked from.
    fn menu(&mut self, _menu: Menu) {}
    /// A sentence for the page's author: an image that could not be shown.
    fn diagnostic(&mut self, _words: String) {}
    /// The page is gone, and why: the host takes it down. Nothing is told after it.
    fn ended(&mut self, _why: Fault) {}
    /// The time the page waits to be woken at changed and nothing is to be drawn now: the host arms
    /// its clock for [`crate::Page::wake_at`].
    fn rearm(&mut self) {}
}

impl Drained {
    /// Hands what the page said to `host`, in the order a host does it: what was said to the program
    /// first; then each reply in turn — its wish to close, its redraw, the pointer's shape (a reply
    /// to the pointer only), the fields' texts, the keyboard, the pasteboard, the link, and the menu
    /// last; then the images' diagnostics; then the end, after which nothing; and last the frame or
    /// the clock it wants. A reply's texts and keyboard that name a field the page has not laid out
    /// yet are not among them: the drive held them ([`crate::Page::drain`]) for the frame that lays
    /// it out ([`crate::Painted::held`], [`Held::deliver`]).
    pub fn deliver(self, host: &mut dyn Host) {
        for message in self.sent {
            host.send(message);
        }
        for Answer { reply, pointer } in self.replies {
            if reply.close {
                host.close();
            }
            if reply.redraw {
                host.redraw();
            }
            if pointer {
                host.cursor(reply.cursor);
            }
            // The texts first, so that a field the keyboard goes to holds what it is to edit.
            for (field, fill) in reply.fill {
                host.fill(field, fill);
            }
            if let Some(to) = reply.keyboard {
                host.keyboard(to);
            }
            if let Some(text) = reply.copy {
                host.copy(text);
            }
            if let Some(url) = reply.link {
                host.link(url);
            }
            if let Some(menu) = reply.menu {
                host.menu(menu);
            }
        }
        for words in self.diagnostics {
            host.diagnostic(words);
        }
        if let Some(why) = self.ended {
            host.ended(why);
            return;
        }
        if self.redraw {
            host.redraw();
        } else if self.wake {
            host.rearm();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pencere_view::{MenuEntry, Point, Reply};

    #[derive(Default)]
    struct Log(Vec<String>);

    impl Host for Log {
        fn send(&mut self, message: String) {
            self.0.push(format!("send {message}"));
        }
        fn close(&mut self) {
            self.0.push("close".into());
        }
        fn redraw(&mut self) {
            self.0.push("redraw".into());
        }
        fn cursor(&mut self, cursor: Cursor) {
            self.0.push(format!("cursor {cursor:?}"));
        }
        fn fill(&mut self, field: Id, fill: Fill) {
            let name = if field == Id::named("f") { "f" } else { "?" };
            let text = fill.text.unwrap_or_default();
            self.0.push(format!("fill {name} {text}"));
        }
        fn keyboard(&mut self, to: Keyboard) {
            self.0.push(format!("keyboard {to:?}"));
        }
        fn copy(&mut self, text: String) {
            self.0.push(format!("copy {text}"));
        }
        fn link(&mut self, url: String) {
            self.0.push(format!("link {url}"));
        }
        fn menu(&mut self, menu: Menu) {
            self.0.push(format!("menu {}", menu.entries.len()));
        }
        fn diagnostic(&mut self, words: String) {
            self.0.push(format!("diagnostic {words}"));
        }
        fn ended(&mut self, why: Fault) {
            self.0.push(format!("ended {why}"));
        }
        fn rearm(&mut self) {
            self.0.push("rearm".into());
        }
    }

    fn everything() -> Reply {
        Reply {
            redraw: true,
            close: true,
            menu: Some(Menu {
                at: Point::new(1.0, 2.0),
                entries: vec![MenuEntry::Item {
                    title: "Copy".into(),
                    enabled: true,
                    keys: None,
                }],
            }),
            keyboard: Some(Keyboard::Page),
            fill: vec![(Id::named("f"), Fill::text("text"))],
            copy: Some("copied".into()),
            cursor: Cursor::Text,
            link: Some("https://example.com".into()),
            ..Reply::none()
        }
    }

    /// The order is the rule every host keeps: the program first, each reply's parts with the menu
    /// last, the diagnostics, the end; the pointer's shape only for the pointer's answer.
    #[test]
    fn what_a_page_said_is_delivered_in_the_order_a_host_does_it() {
        let mut log = Log::default();
        Drained {
            replies: vec![
                Answer {
                    reply: everything(),
                    pointer: false,
                },
                Answer {
                    reply: Reply::none().with_cursor(Cursor::Pointer),
                    pointer: true,
                },
            ],
            redraw: true,
            diagnostics: vec!["image \"x\" is not one".into()],
            ended: None,
            sent: vec!["{\"call\":{}}".into()],
            wake: true,
        }
        .deliver(&mut log);
        assert_eq!(
            log.0,
            [
                "send {\"call\":{}}",
                "close",
                "redraw",
                "fill f text",
                "keyboard Page",
                "copy copied",
                "link https://example.com",
                "menu 1",
                "cursor Pointer",
                "diagnostic image \"x\" is not one",
                "redraw",
            ]
        );
    }

    /// A page that is gone asks for no frame and no clock after it says so.
    #[test]
    fn nothing_is_delivered_after_the_end_but_what_came_before_it() {
        let mut log = Log::default();
        Drained {
            redraw: true,
            wake: true,
            sent: vec!["{}".into()],
            ended: Some(Fault::Failed("no page for you".into())),
            ..Drained::default()
        }
        .deliver(&mut log);
        assert_eq!(log.0, ["send {}", "ended no page for you"]);
        let mut log = Log::default();
        Drained {
            wake: true,
            ..Drained::default()
        }
        .deliver(&mut log);
        assert_eq!(log.0, ["rearm"]);
    }
}
