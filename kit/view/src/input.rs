//! What the host tells a page, and what a page tells the host back.
//!
//! The engine reads no device: the host turns what the person did (a pointer move, a press, a
//! wheel, a key) into an [`Input`], in the page's points, and a page answers with a [`Reply`] —
//! whether anything it shows changed (so a frame is worth drawing), and the few things only the
//! host can do: pop up a menu, move the keyboard into a text field, copy, close the page.

use crate::edit::{Edited, EditorAsk, Found, Refusal, Selection};
use crate::{Fill, Id, Point, Shortcut};

/// A key the page reads. Letters, symbols and the space bar come as [`Key::Character`] — the
/// character the key makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
    Enter,
    Escape,
    Tab,
    /// Deletes the character before the caret (⌫).
    Backspace,
    /// Deletes the character after it (⌦, fn-⌫ on a Mac's keyboard).
    Delete,
    /// A function key, F1 to F20.
    Function(u8),
    Character(char),
}

/// The modifiers held with a key or a press.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub shift: bool,
    pub control: bool,
    /// The alt key: ⌥, the option key on a Mac.
    pub alt: bool,
    pub command: bool,
}

impl Mods {
    /// Whether control, alt or command is held: the key is a shortcut and not a letter being typed.
    pub fn any_command(self) -> bool {
        self.control || self.alt || self.command
    }
}

/// A pointer button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    /// The wheel pressed down, or the third button.
    Middle,
}

/// One thing the person did, in the page's points (the page's top-left corner is the origin).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Input {
    /// The pointer moved over the page.
    Move(Point),
    /// The pointer left the page.
    Leave,
    Press {
        at: Point,
        button: Button,
        mods: Mods,
        /// Which press of a quick run of them this is, from 1: `2` is the second of a double
        /// click, as the system counts them.
        count: u32,
    },
    Release {
        at: Point,
        button: Button,
    },
    /// The wheel or the trackpad, where no box the host scrolls took it ([`crate::Scrolls`]):
    /// `dx` points the content would move *left* by and `dy` points it would move *up* by
    /// (positive scrolls across and down the page). A page that does something of its own with
    /// the wheel (a zoom, a value that steps) reads it here.
    Scroll {
        at: Point,
        dx: f32,
        dy: f32,
    },
    Key {
        key: Key,
        mods: Mods,
    },
    /// A key the page was told of went up.
    KeyUp {
        key: Key,
        mods: Mods,
    },
    /// The modifiers held changed, with no other key: what a page that shows something else while
    /// a modifier is held (alt for the other action of a button) follows.
    Mods(Mods),
}

/// A place in the document of an editor ([`crate::Element::editor`]): the editor's name and the
/// place, in characters ([`crate::edit`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Place {
    pub editor: Id,
    pub at: usize,
}

/// Everything a page is told: what the person did to the page, the text a text field of the
/// host's now holds, the entry picked of a menu the page asked for. A page that lives apart from
/// the host is told by this one type.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Input(Input),
    /// The host's field named `field` holds this now: the person typed, pasted or deleted (not
    /// said for a text the page put in itself, [`Reply::fill`]).
    Text {
        field: Id,
        text: String,
    },
    /// The keyboard went to the field the person clicked into, or back to the page (`None`)
    /// when the field let it go of itself; not said when it went where the page sent it.
    Focus(Option<Id>),
    /// The person picked the entry with this index of the menu the page asked for.
    MenuPick(usize),
    /// The person asked for the command named this, from the host's list of the page's commands
    /// or by its keys ([`crate::Command`]): the key is not told as well.
    Command(Id),
    /// Files were dropped on the page at `at`, from outside it: their paths, as the system that
    /// the host runs on writes them.
    Files {
        at: Point,
        paths: Vec<String>,
    },
    /// The host scrolled these boxes (the wheel, the trackpad, a scroll bar, or what the page
    /// asked): each one's name and how far it is scrolled now, across and down, points. A page
    /// that builds only what is in view builds what has come into it.
    Scrolled(Vec<(Id, Point)>),
    /// The pointer over an editor: what it did (a move, a press, a release), and the place in the
    /// editor's document under it — what a page that shows what a word is when the pointer rests
    /// on it, or goes to where a name is defined at a ⌘-click, reads. The press is the editor's
    /// as well: it puts the caret.
    Pointer {
        input: Input,
        place: Place,
    },
    /// The document of the editor named `editor` changed ([`Edited`]): the copy of its text the
    /// page reads (`Queries` of the sandbox) is of this version when the page is told.
    Edit {
        editor: Id,
        edited: Edited,
    },
    /// Its carets and selections are elsewhere, or an input method began or stopped composing.
    Selection {
        editor: Id,
        selection: Selection,
    },
    /// Its lines in view are these now, the first and the last.
    Viewport {
        editor: Id,
        first: usize,
        last: usize,
    },
    /// What the page asked of it was not done, for this reason, and nothing of it was.
    EditorRefused {
        editor: Id,
        refusal: Refusal,
    },
    /// What its search finds now ([`Found`]): said after every search the page asks for, and
    /// whenever the count, or the match the main selection is on, changes.
    Found {
        editor: Id,
        found: Found,
    },
    /// The time the page asked to be woken at ([`Reply::wake`]) has come: `now` is the time of the
    /// frame it is woken at, seconds on the clock the page is rendered by. Told only to a page that
    /// asked, and at most once a frame; the page answers as it answers any event.
    Wake {
        now: f64,
    },
}

