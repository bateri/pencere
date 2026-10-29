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

use crate::{Button, Color, Cursor, Id, Mods, Rect};

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
    /// The page's own name for a change it asked for ([`EditorAsk::Change`]'s `tag`): what it made
    /// the change for, told back with it (a reload from disk, a format), as CodeMirror's
    /// `userEvent` is. None for any other change.
    pub tag: Option<String>,
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

/// What a search ([`EditCommand::Find`]) finds now: what a find widget shows ("3 of 12"). The
/// editor says it after every `Find`, searches again as the document changes, and says it again
/// whenever the count, or which match the main selection is on, changes; when the search stops it
/// says a count of 0. The ranges themselves stay the editor's: a page that wants them reads the
/// text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Found {
    /// The version of the document searched.
    pub version: u64,
    /// How many matches it found, at most [`limits::FIND_MATCHES`]; 0 when nothing is searched for.
    pub count: usize,
    /// Which of them the main selection is exactly on, counted from 1 as a find widget shows it;
    /// none when it is on none of them.
    pub current: Option<usize>,
    /// The search stopped before the end of the document (at [`limits::FIND_MATCHES`] matches, or
    /// where a search for whole words had read as much as it may): there may be more than `count`.
    pub more: bool,
}

/// What a page has drawn under, over or beside a range of the text.
#[derive(Clone, Debug, PartialEq)]
pub enum Mark {
    /// The look of the characters of the range: a colour, a ground behind them, a weight, the
    /// face's italic, a line under them or through them. What it does not say is the editor's.
    Style(Look),
    /// A ground for the whole of each line the range touches (the line of an error, of a change).
    Line(Color),
    /// A mark in the gutter (a breakpoint, a change, a warning): by the first line of the range,
    /// along all of its lines, or on the line between two ([`GutterMark`]).
    Gutter(GutterMark),
    /// A mark on the editor's overview ruler, down the right edge of its room (VS Code's
    /// `overviewRuler`): where the range is in the document, as the document is as tall as the
    /// room, so that what is marked is found in a long file. Drawn under the scroll bar's knob.
    Ruler(RulerMark),
    /// A gap for the page to draw in, after the line the range starts on ([`Zone`]).
    Zone(Zone),
    /// The range has a name the page can stand something by: [`Id::anchor`] is its box
    /// ([`pencere_view::EditorMetrics`] says where, for an overlay anchored to it), as an editor's
    /// caret is. A name is an editor's alone: a later decoration of the name is the one that stands.
    Anchor(String),
    /// Text that is not in the document, drawn at the place of the range (an inlay hint, a line's
    /// end: an error, a blame): see [`Widget`].
    Widget(Widget),
    /// The range can be folded away: its first line stays, the lines after it down to the one the
    /// range ends on are hidden while it is folded. Whether it is, is the editor's (the person
    /// folds and unfolds); where it can be, is the page's.
    Fold,
}

/// Text the page puts in a line that is not the document's: where it stands ([`WidgetSide`]) and what
/// it says. It takes room in the line (an inline one) or stands after the line's end (the last); the
/// caret, the keys, copying, searching and the history do not know it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Widget {
    pub parts: Vec<WidgetPart>,
    pub side: WidgetSide,
    /// The group whose options ([`EditorOptions::widgets`]) say how it looks and when it shows.
    pub group: Option<String>,
    /// What the page calls it: told with the pointer's rest over it and a press on it.
    pub id: Option<String>,
    /// What it says to a screen reader.
    pub label: Option<String>,
    pub cursor: Option<Cursor>,
    /// A name for its box ([`Mark::Anchor`]): a card stands by it.
    pub anchor: Option<String>,
    pub color: Option<Color>,
    pub background: Option<Color>,
    /// Points kept inside it before and after its words (the group's `pad` where none).
    pub pad_left: Option<f32>,
    pub pad_right: Option<f32>,
    /// Points kept outside it before and after, between it and the characters beside it (the
    /// group's `margin` where none). An inline widget only: one at a line's end has the group's
    /// `gap`.
    pub margin_left: Option<f32>,
    pub margin_right: Option<f32>,
}

/// One run of a [`Widget`]'s words.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WidgetPart {
    pub text: String,
    pub color: Option<Color>,
    pub background: Option<Color>,
    pub weight: Option<u16>,
    pub italic: Option<bool>,
    /// What the page calls it, where it is a part to press (a type to go to).
    pub id: Option<String>,
}

/// Where a [`Widget`] stands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WidgetSide {
    /// At the range's start, before the character there (`name: |arg`).
    Before,
    /// At the range's end, after the character before it (`x|: int`).
    #[default]
    After,
    /// After the end of the line of the range's start, a gap beyond the text.
    End,
}

/// How a group of widgets look, and when they show ([`EditorOptions::widgets`]).
#[derive(Clone, Debug, PartialEq)]
pub struct WidgetGroup {
    pub show: WidgetShow,
    /// The modifiers `show` goes by.
    pub hold: Mods,
    /// How large its words are, times the editor's text.
    pub size: f32,
    pub color: Option<Color>,
    pub background: Option<Color>,
    /// Points: the corners' radius, the room kept inside it, the gap before a widget at a line's end,
    /// the room kept before and after an inline one, between it and the characters beside it.
    pub radius: f32,
    pub pad: f32,
    pub gap: f32,
    pub margin: f32,
}

impl Default for WidgetGroup {
    fn default() -> Self {
        Self {
            show: WidgetShow::On,
            hold: Mods {
                control: true,
                alt: true,
                ..Mods::default()
            },
            size: 0.9,
            color: None,
            background: None,
            radius: 3.0,
            pad: 3.0,
            gap: 16.0,
            margin: 3.0,
        }
    }
}

/// When a group of widgets shows (VS Code's `inlayHints.enabled`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WidgetShow {
    #[default]
    On,
    Off,
    /// Shown, but not while the group's modifiers are held.
    UnlessHeld,
    /// Shown only while they are held.
    WhenHeld,
}

