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
use pencere_view::{BrowserAsk, Carry, Cursor, Fill, Id, Keyboard, Menu, Panel, Point, Rect};

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
    /// Text the page had an editor copy or cut (`copy`, `cut` of an editor command), in order: the
    /// host puts it on its pasteboard as it does a reply's `copy`, after the replies'.
    pub copied: Vec<Copied>,
    /// The time the page waits to be woken at changed, and no frame took it: the host arms its clock
    /// for the time the page names now ([`crate::Page::wake_at`]), with no frame drawn for it. A
    /// page that asks nothing never says so.
    pub wake: bool,
    /// The pointer's shape changed where it stands (a modifier was held that makes a link of what is
    /// under it) and nothing the pointer did says so: the host takes it while the pointer is over the
    /// page, as it takes the shape of a reply to the pointer.
    pub cursor: Option<Cursor>,
    /// The person began to drag an element out of the view ([`crate::Page::set_carry`]): the host
    /// starts its system's drag with what the element carries, and ends it with
    /// [`crate::Page::carried`]. The page hears none of the pointer's moves nor its release until
    /// then.
    pub carry: Option<CarryOut>,
}

/// A drag out of the view the person began with what an element carries ([`Host::carry`]).
#[derive(Clone, Debug, PartialEq)]
pub struct CarryOut {
    /// The element that carries: [`crate::Page::carried`] says when its drag is over.
    pub id: Id,
    /// What it carries: words or a path, and an address. It has been held to the rule of
    /// `pencere_wire::carry` (one line, at most 64 KiB, no control character; an address only of the
    /// kind a `link` may be), twice: where the sandbox took the page and where the pointer began
    /// the drag.
    pub carry: Carry,
    /// Where the pointer is, in the host's points.
    pub at: Point,
    /// The element's room, in the host's points: what the person holds is drawn from it.
    pub rect: Rect,
}

/// Text for the host's pasteboard that a page had an editor copy or cut, and whose act it was.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Copied {
    /// The text.
    pub text: String,
    /// Whether it was asked in a reply to something the person did ([`Host::copy`]).
    pub by_person: bool,
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
    /// Text a page had an editor copy or cut within this frame, for the host's pasteboard
    /// ([`Host::copy`]).
    Copy(Copied),
    /// What the page asked of a browser the host has not made yet ([`Host::browser`]).
    Browser(Id, BrowserAsk),
}

