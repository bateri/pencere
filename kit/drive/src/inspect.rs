//! What a page of the kit says of itself to a reader that is not the person at the screen: each
//! element of the last page it laid out, named or not, with its place in the tree the page wrote,
//! its box, its room and style, and what a screen reader says of it; and what its frames cost (how
//! many elements the page is, how many pages the script made, how long the script took to answer each
//! call, how long laying out and painting took).
//!
//! **It is off until a host asks** ([`crate::Page::inspect`]): off, nothing is measured, no clock
//! is read and no model is built, so a page that is not looked at pays nothing for being lookable.
//! **No text of the page's own is in it** but the words a screen reader says, which are what
//! [`crate::Reading`] says (cleaned the same way); the value of a secret field is in no reading
//! and so in no model. The model stays where it is made: nothing of it is written to the program or
//! to the wire.
//!
//! The text ([`Inspection::text`]) is the same for every host, so that a reader sees the same page
//! whether a test, a terminal or a panel made it. What was measured is on lines of its own
//! (`cost`), so that the rest of the text is the same each time the page is.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::time::{Duration, Instant};

use pencere_sandbox::{Call, CallKind};
use pencere_view::{
    Accessible, AccessibleRole, Direction, Display, Element, FieldKind, Inspected, Laid, Length,
    Overflow, Placing, Rect, Size, State,
};

use crate::Placement;
use crate::reading::{WORDS_LIMIT, clean, reference, role_of, state_words};

/// How many calls of the script the page keeps for the next frame to say, at most: a host that
/// takes the page's calls but draws no frame does not make it keep them for ever.
const CALLS_KEPT: usize = 256;

/// How many frames that did something a page keeps the cost of, at most.
const FRAMES_KEPT: usize = 16;

/// How many of them the text says.
const FRAMES_SAID: usize = 4;

/// What a screen reader says for an element, as a reading says it.
#[derive(Clone, Debug, PartialEq)]
pub struct Said {
    /// The role, in the word a reading uses.
    pub role: String,
    /// The words, cleaned as a reading cleans them.
    pub words: String,
    /// What is said of its state, as a reading says it (`checked`, `value "…"`, `2 of 5`), empty
    /// where it says none.
    pub state: String,
    /// A press on it does something.
    pub pressable: bool,
}

/// An element of the page, and what is said for it.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// The element. Its box and clip, its room, border and scrolling are in the host's points, as
    /// the boxes of [`crate::Outline`] are; what its `placing` says is the page's, as the page wrote
    /// it.
    pub element: Inspected,
    pub said: Vec<Said>,
}

/// What a frame cost, as it was measured ([`crate::Page::inspect`]): a frame that drew the page it
/// had, made no page, laid nothing out and called no script but to settle it is not kept.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cost {
    /// How many elements the page the frame drew is made of.
    pub elements: usize,
    /// How many pages the script made since the frame before.
    pub renders: u64,
    /// The calls into the script that the frame took in since the frame before, in order, and how
    /// long it took to answer each.
    pub script: Vec<Call>,
    /// How many times the frame laid the page out.
    pub layouts: u32,
    /// How long laying out took, all of it together.
    pub layout: Duration,
    /// How long painting took.
    pub paint: Duration,
}

/// A page, as its last frame left it.
#[derive(Clone, Debug, PartialEq)]
pub struct Inspection {
    /// The page's size, in the host's points.
    pub size: Size,
    /// The elements in the order of the tree, what is going out last.
    pub items: Vec<Item>,
    /// How many elements the page is made of: what is going out is not counted.
    pub elements: usize,
    /// What the last frames that did something cost, the newest last.
    pub frames: Vec<Cost>,
}