/// A marker in a column of the gutter (a breakpoint, a change, a warning, a bookmark, a result of a
/// test run): where it is is the decoration's range, in which column and how it looks are here.
#[derive(Clone, Debug, PartialEq)]
pub struct GutterMark {
    /// The column it stands in, by the name the page gave it in [`EditorOptions::gutters`]; none
    /// (empty), the column of dots, `"marks"`. A marker whose column the editor has not is not drawn.
    pub column: String,
    pub shape: GutterShape,
    /// Its colour; of an image, what it is tinted (an image whose colours are its own is not).
    pub color: Color,
    /// What the page calls it: told with an event of the person's pointer over it or on it
    /// ([`GutterEvent`]), so that a page tells its markers apart.
    pub id: Option<String>,
    /// What it is in words: what a screen reader says of it, and what the host says while the
    /// pointer rests on it (a tooltip, the keys that do the same in brackets).
    pub label: Option<String>,
    /// The pointer's shape over it (the pointer's hand where the column takes presses, else the
    /// arrow).
    pub cursor: Option<Cursor>,
    /// Its colour while the pointer is over it, and while it is pressed: laid over it as
    /// [`EditorColors::mark_hover`] says where the page says none.
    pub hover: Option<Color>,
    pub pressed: Option<Color>,
}

impl GutterMark {
    /// A dot by the first line, as a gutter mark was drawn before it had a shape.
    pub fn dot(color: Color) -> Self {
        Self::new(GutterShape::Dot, color)
    }

    /// A marker of `shape` in the column of dots, with nothing else said of it.
    pub fn new(shape: GutterShape, color: Color) -> Self {
        Self {
            column: String::new(),
            shape,
            color,
            id: None,
            label: None,
            cursor: None,
            hover: None,
            pressed: None,
        }
    }
}

/// What a gutter marker looks like.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum GutterShape {
    /// A dot, centred on the first line of the range.
    #[default]
    Dot,
    /// A thin bar along every line the range touches (a range that ends where a line starts stops
    /// at the line before), the rows a long line wraps into included: [`MarkLooks::bar`] wide, and
    /// [`MarkLooks::hover`] wide while the pointer is over it.
    Bar,
    /// A small triangle on the edge above the range's first line: where lines were taken away.
    Above,
    /// The same on the edge below the range's first line.
    Below,
    /// A picture of the view's bundle (`"assets/breakpoint.svg"`), as tall as the row, as wide as
    /// its proportions say, in the middle of the column, on the first line of the range.
    Image(String),
}

/// A mark on the editor's overview ruler.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RulerMark {
    pub color: Color,
    pub lane: RulerLane,
}

/// Which part of the width of the overview ruler a mark takes: all of it, or a third (VS Code's
/// lanes), so that marks of different things stand side by side.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RulerLane {
    #[default]
    Full,
    Left,
    Center,
    Right,
}

/// A gap the editor keeps open after a line, for the page to draw in (VS Code's zone widget,
/// CodeMirror's block widget): `height` points, taken up in whole rows. It is where the text goes
/// on after the line, so what the page puts in it scrolls with the text; an element of the page that
/// is a child of the editor's element and says it is the zone's (`Element::zone`) is put there. The
/// decoration's range says the line: the one its first character is on. It is carried over the
/// changes as any decoration is.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Zone {
    pub id: Id,
    pub height: f32,
    /// Whether the gap is after the line or before it (a code lens: the lines above a function).
    pub side: ZoneSide,
}

/// Where a [`Zone`]'s gap is, by its line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ZoneSide {
    /// After the line's rows.
    #[default]
    Below,
    /// Before them: between the line before it that shows and the line. A line folded away has none
    /// to show; the first line of a fold keeps its own.
    Above,
}

/// One column of the gutter ([`EditorOptions::gutters`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    /// What the page calls it: `"numbers"` (the numbers of the lines), `"folds"` (the buttons that
    /// fold) and `"marks"` (the column of dots) are the editor's; any other is the page's, a place
    /// for markers.
    pub name: String,
    /// How wide it is, points ([`limits::COLUMN`]): none is what the editor makes of its own (the
    /// numbers fit the largest number, the dots and the buttons that fold are as wide as the row's
    /// text is tall times 0.6 and 0.8), and of a column of the page's the same as the dots'.
    pub width: Option<f32>,
    /// What of the pointer the page hears of this column ([`GutterEvent`]); none: of a column of the
    /// page's its presses (the left button's and the other's: [`ColumnEvents::default`]), of the
    /// editor's nothing.
    pub events: Option<ColumnEvents>,
    /// What shows, faint, in the cell the pointer is over where nothing stands in the column yet: a
    /// breakpoint to be put (VS Code's faded red dot).
    pub hover: Option<Preview>,
    /// How much room the numbers keep to their left, points ([`limits::COLUMN`]; of `"numbers"`
    /// alone, in the width the editor makes of its own: a column of a width the page says is as wide
    /// as that): none is the editor's padding where the column is the first, so that the numbers do
    /// not stand against the room's edge, and none where another column is before it.
    pub pad: Option<f32>,
}

/// Which things the pointer does in a column the page hears of: it hears the rare ones only, the
/// frequent (how a marker looks while the pointer is over it, the preview in an empty cell) are the
/// editor's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColumnEvents {
    /// A press of the main button. The line is not selected, nothing is folded, the caret does not
    /// move; the editor has the keyboard.
    pub press: bool,
    /// A press of another button, or of the main one with ⌃ held (a menu's).
    pub menu: bool,
    /// The pointer came to a cell, and went from it.
    pub hover: bool,
}

impl Default for ColumnEvents {
    /// The presses of a column of the page's.
    fn default() -> Self {
        Self {
            press: true,
            menu: true,
            hover: false,
        }
    }
}

impl Column {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            width: None,
            events: None,
            hover: None,
            pad: None,
        }
    }

    /// Whether the column is the editor's (`"numbers"`, `"folds"`, `"marks"`).
    pub fn is_editors(&self) -> bool {
        matches!(self.name.as_str(), "numbers" | "folds" | "marks")
    }

    /// What of the pointer the page hears of the column.
    pub fn heard(&self) -> ColumnEvents {
        self.events.unwrap_or_else(|| {
            if self.is_editors() {
                ColumnEvents {
                    press: false,
                    menu: false,
                    hover: false,
                }
            } else {
                ColumnEvents::default()
            }
        })
    }
}

/// A marker shown where nothing stands, for the pointer is over the cell ([`Column::hover`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Preview {
    pub shape: GutterShape,
    pub color: Color,
}

