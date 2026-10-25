//! The page interface (`wit/view.wit`) and the two crossings it asks for: what a module hands
//! out is read into the engine's own types — checked, because the module is not trusted — and
//! what the engine tells a module is put into the interface's.

use std::sync::Arc;

use pencere_view::wire::{
    self, Direction, Display, Fit, Image, Items, Justify, Kind, Length, Meta, Node, Overflow,
    Spark, Style, TextStyle,
};
use pencere_view::{
    Align, Border, Button, Color, Cursor, Element, Event, Extent, Family, Id, Input, Key, Keyboard,
    Laid, Menu, MenuEntry, Mods, Point, Rect, Reply, Rgba, Role, Shadow, Size, Source, Track,
};
use pencere_wire::bundle::{LUA_MEDIA_TYPE, is_path};
use pencere_wire::json::unescaped_len;
use wasmtime::Store;
use wasmtime::component::{Component, HasSelf, Linker};

use crate::images::Assets;
use crate::luau::Script;
use crate::module::{Frame, Runner};
use crate::sandbox::{self, Fault, Host, ModuleLimits, Program};

// The interface is read from the crate's own copy, `wit/view.wit`: a published crate cannot read
// outside itself. `kit/wit/view.wit` is the source; `make check` fails while they differ.
wasmtime::component::bindgen!({
    path: "wit",
    world: "view",
});

use pencere::view::types as wit;

// --- What the module asks of the layout ----------------------------------------------------

impl pencere::view::types::Host for Host {}

impl pencere::view::program::Host for Host {
    fn send(&mut self, message: String) -> pencere::view::program::Delivery {
        use pencere::view::program::Delivery;
        let (messages, bytes) = self.allowed;
        // A message is as long as the protocol counts it (escapes undone), as the terminal and
        // the program count it: a page that writes ASCII is not held to less than one that need
        // not.
        let size = unescaped_len(&message);
        if self.sent.len() >= messages || size > bytes {
            // Not kept, and the application closes the module when the call is over: these
            // are limits on the work of a call, not on what the program can take.
            self.overflowed = true;
            return Delivery::Full;
        }
        // The program may not have read what it was told before. Not an error: the module
        // keeps the message and is called again when the program has read.
        if !self.program.may_say(size) {
            return Delivery::Full;
        }
        self.sent.push(message);
        Delivery::Accepted
    }
}

impl pencere::view::diagnostics::Host for Host {
    fn fail(&mut self, message: String) {
        // The first thing a module says is why; what it says after is a consequence.
        if self.failed.is_none() {
            self.failed = Some(sandbox::failure_words(&message));
        }
    }
}

impl pencere::view::script::Host for Host {
    fn path(&mut self) -> String {
        self.script
            .as_ref()
            .map_or_else(String::new, |script| script.path.clone())
    }

    fn source(&mut self) -> Vec<u8> {
        self.script
            .as_ref()
            .map_or_else(Vec::new, |script| script.source.clone())
    }

    fn has_module(&mut self, path: String) -> bool {
        self.bundle_script(&path).is_some()
    }

    fn module(&mut self, path: String) -> Result<Vec<u8>, String> {
        let Some(bundle) = self.bundle_script(&path) else {
            return Err(format!(
                "{path:?} is not a script of the bundle (a file the manifest types {LUA_MEDIA_TYPE})"
            ));
        };
        let Some(bytes) = bundle.read(&path) else {
            return Err(format!("{path:?} cannot be read from the bundle"));
        };
        let total = self.given.saturating_add(bytes.len());
        if total > self.most {
            return Err(format!(
                "the scripts of the page are held to {} bytes together, and {path:?} would make them {total}",
                self.most
            ));
        }
        self.given = total;
        Ok(bytes.to_vec())
    }
}

impl Host {
    /// The bundle, if `path` is a script of it: a path the manifest could list, typed as Lua.
    /// A module's `require` is text a script wrote, so nothing but the manifest's word on a
    /// well-formed path is taken for a script.
    fn bundle_script(&self, path: &str) -> Option<Arc<dyn Assets>> {
        let bundle = self.script.as_ref()?.bundle.as_ref()?;
        let typed = is_path(path) && bundle.media_type(path).as_deref() == Some(LUA_MEDIA_TYPE);
        typed.then(|| Arc::clone(bundle))
    }
}

fn rect_of(rect: Rect) -> wit::Rect {
    wit::Rect {
        x: rect.x,
        y: rect.y,
        w: rect.w,
        h: rect.h,
    }
}

impl Host {
    /// The layout the module asks of, if the application has laid anything out yet.
    fn laid(&self) -> Option<&Laid> {
        self.laid.as_deref()
    }
}

impl pencere::view::layout::Host for Host {
    fn hit(&mut self, at: wit::Point) -> Option<u64> {
        let laid = self.laid()?;
        laid.hit(Point::new(at.x, at.y)).map(id_out)
    }

    fn ids_at(&mut self, at: wit::Point) -> Vec<u64> {
        self.laid().map_or_else(Vec::new, |laid| {
            laid.ids_at(Point::new(at.x, at.y))
                .into_iter()
                .map(id_out)
                .collect()
        })
    }

    fn bounds(&mut self, id: u64) -> Option<wit::Rect> {
        self.laid()?.bounds(Id::new(id)).map(rect_of)
    }

    fn scroll_extent(&mut self, id: u64) -> Option<wit::Extent> {
        let Extent { content, view } = self.laid()?.scroll_extent(Id::new(id))?;
        Some(wit::Extent { content, view })
    }

    fn focus_order(&mut self) -> Vec<u64> {
        self.laid().map_or_else(Vec::new, |laid| {
            laid.focus_order().into_iter().map(id_out).collect()
        })
    }

    fn path_of(&mut self, id: u64) -> Vec<u64> {
        self.laid().map_or_else(Vec::new, |laid| {
            laid.path_of(Id::new(id)).into_iter().map(id_out).collect()
        })
    }

    fn text_field(&mut self) -> Option<wit::Rect> {
        self.laid()?.text_field().map(rect_of)
    }
}

fn id_out(id: Id) -> u64 {
    id.number()
}

// --- What the engine tells the module -----------------------------------------------------

fn point_in(at: Point) -> wit::Point {
    wit::Point { x: at.x, y: at.y }
}

fn button_in(button: Button) -> wit::Button {
    match button {
        Button::Left => wit::Button::Left,
        Button::Right => wit::Button::Right,
    }
}

fn mods_in(mods: Mods) -> wit::Mods {
    wit::Mods {
        shift: mods.shift,
        control: mods.control,
        alt: mods.alt,
        command: mods.command,
    }
}

fn key_in(key: Key) -> wit::Key {
    match key {
        Key::Up => wit::Key::Up,
        Key::Down => wit::Key::Down,
        Key::Left => wit::Key::Left,
        Key::Right => wit::Key::Right,
        Key::PageUp => wit::Key::PageUp,
        Key::PageDown => wit::Key::PageDown,
        Key::Home => wit::Key::Home,
        Key::End => wit::Key::End,
        Key::Enter => wit::Key::Enter,
        Key::Escape => wit::Key::Escape,
        Key::Tab => wit::Key::Tab,
        Key::Character(c) => wit::Key::Character(c),
    }
}