impl Held {
    /// Hands what was asked to `host`, as [`Drained::deliver`] hands it.
    pub fn deliver(self, host: &mut dyn Host) {
        match self {
            Self::Fill(field, fill) => host.fill(field, fill),
            Self::Keyboard(to) => host.keyboard(to),
            Self::Copy(Copied { text, by_person }) => host.copy(text, by_person),
            Self::Browser(browser, ask) => host.browser(browser, ask),
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
    /// What the page asks of the browser named `browser`, one of those the host made for the page
    /// (a name it has not made is passed over, as a field's is). Told only to a host that said it
    /// shows browsers ([`crate::Page::set_browsers`]), and only an ask the browser may be given: an
    /// address of a kind it does not go to is refused before it gets here, and the page told.
    fn browser(&mut self, _browser: Id, _ask: BrowserAsk) {}
    /// How many points larger than the host's own text the page asks to be drawn (below 0: smaller):
    /// the size of this view, which the host puts into the size it draws it at, within what it can
    /// draw ([`crate::Placement::zoom_for`]), and leaves its own text as it was. The page says it
    /// when it wants it and is not told what came of it. It is not the person's act: any reply says
    /// it, a wake's and a program's message's included.
    fn text_size(&mut self, _points: i32) {}
    /// Text for the pasteboard, and whether the person's own word asked for it (`by_person`): a
    /// reply to a press or its release, a pick in a menu or a command, and of the keys only the
    /// system's copy and cut ([`pencere_view::copy_key`]); the copy or cut an editor makes of what
    /// the page asked in such a reply included. It is `false` for the reply to any other key (the
    /// page chose what it does), to a message of the program, to a wake or to a change of the room,
    /// and for the editor's account of what it did: the page and its program can make the host
    /// write the pasteboard there, and a host that keeps the program from it (the terminal's
    /// `osc52 = "off"`) writes `text` only where `by_person` is `true`. The editor's own copy key
    /// is not told here: it is a [`Route::Copy`] of [`crate::Page::key`], the host's to put on its
    /// pasteboard, and a key the page chose for it (`keymap = { f6 = "copy" }`) is the person's
    /// word only where [`pencere_view::copy_key`] says so.
    fn copy(&mut self, _text: String, _by_person: bool) {}
    /// The text on the host's pasteboard, for the entry of a menu the host performs itself
    /// ([`pencere_view::MenuEntry::Paste`]) that the person picked ([`crate::Page::pick`]); `None`
    /// where there is none. It is pasted in the editor and told to no one: a page never reads the
    /// pasteboard.
    fn paste(&mut self) -> Option<String> {
        None
    }
    /// A link to open in the person's browser, held to the rule a web view's `link` is held to (an
    /// `http` or `https` address, the person asked first).
    fn link(&mut self, _url: String) {}
    /// A menu to show, at its place in the host's points; the entry the person picks is told the
    /// page with [`crate::Page::pick`], with the menu's `serial` ([`Menu::serial`]) that the host
    /// keeps from showing it. Told last of a reply: a menu may hold the host until it is picked
    /// from. A host shows one menu at a time and does not show one asked for while another is up
    /// (the page is not told): what the person picks is then of the menu they saw, whichever the
    /// page asked for last, and the host must hand back the serial of the one it showed.
    fn menu(&mut self, _menu: Menu) {}
    /// A panel for the person to give the page files or a place to save in: asked for in a reply to
    /// something the person did (the sandbox passes over one asked for in another). What the person
    /// chooses is told the page with [`crate::Page::give`] or [`crate::Page::saving`]; a host
    /// shows one at a time, and drops one asked for while another is up (the session of a terminal
    /// that hosts pencere holds it to that, `Session::module_pick`).
    fn panel(&mut self, _panel: Panel) {}
    /// The person began to drag an element out of the view: the host starts its system's drag with
    /// what the element carries and shows the person the words it will insert, never a label the
    /// page chose (a `path` is words to it: it never reads one). It ends the drag with
    /// [`crate::Page::carried`]. Told only to a host that said it carries ([`crate::Page::set_carry`]).
    fn carry(&mut self, _carry: CarryOut) {}
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
    /// to the pointer only), the fields' texts, the keyboard, the size of its text, the pasteboard, the link, the panel,
    /// and the menu last; then the drag out of the view the person began; then the images' diagnostics; then the end, after which nothing; and last the frame or
    /// the clock it wants. A reply's texts and keyboard that name a field the page has not laid out
    /// yet are not among them: the drive held them ([`crate::Page::drain`]) for the frame that lays
    /// it out ([`crate::Painted::held`], [`Held::deliver`]).
    pub fn deliver(self, host: &mut dyn Host) {
        for message in self.sent {
            host.send(message);
        }
        for Answer {
            reply,
            pointer,
            person,
        } in self.replies
        {
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
            for (browser, ask) in reply.browser {
                host.browser(browser, ask);
            }
            if let Some(points) = reply.text_size {
                host.text_size(points);
            }
            if let Some(text) = reply.copy {
                host.copy(text, person);
            }
            if let Some(url) = reply.link {
                host.link(url);
            }
            if let Some(panel) = reply.panel {
                host.panel(panel);
            }
            if let Some(menu) = reply.menu {
                host.menu(menu);
            }
        }
        for Copied { text, by_person } in self.copied {
            host.copy(text, by_person);
        }
        if let Some(carry) = self.carry {
            host.carry(carry);
        }
        for words in self.diagnostics {
            host.diagnostic(words);
        }
        if let Some(cursor) = self.cursor {
            host.cursor(cursor);
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
    use pencere_view::{MenuEntry, Point, Rect, Reply};

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
        fn text_size(&mut self, points: i32) {
            self.0.push(format!("text_size {points}"));
        }
        fn copy(&mut self, text: String, by_person: bool) {
            self.0.push(format!("copy {text} {by_person}"));
        }
        fn link(&mut self, url: String) {
            self.0.push(format!("link {url}"));
        }
        fn menu(&mut self, menu: Menu) {
            self.0.push(format!("menu {}", menu.entries.len()));
        }
        fn panel(&mut self, panel: Panel) {
            self.0.push(format!("panel {panel:?}"));
        }
        fn carry(&mut self, carry: CarryOut) {
            self.0.push(format!("carry {}", carry.carry.words()));
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
                serial: 1,
            }),
            keyboard: Some(Keyboard::Page),
            fill: vec![(Id::named("f"), Fill::text("text"))],
            copy: Some("copied".into()),
            text_size: Some(-2),
            cursor: Cursor::Text,
            link: Some("https://example.com".into()),
            panel: Some(Panel::Pick { multiple: true }),
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
                    person: true,
                },
                Answer {
                    reply: Reply::none().with_cursor(Cursor::Pointer),
                    pointer: true,
                    person: true,
                },
            ],
            redraw: true,
            diagnostics: vec!["image \"x\" is not one".into()],
            ended: None,
            sent: vec!["{\"call\":{}}".into()],
            wake: true,
            cursor: None,
            copied: vec![Copied {
                text: "cut".into(),
                by_person: false,
            }],
            carry: Some(CarryOut {
                id: Id::named("tab"),
                carry: Carry::text("hello"),
                at: Point::new(3.0, 4.0),
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
            }),
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
                "text_size -2",
                "copy copied true",
                "link https://example.com",
                "panel Pick { multiple: true }",
                "menu 1",
                "cursor Pointer",
                "copy cut false",
                "carry hello",
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