impl Inspection {
    /// The page `laid` out from `page` (`None` where the tree it was laid out from is not at
    /// hand: no element says how it was placed), with what its frames cost. `laid` is the page
    /// where the page put it, with no loop's movement ([`Laid::without_loops`]).
    pub(crate) fn of(
        laid: &Laid,
        page: Option<&Element>,
        place: Placement,
        frames: Vec<Cost>,
    ) -> Self {
        let said = laid.accessible();
        let z = place.zoom;
        let stand_in = pencere_view::div();
        let items: Vec<Item> = laid
            .inspect(page.unwrap_or(&stand_in))
            .into_iter()
            .map(|mut element| {
                let said = element
                    .accessible
                    .iter()
                    .filter_map(|&at| said.get(at))
                    .map(said_of)
                    .collect();
                if page.is_none() {
                    element.placing = None;
                }
                element.rect = place.to_host(element.rect);
                element.clip = place.to_host(element.clip);
                element.padding = element.padding.map(|room| room * z);
                if let Some(border) = &mut element.border {
                    border.width *= z;
                }
                element.scroll = (element.scroll.0 * z, element.scroll.1 * z);
                if let Some(content) = &mut element.content {
                    *content = Size::new(content.w * z, content.h * z);
                }
                Item { element, said }
            })
            .collect();
        let elements = items.iter().filter(|item| !item.element.exiting).count();
        let scale = laid.scale.max(f32::MIN_POSITIVE);
        Self {
            size: Size::new(laid.size.w / scale * z, laid.size.h / scale * z),
            items,
            elements,
            frames,
        }
    }

    /// The model written for a reader to read: a line for the page, then a line for each element,
    /// indented by its depth in the tree, with what is said for it under it, and last, on lines of
    /// their own, what the last frames cost.
    pub fn text(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "page {}x{} · {} elements",
            number(self.size.w),
            number(self.size.h),
            self.elements
        );
        let mut going = false;
        for item in &self.items {
            let element = &item.element;
            if element.exiting && !going {
                going = true;
                out.push_str("going out:\n");
            }
            let depth = element.path.as_ref().map_or(1, Vec::len);
            let pad = "  ".repeat(depth);
            let _ = writeln!(out, "{pad}{}", describe(element));
            for said in &item.said {
                let _ = write!(out, "{pad}  says {} {}", said.role, quoted(&said.words));
                if !said.state.is_empty() {
                    let _ = write!(out, " ({})", said.state);
                }
                out.push('\n');
            }
        }
        let shown = self.frames.len().saturating_sub(FRAMES_SAID);
        for cost in self.frames.iter().skip(shown) {
            let _ = writeln!(out, "cost {}", cost.line());
        }
        out
    }
}

impl Cost {
    /// The frame in a line: what it drew and what it took.
    fn line(&self) -> String {
        let mut line = format!(
            "{} elements · {} {} · {} {}",
            self.elements,
            self.renders,
            if self.renders == 1 {
                "render"
            } else {
                "renders"
            },
            self.layouts,
            if self.layouts == 1 {
                "layout"
            } else {
                "layouts"
            },
        );
        let _ = write!(
            line,
            " · layout {} · paint {}",
            millis(self.layout),
            millis(self.paint)
        );
        if self.script.is_empty() {
            line.push_str(" · script none");
        } else {
            let total: Duration = self.script.iter().map(|call| call.took).sum();
            let _ = write!(line, " · script {}", millis(total));
            let mut parts: Vec<String> = Vec::new();
            for kind in [
                CallKind::Render,
                CallKind::Event,
                CallKind::Message,
                CallKind::Settle,
                CallKind::Room,
            ] {
                let some: Vec<&Call> = self
                    .script
                    .iter()
                    .filter(|call| call.kind == kind)
                    .collect();
                if !some.is_empty() {
                    let took: Duration = some.iter().map(|call| call.took).sum();
                    parts.push(format!(
                        "{} {} {}",
                        some.len(),
                        kind_word(kind),
                        millis(took)
                    ));
                }
            }
            let _ = write!(line, " ({})", parts.join(", "));
        }
        line
    }
}

/// What a page keeps of the cost of its frames while it is looked at ([`crate::Page::inspect`]):
/// every read of the clock is made here and only while it is on.
#[derive(Default)]
pub(crate) struct Meter {
    pub(crate) on: bool,
    /// The calls of the script not yet said by a frame.
    calls: Vec<Call>,
    /// How many pages the module had made at the last frame.
    renders: u64,
    /// How many times, and for how long, the frame in the making laid out and painted.
    layouts: u32,
    layout: Duration,
    paint: Duration,
    kept: VecDeque<Cost>,
    /// How many times the clock was read: what a test counts.
    #[cfg(test)]
    pub(crate) reads: usize,
}

