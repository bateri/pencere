//! What the host tells a page, and what a page tells the host back.
//!
//! The engine reads no device: the host turns what the person did (a pointer move, a press, a
//! wheel, a key) into an [`Input`], in the page's points, and a page answers with a [`Reply`] —
//! whether anything it shows changed (so a frame is worth drawing), and the few things only the
//! host can do: pop up a menu, move the keyboard into a text field, copy, close the page.

use crate::{Id, Point};

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
    /// The wheel or the trackpad: `dx` points the content moves *left* by and `dy` points it
    /// moves *up* by (positive scrolls across and down the page).
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
}

/// The shape the pointer takes over the page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cursor {
    #[default]
    Arrow,
    /// Over something that answers a click.
    Pointer,
    /// Over the room of a text field the host has put over the page.
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
    /// The text field named by this id, which the host put over the page (the room of an
    /// element that said it is one): the page asks for the keyboard to type into it.
    Field(Id),
}

/// One line of a menu the host pops up for the page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuEntry {
    Item {
        title: String,
        /// The key that does the same on the page, shown at the end of the line.
        key: Option<char>,
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
    /// Texts for fields to hold from now on, in place of what they hold (a field emptied after
    /// what was typed was taken, one filled with what is to be edited): by the field's id. A
    /// field the page does not name keeps what the person typed.
    pub fill: Vec<(Id, String)>,
    /// Text for the pasteboard.
    pub copy: Option<String>,
    /// The pointer's shape after this input.
    pub cursor: Cursor,
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
}