fn event_in(event: Event) -> wit::Event {
    match event {
        Event::Input(Input::Move(at)) => wit::Event::PointerMove(point_in(at)),
        Event::Input(Input::Leave) => wit::Event::PointerLeave,
        Event::Input(Input::Press { at, button, mods }) => wit::Event::Press(wit::Press {
            at: point_in(at),
            button: button_in(button),
            mods: mods_in(mods),
        }),
        Event::Input(Input::Release { at, button }) => wit::Event::Release(wit::Release {
            at: point_in(at),
            button: button_in(button),
        }),
        Event::Input(Input::Scroll { at, dy }) => wit::Event::Scroll(wit::Scroll {
            at: point_in(at),
            dy,
        }),
        Event::Input(Input::Key { key, mods }) => wit::Event::Key(wit::KeyPress {
            key: key_in(key),
            mods: mods_in(mods),
        }),
        Event::Text(text) => wit::Event::Text(text),
        Event::MenuPick(index) => wit::Event::MenuPick(u32::try_from(index).unwrap_or(u32::MAX)),
    }
}

// --- What the module hands out ------------------------------------------------------------

/// A coordinate or a length from the module: finite and within what a page can mean. A page
/// that says NaN or a billion points is not trusted to lay anything out with it.
fn pt(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(-1.0e6, 1.0e6)
    } else {
        0.0
    }
}

fn opt_pt(value: Option<f32>) -> Option<f32> {
    value.map(pt)
}

fn share(value: f32) -> f32 {
    pt(value).max(0.0)
}

fn role_out(role: &wit::Role) -> Role {
    match role {
        wit::Role::Background => Role::Background,
        wit::Role::Foreground => Role::Foreground,
        wit::Role::Dim => Role::Dim,
        wit::Role::Accent => Role::Accent,
        wit::Role::Cursor => Role::Cursor,
        wit::Role::Success => Role::Success,
        wit::Role::Error => Role::Error,
        wit::Role::Info => Role::Info,
        wit::Role::Warning => Role::Warning,
        wit::Role::Raised(steps) => Role::Raised(*steps),
        wit::Role::Quiet => Role::Quiet,
        wit::Role::Border => Role::Border,
        wit::Role::Shadow => Role::Shadow,
        wit::Role::Selection => Role::Selection,
        wit::Role::Black => Role::Black,
        wit::Role::Red => Role::Red,
        wit::Role::Green => Role::Green,
        wit::Role::Yellow => Role::Yellow,
        wit::Role::Blue => Role::Blue,
        wit::Role::Magenta => Role::Magenta,
        wit::Role::Cyan => Role::Cyan,
        wit::Role::White => Role::White,
        wit::Role::BrightBlack => Role::BrightBlack,
        wit::Role::BrightRed => Role::BrightRed,
        wit::Role::BrightGreen => Role::BrightGreen,
        wit::Role::BrightYellow => Role::BrightYellow,
        wit::Role::BrightBlue => Role::BrightBlue,
        wit::Role::BrightMagenta => Role::BrightMagenta,
        wit::Role::BrightCyan => Role::BrightCyan,
        wit::Role::BrightWhite => Role::BrightWhite,
    }
}

fn rgba_out(value: &wit::Rgba) -> Rgba {
    Rgba::new(value.r, value.g, value.b, value.a)
}

fn color_out(color: &wit::Color) -> Color {
    let alpha = if color.alpha.is_finite() {
        color.alpha.clamp(0.0, 1.0)
    } else {
        1.0
    };
    let source = match &color.source {
        wit::Source::Role(role) => Source::Role(role_out(role)),
        wit::Source::Rgba(forms) => Source::Rgba {
            light: rgba_out(&forms.light),
            dark: forms.dark.as_ref().map(rgba_out),
        },
    };
    Color { source, alpha }
}

fn length_out(length: &wit::Length) -> Length {
    match length {
        wit::Length::Auto => Length::Auto,
        wit::Length::Px(points) => Length::Px(pt(*points)),
        wit::Length::Percent(percent) => Length::Percent(pt(*percent).clamp(0.0, 100.0)),
    }
}

fn track_out(track: &wit::Track) -> Track {
    match track {
        wit::Track::Px(points) => Track::Px(share(*points)),
        wit::Track::Fr(fr) => Track::Fr(share(*fr)),
        wit::Track::Range(wit::Range { min, fr }) => Track::Range {
            min: share(*min),
            fr: share(*fr),
        },
        wit::Track::Auto => Track::Auto,
    }
}

/// The three lists a frame says its words, values and tracks in (and a menu its titles in), and
/// what has been taken out of them so far.
///
/// An element names the part it means, and a part is read once for every element that names it:
/// what a module can make the application copy is counted here, where the copy is made, as the
/// sum of what the elements ask for, and not guessed from how large the lists are. The lists
/// themselves were taken in once, and are as large as the module's memory lets them be.
struct Lists<'a> {
    text: &'a str,
    numbers: &'a [f32],
    tracks: &'a [wit::Track],
    /// Bytes of parts read so far.
    read: usize,
    /// The most bytes a frame may say.
    most: usize,
}

impl<'a> Lists<'a> {
    /// The lists of a frame, if they are no larger together than a frame may be.
    fn new(
        text: &'a str,
        numbers: &'a [f32],
        tracks: &'a [wit::Track],
        most: usize,
    ) -> Result<Self, Fault> {
        let held = text
            .len()
            .saturating_add(numbers.len().saturating_mul(size_of::<f32>()))
            .saturating_add(tracks.len().saturating_mul(size_of::<Track>()));
        if held > most {
            return Err(Fault::Malformed(format!(
                "a page that says {held} bytes (at most {most})"
            )));
        }
        Ok(Self {
            text,
            numbers,
            tracks,
            read: 0,
            most,
        })
    }

    /// Where `part` stands in a list of `held` items, if it is within it.
    fn within(part: &wit::Slice, held: usize, what: &str) -> Result<std::ops::Range<usize>, Fault> {
        let start = usize::try_from(part.start).unwrap_or(usize::MAX);
        let len = usize::try_from(part.len).unwrap_or(usize::MAX);
        match start.checked_add(len) {
            Some(end) if end <= held => Ok(start..end),
            _ => Err(Fault::Malformed(format!(
                "{what}: {len} items from the {start}th of a list of {held}"
            ))),
        }
    }

    /// Counts `bytes` more against what a frame may say.
    fn charge(&mut self, bytes: usize) -> Result<(), Fault> {
        self.read = self.read.saturating_add(bytes);
        if self.read > self.most {
            return Err(Fault::Malformed(format!(
                "a page that says more than {} bytes",
                self.most
            )));
        }
        Ok(())
    }