/// What the pointer did in a gutter column that takes events ([`Column::events`]): a marker's
/// column, or a column of the page's, of one editor.
#[derive(Clone, Debug, PartialEq)]
pub struct GutterEvent {
    pub editor: Id,
    pub column: String,
    /// The line under the pointer, from 0 (one a bar goes along: the one the pointer is on).
    pub line: usize,
    pub kind: GutterKind,
    /// For a press: the button, how many in a quick run, the modifiers held. Left, 1 and none for
    /// the pointer's coming and going.
    pub button: Button,
    pub count: u32,
    pub mods: Mods,
    /// The marker under the pointer, none where the cell is empty (a page puts one there).
    pub marker: Option<GutterHit>,
    /// The cell, points of the page: the column's width by the line's row: where a page stands a
    /// tooltip or a menu.
    pub rect: Rect,
}

/// A marker under the pointer.
#[derive(Clone, Debug, PartialEq)]
pub struct GutterHit {
    pub id: Option<String>,
    /// The layer of decorations it is of, and its range.
    pub layer: String,
    pub from: usize,
    pub to: usize,
}

/// What the pointer did.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GutterKind {
    Press,
    /// It came to a cell that is not the one it was on (another line, another marker, another
    /// column), or went from one.
    Enter,
    Leave,
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
    /// What the page calls it: told with the pointer's resting over it ([`HoverEvent::hits`]), so
    /// that a page finds the diagnostic or the link under the pointer without searching its own
    /// lists.
    pub id: Option<String>,
    /// Drawn (and its `cursor` given) only while exactly these modifiers are held (`mod` is the
    /// host's main modifier). A link that shows while ⌘ is held ([`Look::over`], [`Look::cursor`]).
    pub held: Option<Mods>,
    /// Drawn only while the pointer is over the characters of its range.
    pub over: bool,
    /// The pointer's shape over the range where it is drawn: the hand (`Cursor::Pointer`) for a look
    /// that is `held`, none otherwise.
    pub cursor: Option<Cursor>,
}

impl Look {
    /// Whether the look is drawn as the modifiers `mods` are held and the pointer is (or is not)
    /// over its range.
    pub fn shows(&self, mods: Mods, over: bool) -> bool {
        self.held.is_none_or(|held| {
            held.control == mods.control
                && held.alt == mods.alt
                && held.command == mods.command
                && held.shift == mods.shift
        }) && (!self.over || over)
    }
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

/// How a language writes its comments, for the commands that comment and uncomment
/// ([`EditCommand::ToggleComment`], [`EditCommand::ToggleBlockComment`]): a language says either
/// or both. None, and those commands do nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct CommentTokens {
    /// What starts a comment that runs to the end of the line (`"--"`, `"//"`, `"#"`).
    pub line: Option<String>,
    /// What opens a comment and what closes it, for a comment that runs between them
    /// (`("/*", "*/")`, `("<!--", "-->")`).
    pub block: Option<(String, String)>,
}

impl CommentTokens {
    /// A language whose comments run from `token` to the end of the line.
    pub fn line(token: impl Into<String>) -> Self {
        Self {
            line: Some(token.into()),
            block: None,
        }
    }

    /// A language whose comments run from `open` to `close`.
    pub fn block(open: impl Into<String>, close: impl Into<String>) -> Self {
        Self {
            line: None,
            block: Some((open.into(), close.into())),
        }
    }
}

/// How a language indents its lines, said as regular expressions (the syntax of Rust's `regex`:
/// no look-around, no back-references; compiled in the case written, held to
/// [`limits::PATTERN_SIZE`]) that the editor matches against a line's text, its break left out
/// (a line longer than 10 000 characters is not read). A language says either or both; with
/// neither, a line break keeps the line's indent and what is typed moves no line.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct IndentRules {
    /// A line that opens a block (`then$`, `:\s*$`): a line break after it gives the new line a
    /// level of indent more than it has, as an opener of the editor's pairs does. What is matched
    /// is the line's text before the caret.
    pub increase: Option<String>,
    /// A line that closes one (`^\s*end\b`, `^\s*\}`): when what is typed makes a line match it, the
    /// line goes a level out from the indent the line above it gives, where it is not already
    /// further out. The line above is the nearest one with text; it gives the lines below it its
    /// own indent, and a level more where it matches `increase` or ends with an opener.
    pub decrease: Option<String>,
}

/// Where the regions an editor folds come from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Folds {
    /// The page's alone: its fold decorations ([`Mark::Fold`]).
    #[default]
    Page,
    /// The page's, and the editor's own from the indent: a line with text folds the lines after it
    /// that are further in, and the lines with no text among them; those with no text after the
    /// last of them are not in it, and a run of more than 1 000 of them ends it. A tab is as far
    /// in as to its next stop. Where the page's fold and the indent's start on one line, the
    /// page's is the one.
    Indent,
}

