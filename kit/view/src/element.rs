//! The page's elements and the chain that writes them: `div().row().gap(8.0).child(...)`.
//!
//! A page is a tree of six kinds of element — a **box** ([`div`]), a line of **text**
//! ([`text`]), a text of many looks ([`crate::rich`]), a small **sparkline** ([`spark`]), an
//! **image** ([`image`]) and a **canvas** of paths ([`canvas`]). Every value is in points. A box
//! lays
//! its children out as flexbox or as a grid (the way a style sheet would), paints a fill, a
//! border and a shadow, and sets the text of the boxes under it; a box is a column unless
//! told otherwise.

use crate::style::{
    Anchor, Border, Direction, Display, Gradient, Items, Justify, Length, Overflow, Shadow, Side,
    Style, Track,
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
    /// What the element is in words, for a person who does not see it ([`Element::label`]).
    pub label: Option<String>,
    /// How a change of the element moves instead of jumping ([`Element::motion`]).
    pub motion: Option<Motion>,
    /// What the host says of the element while the pointer rests on it ([`Element::tip`]).
    pub tip: Option<Tip>,
    /// What it is to a screen reader, where the page says ([`Element::role`]).
    pub role: Option<AccessibleRole>,
    /// Where it stands, to a screen reader ([`State`]).
    pub state: State,
    /// What the page can be told to do while the keyboard is on it or inside it.
    pub commands: Vec<Command>,
    /// The keyboard is on it: its commands, and those of the boxes around it, can be done.
    pub focused: bool,
    /// It can be picked up and moved: pressed and dragged, it is what is moved ([`crate::Ui`]).
    pub draggable: bool,
    /// What is moved can be put down on it.
    pub drop_target: bool,
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

    /// What the element is in words, for a person who does not see it: a screen reader says it.
    /// A button that is an icon alone says what it does; an element with none is read by the
    /// words inside it.
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
    /// checkbox, a switch, a tab, a link, a heading. (A text field is one by [`Element::field`],
    /// and is not made one here.)
    #[must_use]
    pub fn role(mut self, role: AccessibleRole) -> Self {
        if !matches!(role, AccessibleRole::Field(_)) {
            self.meta.role = Some(role);
        }
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

    /// A box whose content is bigger than itself and moves behind it: nothing is drawn
    /// outside the box, and the content is moved up by `offset` points.
    #[must_use]
    pub fn scroll_y(self, offset: f32) -> Self {
        self.set(|s| {
            s.overflow = Overflow::Scroll;
            s.scroll.1 = offset;
        })
    }

    /// The same, across: the content moves left behind the box by `offset` points.
    #[must_use]
    pub fn scroll_x(self, offset: f32) -> Self {
        self.set(|s| {
            s.overflow = Overflow::Scroll;
            s.scroll.0 = offset;
        })
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

    /// A border of `width` points inside the box, over the fill.
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
        self.set(|s| s.truncate = true)
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
