//! The page's elements and the chain that writes them: `div().row().gap(8.0).child(...)`.
//!
//! A page is a tree of six kinds of element — a **box** ([`div`]), a line of **text**
//! ([`text`]), a text of many looks ([`crate::rich`]), a small **sparkline** ([`spark`]), an
//! **image** ([`image`]) and a **canvas** of paths ([`canvas`]). Every value is in points. A box
//! lays
//! its children out as flexbox or as a grid (the way a style sheet would), paints a fill, a
//! border and a shadow, and sets the text of the boxes under it; a box is a column unless
//! told otherwise.

use crate::BrowserOptions;
use crate::edit::EditorOptions;
use crate::style::{
    Anchor, Border, Direction, Display, Gradient, Items, Justify, Length, Overflow, Ring, Shadow,
    Side, Style, Track, Truncate,
};
use crate::{AccessibleRole, Align, Color, Command, Family, Id, Motion, Path, Span, Tip};

/// What an element is.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Box,
    Text(String),
    /// A text of many looks: its parts, in order.
    Rich(Vec<Span>),
    Spark(Spark),
    Image(Image),
    /// Paths drawn in the box, in its points.
    Canvas(Vec<Path>),
}

/// The data of a sparkline: values over time, newest last, drawn as a line with the area
/// under it softly filled.
#[derive(Clone, Debug, PartialEq)]
pub struct Spark {
    pub values: Vec<f32>,
    pub color: Color,
    /// The value the top of the box stands for; the largest value (plus a little head room)
    /// when `None`.
    pub peak: Option<f32>,
}

/// How an image fills the box it is given.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fit {
    /// The whole image, as large as the box lets it be with its proportions kept; what the box
    /// has left over stays empty.
    #[default]
    Contain,
    /// The box filled, the proportions kept: what does not fit the box is cut off, evenly on
    /// both sides.
    Cover,
}

/// The data of an image: a file of the view's bundle, drawn into the element's box.
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    /// The path of the file in the view's bundle, as its manifest names it.
    pub src: String,
    pub fit: Fit,
    /// Where there is one, the image gives its shape and not its colours: the colour is this one
    /// and the image's opacity, pixel by pixel, is the amount of it. How much of it shows is the
    /// element's opacity ([`Element::opacity`]).
    pub tint: Option<Color>,
    /// Which page of the file is shown, from 1, where the file has pages (a PDF); an image that
    /// has none is its only page, and a page it does not have is no image.
    pub page: u32,
}

/// What a text field the host puts over the page takes ([`Element::field`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FieldKind {
    /// One line: enter is a key for the page.
    #[default]
    Line,
    /// Lines: enter breaks the line, and the field scrolls the lines it has no room for.
    Lines,
    /// One line whose characters are not shown (a password), and which is never copied.
    Secret,
}

/// Where an element stands, to a screen reader, besides what it is: whether it is checked, chosen,
/// open or of no use now, and the value it shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    /// A checkbox, a switch, a toggle or a radio button: on or off. `None` for what is neither.
    pub checked: Option<bool>,
    /// The one chosen of several: the tab in front, the row picked.
    pub selected: bool,
    /// Something that opens (a group, a menu's button): open or shut. `None` for what does not.
    pub expanded: Option<bool>,
    /// It cannot be used now: it does not answer the pointer, and a screen reader says so.
    pub disabled: bool,
    /// The value it shows, in words: a progress's `"40%"`, a slider's `"12 px"`.
    pub value: Option<String>,
    /// A checkbox that is neither on nor off (some of what it stands for is chosen): `checked` is
    /// `None` then.
    pub mixed: bool,
    /// How deep it is in a tree, from 1.
    pub level: Option<u32>,
    /// How many there are of the set it is one of (a list, the tabs, the options), and which it is,
    /// from 1.
    pub setsize: Option<u32>,
    pub posinset: Option<u32>,
    /// How many rows and columns the whole table has, which a page that builds only what is in view
    /// has more of than it says items.
    pub rowcount: Option<u32>,
    pub colcount: Option<u32>,
    /// Which row (on a row) and which column (on a cell or a header) it is, from 1.
    pub rowindex: Option<u32>,
    pub colindex: Option<u32>,
    /// How the table is sorted by this column (on a header).
    pub sort: Option<Sort>,
}

/// How a column sorts a table ([`State::sort`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sort {
    Ascending,
    Descending,
}

/// The group a box that scrolls goes with, and the way it goes ([`Element::scroll_link`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollLink {
    /// The name the boxes that go together share.
    pub name: Id,
    pub axis: LinkAxis,
}

/// The way boxes that are linked go together.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LinkAxis {
    /// Down: they stand at the same offset down.
    #[default]
    Y,
    /// Across.
    X,
    /// Both ways.
    Both,
}

impl LinkAxis {
    /// Whether the boxes go together down.
    pub fn down(self) -> bool {
        matches!(self, Self::Y | Self::Both)
    }

    /// Whether the boxes go together across.
    pub fn across(self) -> bool {
        matches!(self, Self::X | Self::Both)
    }
}