/// Which spaces and tabs an editor shows (VS Code's `renderWhitespace`): a space is a dot in the
/// middle of its place, a tab an arrow as wide as it is, and any other space (a no-break space, the
/// spaces of typesetting, an ideographic space) a box as wide as it is. A line break, a `\r` before
/// it and the other controls show nothing, and count as spaces for where a line's text ends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Whitespace {
    /// None of them: a plain editor.
    #[default]
    None,
    /// Every one but a space alone between two words: those before a line's text and after it,
    /// those in a run of two or more, and every tab and other space.
    Boundary,
    /// Those in what is selected.
    Selection,
    /// Those after the last word of a line (every one of a line with no words).
    Trailing,
    /// Every one.
    All,
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
    /// How the language writes its comments, for the commands that comment lines and ranges.
    pub comment: CommentTokens,
    /// The brackets and quotes that close themselves when they are typed, opener and closer.
    pub pairs: Vec<(char, char)>,
    /// How the language indents its lines: what a line break and what is typed do to the indent.
    pub indent: IndentRules,
    /// Where the regions that fold come from: the page's alone, or the indent's as well.
    pub folds: Folds,
    /// How tall a row is, times the font's size: its text is in the middle of it, with a point
    /// at least above and below (1.6, the golden ratio editors of code read comfortably at;
    /// [`limits::LINE_HEIGHT`]).
    pub line_height: f32,
    /// The room kept around the text, points ([`limits::PADDING`]).
    pub padding: f32,
    /// How many rows are kept between the caret and the top or the bottom of the room when the
    /// editor scrolls to keep the caret in view (at most half the rows in view).
    pub scroll_margin: u8,
    /// What the numbers of the lines in the gutter count.
    pub numbers: Numbers,
    /// Where the ground of the main caret's line is drawn (its colour is `colors.current_line`).
    pub current_line: LineHighlight,
    /// Which spaces and tabs are shown.
    pub whitespace: Whitespace,
    /// A thin line down each level of indent (VS Code's indent guides): a line shows one for each
    /// level its indent reaches into (a level is `tab_width` columns, a tab as far as its next
    /// stop), a line with no text those of the lines with text around it, as VS Code shows them;
    /// a line that wraps shows them on its first row. The guide of the block the main caret is in
    /// is drawn in its own colour while the editor has the keyboard ([`EditorColors`]).
    pub indent_guides: bool,
    /// The other places the main selection's text is, marked where they are in view (CodeMirror's
    /// `highlightSelectionMatches`, VS Code's selection highlight): only drawn, the selection as it
    /// is ([`Matches`]).
    pub matches: Matches,
    /// The gutter's columns, from the left: none, and they are `"marks"`, `"numbers"` and `"folds"`.
    /// A page lists the columns it has, the editor's by name and its own by the name it gives them;
    /// one it leaves out is not there (no `"folds"`, no buttons that fold).
    pub gutters: Vec<Column>,
    /// How the gutter's bars and the overview ruler's marks are drawn.
    pub marks: MarkLooks,
    pub caret: Caret,
    /// How a selection looks.
    pub selection: SelectionLook,
    pub colors: EditorColors,
    /// What of the pointer and the keys the page hears of beyond the rest ([`EditorEvents`]).
    pub events: EditorEvents,
    /// How the pointer's resting over the text is told ([`HoverOptions`]).
    pub hover: HoverOptions,
    /// How a press is taken, by the button, the modifiers and how many in a quick run: what the page
    /// says here is taken as it says, what it does not is as the editor takes it ([`Gesture`]).
    pub gestures: Vec<(Gesture, Pressing)>,
    /// How the editor's keys are read where the page says another, by the keys ([`Keybinding`]).
    pub keymap: Vec<Keybinding>,
    /// How the pointer's moving over a word with the modifiers of a press that is the page's is
    /// told ([`LinkOptions`]).
    pub links: LinkOptions,
    /// How long a chord of keys waits for its next key, seconds ([`limits::CHORD_TIMEOUT`]); VS Code's
    /// waits one.
    pub chord_timeout: f32,
    /// How the groups of widgets look and when they show, by name; a widget of a group not here, or
    /// of none, is as [`WidgetGroup::default`] says.
    pub widgets: Vec<(String, WidgetGroup)>,
}

/// A press as a page names it: a button, the modifiers, and how many presses of a quick run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gesture {
    pub button: Button,
    pub mods: Mods,
    pub count: u32,
}

/// What a press does in an editor's text ([`EditorOptions::gestures`]); the drag that follows it is
/// of the same kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pressing {
    /// Puts the caret there; dragged, selects.
    Place,
    /// Selects from the main selection's start to there.
    Extend,
    /// Adds a caret there.
    AddCaret,
    /// Starts a column.
    Column,
    /// Takes the word (dragged, grows by words).
    Word,
    /// Takes the line (dragged, grows by lines).
    Line,
    /// Is the page's: told (`target`, [`TargetEvent`]); the caret does not move and the editor has
    /// the keyboard.
    Target,
    /// Does nothing.
    Nothing,
}

/// The editor's keys the page changes ([`EditorOptions::keymap`]): one key or a chord of several, and
/// what they do, or `None`: nothing of the editor's (the page's to read as a key).
#[derive(Clone, Debug, PartialEq)]
pub struct Keybinding {
    pub keys: Vec<crate::Shortcut>,
    pub action: Option<KeyAction>,
}

/// What the editor's keys do.
#[derive(Clone, Debug, PartialEq)]
pub enum KeyAction {
    Command(EditCommand),
    Tab,
    Copy,
    Cut,
    Paste,
    /// Back to one selection where there are several.
    Escape,
    /// Deletes to where the line starts, and to where it ends.
    ToLineStart,
    ToLineEnd,
}

/// How the pointer over a word with a press that is the page's held is told.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinkOptions {
    /// The most often a word is told, seconds ([`limits::LINK_DELAY`]).
    pub delay: f32,
}

impl Default for LinkOptions {
    fn default() -> Self {
        Self { delay: 0.05 }
    }
}

/// The pointer over a word that a press of the page's is held for, or that press ([`EditorEvents::target`]).
#[derive(Clone, Debug, PartialEq)]
pub struct TargetEvent {
    pub editor: Id,
    pub kind: TargetKind,
    /// What is under the pointer: a word, or a range the page drew as a link (`held` and `id`).
    pub source: TargetSource,
    pub at: usize,
    pub from: usize,
    pub to: usize,
    /// The link's `id`, where it is one.
    pub id: Option<String>,
    /// The word's or the link's box, points of the page.
    pub rect: Rect,
    pub mods: Mods,
    /// For a press: the button and how many in a quick run.
    pub button: Button,
    pub count: u32,
    pub version: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TargetKind {
    /// The modifiers of a press of the page's are held and the pointer came to a word.
    Enter,
    /// They were let go, the pointer left the word, or the text changed under it.
    Leave,
    Press,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TargetSource {
    Word,
    Link,
    /// A widget with a name (or a part of one), pressed whatever the modifiers.
    Widget,
}

/// Which events of its own an editor tells the page: none unless the page asks, as the rare things
/// are, and those that come at the rate of the pointer's moving are never told.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EditorEvents {
    /// The pointer rested over a word or left it ([`HoverEvent`]).
    pub hover: bool,
    /// The pointer over a word with the modifiers of a press that is the page's, and that press
    /// ([`TargetEvent`]).
    pub target: bool,
}

/// How long the pointer rests before it is told of ([`EditorOptions::hover`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HoverOptions {
    /// Seconds the pointer rests over a word before the page is told ([`limits::HOVER_DELAY`]);
    /// VS Code's `editor.hover.delay` (300 ms) and Zed's `hover_popover_delay`.
    pub delay: f32,
}