    /// The words of `part`, no longer than `most` bytes.
    fn words(&mut self, part: &wit::Slice, most: usize, what: &str) -> Result<&'a str, Fault> {
        let range = Self::within(part, self.text.len(), what)?;
        if range.len() > most {
            return Err(Fault::Malformed(format!(
                "{what} of {} bytes (at most {most})",
                range.len()
            )));
        }
        self.charge(range.len())?;
        self.text.get(range).ok_or_else(|| {
            Fault::Malformed(format!(
                "{what} that does not begin and end between characters"
            ))
        })
    }

    /// The values of `part`, no more than `most`.
    fn values(&mut self, part: &wit::Slice, most: usize) -> Result<Vec<f32>, Fault> {
        let range = Self::within(part, self.numbers.len(), "a sparkline")?;
        if range.len() > most {
            return Err(Fault::Malformed(format!(
                "a sparkline of {} values (at most {most})",
                range.len()
            )));
        }
        self.charge(range.len().saturating_mul(size_of::<f32>()))?;
        Ok(self
            .numbers
            .get(range)
            .unwrap_or_default()
            .iter()
            .copied()
            .map(pt)
            .collect())
    }

    /// The tracks of `part`, no more than `most`.
    fn tracks(&mut self, part: &wit::Slice, most: usize) -> Result<Vec<Track>, Fault> {
        let range = Self::within(part, self.tracks.len(), "a grid")?;
        if range.len() > most {
            return Err(Fault::Malformed(format!(
                "a grid with {} tracks (at most {most})",
                range.len()
            )));
        }
        self.charge(range.len().saturating_mul(size_of::<Track>()))?;
        Ok(self
            .tracks
            .get(range)
            .unwrap_or_default()
            .iter()
            .map(track_out)
            .collect())
    }
}

fn edges_out(edges: &wit::Edges) -> [f32; 4] {
    [
        pt(edges.top),
        pt(edges.right),
        pt(edges.bottom),
        pt(edges.left),
    ]
}

fn family_out(family: wit::Family) -> Family {
    match family {
        wit::Family::System => Family::System,
        wit::Family::Terminal => Family::Terminal,
    }
}

fn align_out(align: wit::Align) -> Align {
    match align {
        wit::Align::Start => Align::Start,
        wit::Align::Center => Align::Center,
        wit::Align::End => Align::End,
    }
}

fn text_style_out(text: &wit::TextStyle) -> TextStyle {
    TextStyle {
        family: text.family.map(family_out),
        // A line of text set in a size that is not one is not drawn at all. Sizes go in half
        // points and weights in fifties: each new pair opens a font on the application's thread
        // and is kept, so a page that said a new one every frame would open fonts without end.
        size: text
            .size
            .map(|size| (pt(size).clamp(1.0, 256.0) * 2.0).round() / 2.0),
        weight: text
            .weight
            .map(|weight| (weight.clamp(100, 900) + 25) / 50 * 50),
        color: text.color.as_ref().map(color_out),
        align: text.align.map(align_out),
    }
}

fn style_out(
    style: &wit::Style,
    limits: &ModuleLimits,
    lists: &mut Lists<'_>,
) -> Result<Style, Fault> {
    Ok(Style {
        display: match style.display {
            wit::Display::Flex => Display::Flex,
            wit::Display::Grid => Display::Grid,
            wit::Display::None => Display::None,
        },
        direction: match style.direction {
            wit::Direction::Row => Direction::Row,
            wit::Direction::Column => Direction::Column,
        },
        wrap: style.wrap,
        absolute: style.absolute,
        inset: [
            opt_pt(style.inset.top),
            opt_pt(style.inset.right),
            opt_pt(style.inset.bottom),
            opt_pt(style.inset.left),
        ],
        width: length_out(&style.width),
        height: length_out(&style.height),
        min_width: length_out(&style.min_width),
        min_height: length_out(&style.min_height),
        max_width: length_out(&style.max_width),
        max_height: length_out(&style.max_height),
        padding: edges_out(&style.padding),
        margin: edges_out(&style.margin),
        gap: (pt(style.gap_x), pt(style.gap_y)),
        grow: share(style.grow),
        shrink: style.shrink.map(share),
        items: style.items.map(|items| match items {
            wit::Items::Start => Items::Start,
            wit::Items::Center => Items::Center,
            wit::Items::End => Items::End,
            wit::Items::Stretch => Items::Stretch,
        }),
        justify: style.justify.map(|justify| match justify {
            wit::Justify::Start => Justify::Start,
            wit::Justify::Center => Justify::Center,
            wit::Justify::End => Justify::End,
            wit::Justify::Between => Justify::Between,
        }),
        columns: lists.tracks(&style.columns, limits.tracks)?,
        rows: lists.tracks(&style.rows, limits.tracks)?,
        // A span past the tracks a grid may have would make the layout build an implicit grid
        // that wide, on the application's thread and outside the module's time.
        column_span: style
            .column_span
            .min(u16::try_from(limits.tracks).unwrap_or(u16::MAX)),
        overflow: match style.overflow {
            wit::Overflow::Visible => Overflow::Visible,
            wit::Overflow::Hidden => Overflow::Hidden,
            wit::Overflow::Scroll => Overflow::Scroll,
        },
        scroll: (pt(style.scroll_x), pt(style.scroll_y)),
        fill: style.fill.as_ref().map(color_out),
        border: style.border.as_ref().map(|border| Border {
            width: share(border.width),
            color: color_out(&border.color),
        }),
        radii: [
            share(style.radii.top_left),
            share(style.radii.top_right),
            share(style.radii.bottom_right),
            share(style.radii.bottom_left),
        ],
        shadow: style.shadow.as_ref().map(|shadow| Shadow {
            dx: pt(shadow.dx),
            dy: pt(shadow.dy),
            blur: share(shadow.blur),
            color: color_out(&shadow.color),
        }),
        text: text_style_out(&style.text),
        truncate: style.truncate,
    })
}

fn kind_out(
    content: &wit::Content,
    limits: &ModuleLimits,
    lists: &mut Lists<'_>,
) -> Result<Kind, Fault> {
    Ok(match content {
        wit::Content::Box => Kind::Box,
        wit::Content::Text(part) => {
            Kind::Text(lists.words(part, limits.text, "a line of text")?.to_owned())
        }
        wit::Content::Spark(spark) => Kind::Spark(Spark {
            values: lists.values(&spark.values, limits.values)?,
            color: color_out(&spark.color),
            peak: spark.peak.map(pt),
        }),
        wit::Content::Image(image) => Kind::Image(Image {
            src: lists
                .words(&image.src, limits.text, "an image path")?
                .to_owned(),
            fit: match image.fit {
                wit::Fit::Contain => Fit::Contain,
                wit::Fit::Cover => Fit::Cover,
            },
            tint: image.tint.as_ref().map(color_out),
            // Not a share of an image that is not one: all of it shows.
            opacity: if image.opacity.is_finite() {
                image.opacity.clamp(0.0, 1.0)
            } else {
                1.0
            },
        }),
    })
}