/// What an element is to the pointer, the keyboard and a screen reader, apart from how it looks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Meta {
    pub id: Option<Id>,
    /// The pointer can hover it and press it (it answers a click).
    pub pressable: bool,
    /// The keyboard can be on it.
    pub focusable: bool,
    /// The host puts a text field of its own, of this kind, over this box ([`Element::field`]).
    pub field: Option<FieldKind>,
    /// How many lines the box of a field of lines is as tall as ([`Element::field_rows`]).
    pub field_rows: Option<u16>,
    /// The box is the room of an editor of the host's, set up so ([`Element::editor`]). Kept
    /// apart from the element: an editor's options are several times what the rest of an element
    /// is, and nearly no element of a page is an editor.
    pub editor: Option<Box<EditorOptions>>,
    /// The box is the room of a browser of the host's, set up so ([`Element::browser`]).
    pub browser: Option<Box<BrowserOptions>>,
    /// What the element is in words, for a person who does not see it ([`Element::label`]).
    pub label: Option<String>,
    /// The element is the content of the zone of this name in the editor it is a child of
    /// ([`Element::zone`]).
    pub zone: Option<Id>,
    /// The box that scrolls goes with the others of this name ([`Element::scroll_link`]).
    pub scroll_link: Option<ScrollLink>,
    /// How a change of the element moves instead of jumping ([`Element::motion`]).
    pub motion: Option<Motion>,
    /// What the host says of the element while the pointer rests on it ([`Element::tip`]).
    pub tip: Option<Tip>,
    /// What it is to a screen reader, where the page says ([`Element::role`]).
    pub role: Option<AccessibleRole>,
    /// Where it stands, to a screen reader ([`State`]).
    pub state: State,
    /// The element is the root of a layer the page draws over the rest (a modal, a popover, a
    /// toast): `Some(true)` for one that shuts the page behind it out of what a screen reader and an
    /// agent read, `Some(false)` for one that leaves it ([`Element::layer`]).
    pub layer: Option<bool>,
    /// What a screen reader says of the element where its own name comes to no words
    /// ([`Element::fallback_name`]).
    pub fallback: Option<String>,
    /// What the page calls the kind of element this is, in its own words ("process", "issue card"),
    /// where no role is it ([`Element::role_description`]).
    pub role_description: Option<String>,
    /// What the page can be told to do while the keyboard is on it or inside it.
    pub commands: Vec<Command>,
    /// The keyboard is on it: its commands, and those of the boxes around it, can be done.
    pub focused: bool,
    /// It can be picked up and moved: pressed and dragged, it is what is moved ([`crate::Ui`]).
    pub draggable: bool,
    /// What is moved can be put down on it.
    pub drop_target: bool,
    /// What a person who drags it out of the view takes with them ([`Element::carry`]).
    pub carry: Option<Carry>,
}

/// What is handed to the host's system drag, and is what the person drags out of the view
/// ([`Element::carry`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Payload {
    /// Words: what a terminal pane or a text field is given as they are.
    Text(String),
    /// A path of a file or a folder, as words: a terminal quotes it for its shell, and a host
    /// never reads it (a view reaches no file of its own, only those it is given by handle).
    Path(String),
}

/// What a drag out of the view carries: the payload, and an address that goes with it for the
/// applications that take one. The rule of both is `pencere_wire::carry`'s (one line, no control
/// character, at most 64 KiB; an address only of the kind a `link` may be): a host never hands on
/// what breaks it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Carry {
    pub payload: Payload,
    pub url: Option<String>,
}

impl Carry {
    /// A drag that carries `words`.
    pub fn text(words: impl Into<String>) -> Self {
        Self {
            payload: Payload::Text(words.into()),
            url: None,
        }
    }

    /// A drag that carries the path `path`.
    pub fn path(path: impl Into<String>) -> Self {
        Self {
            payload: Payload::Path(path.into()),
            url: None,
        }
    }

    /// With an address that goes with it.
    #[must_use]
    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    /// The words of the payload, whichever it is.
    pub fn words(&self) -> &str {
        match &self.payload {
            Payload::Text(words) | Payload::Path(words) => words,
        }
    }
}

/// One element of a page, and the elements inside it.
#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    pub(crate) kind: Kind,
    pub(crate) style: Style,
    pub(crate) children: Vec<Element>,
    pub(crate) meta: Meta,
}

/// A box: a column of its children until told otherwise.
pub fn div() -> Element {
    Element::of(Kind::Box)
}

/// One line of text, in the font, size and colour of the boxes it is in.
pub fn text(content: impl Into<String>) -> Element {
    Element::of(Kind::Text(content.into()))
}

/// A sparkline of `values` (newest last), `color` the line.
pub fn spark(values: &[f32], color: impl Into<Color>) -> Element {
    Element::of(Kind::Spark(Spark {
        values: values.to_vec(),
        color: color.into(),
        peak: None,
    }))
}

/// An image: the file of the view's bundle at `src`, in the box of the element (its size is the
/// element's style; where one side is not said, the image's proportions give it, and where
/// neither is, the image's own size in pixels, one point to a pixel).
pub fn image(src: impl Into<String>, fit: Fit) -> Element {
    Element::of(Kind::Image(Image {
        src: src.into(),
        fit,
        tint: None,
        page: 1,
    }))
}

/// A box that takes the room left over: what pushes the elements after it to the end.
pub fn spacer() -> Element {
    div().grow(1.0)
}

/// A text of many looks: `spans` one after the other, as one paragraph that runs from line to line
/// where it `wrap`s. What a part does not say is the element's (its colour, weight, face, and
/// whether its words are underlined or struck), as for [`text`].
pub fn rich(spans: impl IntoIterator<Item = Span>) -> Element {
    Element::of(Kind::Rich(spans.into_iter().collect()))
}

/// A box with `paths` drawn in it, in its points (its top-left corner is the origin). It has no
/// size of its own: its style gives it one.
pub fn canvas(paths: impl IntoIterator<Item = Path>) -> Element {
    Element::of(Kind::Canvas(paths.into_iter().collect()))
}