impl Default for HoverOptions {
    fn default() -> Self {
        Self { delay: 0.3 }
    }
}

/// The pointer rested over a word of an editor's text, or left it ([`EditorEvents::hover`]).
#[derive(Clone, Debug, PartialEq)]
pub struct HoverEvent {
    pub editor: Id,
    pub kind: HoverKind,
    /// The place under the pointer, and the word (or run of marks) it is on: its range.
    pub at: usize,
    pub from: usize,
    pub to: usize,
    /// The line of `at`, from 0.
    pub line: usize,
    /// The decorations with a name (`id`) that hold `at`: the layer, the name and the range of each,
    /// the later over the earlier.
    pub hits: Vec<HoverHit>,
    /// Whether it is a word of the text the pointer is on or a widget (`id` is then the widget's, or
    /// of its part).
    pub over: HoverOver,
    pub id: Option<String>,
    /// The word's box, points of the page: where a card is stood.
    pub rect: Rect,
    /// The modifiers held as the drive last knew.
    pub mods: Mods,
    /// The document's version the places are of.
    pub version: u64,
}

/// What the pointer rests on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HoverOver {
    Text,
    Widget,
}

/// What the pointer did of resting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HoverKind {
    /// It stayed the delay over a word.
    Dwell,
    /// It went from a word it was told of: at once, so that a page that keeps its card where the
    /// pointer goes next (the card's own box) decides how long.
    Leave,
}

/// A decoration with a name under the pointer.
#[derive(Clone, Debug, PartialEq)]
pub struct HoverHit {
    pub layer: String,
    pub id: String,
    pub from: usize,
    pub to: usize,
}

/// The measures of the gutter's bars and of the overview ruler's marks, points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarkLooks {
    /// How wide a bar is (VS Code's change bars: 3), and so how large a triangle is: three times
    /// as tall ([`limits::MARK_WIDTH`]).
    pub bar: f32,
    /// How wide a bar the pointer is over is ([`limits::MARK_WIDTH`]).
    pub hover: f32,
    /// How wide the overview ruler is, and so how wide a mark that takes all of it ([`limits::MARK_WIDTH`]).
    pub ruler: f32,
}

impl Default for MarkLooks {
    fn default() -> Self {
        Self {
            bar: 3.0,
            hover: 6.0,
            ruler: 4.0,
        }
    }
}

/// Which other places of the main selection's text an editor marks ([`EditorOptions::matches`]).
/// What is marked is the text of a main selection on one line, of at most
/// [`limits::MATCH_LENGTH`] characters and not all spaces, in the case written, at most
/// [`limits::MATCHES_IN_VIEW`] of them in view, the selection itself not among them; a selection
/// that is a whole word (no letter, digit or `_` just before it or just after it) marks only the
/// places where it is a whole word, and any other marks every place it is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Matches {
    /// None: a plain editor.
    #[default]
    None,
    /// The other places of what is selected.
    Selection,
    /// Those, and, where the main selection is a caret in a word, the other places that word is,
    /// as whole words.
    Word,
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
            comment: CommentTokens::default(),
            pairs: Vec::new(),
            indent: IndentRules::default(),
            folds: Folds::Page,
            line_height: 1.6,
            padding: 4.0,
            scroll_margin: 0,
            numbers: Numbers::Absolute,
            current_line: LineHighlight::Text,
            whitespace: Whitespace::None,
            indent_guides: false,
            matches: Matches::None,
            gutters: Vec::new(),
            marks: MarkLooks::default(),
            caret: Caret::default(),
            selection: SelectionLook::default(),
            colors: EditorColors::default(),
            events: EditorEvents::default(),
            hover: HoverOptions::default(),
            gestures: Vec::new(),
            keymap: Vec::new(),
            links: LinkOptions::default(),
            chord_timeout: 1.0,
            widgets: Vec::new(),
        }
    }
}

impl EditorOptions {
    /// The options with every number held to what an editor may take ([`limits`]): one past an
    /// end is taken to it, one that is not a number is the default.
    #[must_use]
    pub fn held(mut self) -> Self {
        let default = Self::default();
        let hold = |value: f32, range: std::ops::RangeInclusive<f32>, default: f32| {
            if value.is_nan() {
                default
            } else {
                value.clamp(*range.start(), *range.end())
            }
        };
        self.tab_width = self.tab_width.clamp(1, limits::TAB_WIDTH);
        self.line_height = hold(self.line_height, limits::LINE_HEIGHT, default.line_height);
        self.padding = hold(self.padding, limits::PADDING, default.padding);
        self.caret.width = hold(self.caret.width, limits::CARET_WIDTH, default.caret.width);
        self.caret.radius = hold(self.caret.radius, limits::ROUNDING, default.caret.radius);
        self.caret.glow = hold(self.caret.glow, limits::CARET_GLOW, default.caret.glow);
        self.caret.blink_interval = hold(
            self.caret.blink_interval,
            limits::BLINK_INTERVAL,
            default.caret.blink_interval,
        );
        self.caret.motion = self.caret.motion.held();
        self.caret.blink_for = hold(
            self.caret.blink_for,
            limits::BLINK_FOR,
            default.caret.blink_for,
        );
        self.selection.radius = hold(
            self.selection.radius,
            limits::ROUNDING,
            default.selection.radius,
        );
        self.marks.bar = hold(self.marks.bar, limits::MARK_WIDTH, default.marks.bar);
        self.marks.hover = hold(self.marks.hover, limits::MARK_WIDTH, default.marks.hover);
        self.marks.ruler = hold(self.marks.ruler, limits::MARK_WIDTH, default.marks.ruler);
        self.hover.delay = hold(self.hover.delay, limits::HOVER_DELAY, default.hover.delay);
        self.links.delay = hold(self.links.delay, limits::LINK_DELAY, default.links.delay);
        self.chord_timeout = hold(
            self.chord_timeout,
            limits::CHORD_TIMEOUT,
            default.chord_timeout,
        );
        self
    }
}

/// A colour an editor draws something in: the theme's (what the editor draws it in when the page
/// says nothing), none (it is not drawn), or the page's own.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Tint {
    #[default]
    Theme,
    Off,
    Color(Color),
}

