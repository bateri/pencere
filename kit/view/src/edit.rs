//! What a page and the host's editor say to each other about a text being edited, as plain data:
//! the shape of what goes over the wire, apart from the engine that does the editing.
//!
//! An editor ([`crate::Element::editor`]) is the host's, as a text field is: the document, its
//! selections, its history, what an input method is composing, and how its lines are laid out and
//! drawn are the host's editor's (the `pencere-editor` crate), the same in every host. A page puts
//! an editor's room on the page and its options ([`EditorOptions`]), is told what changed
//! ([`Change`], by the version it made), and says what it wants changed ([`Change`]s against the
//! version it saw), selected, coloured, folded and marked ([`Decorate`]). It reads the text itself
//! through what the host posts it ([`TextSnapshot`]).
//!
//! A place in a document is counted in **characters**: Unicode code points, from 0 at the start
//! (what Lua's `utf8` library counts, and LSP's `utf-32` encoding). A range is from one place to
//! another, the end not in it. A version is a number that grows by one with every change made to
//! the document, whoever made it.
//!
//! These types are here, in the engine and not in the editor's crate, so that what carries them
//! (the sandbox, the SDK for pages written in Rust) reads them without linking the editor; so are
//! the limits both sides hold a page to ([`limits`]).

use crate::Color;

/// One change of a document: the characters `from..to` replaced by `insert` (`from == to` inserts,
/// an empty `insert` deletes). The places are those of the version the change is made against.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Change {
    pub from: usize,
    pub to: usize,
    pub insert: String,
}

impl Change {
    /// `insert` put in place of `from..to`.
    pub fn new(from: usize, to: usize, insert: impl Into<String>) -> Self {
        Self {
            from,
            to,
            insert: insert.into(),
        }
    }
}

/// One selected range: from where it was started (`anchor`) to where it is now (`head`, where the
/// caret is drawn). An empty range is a caret alone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Range {
    pub anchor: usize,
    pub head: usize,
}

impl Range {
    pub const fn new(anchor: usize, head: usize) -> Self {
        Self { anchor, head }
    }

    /// A caret at `at`, nothing selected.
    pub const fn caret(at: usize) -> Self {
        Self {
            anchor: at,
            head: at,
        }
    }

    /// The start of the range, whichever way it was made.
    pub fn start(self) -> usize {
        self.anchor.min(self.head)
    }

    /// The end of it.
    pub fn end(self) -> usize {
        self.anchor.max(self.head)
    }

    pub fn is_empty(self) -> bool {
        self.anchor == self.head
    }
}

/// Who made a change of the document.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Origin {
    /// The person: typing, deleting, pasting, an editing command of the editor's own.
    #[default]
    Person,
    /// An input method, composing: text that is not yet what the person means
    /// ([`Selection::composing`]). What it comes to is the person's.
    Compose,
    /// The page, by a change it asked for.
    Page,
    /// Undoing a change.
    Undo,
    /// Doing again a change undone.
    Redo,
    /// The page loading the document (a piece of it).
    Load,
}

impl Origin {
    /// The word the wire says for it.
    pub fn as_str(self) -> &'static str {
        match self {
            Origin::Person => "person",
            Origin::Compose => "compose",
            Origin::Page => "page",
            Origin::Undo => "undo",
            Origin::Redo => "redo",
            Origin::Load => "load",
        }
    }

    /// The origin a word names.
    pub fn named(word: &str) -> Option<Self> {
        Some(match word {
            "person" => Origin::Person,
            "compose" => Origin::Compose,
            "page" => Origin::Page,
            "undo" => Origin::Undo,
            "redo" => Origin::Redo,
            "load" => Origin::Load,
            _ => return None,
        })
    }
}

/// The document changed: the changes that took version `version - 1` to `version`, in the places
/// of `version - 1`, in order and apart (none overlaps the next), and who made them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Edited {
    pub version: u64,
    pub changes: Vec<Change>,
    pub origin: Origin,
}