impl Element {
    fn of(kind: Kind) -> Self {
        Self {
            kind,
            style: Style::default(),
            children: Vec::new(),
            meta: Meta::default(),
        }
    }

    fn set(mut self, change: impl FnOnce(&mut Style)) -> Self {
        change(&mut self.style);
        self
    }

    // --- Children --------------------------------------------------------------------

    /// Adds `child` after the ones already here.
    #[must_use]
    pub fn child(mut self, child: Element) -> Self {
        self.children.push(child);
        self
    }

    /// Adds every element of `children`.
    #[must_use]
    pub fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        self.children.extend(children);
        self
    }

    /// Adds `child` when there is one.
    #[must_use]
    pub fn child_if(self, child: Option<Element>) -> Self {
        match child {
            Some(child) => self.child(child),
            None => self,
        }
    }

    // --- Names and answers ---------------------------------------------------------------

    /// Names the element, so what is kept about it between builds (a scroll position, a
    /// fade) and what the host asks of it (its box) can find it.
    #[must_use]
    pub fn id(mut self, id: Id) -> Self {
        self.meta.id = Some(id);
        self
    }

    /// Names the element and makes it something the pointer answers: it can be hovered,
    /// pressed and clicked (`Ui::hover`, `Ui::press`), and the pointer takes the hand's
    /// shape over it.
    #[must_use]
    pub fn press(mut self, id: Id) -> Self {
        self.meta.id = Some(id);
        self.meta.pressable = true;
        self
    }

    /// The keyboard can be on this element (it is named too: see [`Element::id`]); focus
    /// moves between such elements in the order they are written.
    #[must_use]
    pub fn focusable(mut self) -> Self {
        self.meta.focusable = true;
        self
    }

    /// The box is the room of a text field the host puts over the page, named `id` — the page
    /// does not draw text that is typed into, a field of the host's does (the host asks where
    /// each stands: [`crate::Laid::fields`]). A page has as many as it names. A text inside it is
    /// what the field says while it is empty (`div().field(id, FieldKind::Line).child(text("Filter
    /// by name…"))`): the host's field shows it, the page does not draw it.
    #[must_use]
    pub fn field(mut self, id: Id, kind: FieldKind) -> Self {
        self.meta.id = Some(id);
        self.meta.field = Some(kind);
        self
    }

    /// A one-line field named `id` ([`Element::field`]).
    #[must_use]
    pub fn text_field(self, id: Id) -> Self {
        self.field(id, FieldKind::Line)
    }

    /// The box of a field of lines ([`FieldKind::Lines`]) is as tall as `rows` lines of the host's
    /// field, and what is between the box and its text (the padding, the border): the page says how
    /// many lines it wants and the host's own line decides the height, whatever the size of the
    /// text and the host's zoom. **It takes the place of the height** (`h`, `h_pct`); the least and
    /// the most height still hold. A count is from 1 to 100, and a number outside it is the
    /// nearest. A field that is not of lines has no use for it. The text of the field stands in the
    /// box less its padding and border ([`crate::Field::text`]).
    #[must_use]
    pub fn field_rows(mut self, rows: u16) -> Self {
        self.meta.field_rows = Some(rows);
        self
    }

    /// The box is the room of an editor of text the host puts there, named `id`, set up as
    /// `options` say (`div().editor(id, EditorOptions::default()).grow(1.0)`): the host keeps its
    /// document, edits it, lays out and draws its lines, and scrolls them as it scrolls a box
    /// ([`Element::scroll`]); the page is told what changed and asks for changes
    /// ([`crate::edit`]). The box's fill and border are the page's. Its document lives as long as an
    /// editor of that name is on the page, hidden or not ([`crate::Laid::editors`]). A text inside it
    /// is what it says while its document is empty, where its options say nothing; the page does
    /// not draw it. The caret has a name of its own to stand beside ([`Id::caret`]).
    #[must_use]
    pub fn editor(mut self, id: Id, options: EditorOptions) -> Self {
        self.meta.id = Some(id);
        self.meta.editor = Some(Box::new(options));
        self.set(|s| s.overflow = Overflow::Scroll)
    }

    /// The box is the room of a browser the host puts there, named `id`, set up as `options` say
    /// (`div().browser(id, BrowserOptions::default()).grow(1.0)`): the host's engine loads the pages
    /// and draws them in the room, which it scrolls and takes the pointer and the keyboard in. The
    /// page says where the browser goes ([`crate::BrowserAsk`]) and is told where it is and what it
    /// is doing ([`crate::BrowserState`]), and nothing of what it shows. The box's fill and border
    /// are the page's, and what stands in it is drawn over the browser. A browser lives as long as
    /// one of that name is on the page, hidden or not ([`crate::Laid::browsers`]).
    #[must_use]
    pub fn browser(mut self, id: Id, options: BrowserOptions) -> Self {
        self.meta.id = Some(id);
        self.meta.browser = Some(Box::new(options));
        self.set(|s| s.overflow = Overflow::Hidden)
    }

    /// The element stands in the gap the editor it is a child of keeps open for the zone named `id`
    /// ([`crate::edit::Zone`]): its top is where the gap starts (it is laid out as a box of the
    /// editor's room that is wide as the room and as tall as the page says, and then put there, so
    /// that it scrolls with the text), and it is not there while the editor has no such gap.
    #[must_use]
    pub fn zone(mut self, id: Id) -> Self {
        self.meta.zone = Some(id);
        self.set(|s| {
            s.absolute = true;
            s.inset = [Some(0.0), Some(0.0), None, Some(0.0)];
        })
    }

    /// What the element is in words, for a person who does not see it: a screen reader says it.
    /// A button that is an icon alone says what it does; an element with none is read by the
    /// words inside it, and, where there are none and it is one a person presses (a button, a link,
    /// a checkbox, a switch, a radio, a tab, a menu item), by its [`Element::tip`] (not its keys);
    /// a heading, a picture or a progress with none is not named by its tip. The words inside an
    /// element that has a label and holds controls are read apart only where the label does not
    /// say them already.
    #[must_use]
    pub fn label(mut self, words: impl Into<String>) -> Self {
        self.meta.label = Some(words.into());
        self
    }

    /// How a change of this element moves instead of jumping: its colours, its opacity, its size
    /// and its place, how it comes in and how it goes ([`Motion`]). The host does the moving,
    /// frame by frame, without the page; the element is followed by its id, and one with no id
    /// does not move.
    #[must_use]
    pub fn motion(mut self, motion: Motion) -> Self {
        self.meta.motion = Some(motion);
        self
    }

    /// What the host says of this element while the pointer rests on it, with the keys that do
    /// the same where there are some (`"Containers"`, `"1"`): the tooltip of a button that is an
    /// icon alone, the key of a section.
    #[must_use]
    pub fn tip(mut self, words: impl Into<String>, keys: Option<String>) -> Self {
        self.meta.tip = Some(Tip {
            words: words.into(),
            keys,
        });
        self
    }

    /// What the element is to a screen reader, where its look says it and its answers do not: a
    /// checkbox, a switch, a tab, a link, a heading. (A text field is one by [`Element::field`], an
    /// editor by [`Element::editor`], a browser by [`Element::browser`], and none is made one here.)
    #[must_use]
    pub fn role(mut self, role: AccessibleRole) -> Self {
        if !matches!(
            role,
            AccessibleRole::Field(_) | AccessibleRole::Editor | AccessibleRole::Browser
        ) {
            self.meta.role = Some(role);
        }
        self
    }

    /// The element is the root of a layer drawn over the page: with `modal` it shuts the page behind
    /// it out of what a screen reader and an agent read (the nearest layer of the kind in front
    /// wins), without it (a popover, a toast) the page stays. The element is an overlay, written where
    /// the page writes it; what is inside it, and what it opens, is in the layer.
    #[must_use]
    pub fn layer(mut self, modal: bool) -> Self {
        self.meta.layer = Some(modal);
        self
    }

    /// What a screen reader says of the element where its own name comes to no words (no label, no
    /// words inside it, no tip): a row of controls that has none of its own is "Row 3". It never
    /// replaces what the element has.
    #[must_use]
    pub fn fallback_name(mut self, words: impl Into<String>) -> Self {
        self.meta.fallback = Some(words.into());
        self
    }

    /// On or off: a checkbox, a switch, a toggle button.
    #[must_use]
    pub fn checked(mut self, on: bool) -> Self {
        self.meta.state.checked = Some(on);
        self
    }

    /// The one chosen of several: the tab in front, the row picked.
    #[must_use]
    pub fn selected(mut self, chosen: bool) -> Self {
        self.meta.state.selected = chosen;
        self
    }

    /// Open or shut: a group that opens, the button of a menu.
    #[must_use]
    pub fn expanded(mut self, open: bool) -> Self {
        self.meta.state.expanded = Some(open);
        self
    }

    /// Of no use now: it does not answer the pointer (the element under it does), and a screen
    /// reader says it cannot be used.
    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.meta.state.disabled = disabled;
        self
    }

    /// Neither on nor off: a checkbox that stands for some that are chosen and some that are not.
    #[must_use]
    pub fn mixed(mut self, mixed: bool) -> Self {
        self.meta.state.mixed = mixed;
        if mixed {
            self.meta.state.checked = None;
        }
        self
    }

    /// How deep the element is in a tree, from 1.
    #[must_use]
    pub fn level(mut self, level: u32) -> Self {
        self.meta.state.level = Some(level);
        self
    }

    /// Which of `size` the element is, from 1: a tab, an option, an item of a list that is built
    /// only where it shows.
    #[must_use]
    pub fn position(mut self, index: u32, size: u32) -> Self {
        self.meta.state.posinset = Some(index);
        self.meta.state.setsize = Some(size);
        self
    }

    /// How many rows and columns the whole table has, said on its element.
    #[must_use]
    pub fn table_size(mut self, rows: u32, columns: u32) -> Self {
        self.meta.state.rowcount = Some(rows);
        self.meta.state.colcount = Some(columns);
        self
    }

    /// Which row and which column of the table the element is, from 1 (a row has no column, a cell
    /// both).
    #[must_use]
    pub fn cell_at(mut self, row: Option<u32>, column: Option<u32>) -> Self {
        self.meta.state.rowindex = row;
        self.meta.state.colindex = column;
        self
    }

    /// How the table is sorted by the column this header is.
    #[must_use]
    pub fn sorted(mut self, order: Sort) -> Self {
        self.meta.state.sort = Some(order);
        self
    }

    /// What the page calls the kind of element this is, in its own words: a screen reader says it
    /// in place of the role's.
    #[must_use]
    pub fn role_description(mut self, words: impl Into<String>) -> Self {
        self.meta.role_description = Some(words.into());
        self
    }

    /// The value the element shows, in words, for a screen reader: `"40%"`.
    #[must_use]
    pub fn value(mut self, words: impl Into<String>) -> Self {
        self.meta.state.value = Some(words.into());
        self
    }

    /// Something the page can be told to do while the keyboard is on this element or inside it
    /// (or always, on the page's outermost box): the host lists it and does it by its keys.
    #[must_use]
    pub fn command(mut self, command: Command) -> Self {
        self.meta.commands.push(command);
        self
    }

    /// The keyboard is on this element: what the page's [`crate::Ui`] says has the focus. Its
    /// commands and those of the boxes around it are the ones that can be done.
    #[must_use]
    pub fn focused(mut self, focused: bool) -> Self {
        self.meta.focused = focused;
        self
    }

    /// Names the element and makes it something that can be picked up and moved: pressed and
    /// dragged, it is what [`crate::Ui::dragging`] says is on its way. It answers the pointer as a
    /// pressed element does.
    #[must_use]
    pub fn draggable(mut self, id: Id) -> Self {
        self.meta.id = Some(id);
        self.meta.draggable = true;
        self
    }

    /// Makes the element something a person can drag out of the view, to a terminal pane or another
    /// application: pressed on and moved past [`crate::DRAG_START`] points, the host's system drag
    /// begins with `carry` ([`crate::Laid::carry_at`], and the page is told when it ends:
    /// [`crate::Event::Carried`]). An element that is also [`Element::draggable`] is the page's own
    /// drag while the pointer is in the view, and the system's from the moment it leaves it. The
    /// element needs a name ([`Element::id`]): one with none does not carry.
    #[must_use]
    pub fn carry(mut self, carry: Carry) -> Self {
        self.meta.carry = Some(carry);
        self
    }

    /// Names the element and makes it a place what is dragged can be put down on.
    #[must_use]
    pub fn drop_target(mut self, id: Id) -> Self {
        self.meta.id = Some(id);
        self.meta.drop_target = true;
        self
    }

    // --- Layout: the box ---------------------------------------------------------------

    /// Lays the children out in a row, left to right.
    #[must_use]
    pub fn row(self) -> Self {
        self.set(|s| {
            s.display = Display::Flex;
            s.direction = Direction::Row;
        })
    }

    /// A row (or column) that is too long for its box goes on in a new line. On a text: its
    /// words go on in new lines where its box is too narrow for them (and at its line breaks),
    /// and the box is as tall as the lines.
    #[must_use]
    pub fn wrap(self) -> Self {
        self.set(|s| s.wrap = true)
    }

    /// Lays the children out in a column, top to bottom (the default).
    #[must_use]
    pub fn col(self) -> Self {
        self.set(|s| {
            s.display = Display::Flex;
            s.direction = Direction::Column;
        })
    }

    /// Lays the children out in a grid of `columns`; the children fill it row by row, new
    /// rows as many as they need.
    #[must_use]
    pub fn grid(self, columns: impl IntoIterator<Item = Track>) -> Self {
        let columns: Vec<Track> = columns.into_iter().collect();
        self.set(|s| {
            s.display = Display::Grid;
            s.columns = columns;
        })
    }

    /// The grid's rows, where they are not simply as tall as their content.
    #[must_use]
    pub fn grid_rows(self, rows: impl IntoIterator<Item = Track>) -> Self {
        let rows: Vec<Track> = rows.into_iter().collect();
        self.set(|s| s.rows = rows)
    }

    /// Takes `columns` grid columns.
    #[must_use]
    pub fn span(self, columns: u16) -> Self {
        self.set(|s| s.column_span = columns)
    }

    /// Takes no room and draws nothing.
    #[must_use]
    pub fn hidden(self) -> Self {
        self.set(|s| s.display = Display::None)
    }

    /// The children are aligned to the start of the cross axis (the top of a row).
    #[must_use]
    pub fn items_start(self) -> Self {
        self.set(|s| s.items = Some(Items::Start))
    }

    /// The children are centred on the cross axis (vertically in a row).
    #[must_use]
    pub fn items_center(self) -> Self {
        self.set(|s| s.items = Some(Items::Center))
    }

    /// The children are aligned to the end of the cross axis.
    #[must_use]
    pub fn items_end(self) -> Self {
        self.set(|s| s.items = Some(Items::End))
    }

    /// The children fill the cross axis (the default).
    #[must_use]
    pub fn items_stretch(self) -> Self {
        self.set(|s| s.items = Some(Items::Stretch))
    }

    /// The children of a row stand on one baseline: the first line of words of each is on the same
    /// line, whatever size its words are (a style sheet's `align-items: baseline`). A child with no
    /// words in it (a picture, an empty box) has its bottom edge there, and in a column the children
    /// are aligned to the start of the cross axis.
    #[must_use]
    pub fn items_baseline(self) -> Self {
        self.set(|s| s.items = Some(Items::Baseline))
    }

    /// The children start at the start of the main axis (the default).
    #[must_use]
    pub fn justify_start(self) -> Self {
        self.set(|s| s.justify = Some(Justify::Start))
    }

    /// The children are centred on the main axis.
    #[must_use]
    pub fn justify_center(self) -> Self {
        self.set(|s| s.justify = Some(Justify::Center))
    }

    /// The children are pushed to the end of the main axis.
    #[must_use]
    pub fn justify_end(self) -> Self {
        self.set(|s| s.justify = Some(Justify::End))
    }

    /// The room left over goes between the children.
    #[must_use]
    pub fn justify_between(self) -> Self {
        self.set(|s| s.justify = Some(Justify::Between))
    }

    /// The same gap between children on both axes, points.
    #[must_use]
    pub fn gap(self, points: f32) -> Self {
        self.set(|s| s.gap = (points, points))
    }

    /// The gap between columns (the children of a row), points.
    #[must_use]
    pub fn gap_x(self, points: f32) -> Self {
        self.set(|s| s.gap.0 = points)
    }

    /// The gap between rows (the children of a column), points.
    #[must_use]
    pub fn gap_y(self, points: f32) -> Self {
        self.set(|s| s.gap.1 = points)
    }

    // --- Layout: the element in its parent ---------------------------------------------

    /// How much of the room left over this element takes, relative to its siblings.
    #[must_use]
    pub fn grow(self, share: f32) -> Self {
        self.set(|s| s.grow = share)
    }

    /// Never gives way when the line is too long.
    #[must_use]
    pub fn no_shrink(self) -> Self {
        self.set(|s| s.shrink = Some(0.0))
    }

    /// Takes the width of the parent.
    #[must_use]
    pub fn w_full(self) -> Self {
        self.set(|s| s.width = Length::Percent(100.0))
    }

    /// Takes the height of the parent.
    #[must_use]
    pub fn h_full(self) -> Self {
        self.set(|s| s.height = Length::Percent(100.0))
    }

    /// Takes the width and the height of the parent.
    #[must_use]
    pub fn size_full(self) -> Self {
        self.w_full().h_full()
    }

    /// The width, points.
    #[must_use]
    pub fn w(self, points: f32) -> Self {
        self.set(|s| s.width = Length::Px(points))
    }

    /// A share of the parent's width, `0.0..=100.0`.
    #[must_use]
    pub fn w_pct(self, percent: f32) -> Self {
        self.set(|s| s.width = Length::Percent(percent))
    }

    /// A share of the parent's height, `0.0..=100.0`.
    #[must_use]
    pub fn h_pct(self, percent: f32) -> Self {
        self.set(|s| s.height = Length::Percent(percent))
    }

    /// The height, points.
    #[must_use]
    pub fn h(self, points: f32) -> Self {
        self.set(|s| s.height = Length::Px(points))
    }

    /// The width and the height, points.
    #[must_use]
    pub fn size(self, width: f32, height: f32) -> Self {
        self.w(width).h(height)
    }

    /// The least width, points. A line of text that may be cut (`truncate`) has none by
    /// default.
    #[must_use]
    pub fn min_w(self, points: f32) -> Self {
        self.set(|s| s.min_width = Length::Px(points))
    }

    /// The least height, points.
    #[must_use]
    pub fn min_h(self, points: f32) -> Self {
        self.set(|s| s.min_height = Length::Px(points))
    }

    /// The greatest width, points.
    #[must_use]
    pub fn max_w(self, points: f32) -> Self {
        self.set(|s| s.max_width = Length::Px(points))
    }

    /// The greatest height, points.
    #[must_use]
    pub fn max_h(self, points: f32) -> Self {
        self.set(|s| s.max_height = Length::Px(points))
    }

    /// Room inside the box on all four sides, points.
    #[must_use]
    pub fn p(self, points: f32) -> Self {
        self.set(|s| s.padding = [points; 4])
    }

    /// Room inside the box left and right, points.
    #[must_use]
    pub fn px(self, points: f32) -> Self {
        self.set(|s| {
            s.padding[1] = points;
            s.padding[3] = points;
        })
    }

    /// Room inside the box above and below, points.
    #[must_use]
    pub fn py(self, points: f32) -> Self {
        self.set(|s| {
            s.padding[0] = points;
            s.padding[2] = points;
        })
    }

    #[must_use]
    pub fn pt(self, points: f32) -> Self {
        self.set(|s| s.padding[0] = points)
    }

    #[must_use]
    pub fn pr(self, points: f32) -> Self {
        self.set(|s| s.padding[1] = points)
    }

    #[must_use]
    pub fn pb(self, points: f32) -> Self {
        self.set(|s| s.padding[2] = points)
    }

    #[must_use]
    pub fn pl(self, points: f32) -> Self {
        self.set(|s| s.padding[3] = points)
    }

    /// Room outside the box above it, points.
    #[must_use]
    pub fn mt(self, points: f32) -> Self {
        self.set(|s| s.margin[0] = points)
    }

    /// Room outside the box to its right, points.
    #[must_use]
    pub fn mr(self, points: f32) -> Self {
        self.set(|s| s.margin[1] = points)
    }

    /// Room outside the box below it, points.
    #[must_use]
    pub fn mb(self, points: f32) -> Self {
        self.set(|s| s.margin[2] = points)
    }

    /// Room outside the box to its left, points.
    #[must_use]
    pub fn ml(self, points: f32) -> Self {
        self.set(|s| s.margin[3] = points)
    }

    /// Room outside the box on all four sides, points.
    #[must_use]
    pub fn m(self, points: f32) -> Self {
        self.set(|s| s.margin = [points; 4])
    }

    /// Room outside the box left and right, points.
    #[must_use]
    pub fn mx(self, points: f32) -> Self {
        self.set(|s| {
            s.margin[1] = points;
            s.margin[3] = points;
        })
    }

    /// Room outside the box above and below, points.
    #[must_use]
    pub fn my(self, points: f32) -> Self {
        self.set(|s| {
            s.margin[0] = points;
            s.margin[2] = points;
        })
    }

    /// Takes the box out of the flow: it is placed against the parent's box by `inset`
    /// (and the size it is given), over its siblings.
    #[must_use]
    pub fn absolute(self) -> Self {
        self.set(|s| s.absolute = true)
    }

    /// For an absolute box: its edges' distances from the parent's, points (top, right,
    /// bottom, left; `None` leaves an edge free).
    #[must_use]
    pub fn inset(
        self,
        top: Option<f32>,
        right: Option<f32>,
        bottom: Option<f32>,
        left: Option<f32>,
    ) -> Self {
        self.set(|s| s.inset = [top, right, bottom, left])
    }

    /// Takes the box out of the flow and out of every box around it: it is placed against the
    /// page by `inset` (`inset(Some(0.0), Some(0.0), Some(0.0), Some(0.0))` is the whole page, the
    /// ground of a dialog), or beside an element ([`Element::anchor`]), and it is drawn over
    /// everything else, cut by nothing that holds it, and the first the pointer finds. A menu that
    /// drops from a button, a dialog, a toast. What it is inside still holds it for the rest: it is
    /// as faint as that is, hidden with it, and its keys go out through it.
    #[must_use]
    pub fn overlay(self) -> Self {
        self.set(|s| s.overlay = true)
    }

    /// An overlay beside the element named `to`, on `side` of it, its start edge lined up with the
    /// element's ([`Element::anchor_align`], [`Element::anchor_gap`]); where the page has not the
    /// room for it there, it goes on the other side, and it is moved along that side to stay on
    /// the page. Where `to` is not on the page, it stands where its `inset` puts it.
    #[must_use]
    pub fn anchor(self, to: Id, side: Side) -> Self {
        self.set(|s| {
            s.overlay = true;
            s.anchor = Some(Anchor {
                to,
                side,
                align: Align::Start,
                gap: 0.0,
            });
        })
    }

    /// How far an anchored overlay stands from its element, points.
    #[must_use]
    pub fn anchor_gap(self, points: f32) -> Self {
        self.set(|s| {
            if let Some(anchor) = &mut s.anchor {
                anchor.gap = points;
            }
        })
    }

    /// What an anchored overlay lines up with along the side of its element: its start (the
    /// left, or the top), its middle, or its end.
    #[must_use]
    pub fn anchor_align(self, align: Align) -> Self {
        self.set(|s| {
            if let Some(anchor) = &mut s.anchor {
                anchor.align = align;
            }
        })
    }

    /// Nothing is drawn outside the box.
    #[must_use]
    pub fn clip(self) -> Self {
        self.set(|s| s.overflow = Overflow::Hidden)
    }

    /// A box whose content can be bigger than itself and is scrolled behind it, down and across:
    /// nothing is drawn outside the box. How far it is scrolled is the host's, kept by the box's
    /// name ([`Element::id`]: one with none does not scroll), moved by the wheel, the trackpad and
    /// the scroll bars as the host's system moves them ([`crate::Scrolls`]). The page is told where
    /// it stands ([`crate::Event::Scrolled`], [`crate::Extent::offset`]) and asks for it to go
    /// somewhere ([`crate::ScrollTo`]).
    #[must_use]
    pub fn scroll(self) -> Self {
        self.set(|s| s.overflow = Overflow::Scroll)
    }

    /// A box that scrolls ([`Element::scroll`]) that the host lets be zoomed: pinched, by the
    /// command key with the wheel, by a double tap with two fingers ([`crate::Scrolls`]). What is
    /// inside it is laid out larger as the person zooms, a text in a larger face and a picture in
    /// a larger size, and the box stays the size it was given.
    #[must_use]
    pub fn zoom(self, zoom: crate::Zoom) -> Self {
        self.set(|s| s.zoom = Some(zoom))
    }

    /// A box that scrolls ([`Element::scroll`]) that goes with the others that say the same `name`:
    /// where one of them is scrolled (by the wheel, the trackpad, a bar, an ask of the page, a caret
    /// brought into view) the others are scrolled to the same offset, at the same frame, along
    /// `axis`. Each stops at its own end and the offset of the one that was moved is kept, so that they
    /// stand together again where the one that was stopped can follow ([`crate::Scrolls`]). The
    /// elastic pull past an end is the one the person holds alone. A box with no id does not scroll,
    /// and so is linked to none.
    #[must_use]
    pub fn scroll_link(mut self, name: Id, axis: LinkAxis) -> Self {
        self.meta.scroll_link = Some(ScrollLink { name, axis });
        self
    }

    /// Scrolled `x` across and `y` down, as the host puts it before it lays the page out
    /// ([`crate::Scrolls::apply`]): for the engine's tests.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn scrolled(self, x: f32, y: f32) -> Self {
        self.set(|s| s.scroll = (x, y))
    }

    // --- Looks -------------------------------------------------------------------------

    /// The fill.
    #[must_use]
    pub fn bg(self, color: impl Into<Color>) -> Self {
        let color = color.into();
        self.set(|s| s.fill = Some(color))
    }

    /// A fill that runs from `from` to `to` along a line at `angle` degrees, as a style sheet's
    /// `linear-gradient`: `180` from the top down, `90` from the left to the right.
    #[must_use]
    pub fn bg_gradient(self, angle: f32, from: impl Into<Color>, to: impl Into<Color>) -> Self {
        let (from, to) = (from.into(), to.into());
        self.set(|s| {
            s.gradient = Some(Gradient {
                angle: if angle.is_finite() { angle } else { 180.0 },
                from,
                to,
            });
        })
    }

    /// A border of `width` points on every side, inside the box and over the fill. It takes room as
    /// the padding does: the width and the height a page gives are the box's, border included, and
    /// what the box holds stands inside it.
    #[must_use]
    pub fn border(self, width: f32, color: impl Into<Color>) -> Self {
        let color = color.into();
        self.set(|s| s.border = Some(Border { width, color }))
    }

    /// The same radius on every corner, points.
    #[must_use]
    pub fn rounded(self, points: f32) -> Self {
        self.set(|s| s.radii = [points; 4])
    }

    /// A radius per corner: top-left, top-right, bottom-right, bottom-left; points.
    #[must_use]
    pub fn rounded_corners(self, radii: [f32; 4]) -> Self {
        self.set(|s| s.radii = radii)
    }

    /// A ring `width` points thick around the box, over it and what is inside it, as a style sheet's
    /// `outline` and `outline-offset`: its inner edge is `offset` points from the box's edge, so that
    /// a ring of 2 at an offset of 1 stands 1 point clear of the box, and one at `-width` is inside
    /// the box's edge. Its corners follow the box's (the inner edge has the box's radius and the
    /// offset, and none where the box has none). It takes no room, answers no press and is cut by the
    /// boxes around the box, not by the box itself: a ring that is outside a box that clips is cut, so
    /// a page puts it inside. It is drawn at the end of its layer (the page, or the overlay it is
    /// in), over the boxes that come after its box, and at the end of the box around it that clips or
    /// scrolls where there is one, under that box's scroll bars.
    #[must_use]
    pub fn outline(self, width: f32, color: impl Into<Color>, offset: f32) -> Self {
        let color = color.into();
        self.set(|s| {
            s.outline = Some(Ring {
                width,
                color,
                offset,
            })
        })
    }

    /// A soft shadow `(dx, dy)` away from the box, blurred over `blur`; points.
    #[must_use]
    pub fn shadow(self, dx: f32, dy: f32, blur: f32, color: impl Into<Color>) -> Self {
        let color = color.into();
        self.set(|s| {
            s.shadow = Some(Shadow {
                dx,
                dy,
                blur,
                color,
            })
        })
    }

    // --- Text (set here, inherited by the elements inside) -------------------------------

    /// The size of the text, points.
    #[must_use]
    pub fn text_size(self, points: f32) -> Self {
        self.set(|s| s.text.size = Some(points))
    }

    /// The weight of the text on the CSS scale (400 regular, 700 bold).
    #[must_use]
    pub fn weight(self, weight: u16) -> Self {
        self.set(|s| s.text.weight = Some(weight))
    }

    /// The colour of the text.
    #[must_use]
    pub fn color(self, color: impl Into<Color>) -> Self {
        let color = color.into();
        self.set(|s| s.text.color = Some(color))
    }

    /// The face of the text.
    #[must_use]
    pub fn family(self, family: Family) -> Self {
        self.set(|s| s.text.family = Some(family))
    }

    /// Where a line stands in the box it is given: the start (default), the middle or the
    /// end.
    #[must_use]
    pub fn align(self, align: Align) -> Self {
        self.set(|s| s.text.align = Some(align))
    }

    /// The face's italic form for the text.
    #[must_use]
    pub fn italic(self) -> Self {
        self.set(|s| s.text.italic = Some(true))
    }

    /// A line under the words (as a link has).
    #[must_use]
    pub fn underline(self) -> Self {
        self.set(|s| s.text.underline = Some(true))
    }

    /// A line through the words (as a thing that was given up has).
    #[must_use]
    pub fn strike(self) -> Self {
        self.set(|s| s.text.strike = Some(true))
    }

    /// A line of text that does not fit ends in "…" and is the first to give its room away.
    #[must_use]
    pub fn truncate(self) -> Self {
        self.set(|s| s.truncate = Truncate::Ellipsis)
    }

    /// A line of text that does not fit is cut at the end of its box, where it thins out
    /// ([`crate::FadeEdge`]) in place of the "…", and is the first to give its room away.
    #[must_use]
    pub fn truncate_fade(self) -> Self {
        self.set(|s| s.truncate = Truncate::Fade)
    }

    /// The page of the file an image shows, from 1: a PDF has as many as it says, and the first is
    /// the one shown where this is not said. Has no effect on anything but an image.
    #[must_use]
    pub fn page(mut self, page: u32) -> Self {
        if let Kind::Image(image) = &mut self.kind {
            image.page = page.max(1);
        }
        self
    }

    /// Paints an image in `color`: the colour of every pixel of it is this one, and what is kept of
    /// the image is its opacity, so that a one-colour icon wears the colours of the theme. Has
    /// no effect on anything but an image.
    #[must_use]
    pub fn tint(mut self, color: impl Into<Color>) -> Self {
        if let Kind::Image(image) = &mut self.kind {
            image.tint = Some(color.into());
        }
        self
    }

    /// How much of the element shows with everything inside it, `0.0` to `1.0`: each thing it
    /// draws is that much fainter (where the things inside overlap, they show through each
    /// other). A panel that fades in, a part that is not to be used now.
    #[must_use]
    pub fn opacity(self, opacity: f32) -> Self {
        self.set(|s| s.opacity = Some(opacity))
    }

    /// A picture or a canvas turned `degrees` clockwise about its middle: its box does not move.
    #[must_use]
    pub fn turned(self, degrees: f32) -> Self {
        self.set(|s| s.turn = degrees)
    }

    /// A picture or a canvas drawn `scale` times as large, about its middle: its box does not
    /// change.
    #[must_use]
    pub fn scaled(self, scale: f32) -> Self {
        self.set(|s| s.scale = Some(scale))
    }

    /// The largest value a sparkline's top stands for (the greatest value of the line, with
    /// a little head room, when not set).
    #[must_use]
    pub fn peak(mut self, value: f32) -> Self {
        if let Kind::Spark(spark) = &mut self.kind {
            spark.peak = Some(value);
        }
        self
    }
}