impl Tint {
    /// The colour to draw in: `theme` where the page said nothing, none where it said not to draw.
    pub fn or(self, theme: Color) -> Option<Color> {
        match self {
            Self::Theme => Some(theme),
            Self::Off => None,
            Self::Color(color) => Some(color),
        }
    }
}

/// How a caret is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CaretShape {
    /// A bar before the character.
    #[default]
    Bar,
    /// A block over the character (a space wide at the end of a line), the character drawn again
    /// on it in the caret's `text` colour: the caret of an editor of modes, while it is not typing.
    Block,
    /// A line under the character.
    Underline,
}

/// How tall a caret is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CaretHeight {
    /// As tall as its row, as the current line's ground is (as Zed and VS Code draw it).
    #[default]
    Row,
    /// As tall as the text: the room the row keeps above and below it left out.
    Text,
}

/// What a caret is while the window or the pane that holds its editor does not have the keyboard
/// ([`Caret::unfocused`]; an editor the keyboard is not in, in a page that has it, draws none).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CaretUnfocused {
    /// Not drawn.
    #[default]
    Hidden,
    /// A block is an outline, with the character under it as it is; a bar and a line are drawn as
    /// they are when it has the keyboard (a terminal's cursor of a window that is not in front).
    Hollow,
    /// Drawn as it is when it has the keyboard, and still: it does not blink.
    Solid,
}

/// How a caret goes from a place to another ([`Caret::motion`]): the host moves it on its own clock,
/// a terminal's cursor does the same (`cursor_motion`). With Reduce Motion on it is `Snap`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum CaretMotion {
    /// At once.
    #[default]
    Snap,
    /// Along a curve that is fast at first and slow at the end, over a time that does not depend on
    /// how far it goes: seconds ([`limits::CARET_EASE`]); a terminal's takes 0.18.
    Ease(f32),
    /// As a spring that settles with no overshoot, so that a key held down keeps its speed from one
    /// step to the next: how stiff it is, radians a second ([`limits::CARET_SPRING`]); a terminal's
    /// is 30, which settles in about a quarter of a second.
    Spring(f32),
}

impl CaretMotion {
    /// What a terminal's `ease` takes, seconds.
    pub const EASE: f32 = 0.18;
    /// What a terminal's `spring` is, radians a second.
    pub const SPRING: f32 = 30.0;

    /// The motion with its number held to what a caret may take ([`limits`]): one that is not a
    /// number is a terminal's.
    #[must_use]
    pub fn held(self) -> Self {
        let hold = |value: f32, range: std::ops::RangeInclusive<f32>, default: f32| {
            if value.is_nan() {
                default
            } else {
                value.clamp(*range.start(), *range.end())
            }
        };
        match self {
            Self::Snap => Self::Snap,
            Self::Ease(seconds) => Self::Ease(hold(seconds, limits::CARET_EASE, Self::EASE)),
            Self::Spring(stiffness) => {
                Self::Spring(hold(stiffness, limits::CARET_SPRING, Self::SPRING))
            }
        }
    }
}

/// How the carets look. The host blinks them, on the options it is given here.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Caret {
    pub shape: CaretShape,
    pub height: CaretHeight,
    /// How wide a bar is (2, as Zed draws it), and how thick the line of `Underline` is, points
    /// ([`limits::CARET_WIDTH`]); a block is as wide as its character.
    pub width: f32,
    /// The caret's colour (the theme's: its cursor colour); off, and no caret is drawn.
    pub color: Tint,
    /// The colour of the character under a block (the theme's: its background).
    pub text: Tint,
    /// How round its corners are, as a fraction of a row's height ([`limits::ROUNDING`]); held to
    /// half of the caret's width and of its height. 0: square.
    pub radius: f32,
    /// The soft light round it, one number ([`limits::CARET_GLOW`]): how far it reaches and how
    /// dark it is together, in the caret's colour; 1 is a terminal's cursor's. 0: none.
    pub glow: f32,
    /// Whether it blinks (a caret that does not is lit, and the host asks for no frame for it).
    pub blink: bool,
    /// How long it is lit, and then dark, seconds ([`limits::BLINK_INTERVAL`]): VS Code's
    /// `ViewCursors.BLINK_INTERVAL` is half a second.
    pub blink_interval: f32,
    /// How long after it last moved it goes on blinking, seconds ([`limits::BLINK_FOR`]), and
    /// stays lit after (a page at rest draws no frames); 0: it never stops.
    pub blink_for: f32,
    /// What it is while the window or the pane does not have the keyboard.
    pub unfocused: CaretUnfocused,
    /// How it goes from a place to another.
    pub motion: CaretMotion,
}

impl Default for Caret {
    fn default() -> Self {
        Self {
            shape: CaretShape::Bar,
            height: CaretHeight::Row,
            width: 2.0,
            color: Tint::Theme,
            text: Tint::Theme,
            radius: 0.0,
            glow: 0.0,
            blink: true,
            blink_interval: 0.5,
            blink_for: 15.0,
            unfocused: CaretUnfocused::Hidden,
            motion: CaretMotion::Snap,
        }
    }
}

/// How a selection looks ([`EditorOptions::selection`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SelectionLook {
    /// How round the corners of a selection are, as a fraction of a row's height
    /// ([`limits::ROUNDING`]): 0, square. Where lines of a selection meet, the corners that would
    /// be turns inward are rounded too ([`crate::bands`]), as a terminal's selection is. A fraction,
    /// not points, so that the corner grows with the text.
    pub radius: f32,
}

/// What the numbers of the lines in the gutter count.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Numbers {
    /// Each line its own number, from 1.
    #[default]
    Absolute,
    /// The main caret's line its own number, every other line how many lines it is from it, a
    /// folded region one line (the count a move of that many lines up or down goes).
    Relative,
    /// None: the gutter keeps its marks and the buttons that fold.
    Hidden,
}

/// Where an editor draws the ground of the main caret's line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LineHighlight {
    /// Across the text, not the gutter.
    #[default]
    Text,
    /// In the gutter only.
    Gutter,
    /// Across the gutter and the text.
    All,
}