/// Where the carets and the selections are: the ranges, in order and apart, and which of them is
/// the main one (where an input method composes, what a page's popup stands by).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub ranges: Vec<Range>,
    /// The place of the main range in `ranges`.
    pub main: usize,
    /// The version of the document the places are in.
    pub version: u64,
    /// An input method is composing: the text at the main caret is not yet what the person means,
    /// and the host does none of the page's commands until it is.
    pub composing: bool,
}

impl Default for Selection {
    fn default() -> Self {
        Self {
            ranges: vec![Range::caret(0)],
            main: 0,
            version: 0,
            composing: false,
        }
    }
}

/// What a page has drawn under, over or beside a range of the text.
#[derive(Clone, Debug, PartialEq)]
pub enum Mark {
    /// The look of the characters of the range: a colour, a ground behind them, a weight, the
    /// face's italic, a line under them or through them. What it does not say is the editor's.
    Style(Look),
    /// A ground for the whole of each line the range touches (the line of an error, of a change).
    Line(Color),
    /// A mark in the gutter by the first line of the range (a breakpoint, a change, a warning).
    Gutter(Color),
    /// The range can be folded away: its first line stays, the lines after it down to the one the
    /// range ends on are hidden while it is folded. Whether it is, is the editor's (the person
    /// folds and unfolds); where it can be, is the page's.
    Fold,
}

/// The look a [`Mark::Style`] gives the characters of its range.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Look {
    pub color: Option<Color>,
    pub background: Option<Color>,
    /// On the CSS scale (400 regular, 700 bold).
    pub weight: Option<u16>,
    pub italic: Option<bool>,
    pub underline: Option<Underline>,
    pub strike: bool,
}

/// A line under some characters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Underline {
    /// Straight, as a link's.
    Straight(Color),
    /// Wavy, as an error's.
    Wavy(Color),
}

/// One decoration: a mark over the characters `from..to` (an empty range marks a place: a gutter
/// mark, a line's ground).
#[derive(Clone, Debug, PartialEq)]
pub struct Decoration {
    pub from: usize,
    pub to: usize,
    pub mark: Mark,
}

impl Decoration {
    pub fn new(from: usize, to: usize, mark: Mark) -> Self {
        Self { from, to, mark }
    }
}

/// A page's decorations of one layer for one range of the document, in place of those the layer
/// had there: the layer's decorations that overlap `from..to` (or, being empty, stand in it) go,
/// these come, and the rest of the layer and the other layers stay. The colours of a syntax are
/// sent for the lines in view and those that changed, the errors in a layer of their own, and
/// neither takes the other away. The places are those of `version`: decorations of a version
/// older than the document's are carried over the changes made since.
#[derive(Clone, Debug, PartialEq)]
pub struct Decorate {
    /// The layer's name (`"syntax"`, `"diagnostics"`, `"folds"`): the page's to choose.
    pub layer: String,
    pub version: u64,
    pub from: usize,
    pub to: usize,
    /// In order of where they start; each inside `from..to`.
    pub decorations: Vec<Decoration>,
}

/// What the keys do in an editor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Keys {
    /// The editor's: a page's commands first, then the editor's own, then what is typed.
    #[default]
    Editor,
    /// The page's, every one of them: the editor types nothing and does none of its own commands
    /// for a key; the page drives it (with [`EditCommand`]s and changes) as an editor of modes
    /// does. An input method and the pasteboard are still the editor's.
    Page,
}

/// How an editor is set up ([`crate::Element::editor`]): said every frame, like the rest of the page.
#[derive(Clone, Debug, PartialEq)]
pub struct EditorOptions {
    /// Long lines go on in the next line where the room ends, instead of the room scrolling across.
    pub wrap: bool,
    /// How many columns a tab takes, and how many spaces a level of indent is.
    pub tab_width: u8,
    /// A level of indent is a tab, not `tab_width` spaces.
    pub hard_tabs: bool,
    /// The gutter: the numbers of the lines, the marks, the buttons that fold.
    pub gutter: bool,
    /// The person cannot change the text (they can still select it and copy it); the page can.
    pub read_only: bool,
    pub keys: Keys,
    /// What the editor says while its document is empty.
    pub hint: Option<String>,
    /// What starts a comment that runs to the end of the line, for the command that comments
    /// lines (`"--"`, `"//"`); none, and the command does nothing.
    pub comment: Option<String>,
    /// The brackets and quotes that close themselves when they are typed, opener and closer.
    pub pairs: Vec<(char, char)>,
}