fn node_out(node: &wit::Node, limits: &ModuleLimits, lists: &mut Lists<'_>) -> Result<Node, Fault> {
    Ok(Node {
        style: style_out(&node.style, limits, lists)?,
        kind: kind_out(&node.content, limits, lists)?,
        meta: Meta {
            id: node.id.map(Id::new),
            pressable: node.pressable,
            focusable: node.focusable,
            text_field: node.text_field,
        },
        children: node.children,
    })
}

/// The tree a module handed out, if it is one within the limits.
fn frame_out(frame: wit::Frame, limits: &ModuleLimits) -> Result<Frame, Fault> {
    if frame.nodes.len() > limits.nodes {
        return Err(Fault::Malformed(format!(
            "a page of {} elements (at most {})",
            frame.nodes.len(),
            limits.nodes
        )));
    }
    let mut lists = Lists::new(
        &frame.text,
        &frame.numbers,
        &frame.tracks,
        limits.frame_bytes,
    )?;
    let nodes = frame
        .nodes
        .iter()
        .map(|node| node_out(node, limits, &mut lists))
        .collect::<Result<Vec<_>, _>>()?;
    let root = Element::unfold(nodes, limits.depth)
        .map_err(|why: wire::Malformed| Fault::Malformed(why.to_string()))?;
    Ok(Frame {
        root,
        animating: frame.animating,
        // A time that is not one (NaN, an infinity) asks for nothing.
        wake_at: frame.wake_at.filter(|at| at.is_finite()),
    })
}

fn reply_out(reply: wit::Reply, limits: &ModuleLimits) -> Result<Reply, Fault> {
    let text = |text: String| {
        if text.len() > limits.text.saturating_mul(16) {
            Err(Fault::Malformed(format!(
                "a text of {} bytes in a reply",
                text.len()
            )))
        } else {
            Ok(text)
        }
    };
    let menu = match reply.menu {
        None => None,
        Some(menu) => {
            if menu.entries.len() > limits.menu {
                return Err(Fault::Malformed(format!(
                    "a menu of {} entries (at most {})",
                    menu.entries.len(),
                    limits.menu
                )));
            }
            let mut titles = Lists::new(&menu.titles, &[], &[], limits.frame_bytes)?;
            let mut entries = Vec::with_capacity(menu.entries.len());
            for entry in &menu.entries {
                entries.push(match entry {
                    wit::MenuEntry::Separator => MenuEntry::Separator,
                    wit::MenuEntry::Item(item) => MenuEntry::Item {
                        title: titles
                            .words(&item.title, limits.text, "a menu title")?
                            .to_owned(),
                        key: item.key,
                        enabled: item.enabled,
                    },
                });
            }
            Some(Menu {
                at: Point::new(pt(menu.at.x), pt(menu.at.y)),
                entries,
            })
        }
    };
    Ok(Reply {
        redraw: reply.redraw,
        close: reply.close,
        menu,
        keyboard: reply.keyboard.map(|keyboard| match keyboard {
            wit::Keyboard::Page => Keyboard::Page,
            wit::Keyboard::Field => Keyboard::Field,
        }),
        copy: reply.copy.map(text).transpose()?,
        cursor: match reply.cursor {
            wit::Cursor::Arrow => Cursor::Arrow,
            wit::Cursor::Pointer => Cursor::Pointer,
            wit::Cursor::Text => Cursor::Text,
        },
    })
}

// --- Compiling and the version of the interface -------------------------------------------

/// The interface this crate speaks: the package of `wit/view.wit`.
const INTERFACE: &str = "pencere:view@1.0.0";

/// Compiles `bytes`, if that is done within `within`.
///
/// What a program sends can take as long as its author likes to compile, and the compiler cannot
/// be interrupted. So it runs on a thread of its own and is waited for this long: a component
/// that takes longer is refused, and the thread ends when the compiler does (its result is
/// dropped), which a component of the size that is run bounds.
fn compile(
    engine: &wasmtime::Engine,
    bytes: &[u8],
    within: std::time::Duration,
) -> Result<Component, Fault> {
    let (engine, bytes) = (engine.clone(), bytes.to_vec());
    let compiled = waited_for(within, move || {
        Component::new(&engine, &bytes).map_err(|why| format!("{why:#}"))
    })?;
    match compiled {
        Some(Ok(component)) => Ok(component),
        Some(Err(why)) => Err(Fault::Load(why)),
        None => Err(Fault::Load(format!(
            "compiling the component took longer than the {} seconds it is given",
            within.as_secs_f64()
        ))),
    }
}

/// What `work` gives, on a thread of its own, if it gives it within `within`; nothing if it does
/// not.
fn waited_for<T: Send + 'static>(
    within: std::time::Duration,
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<Option<T>, Fault> {
    let (done, result) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("view-compile".into())
        .spawn(move || {
            let _ = done.send(work());
        })
        .map_err(|_| Fault::Load("could not start a thread to compile the component".into()))?;
    match result.recv_timeout(within) {
        Ok(given) => Ok(Some(given)),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Ok(None),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            Err(Fault::Load("the compiler gave up on the component".into()))
        }
    }
}

/// The sentence for a component that is written against another version of the interface than
/// the one this terminal opens, if that is what it is: it names both (the specification asks for
/// as much). `None` for a component that names no version of the interface, or this one.
fn other_interface(engine: &wasmtime::Engine, component: &Component) -> Option<String> {
    let ty = component.component_type();
    let names = ty
        .imports(engine)
        .map(|(name, _)| name)
        .chain(ty.exports(engine).map(|(name, _)| name));
    let version = names
        .filter_map(|name| name.strip_prefix("pencere:view/"))
        .filter_map(|name| name.split_once('@').map(|(_, version)| version))
        .find(|version| Some(*version) != INTERFACE.strip_prefix("pencere:view@"))?;
    Some(format!(
        "this view is written against `pencere:view@{version}`; this terminal opens `{INTERFACE}`"
    ))
}

// --- The module --------------------------------------------------------------------------

/// A module of the page interface that is running: its store, and the budget its calls have.
pub(crate) struct World {
    store: Store<Host>,
    view: View,
    limits: ModuleLimits,
}

impl World {
    /// Compiles `bytes` (a component, binary or text) and starts it. The compiling is slow —
    /// the caller is on a thread of its own — and is held to [`ModuleLimits::compile`].
    pub(crate) fn load(
        bytes: &[u8],
        limits: ModuleLimits,
        program: Arc<dyn Program>,
    ) -> Result<Self, Fault> {
        if bytes.len() > limits.component {
            return Err(Fault::Load(format!(
                "the component is {} bytes; the most that is run is {}",
                bytes.len(),
                limits.component
            )));
        }
        let engine = sandbox::engine()?;
        let component = compile(&engine, bytes, limits.compile)?;
        Self::start(engine.clone(), &component, limits, program, None).map_err(
            |fault| match fault {
                // A component that does not start because it speaks another version of the
                // interface is told so, in words, and not in the runtime's.
                Fault::Load(raw) => {
                    Fault::Load(other_interface(&engine, &component).unwrap_or(raw))
                }
                other => other,
            },
        )
    }