/// The colours of what an editor draws besides its text: the text is the element's colour and the
/// looks the page decorates it with ([`Mark::Style`]). Each is the theme's where the page says
/// nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EditorColors {
    /// The ground of the main caret's line while the editor has the keyboard (the theme's: the
    /// foreground at 5%).
    pub current_line: Tint,
    /// The ground of what is selected while the editor has the keyboard (the theme's selection),
    /// and while it has not (the theme's selection at half).
    pub selection: Tint,
    pub selection_blurred: Tint,
    /// The ground of what a search found (the theme's: its warning colour at 30%).
    pub found: Tint,
    /// The ring around the bracket by the caret and the one that matches it (the theme's: its dim
    /// colour at 60%).
    pub brackets: Tint,
    /// The numbers of the lines (the theme's quiet colour), and the number of the main caret's
    /// line (its dim colour).
    pub numbers: Tint,
    pub current_number: Tint,
    /// The ground of the gutter (the theme's: what the editor is on, else the background).
    pub gutter: Tint,
    /// The buttons that fold and the `⋯` after a folded line (the theme's dim colour), and the
    /// ground of that `⋯` (the foreground at 8%).
    pub folds: Tint,
    pub folded: Tint,
    /// What the editor says while its document is empty (the theme's quiet colour).
    pub hint: Tint,
    /// The dots, arrows and boxes of the spaces and tabs shown (the theme's border colour: the
    /// hairline, seen and never read).
    pub whitespace: Tint,
    /// The guides of the indent (the theme's border colour), and the guide of the block the main
    /// caret is in while the editor has the keyboard (the theme's quiet colour; off, and it is drawn
    /// as the others are). With the guides off and that one not, it alone is drawn.
    pub indent_guides: Tint,
    pub active_indent_guide: Tint,
    /// The ground of the other places of the main selection's text ([`EditorOptions::matches`]):
    /// the theme's selection at 40%.
    pub matches: Tint,
    /// What is laid over a gutter mark the pointer is over, and that answers a press (the theme's
    /// foreground at 30%).
    pub mark_hover: Tint,
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
    /// The lines of each range commented, or uncommented when every one of them is: with the
    /// language's line comment, or where it has none with its block comment around the lines of
    /// each range ([`CommentTokens`]).
    ToggleComment,
    /// Each range put inside the language's block comment, or taken out of it where it is in one
    /// already (its tokens just inside the range or just outside it, a space or a tab between
    /// them and the text taken too); a caret's range is its line's text, its indent left out. Where
    /// some ranges are in one and some are not, those that are not are put in one.
    ToggleBlockComment,
    SelectAll,
    /// A caret more on the line above, or below, at the same place across.
    AddCaretAbove,
    AddCaretBelow,
    /// The next place the main range's text is found, selected as well; where the main range is a
    /// caret, the word it is in (or touches) selected first: a word as Unicode's rules break the
    /// text, cut where a mark stands inside it (`bar.baz` is two).
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
    /// Every region that folds and is in none that does, folded: the outermost (CodeMirror's
    /// `foldAll`).
    FoldAll,
    /// The lines of each range taken out, line breaks and all; the caret goes to the line that
    /// comes in their place, as far along it as it was (CodeMirror's `deleteLine`).
    DeleteLines,
    /// The lines of each range selected whole, with the break after them; a range that is whole
    /// lines already takes the next line too (VS Code's `expandLineSelection`).
    SelectLine,
    /// The lines of each range once more above them; the ranges stay on the upper of the two, the
    /// copy (`DuplicateLines` copies below).
    DuplicateLinesUp,
    /// Each line of a range joined with the line after it (a caret's with the next), the indent of
    /// the later line taken away and one space between them where neither side has one or is
    /// empty; the caret where they meet (VS Code's `joinLines`).
    JoinLines,
    /// A line after each caret's line, as indented as a line break there would make it, the caret
    /// on it (CodeMirror's `insertBlankLine`).
    InsertBlankLine,
    /// A line break at each caret, the caret staying before it (CodeMirror's `splitLine`).
    SplitLine,
    /// The characters on either side of each caret swapped, the caret after them; at the end of a
    /// line the two before it (CodeMirror's `transposeChars`).
    TransposeChars,
    /// From each caret to the start of its line taken out, or the line break before it where it is
    /// at the start (CodeMirror's `deleteLineBoundaryBackward`).
    DeleteToLineStart,
    /// From each caret to the end of its line taken out, or the line break after it where it is at
    /// the end (CodeMirror's `deleteToLineEnd`, Emacs's `kill-line`).
    DeleteToLineEnd,
    /// Looks for `pattern` in the document and marks what it finds (a regular expression where
    /// `regex`, the words as they are where not; where `case` is false, in either case; where
    /// `word`, only a match with no letter, digit or `_` just before it or just after it), and
    /// selects the first match at the main selection or after it. The editor searches again as the
    /// document changes, and says what it finds ([`Found`]) until the search stops.
    Find {
        pattern: String,
        regex: bool,
        case: bool,
        word: bool,
    },
    /// Selects the next, or the one before, of what was found, from the main caret.
    FindNext,
    FindPrevious,
    /// Forgets what was found.
    StopFinding,
    /// Where the main selection is a match, puts `text` in its place and selects the next match;
    /// where it is not, selects the next match from it. A regular expression's `text` says what
    /// the match's groups hold: `$1` or `${1}` by number, `$name` or `${name}` by name, `$$` a `$`
    /// (a name is the longest run of letters, digits and `_`: `${1}x`, not `$1x`); the words of a
    /// search that is no regular expression go in as they are.
    Replace(String),
    /// Puts `text` in place of every match in the document, as [`EditCommand::Replace`] does one:
    /// one change, undone as one. Every match, not only the [`limits::FIND_MATCHES`] a search
    /// marks; only a search for whole words that runs out of what it may read again (a pattern
    /// that matches inside words again and again) stops short, and then says there may be more.
    ReplaceAll(String),
    /// Every match selected, the main one the match the main selection was on (else the one
    /// [`EditCommand::Find`] would select).
    SelectFound,
    /// Every place the main selection's text is, selected, the main selection still the main one;
    /// where it is a caret, every place the word it is in is (the word [`EditCommand::SelectNext`]
    /// takes), as a whole word. In the case written and the words as they are, at most
    /// [`limits::FIND_MATCHES`] of them; what is searched for ([`EditCommand::Find`]) stays as it
    /// was.
    SelectOccurrences,
}