impl Meter {
    /// Looks, or no more; `renders` is how many pages the module has made so far.
    pub(crate) fn set(&mut self, on: bool, renders: u64) {
        if self.on != on {
            *self = Self {
                on,
                renders,
                ..Self::default()
            };
        }
    }

    /// The calls the module timed, kept for the frame that says them.
    pub(crate) fn take(&mut self, calls: Vec<Call>) {
        if self.on {
            self.calls.extend(calls);
            let over = self.calls.len().saturating_sub(CALLS_KEPT);
            self.calls.drain(..over);
        }
    }

    /// The clock, where it is looked at: the time a stretch began.
    pub(crate) fn began(&mut self) -> Option<Instant> {
        if !self.on {
            return None;
        }
        #[cfg(test)]
        {
            self.reads += 1;
        }
        Some(Instant::now())
    }

    /// A layout that began at `began` is done.
    pub(crate) fn laid(&mut self, began: Option<Instant>) {
        if let Some(began) = began {
            self.layouts += 1;
            self.layout += began.elapsed();
        }
    }

    /// The painting that began at `began` is done.
    pub(crate) fn painted(&mut self, began: Option<Instant>) {
        if let Some(began) = began {
            self.paint += began.elapsed();
        }
    }

    /// A frame begins: what it costs is counted from nothing.
    pub(crate) fn frame_begins(&mut self) {
        self.layouts = 0;
        self.layout = Duration::ZERO;
        self.paint = Duration::ZERO;
    }

    /// A frame of a page of `elements` is drawn, and the module has made `renders` pages so far.
    pub(crate) fn frame_ends(&mut self, renders: u64, elements: impl FnOnce() -> usize) {
        if !self.on {
            return;
        }
        let made = renders.saturating_sub(self.renders);
        self.renders = renders;
        // The module is asked to settle against the layout at every frame, so a call to settle is
        // not work of the page's: a frame that made no page, laid nothing out and called nothing
        // but that is the frame of a fade or of a page at rest.
        let worked = made > 0
            || self.layouts > 0
            || self.calls.iter().any(|call| call.kind != CallKind::Settle);
        if !worked {
            self.calls.clear();
            return;
        }
        if self.kept.len() == FRAMES_KEPT {
            self.kept.pop_front();
        }
        self.kept.push_back(Cost {
            elements: elements(),
            renders: made,
            script: std::mem::take(&mut self.calls),
            layouts: self.layouts,
            layout: self.layout,
            paint: self.paint,
        });
    }

    /// The frames kept, the newest last.
    pub(crate) fn frames(&self) -> Vec<Cost> {
        self.kept.iter().cloned().collect()
    }
}

fn said_of(item: &Accessible) -> Said {
    // A secret field's value is never said, whatever the page put in its state.
    let mut state: State = item.state.clone();
    if matches!(item.role, AccessibleRole::Field(FieldKind::Secret)) {
        state.value = None;
    }
    Said {
        role: role_of(item.role),
        words: clean(&item.words, WORDS_LIMIT),
        state: state_words(&state, &item.detail),
        pressable: item.pressable,
    }
}

/// A line for an element: what it is, its name, its box and what it is in.
fn describe(element: &Inspected) -> String {
    let mut line = String::new();
    if element.exiting {
        line.push_str("exiting ");
    }
    line.push_str(element.shape.name());
    if let Some(field) = element.field {
        line.push_str(match field {
            FieldKind::Line => " field",
            FieldKind::Lines => " field (lines)",
            FieldKind::Secret => " field (secret)",
        });
    }
    if let Some(id) = element.id {
        let _ = write!(line, " #{}", reference(id));
    }
    let Rect { x, y, w, h } = element.rect;
    let _ = write!(
        line,
        " {},{} {}x{}",
        number(x),
        number(y),
        number(w),
        number(h)
    );
    if cut(element.rect, element.clip) {
        let Rect { x, y, w, h } = element.clip;
        let _ = write!(
            line,
            " clipped to {},{} {}x{}",
            number(x),
            number(y),
            number(w),
            number(h)
        );
    }
    if element.padding != [0.0; 4] {
        let _ = write!(line, " padding {}", edges(element.padding));
    }
    if let Some(border) = element.border {
        let _ = write!(line, " border {}", number(border.width));
    }
    if let Some(content) = element.content {
        let _ = write!(
            line,
            " scrolls {},{} of {}x{}",
            number(element.scroll.0),
            number(element.scroll.1),
            number(content.w),
            number(content.h)
        );
    }
    if element.hidden {
        line.push_str(" hidden");
    }
    if element.overlay {
        line.push_str(" overlay");
    }
    if let Some(placing) = &element.placing {
        line.push_str(&placed(placing));
    }
    line
}