    /// Starts the interpreter on `script`: the component of [`crate::luau`] (compiled once for
    /// the process, so that the next page of a script starts at once) and the script it runs.
    pub(crate) fn load_script(
        script: Script,
        limits: ModuleLimits,
        program: Arc<dyn Program>,
    ) -> Result<Self, Fault> {
        if script.source.len() > limits.component {
            return Err(Fault::Load(format!(
                "the script is {} bytes; the most that is run is {}",
                script.source.len(),
                limits.component
            )));
        }
        let engine = sandbox::engine()?;
        let component = crate::luau::component(&engine)?;
        let mut world = Self::start(engine, &component, limits, program, Some(script))?;
        // The interpreter starts the script when it is first called. Call it now, under the time
        // a module has to start in: a script that does not start (it has an error, or it never
        // finishes) is found out here, and its first page is not charged for its start.
        let page = world.view.pencere_view_page();
        sandbox::call(&mut world.store, world.limits.start, |store| {
            page.call_settle(store)
        })?;
        Ok(world)
    }

    fn start(
        engine: wasmtime::Engine,
        component: &Component,
        limits: ModuleLimits,
        program: Arc<dyn Program>,
        script: Option<Script>,
    ) -> Result<Self, Fault> {
        let mut linker = Linker::<Host>::new(&engine);
        View::add_to_linker::<Host, HasSelf<Host>>(&mut linker, |host| host)
            .map_err(|why| Fault::Load(format!("{why:#}")))?;
        let mut store = sandbox::store(&engine, &limits, program, script);
        // The module's own start runs under the start budget; what it does not offer to the
        // page is found out here (an import nobody gave).
        let view = sandbox::call(&mut store, limits.start, |store| {
            View::instantiate(store, component, &linker)
        })
        .map_err(|fault| match fault {
            Fault::Trap(why) => Fault::Load(why),
            other => other,
        })?;
        Ok(Self {
            store,
            view,
            limits,
        })
    }
}

impl World {
    /// A call that sent more than a call may is the end of the module: what it sent is dropped.
    fn within_what_a_call_may_send(&mut self) -> Result<(), Fault> {
        let host = self.store.data_mut();
        if std::mem::take(&mut host.overflowed) {
            host.sent.clear();
            return Err(Fault::Flooded(format!(
                "at most {} messages of {} bytes in one call",
                self.limits.messages, self.limits.message
            )));
        }
        Ok(())
    }
}

impl Runner for World {
    fn render(&mut self, size: Size, now: f64, reduce_motion: bool) -> Result<Frame, Fault> {
        let page = self.view.pencere_view_page();
        let size = wit::Size {
            w: size.w,
            h: size.h,
        };
        let frame = sandbox::call(&mut self.store, self.limits.call, |store| {
            page.call_render(store, size, now, reduce_motion)
        })?;
        self.within_what_a_call_may_send()?;
        frame_out(frame, &self.limits)
    }

    fn settle(&mut self) -> Result<bool, Fault> {
        let page = self.view.pencere_view_page();
        let changed = sandbox::call(&mut self.store, self.limits.call, |store| {
            page.call_settle(store)
        })?;
        self.within_what_a_call_may_send()?;
        Ok(changed)
    }

    fn handle(&mut self, event: Event) -> Result<Reply, Fault> {
        let page = self.view.pencere_view_page();
        let event = event_in(event);
        let reply = sandbox::call(&mut self.store, self.limits.call, |store| {
            page.call_handle(store, &event)
        })?;
        self.within_what_a_call_may_send()?;
        reply_out(reply, &self.limits)
    }

    fn message(&mut self, text: String) -> Result<Reply, Fault> {
        let page = self.view.pencere_view_page();
        let reply = sandbox::call(&mut self.store, self.limits.call, |store| {
            page.call_message(store, &text)
        })?;
        self.within_what_a_call_may_send()?;
        reply_out(reply, &self.limits)
    }

    fn room(&mut self) -> Result<Reply, Fault> {
        let page = self.view.pencere_view_page();
        let reply = sandbox::call(&mut self.store, self.limits.call, |store| {
            page.call_room(store)
        })?;
        self.within_what_a_call_may_send()?;
        reply_out(reply, &self.limits)
    }

    fn take_sent(&mut self) -> Vec<String> {
        std::mem::take(&mut self.store.data_mut().sent)
    }