/// The shape the pointer takes over the page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cursor {
    #[default]
    Arrow,
    /// Over something that answers a click.
    Pointer,
    /// Over the room of a text field or an editor the host has put over the page.
    Text,
    /// Over an edge that is dragged left and right (a column's, a divider's).
    ResizeHorizontal,
    /// Over an edge that is dragged up and down.
    ResizeVertical,
    /// Over something that cannot be used now.
    NotAllowed,
    /// Over something that can be picked up and moved.
    Grab,
    /// While it is being moved.
    Grabbing,
    /// Where a point is picked.
    Crosshair,
}

/// Where the keyboard goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keyboard {
    /// The page's own keys.
    Page,
    /// The text field or the editor named by this id, which the host put over the page (the room
    /// of an element that said it is one): the page asks for the keyboard to type into it.
    Field(Id),
}

/// One line of a menu the host pops up for the page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuEntry {
    Item {
        title: String,
        /// The keys that do the same on the page, shown at the end of the line, written as a
        /// command's are (`ctrl+w`, `f2`).
        keys: Option<Shortcut>,
        enabled: bool,
    },
    Separator,
}

/// A menu to pop up at `at` (the page's points); the host answers with the index of the entry
/// picked ([`MenuEntry`]s in order, separators counted).
#[derive(Clone, Debug, PartialEq)]
pub struct Menu {
    pub at: Point,
    pub entries: Vec<MenuEntry>,
}

/// What a page says back after an [`Input`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reply {
    /// Something the page shows changed: a frame is worth drawing. An input that changed
    /// nothing (a pointer moving inside the same row) answers `false`, and a page at rest
    /// costs no frames.
    pub redraw: bool,
    /// The page asks to be closed.
    pub close: bool,
    /// A menu to pop up.
    pub menu: Option<Menu>,
    /// The keyboard goes somewhere else than where it was.
    pub keyboard: Option<Keyboard>,
    /// What fields are to hold from now on, in place of what they hold (a field emptied after what
    /// was typed was taken, one filled with what is to be edited), and what is selected in them
    /// ([`Fill`]): by the field's id. A field the page does not name keeps what the person typed.
    pub fill: Vec<(Id, Fill)>,
    /// Text for the pasteboard.
    pub copy: Option<String>,
    /// The pointer's shape after this input.
    pub cursor: Cursor,
    /// What the page asks of its editors, in order: a piece of a document to load, changes,
    /// selections, colours and marks ([`EditorAsk`]), each with the editor's name.
    pub editor: Vec<(Id, EditorAsk)>,
    /// A link the page asks the host to open in the person's browser, as a web page's `link`
    /// message asks (specification §9.7, §9.8): an `http` or `https` address, every byte of it
    /// printable ASCII without the space, at most 8192 bytes. The host should ask the person first,
    /// drops any other link, and never tells the page what came of it. It is the host's, not a
    /// message to the program.
    pub link: Option<String>,
    /// The page asks to be woken this many seconds from now (0: at the next frame): once the time
    /// has come it is told [`Event::Wake`], and answers as it answers any event — the work a page
    /// does over time (a long document read a piece at a time, a save after the person stops
    /// typing) goes on with no event of the person's. Of the times a page asks, the earliest
    /// stands, so that what one part of it asks is not put off by another; a wake that came is
    /// gone, and the page asks again for another. A number of seconds: 0 or more, finite.
    pub wake: Option<f64>,
}

impl Reply {
    /// Nothing changed.
    pub fn none() -> Self {
        Self::default()
    }

    /// Something changed: draw.
    pub fn redraw() -> Self {
        Self {
            redraw: true,
            ..Self::default()
        }
    }

    /// The same reply with the pointer's shape `cursor`.
    #[must_use]
    pub fn with_cursor(mut self, cursor: Cursor) -> Self {
        self.cursor = cursor;
        self
    }

    /// The same reply, asking the host to open `url` in the person's browser ([`Reply::link`]).
    #[must_use]
    pub fn with_link(mut self, url: impl Into<String>) -> Self {
        self.link = Some(url.into());
        self
    }

    /// The same reply, asking to be woken `seconds` from now ([`Reply::wake`]); where it asks
    /// already, the earlier of the two.
    #[must_use]
    pub fn with_wake(mut self, seconds: f64) -> Self {
        self.wake = Some(self.wake.map_or(seconds, |asked| asked.min(seconds)));
        self
    }
}