/// What the page wrote of how an element is placed, where it is not what a style says by default.
fn placed(placing: &Placing) -> String {
    let mut out = String::new();
    let mut put = |words: String| {
        out.push_str(" · ");
        out.push_str(&words);
    };
    if placing.display == Display::Grid {
        put("grid".to_owned());
    } else if placing.display == Display::None {
        put("display none".to_owned());
    } else if placing.direction == Direction::Row {
        put("row".to_owned());
    }
    if placing.wrap {
        put("wrap".to_owned());
    }
    if placing.absolute {
        put("absolute".to_owned());
    }
    for (name, length) in [
        ("w", placing.width),
        ("h", placing.height),
        ("min-w", placing.min_width),
        ("min-h", placing.min_height),
        ("max-w", placing.max_width),
        ("max-h", placing.max_height),
    ] {
        match length {
            Length::Auto => {}
            Length::Px(points) => put(format!("{name} {}", number(points))),
            Length::Percent(share) => put(format!("{name} {}%", number(share))),
        }
    }
    if placing.margin != [0.0; 4] {
        put(format!("margin {}", edges(placing.margin)));
    }
    if placing.gap != (0.0, 0.0) {
        put(format!(
            "gap {},{}",
            number(placing.gap.0),
            number(placing.gap.1)
        ));
    }
    if placing.grow != 0.0 {
        put(format!("grow {}", number(placing.grow)));
    }
    if placing.inset.iter().any(Option::is_some) {
        let side = |at: Option<f32>| at.map_or_else(|| "-".to_owned(), number);
        let [top, right, bottom, left] = placing.inset;
        put(format!(
            "inset {},{},{},{}",
            side(top),
            side(right),
            side(bottom),
            side(left)
        ));
    }
    match placing.overflow {
        Overflow::Visible => {}
        Overflow::Hidden => put("clips".to_owned()),
        Overflow::Scroll => put("scrolls".to_owned()),
    }
    out
}

/// Whether `clip` cuts `rect`: some of it is not in `clip`.
fn cut(rect: Rect, clip: Rect) -> bool {
    clip.x > rect.x + 0.01
        || clip.y > rect.y + 0.01
        || clip.right() < rect.right() - 0.01
        || clip.bottom() < rect.bottom() - 0.01
}

/// Top, right, bottom, left: one number where all four are the same.
fn edges(sides: [f32; 4]) -> String {
    let [top, right, bottom, left] = sides;
    if top == right && right == bottom && bottom == left {
        number(top)
    } else {
        format!(
            "{},{},{},{}",
            number(top),
            number(right),
            number(bottom),
            number(left)
        )
    }
}

/// A length in points, as short as it can be said: no fraction where there is none, two places
/// where there is.
pub(crate) fn number(value: f32) -> String {
    // What rounds to nothing is nothing, not `-0`.
    if value.abs() < 0.005 {
        "0".to_owned()
    } else if value.is_finite() && (value - value.round()).abs() < 0.005 {
        format!("{value:.0}")
    } else {
        let text = format!("{value:.2}");
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    }
}

fn millis(took: Duration) -> String {
    format!("{:.2} ms", took.as_secs_f64() * 1000.0)
}

fn kind_word(kind: CallKind) -> &'static str {
    match kind {
        CallKind::Render => "render",
        CallKind::Settle => "settle",
        CallKind::Event => "event",
        CallKind::Message => "message",
        CallKind::Room => "room",
    }
}

fn quoted(words: &str) -> String {
    format!("\"{}\"", words.replace('\n', " "))
}