    fn lay_out(&mut self, laid: Arc<Laid>) {
        self.store.data_mut().laid = Some(laid);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::Unbounded;

    fn edges() -> wit::Edges {
        wit::Edges {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        }
    }

    /// A style that sets nothing.
    fn style() -> wit::Style {
        wit::Style {
            display: wit::Display::Flex,
            direction: wit::Direction::Column,
            wrap: false,
            absolute: false,
            inset: wit::Inset {
                top: None,
                right: None,
                bottom: None,
                left: None,
            },
            width: wit::Length::Auto,
            height: wit::Length::Auto,
            min_width: wit::Length::Auto,
            min_height: wit::Length::Auto,
            max_width: wit::Length::Auto,
            max_height: wit::Length::Auto,
            padding: edges(),
            margin: edges(),
            gap_x: 0.0,
            gap_y: 0.0,
            grow: 0.0,
            shrink: None,
            items: None,
            justify: None,
            columns: nothing(),
            rows: nothing(),
            column_span: 0,
            overflow: wit::Overflow::Visible,
            scroll_x: 0.0,
            scroll_y: 0.0,
            fill: None,
            border: None,
            radii: wit::Corners {
                top_left: 0.0,
                top_right: 0.0,
                bottom_right: 0.0,
                bottom_left: 0.0,
            },
            shadow: None,
            text: wit::TextStyle {
                family: None,
                size: None,
                weight: None,
                color: None,
                align: None,
            },
            truncate: false,
        }
    }

    fn node(content: wit::Content, children: u32) -> wit::Node {
        wit::Node {
            content,
            style: style(),
            id: None,
            pressable: false,
            focusable: false,
            text_field: false,
            children,
        }
    }

    /// No part of any list.
    fn nothing() -> wit::Slice {
        wit::Slice { start: 0, len: 0 }
    }

    /// What a module writes of a frame besides its elements: the lists they name the parts of.
    #[derive(Default)]
    struct Lists {
        text: String,
        numbers: Vec<f32>,
        tracks: Vec<wit::Track>,
    }

    impl Lists {
        fn words(&mut self, words: &str) -> wit::Slice {
            let start = self.text.len();
            self.text.push_str(words);
            wit::Slice {
                start: start as u32,
                len: words.len() as u32,
            }
        }

        fn values(&mut self, values: &[f32]) -> wit::Slice {
            let start = self.numbers.len();
            self.numbers.extend_from_slice(values);
            wit::Slice {
                start: start as u32,
                len: values.len() as u32,
            }
        }

        fn tracks(&mut self, tracks: &[wit::Track]) -> wit::Slice {
            let start = self.tracks.len();
            self.tracks.extend_from_slice(tracks);
            wit::Slice {
                start: start as u32,
                len: tracks.len() as u32,
            }
        }

        fn frame(self, nodes: Vec<wit::Node>) -> wit::Frame {
            wit::Frame {
                nodes,
                text: self.text,
                numbers: self.numbers,
                tracks: self.tracks,
                animating: false,
                wake_at: None,
            }
        }
    }

    fn frame(nodes: Vec<wit::Node>) -> wit::Frame {
        Lists::default().frame(nodes)
    }

    fn container(children: u32) -> wit::Node {
        node(wit::Content::Box, children)
    }

    #[test]
    fn a_page_within_the_limits_is_the_tree_it_says() {
        let mut row = container(1);
        row.id = Some(7);
        row.pressable = true;
        row.style.width = wit::Length::Px(120.0);
        row.style.fill = Some(wit::Color {
            source: wit::Source::Role(wit::Role::Raised(2)),
            alpha: 0.5,
        });
        let mut lists = Lists::default();
        row.style.columns = lists.tracks(&[
            wit::Track::Fr(1.0),
            wit::Track::Range(wit::Range { min: 10.0, fr: 2.0 }),
        ]);
        let words = lists.words("api");
        let mut page = lists.frame(vec![row, node(wit::Content::Text(words), 0)]);
        page.animating = true;
        let made = frame_out(page, &ModuleLimits::default()).expect("a page");
        assert!(made.animating);
        let nodes = made.root.fold();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].meta.id, Some(Id::new(7)));
        assert!(nodes[0].meta.pressable);
        assert_eq!(nodes[0].style.width, Length::Px(120.0));
        assert_eq!(nodes[0].style.fill, Some(Role::Raised(2).alpha(0.5)));
        assert_eq!(
            nodes[0].style.columns,
            vec![Track::Fr(1.0), Track::Range { min: 10.0, fr: 2.0 }]
        );
        assert_eq!(nodes[1].kind, Kind::Text("api".into()));
    }

    #[test]
    fn numbers_that_are_not_numbers_a_page_can_mean_are_made_ones_it_can() {
        let mut odd = container(0);
        odd.style.width = wit::Length::Px(f32::NAN);
        odd.style.height = wit::Length::Percent(1.0e9);
        odd.style.padding.top = f32::INFINITY;
        odd.style.margin.left = -1.0e30;
        odd.style.grow = f32::NEG_INFINITY;
        odd.style.text.size = Some(f32::NAN);
        odd.style.fill = Some(wit::Color {
            source: wit::Source::Role(wit::Role::Accent),
            alpha: f32::NAN,
        });
        odd.style.shadow = Some(wit::Shadow {
            dx: f32::NAN,
            dy: 1.0e30,
            blur: -5.0,
            color: wit::Color {
                source: wit::Source::Role(wit::Role::Shadow),
                alpha: 7.0,
            },
        });
        let made = frame_out(frame(vec![odd]), &ModuleLimits::default()).expect("a page");
        let style = &made.root.fold()[0].style;
        assert_eq!(style.width, Length::Px(0.0));
        assert_eq!(style.height, Length::Percent(100.0));
        assert_eq!(style.padding[0], 0.0);
        assert_eq!(style.margin[3], -1.0e6);
        assert_eq!(style.grow, 0.0);
        assert_eq!(style.text.size, Some(1.0));
        assert_eq!(style.fill, Some(Role::Accent.alpha(1.0)));
        let shadow = style.shadow.expect("a shadow");
        assert_eq!((shadow.dx, shadow.dy, shadow.blur), (0.0, 1.0e6, 0.0));
        assert_eq!(shadow.color, Role::Shadow.alpha(1.0));
    }

    #[test]
    fn the_roles_and_values_a_page_can_say_are_read_as_the_engines_own() {
        use wit::Role as W;
        // The palette in the interface's order is the engine's, colour for colour.
        let palette = [
            W::Black,
            W::Red,
            W::Green,
            W::Yellow,
            W::Blue,
            W::Magenta,
            W::Cyan,
            W::White,
            W::BrightBlack,
            W::BrightRed,
            W::BrightGreen,
            W::BrightYellow,
            W::BrightBlue,
            W::BrightMagenta,
            W::BrightCyan,
            W::BrightWhite,
        ];
        for (at, role) in palette.iter().enumerate() {
            assert_eq!(role_out(role), Role::ANSI[at]);
        }
        assert_eq!(role_out(&W::Selection), Role::Selection);
        let role = |tone, alpha| wit::Color {
            source: wit::Source::Role(tone),
            alpha,
        };
        assert_eq!(
            color_out(&role(W::BrightRed, 0.25)),
            Role::BrightRed.alpha(0.25)
        );
        // A value comes with both its forms, untouched, and its own opacity on top.
        let forms = |light: wit::Rgba, dark| wit::Color {
            source: wit::Source::Rgba(wit::RgbaForms { light, dark }),
            alpha: 2.0,
        };
        let ink = wit::Rgba {
            r: 1,
            g: 2,
            b: 3,
            a: 4,
        };
        let night = wit::Rgba {
            r: 250,
            g: 251,
            b: 252,
            a: 255,
        };
        assert_eq!(
            color_out(&forms(ink, Some(night))),
            Color {
                source: Source::Rgba {
                    light: Rgba::new(1, 2, 3, 4),
                    dark: Some(Rgba::rgb(250, 251, 252)),
                },
                alpha: 1.0,
            }
        );
        assert_eq!(
            color_out(&forms(ink, None)).source,
            Source::Rgba {
                light: Rgba::new(1, 2, 3, 4),
                dark: None,
            }
        );
    }

    #[test]
    fn a_page_past_any_limit_is_not_a_page() {
        let limits = ModuleLimits {
            nodes: 3,
            depth: 3,
            text: 8,
            tracks: 2,
            values: 2,
            ..ModuleLimits::default()
        };
        let refused =
            |page: wit::Frame| matches!(frame_out(page, &limits), Err(Fault::Malformed(_)));
        // Four elements where three are allowed.
        assert!(refused(frame(vec![
            container(3),
            container(0),
            container(0),
            container(0)
        ])));
        // Three deep is allowed and a fourth level is not (a chain of four, with room for them).
        let roomy = ModuleLimits {
            nodes: 10,
            ..limits.clone()
        };
        let chain = |n: u32| {
            (0..n)
                .map(|i| container(u32::from(i + 1 < n)))
                .collect::<Vec<_>>()
        };
        assert!(frame_out(frame(chain(3)), &roomy).is_ok());
        assert!(matches!(
            frame_out(frame(chain(4)), &roomy),
            Err(Fault::Malformed(_))
        ));
        // A line of text that long; a grid with that many tracks; a sparkline with that many values.
        let mut lists = Lists::default();
        let words = lists.words("123456789");
        assert!(refused(
            lists.frame(vec![node(wit::Content::Text(words), 0)])
        ));
        let mut lists = Lists::default();
        let mut grid = container(0);
        grid.style.columns = lists.tracks(&[wit::Track::Auto; 3]);
        assert!(refused(lists.frame(vec![grid])));
        let mut lists = Lists::default();
        let values = lists.values(&[1.0; 3]);
        let line = node(
            wit::Content::Spark(wit::Spark {
                values,
                color: wit::Color {
                    source: wit::Source::Role(wit::Role::Accent),
                    alpha: 1.0,
                },
                peak: None,
            }),
            0,
        );
        assert!(refused(lists.frame(vec![line])));
        // Children it has not got, children beyond the root, and nothing at all.
        assert!(refused(frame(vec![container(2), container(0)])));
        assert!(refused(frame(vec![container(0), container(0)])));
        assert!(refused(frame(Vec::new())));
    }

    #[test]
    fn a_reply_past_a_limit_is_refused_and_one_within_them_is_read() {
        let limits = ModuleLimits {
            menu: 2,
            text: 4,
            ..ModuleLimits::default()
        };
        let reply = |menu: Option<wit::Menu>, copy: Option<String>| wit::Reply {
            redraw: true,
            close: false,
            menu,
            keyboard: Some(wit::Keyboard::Field),
            copy,
            cursor: wit::Cursor::Pointer,
        };
        // A menu as a module writes it: the titles are one string and each entry names its part.
        // `None` is a line between entries.
        let menu = |entries: &[Option<&str>]| {
            let mut titles = String::new();
            let entries = entries
                .iter()
                .map(|entry| match entry {
                    None => wit::MenuEntry::Separator,
                    Some(title) => {
                        let start = titles.len() as u32;
                        titles.push_str(title);
                        wit::MenuEntry::Item(wit::MenuItem {
                            title: wit::Slice {
                                start,
                                len: title.len() as u32,
                            },
                            key: Some('x'),
                            enabled: true,
                        })
                    }
                })
                .collect();
            wit::Menu {
                at: wit::Point {
                    x: 1.0,
                    y: f32::NAN,
                },
                entries,
                titles,
            }
        };
        let read = reply_out(
            reply(Some(menu(&[Some("go"), None])), Some("id".into())),
            &limits,
        )
        .expect("a reply");
        assert_eq!(read.keyboard, Some(Keyboard::Field));
        assert_eq!(read.cursor, Cursor::Pointer);
        assert_eq!(read.copy.as_deref(), Some("id"));
        let menu_read = read.menu.expect("a menu");
        assert_eq!((menu_read.at.x, menu_read.at.y), (1.0, 0.0));
        assert!(matches!(
            menu_read.entries.as_slice(),
            [MenuEntry::Item { title, .. }, MenuEntry::Separator] if title == "go"
        ));
        assert!(
            reply_out(
                reply(Some(menu(&[Some("a"), Some("b"), Some("c")])), None),
                &limits
            )
            .is_err()
        );
        assert!(reply_out(reply(Some(menu(&[Some("too long")])), None), &limits).is_err());
        assert!(reply_out(reply(None, Some("x".repeat(65))), &limits).is_err());
        // A title that names a part past the titles, or one that ends inside a character.
        let mut past = menu(&[Some("go")]);
        past.titles.truncate(1);
        assert!(reply_out(reply(Some(past), None), &limits).is_err());
        let mut inside = menu(&[Some("é")]);
        if let wit::MenuEntry::Item(item) = &mut inside.entries[0] {
            item.title.len = 1;
        }
        assert!(reply_out(reply(Some(inside), None), &limits).is_err());
    }

    #[test]
    fn what_the_person_did_reaches_the_module_as_what_it_was() {
        let at = Point::new(3.0, 4.0);
        assert!(matches!(
            event_in(Event::Input(Input::Move(at))),
            wit::Event::PointerMove(wit::Point { x, y }) if (x, y) == (3.0, 4.0)
        ));
        assert!(matches!(
            event_in(Event::Input(Input::Press {
                at,
                button: Button::Right,
                mods: Mods {
                    shift: true,
                    control: false,
                    alt: true,
                    command: false
                },
            })),
            wit::Event::Press(wit::Press {
                button: wit::Button::Right,
                mods: wit::Mods {
                    shift: true,
                    alt: true,
                    control: false,
                    command: false
                },
                ..
            })
        ));
        assert!(matches!(
            event_in(Event::Input(Input::Key {
                key: Key::Character('ç'),
                mods: Mods::default()
            })),
            wit::Event::Key(wit::KeyPress {
                key: wit::Key::Character('ç'),
                ..
            })
        ));
        assert!(matches!(
            event_in(Event::Text("api".into())),
            wit::Event::Text(text) if text == "api"
        ));
        assert!(matches!(
            event_in(Event::MenuPick(usize::MAX)),
            wit::Event::MenuPick(u32::MAX)
        ));
    }

    #[test]
    fn what_a_module_says_to_its_program_is_kept_within_what_a_call_may_send() {
        use pencere::view::program::Host as _;
        let limits = ModuleLimits {
            messages: 2,
            message: 10,
            ..ModuleLimits::default()
        };
        let mut host = Host::new(&limits, Arc::new(Unbounded));
        host.send("{}".into());
        host.send(r#"{"a":1}"#.into());
        assert_eq!(host.sent, ["{}", r#"{"a":1}"#]);
        assert!(!host.overflowed);
        // A third in one call, or one that is too long: not kept, and the call is past its limit.
        host.send("{}".into());
        assert!(host.overflowed);
        assert_eq!(host.sent.len(), 2);
        let mut host = Host::new(&limits, Arc::new(Unbounded));
        host.send(r#"{"too":"long"}"#.into());
        assert!(host.overflowed);
        assert!(host.sent.is_empty());
    }

    #[test]
    fn a_message_is_as_long_as_the_protocol_counts_it_with_its_escapes_undone() {
        use pencere::view::program::Host as _;
        let limits = ModuleLimits {
            message: 20,
            ..ModuleLimits::default()
        };
        // Four escapes of two bytes each, and the braces and quotes: 16 bytes where the text is 32.
        let escaped = r#"{"a":"\u00e9\u00e9\u00e9\u00e9"}"#;
        assert!(escaped.len() > 20 && pencere_wire::json::unescaped_len(escaped) == 16);
        let mut host = Host::new(&limits, Arc::new(Unbounded));
        host.send(escaped.into());
        assert!(!host.overflowed);
        assert_eq!(host.sent.len(), 1);
        // The program is asked for room for the same number.
        struct Counted(std::sync::atomic::AtomicUsize);
        impl Program for Counted {
            fn may_say(&self, bytes: usize) -> bool {
                self.0.store(bytes, std::sync::atomic::Ordering::SeqCst);
                true
            }
            fn taken(&self, _bytes: usize) {}
        }
        let counted = Arc::new(Counted(std::sync::atomic::AtomicUsize::new(0)));
        let mut host = Host::new(&limits, counted.clone());
        host.send(escaped.into());
        assert_eq!(counted.0.load(std::sync::atomic::Ordering::SeqCst), 16);
        // Past the limit once the escapes are undone.
        let mut host = Host::new(&limits, Arc::new(Unbounded));
        host.send(r#"{"a":"\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9\u00e9"}"#.into());
        assert!(host.overflowed);
    }

    #[test]
    fn elements_that_name_one_large_part_over_and_over_say_it_each_time() {
        // What a hostile module does: one buffer, a hundred elements that all point at it. The
        // buffer was taken in once; what the elements ask for is counted each time.
        let limits = ModuleLimits {
            text: 1 << 20,
            values: 1 << 20,
            frame_bytes: 1000,
            ..ModuleLimits::default()
        };
        let mut lists = Lists::default();
        let words = lists.words(&"x".repeat(400));
        let reading = |lists: Lists, content: &dyn Fn() -> wit::Content, n: usize| {
            let mut nodes = vec![container(u32::try_from(n).unwrap())];
            nodes.extend((0..n).map(|_| node(content(), 0)));
            frame_out(lists.frame(nodes), &limits)
        };
        // Two elements say it twice: 800 bytes is within the 1000 a frame may say.
        assert!(reading(lists, &|| wit::Content::Text(words), 2).is_ok());
        // Three say 1200, and a hundred say forty thousand: refused at the third.
        for n in [3, 100] {
            let mut lists = Lists::default();
            let words = lists.words(&"x".repeat(400));
            let outcome = reading(lists, &|| wit::Content::Text(words), n);
            assert!(
                matches!(&outcome, Err(Fault::Malformed(why)) if why.contains("1000")),
                "{outcome:?}"
            );
        }
        // A sparkline's values, and the tracks of a grid, are counted by what they take.
        let mut lists = Lists::default();
        let values = lists.values(&[1.0; 200]);
        let color = wit::Color {
            source: wit::Source::Role(wit::Role::Accent),
            alpha: 1.0,
        };
        let line = || {
            wit::Content::Spark(wit::Spark {
                values,
                color,
                peak: None,
            })
        };
        assert!(reading(lists, &line, 2).is_err());
        let mut lists = Lists::default();
        let tracks = lists.tracks(&[wit::Track::Auto; 40]);
        let mut nodes = vec![container(30)];
        nodes.extend((0..30).map(|_| {
            let mut grid = container(0);
            grid.style.columns = tracks;
            grid
        }));
        assert!(frame_out(lists.frame(nodes), &limits).is_err());
    }

    #[test]
    fn a_part_that_is_not_in_its_list_or_not_between_characters_is_not_a_page() {
        let limits = ModuleLimits::default();
        let text = |page: Lists, part: wit::Slice| {
            frame_out(page.frame(vec![node(wit::Content::Text(part), 0)]), &limits)
        };
        let mut lists = Lists::default();
        let ok = lists.words("abc");
        assert!(text(lists, ok).is_ok());
        for part in [
            wit::Slice { start: 0, len: 4 },
            wit::Slice { start: 3, len: 1 },
            wit::Slice {
                start: u32::MAX,
                len: u32::MAX,
            },
        ] {
            let mut lists = Lists::default();
            lists.words("abc");
            assert!(
                matches!(text(lists, part), Err(Fault::Malformed(_))),
                "{part:?}"
            );
        }
        // "é" is two bytes: a part that begins or ends between them is not words.
        for part in [
            wit::Slice { start: 1, len: 1 },
            wit::Slice { start: 0, len: 1 },
        ] {
            let mut lists = Lists::default();
            lists.words("é");
            assert!(
                matches!(text(lists, part), Err(Fault::Malformed(why)) if why.contains("between characters")),
                "{part:?}"
            );
        }
        // Lists that are larger together than a frame may be are refused before any is read.
        let small = ModuleLimits {
            frame_bytes: 100,
            ..ModuleLimits::default()
        };
        let mut lists = Lists::default();
        lists.words(&"y".repeat(101));
        assert!(frame_out(lists.frame(vec![container(0)]), &small).is_err());
        let mut lists = Lists::default();
        lists.values(&[0.0; 26]);
        assert!(frame_out(lists.frame(vec![container(0)]), &small).is_err());
    }

    #[test]
    fn compiling_is_held_to_the_time_it_is_given() {
        // A compiler that is still at work when the time is up is not waited for; one that is
        // done within it is. (The real compiler of a small component can be done before a wait of
        // no time at all has begun, so the time is told apart with work that takes its own.)
        let slow = waited_for(std::time::Duration::from_millis(10), || {
            std::thread::sleep(std::time::Duration::from_secs(5));
        });
        assert!(matches!(slow, Ok(None)), "{slow:?}");
        let quick = waited_for(std::time::Duration::from_secs(5), || 7);
        assert!(matches!(quick, Ok(Some(7))), "{quick:?}");
    }

    #[test]
    fn a_module_of_another_version_of_the_interface_is_refused_in_words_that_name_both() {
        // A component that has an instance by the name the page interface had before.
        let old =
            b"(component (instance $page) (export \"pencere:view/page@0.5.0\" (instance $page)))";
        let refused = World::load(old, ModuleLimits::default(), Arc::new(Unbounded));
        match refused.err() {
            Some(Fault::Load(why)) => {
                assert!(
                    why.contains("written against `pencere:view@0.5.0`"),
                    "{why}"
                );
                assert!(why.contains("opens `pencere:view@1.0.0`"), "{why}");
            }
            other => panic!("{other:?}"),
        }
        // The interface this crate speaks is the package of its own `wit/view.wit`.
        let wit = include_str!("../wit/view.wit");
        assert!(
            wit.lines()
                .any(|line| line.trim() == format!("package {INTERFACE};")),
            "{INTERFACE}"
        );
        // A component that names none is not the one this is about: the runtime's words stand.
        let other = World::load(b"(component)", ModuleLimits::default(), Arc::new(Unbounded));
        assert!(matches!(other.err(), Some(Fault::Load(why)) if !why.contains("written against")));
    }

    /// A program that has room for so many bytes in all.
    struct Room(std::sync::atomic::AtomicUsize);

    impl Program for Room {
        fn may_say(&self, bytes: usize) -> bool {
            self.0
                .fetch_update(
                    std::sync::atomic::Ordering::SeqCst,
                    std::sync::atomic::Ordering::SeqCst,
                    |left| left.checked_sub(bytes),
                )
                .is_ok()
        }

        fn taken(&self, _bytes: usize) {}
    }

    #[test]
    fn a_program_without_the_room_makes_the_module_full_and_not_closed() {
        use pencere::view::program::{Delivery, Host as _};
        let mut host = Host::new(
            &ModuleLimits::default(),
            Arc::new(Room(std::sync::atomic::AtomicUsize::new(10))),
        );
        assert!(matches!(host.send("{}".into()), Delivery::Accepted));
        // Not kept, and the module is not at fault: it is told, and keeps the message.
        assert!(matches!(
            host.send(r#"{"a":1234567}"#.into()),
            Delivery::Full
        ));
        assert!(!host.overflowed);
        assert_eq!(host.sent, ["{}"]);
        // A smaller one still fits.
        assert!(matches!(host.send(r#"{"a":1}"#.into()), Delivery::Accepted));
        assert_eq!(host.sent.len(), 2);
    }
}