impl Default for EditorOptions {
    fn default() -> Self {
        Self {
            wrap: false,
            tab_width: 4,
            hard_tabs: false,
            gutter: true,
            read_only: false,
            keys: Keys::Editor,
            hint: None,
            comment: None,
            pairs: Vec::new(),
        }
    }
}

/// Where a caret goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Movement {
    /// One character as a person reads it (a grapheme: a letter with its accents, an emoji).
    Left,
    Right,
    /// To the start of the word before, or the end of the word after.
    WordLeft,
    WordRight,
    /// A line up or down, keeping the place across (as near as the line lets).
    Up,
    Down,
    /// A room's height up or down.
    PageUp,
    PageDown,
    /// To where the line's text starts (its indent past), or where the line starts when it is
    /// there already.
    LineStart,
    LineEnd,
    DocumentStart,
    DocumentEnd,
    /// To the bracket that matches the one at the caret.
    Bracket,
}

/// What a page can ask an editor to do as if the person had (an editor of modes moves and deletes
/// by the page's keys).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditCommand {
    /// Moves every caret; `extend` keeps the anchor where it is, and selects.
    Move {
        movement: Movement,
        extend: bool,
    },
    /// Types `text` at every caret, in place of what is selected.
    Insert(String),
    /// A line break, keeping the line's indent.
    Newline,
    /// Deletes what is selected, or the character before the caret (`DeleteBackward`), after it
    /// (`DeleteForward`), or the word before or after it.
    DeleteBackward,
    DeleteForward,
    DeleteWordBackward,
    DeleteWordForward,
    /// A level of indent more, or less, for the lines of each range.
    Indent,
    Outdent,
    /// The lines of each range one line up, or one line down.
    MoveLinesUp,
    MoveLinesDown,
    /// The lines of each range once more, below.
    DuplicateLines,
    /// The lines of each range commented, or uncommented when every one of them is.
    ToggleComment,
    SelectAll,
    /// A caret more on the line above, or below, at the same place across.
    AddCaretAbove,
    AddCaretBelow,
    /// The next place the main range's text is found, selected as well.
    SelectNext,
    /// Back to the main range alone.
    SingleSelection,
    Undo,
    Redo,
    /// The fold region the main caret's line starts, folded, unfolded or the other way round.
    Fold,
    Unfold,
    ToggleFold,
    UnfoldAll,
    /// Looks for `pattern` in the document and marks what it finds (a regular expression where
    /// `regex`, the words as they are where not; where `case` is false, in either case).
    Find {
        pattern: String,
        regex: bool,
        case: bool,
    },
    /// Selects the next, or the one before, of what was found, from the main caret.
    FindNext,
    FindPrevious,
    /// Forgets what was found.
    StopFinding,
}

/// What a page asks of an editor of its, in a reply.
#[derive(Clone, Debug, PartialEq)]
pub enum EditorAsk {
    /// A piece of the document: the first takes the place of the whole text, each after it (where
    /// the one before said `more`) goes on its end. Nothing of a load is undone, and what could be
    /// undone before cannot be any more; while one goes on, the person cannot change the text.
    Load { text: String, more: bool },
    /// Changes against the version `base`: carried over what was done since, done, and undone as
    /// one.
    Change { base: u64, changes: Vec<Change> },
    /// The selections, in the places of `version` ([`Selection`]).
    Select {
        version: u64,
        ranges: Vec<Range>,
        main: usize,
    },
    /// The place `at` (of `version`) scrolled into view.
    Reveal { version: u64, at: usize },
    /// What the person could have done with a key.
    Command(EditCommand),
    /// A layer's decorations for a range.
    Decorate(Decorate),
}