/// What a page asks of an editor of its, in a reply.
#[derive(Clone, Debug, PartialEq)]
pub enum EditorAsk {
    /// A piece of the document: the first takes the place of the whole text, each after it (where
    /// the one before said `more`) goes on its end. Nothing of a load is undone, and what could be
    /// undone before cannot be any more; while one goes on, the person cannot change the text.
    Load { text: String, more: bool },
    /// Changes against the version `base`: carried over what was done since, done, and undone as
    /// one — or, where not `history`, not undone at all (CodeMirror's `addToHistory: false`):
    /// what can be undone is carried over them, as over the person's, so that a file read again
    /// from disk keeps what was done to it before. `tag` is told back in the `edit` of it
    /// ([`Edited::tag`]).
    Change {
        base: u64,
        changes: Vec<Change>,
        history: bool,
        tag: Option<String>,
    },
    /// The selections, in the places of `version` ([`Selection`]).
    Select {
        version: u64,
        ranges: Vec<Range>,
        main: usize,
    },
    /// The place `at` (of `version`) scrolled into view, where `align` says.
    Reveal {
        version: u64,
        at: usize,
        align: RevealAlign,
    },
    /// What the person could have done with a key.
    Command(EditCommand),
    /// A layer's decorations for a range.
    Decorate(Decorate),
}

/// Where a place an editor reveals is put in its room ([`EditorAsk::Reveal`]), as CodeMirror's
/// `scrollIntoView` and VS Code's `revealLine*` put it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RevealAlign {
    /// Scrolled only as far as it takes to show it (and the editor's `scroll_margin` about it),
    /// and not at all where it shows: what a caret that moves asks.
    #[default]
    Nearest,
    /// Its row at the top of the room, as far as the document goes.
    Start,
    /// Its row in the middle of the room: a place gone to from elsewhere.
    Center,
    /// Its row at the bottom of the room.
    End,
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
    /// The word at `at`, or the run of marks or of spaces, by the rule a double click has: its
    /// range; `None` for a place past the end.
    fn word_range(&self, at: usize) -> Option<(usize, usize)>;
}

/// The limits an editor holds a page to: what every host gives at least (the specification's table
/// of floors). Both the sandbox, which checks the shape of what a page sends, and the editor, which
/// checks what it means for the document, read them here.
pub mod limits {
    /// The longest selection whose other places an editor marks ([`super::Matches`]), characters.
    pub const MATCH_LENGTH: usize = 200;
    /// The most other places of a selection an editor marks in view ([`super::Matches`]).
    pub const MATCHES_IN_VIEW: usize = 100;
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
    /// The most matches a search finds, and the most places a command that selects every match
    /// selects: what is past them is not marked ([`super::Found::more`]).
    pub const FIND_MATCHES: usize = 10_000;
    /// The most memory a regular expression of an editor's (a search's, a rule of its indent's) may
    /// take compiled, bytes: one that would take more is no pattern the editor uses.
    pub const PATTERN_SIZE: usize = 1 << 22;
    /// The most columns a tab may take.
    pub const TAB_WIDTH: u8 = 16;
    /// How tall a row may be, times the font's size.
    pub const LINE_HEIGHT: std::ops::RangeInclusive<f32> = 1.0..=3.0;
    /// How much room may be kept around the text, points.
    pub const PADDING: std::ops::RangeInclusive<f32> = 0.0..=64.0;
    /// How wide a caret's bar, or thick its line, may be, points.
    pub const CARET_WIDTH: std::ops::RangeInclusive<f32> = 0.5..=8.0;
    /// How round a corner is, as a fraction of a row's height.
    pub const ROUNDING: std::ops::RangeInclusive<f32> = 0.0..=0.5;
    /// How strong the light round a caret may be.
    pub const CARET_GLOW: std::ops::RangeInclusive<f32> = 0.0..=3.0;
    /// How long a caret may be lit, and dark, seconds.
    pub const BLINK_INTERVAL: std::ops::RangeInclusive<f32> = 0.05..=5.0;
    /// How long a caret's `ease` may take, seconds.
    pub const CARET_EASE: std::ops::RangeInclusive<f32> = 0.05..=1.0;
    /// How stiff a caret's `spring` may be, radians a second.
    pub const CARET_SPRING: std::ops::RangeInclusive<f32> = 5.0..=100.0;
    /// How long after it moved a caret may go on blinking, seconds.
    pub const BLINK_FOR: std::ops::RangeInclusive<f32> = 0.0..=3600.0;
    /// How wide a column of the gutter may be, points.
    pub const COLUMN: std::ops::RangeInclusive<f32> = 0.0..=64.0;
    /// How many columns the gutter may have.
    pub const COLUMNS: usize = 8;
    /// The most rows a zone may take.
    pub const ZONE_ROWS: usize = 200;
    /// The most names of ranges an editor holds ([`crate::edit::Mark::Anchor`]).
    pub const ANCHORS: usize = 64;
    /// What the pointer's rest may be, seconds.
    pub const HOVER_DELAY: std::ops::RangeInclusive<f32> = 0.0..=10.0;
    /// What the least time between two words told of may be, seconds.
    pub const LINK_DELAY: std::ops::RangeInclusive<f32> = 0.0..=1.0;
    /// What a chord's wait for its next key may be, seconds.
    pub const CHORD_TIMEOUT: std::ops::RangeInclusive<f32> = 0.1..=10.0;
    /// The most entries of a page's table of presses, and of its table of keys.
    pub const BINDINGS: usize = 256;
    /// The most keys one chord has.
    pub const CHORD_KEYS: usize = 4;
    /// The most characters the words of a widget have, parts it has, and widgets standing in a line
    /// inside it.
    pub const WIDGET_TEXT: usize = 200;
    pub const WIDGET_PARTS: usize = 8;
    pub const WIDGETS_IN_LINE: usize = 32;
    /// What a widget group's size (times the text), room and gap may be.
    pub const WIDGET_SIZE: std::ops::RangeInclusive<f32> = 0.5..=2.0;
    pub const WIDGET_ROOM: std::ops::RangeInclusive<f32> = 0.0..=64.0;
    /// How wide a bar, or a mark of the overview ruler, may be, points.
    pub const MARK_WIDTH: std::ops::RangeInclusive<f32> = 1.0..=16.0;
}