/// Why an editor did not do what a page asked: what the page sent was not something it could
/// do, and nothing of it was done ([`crate::Event::EditorRefused`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Refusal {
    /// A range that ends before it starts.
    Reversed,
    /// A place past the end of the document.
    OutOfDocument,
    /// Changes that overlap, or are not in order.
    Overlapping,
    /// A version older than the oldest the editor still knows the changes since.
    StaleVersion,
    /// A version the document has not come to.
    FutureVersion,
    /// The document would be larger than an editor's may be, or than the page's editors' may be
    /// together ([`limits`]).
    TooLarge,
    /// More decorations, or layers of them, than an editor holds.
    TooManyDecorations,
    /// A decoration outside the range it was sent for.
    OutsideRange,
    /// A pattern that is not a regular expression.
    BadPattern,
    /// No editor of that name on the page.
    NoEditor,
}

impl Refusal {
    /// The word the wire says for it.
    pub fn as_str(self) -> &'static str {
        match self {
            Refusal::Reversed => "reversed",
            Refusal::OutOfDocument => "out-of-document",
            Refusal::Overlapping => "overlapping",
            Refusal::StaleVersion => "stale-version",
            Refusal::FutureVersion => "future-version",
            Refusal::TooLarge => "too-large",
            Refusal::TooManyDecorations => "too-many-decorations",
            Refusal::OutsideRange => "outside-range",
            Refusal::BadPattern => "bad-pattern",
            Refusal::NoEditor => "no-editor",
        }
    }

    /// The refusal a word names.
    pub fn named(word: &str) -> Option<Self> {
        Some(match word {
            "reversed" => Refusal::Reversed,
            "out-of-document" => Refusal::OutOfDocument,
            "overlapping" => Refusal::Overlapping,
            "stale-version" => Refusal::StaleVersion,
            "future-version" => Refusal::FutureVersion,
            "too-large" => Refusal::TooLarge,
            "too-many-decorations" => Refusal::TooManyDecorations,
            "outside-range" => Refusal::OutsideRange,
            "bad-pattern" => Refusal::BadPattern,
            "no-editor" => Refusal::NoEditor,
            _ => return None,
        })
    }
}

/// A document's text as it was at one version, which the host posts to the page so that the page
/// reads it at once, without asking across to the host and waiting: the page reads the text of a
/// change in the event that tells it of the change. Taking one costs nothing that grows with the
/// document (the editor's text is a tree whose parts are shared).
pub trait TextSnapshot: Send + Sync + std::fmt::Debug {
    /// The version the text is of.
    fn version(&self) -> u64;
    /// How many characters it has.
    fn len_chars(&self) -> usize;
    /// How many lines: one more than it has line breaks.
    fn len_lines(&self) -> usize;
    /// The characters `from..to`; `None` for a range that is not one of the text.
    fn slice(&self, from: usize, to: usize) -> Option<String>;
    /// Where line `line` (from 0) starts; `None` past the last.
    fn line_start(&self, line: usize) -> Option<usize>;
    /// The line the character at `at` is on (`at` may be the end); `None` past the end.
    fn line_of(&self, at: usize) -> Option<usize>;
}

/// The limits an editor holds a page to: what every host gives at least (the specification's table
/// of floors). Both the sandbox, which checks the shape of what a page sends, and the editor, which
/// checks what it means for the document, read them here.
pub mod limits {
    /// The largest an editor's document may be, bytes of UTF-8.
    pub const DOCUMENT_BYTES: usize = 16 * 1024 * 1024;
    /// The largest the documents of a page's editors may be together, bytes.
    pub const PAGE_DOCUMENTS_BYTES: usize = 64 * 1024 * 1024;
    /// The most text one reply may hand an editor (all its changes and loads together), and the
    /// most one query of the text answers with, bytes: a large document is loaded in pieces.
    pub const REPLY_TEXT_BYTES: usize = 1024 * 1024;
    /// The most decorations one reply may send.
    pub const REPLY_DECORATIONS: usize = 20_000;
    /// The most layers of decorations an editor holds.
    pub const LAYERS: usize = 16;
    /// The most decorations an editor holds, in all its layers.
    pub const DECORATIONS: usize = 200_000;
    /// How many versions back a change or a decoration may be made against: the editor keeps the
    /// changes of the last this many, to carry what comes late over them.
    pub const VERSIONS: u64 = 1024;
}
