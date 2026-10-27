//! The page interface (`wit/view.wit`) and the two crossings it asks for: what a module hands
//! out is read into the engine's own types — checked, because the module is not trusted — and
//! what the engine tells a module is put into the interface's.

use std::collections::HashSet;
use std::sync::Arc;

use pencere_view::edit::{
    Caret, CaretShape, Change, CommentTokens, Decorate, Decoration, EditCommand, EditorAsk,
    EditorColors, EditorOptions, Folds, IndentRules, Keys, Look, Mark, Movement, Numbers, Origin,
    Range, Refusal, Tint, Underline, Whitespace, limits,
};
use pencere_view::wire::{
    self, Direction, Display, FieldKind, Fit, Image, Items, Justify, Kind, Length, Meta, Node,
    Overflow, Spark, Style, TextStyle,
};
use pencere_view::{
    AccessibleRole, Align, Anchor, Border, Button, Color, Command, Cursor, Ease, Element, Event,
    Extent, Family, Gradient, Id, Input, Key, Keyboard, Laid, Menu, MenuEntry, Mix, Mods, Motion,
    Path, Point, Rect, Reply, Rgba, Role, ScrollTo, Segment, Shadow, Shift, Shortcut, Side, Size,
    Source, Span, State, Stroke, Tip, Track,
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
        let Extent {
            content,
            view,
            offset,
        } = self.laid()?.scroll_extent(Id::new(id))?;
        let size = |size: Size| wit::Size {
            w: size.w,
            h: size.h,
        };
        Some(wit::Extent {
            content: size(content),
            view: size(view),
            offset: point_in(offset),
        })
    }

    fn scroll_to(&mut self, id: u64, x: Option<f32>, y: Option<f32>, animate: bool) {
        // What a page may ask before the application takes it: a page that asks without end is
        // not kept asking (the newest ask of a box is the one that counts).
        if self.asks.len() >= MOST_ASKS {
            return;
        }
        let at = |value: Option<f32>| {
            value
                .filter(|v| !v.is_nan())
                .map(|v| v.clamp(0.0, f32::MAX))
        };
        self.asks.push(ScrollTo {
            id: Id::new(id),
            x: at(x),
            y: at(y),
            animate,
        });
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

    fn drag_source(&mut self, at: wit::Point) -> Option<u64> {
        let laid = self.laid()?;
        laid.drag_source(Point::new(at.x, at.y)).map(id_out)
    }

    fn drop_target(&mut self, at: wit::Point) -> Option<u64> {
        let laid = self.laid()?;
        laid.drop_target(Point::new(at.x, at.y)).map(id_out)
    }

    fn editor_version(&mut self, editor: u64) -> Option<u64> {
        Some(self.texts.get(&Id::new(editor))?.version())
    }

    fn editor_length(&mut self, editor: u64) -> Option<u32> {
        u32::try_from(self.texts.get(&Id::new(editor))?.len_chars()).ok()
    }

    fn editor_lines(&mut self, editor: u64) -> Option<u32> {
        u32::try_from(self.texts.get(&Id::new(editor))?.len_lines()).ok()
    }

    fn editor_text(&mut self, editor: u64, from: u32, to: u32) -> Option<String> {
        let text = self.texts.get(&Id::new(editor))?;
        let (from, to) = (usize::try_from(from).ok()?, usize::try_from(to).ok()?);
        // More characters than a query answers with bytes are more bytes: refused before they are
        // copied; a range of fewer that comes to more bytes is refused after.
        if to.saturating_sub(from) > self.query_text {
            return None;
        }
        text.slice(from, to)
            .filter(|words| words.len() <= self.query_text)
    }

    fn line_start(&mut self, editor: u64, line: u32) -> Option<u32> {
        let text = self.texts.get(&Id::new(editor))?;
        u32::try_from(text.line_start(usize::try_from(line).ok()?)?).ok()
    }

    fn line_of(&mut self, editor: u64, at: u32) -> Option<u32> {
        let text = self.texts.get(&Id::new(editor))?;
        u32::try_from(text.line_of(usize::try_from(at).ok()?)?).ok()
    }
}

fn id_out(id: Id) -> u64 {
    id.number()
}

/// The most asks for a box to be scrolled the module keeps before the application takes them.
const MOST_ASKS: usize = 256;

// --- What the engine tells the module -----------------------------------------------------

fn point_in(at: Point) -> wit::Point {
    wit::Point { x: at.x, y: at.y }
}

fn button_in(button: Button) -> wit::Button {
    match button {
        Button::Left => wit::Button::Left,
        Button::Right => wit::Button::Right,
        Button::Middle => wit::Button::Middle,
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
        Key::Backspace => wit::Key::Backspace,
        Key::Delete => wit::Key::Delete,
        Key::Function(n) => wit::Key::Function(n),
        Key::Character(c) => wit::Key::Character(c),
    }
}

fn event_in(event: Event) -> wit::Event {
    match event {
        Event::Input(Input::Move(at)) => wit::Event::PointerMove(point_in(at)),
        Event::Input(Input::Leave) => wit::Event::PointerLeave,
        Event::Input(Input::Press {
            at,
            button,
            mods,
            count,
        }) => wit::Event::Press(wit::Press {
            at: point_in(at),
            button: button_in(button),
            mods: mods_in(mods),
            count,
        }),
        Event::Input(Input::Release { at, button }) => wit::Event::Release(wit::Release {
            at: point_in(at),
            button: button_in(button),
        }),
        Event::Input(Input::Scroll { at, dx, dy }) => wit::Event::Scroll(wit::Scroll {
            at: point_in(at),
            dx,
            dy,
        }),
        Event::Input(Input::Key { key, mods }) => wit::Event::Key(wit::KeyPress {
            key: key_in(key),
            mods: mods_in(mods),
        }),
        Event::Text { field, text } => wit::Event::Text(wit::FieldText {
            field: id_out(field),
            text,
        }),
        Event::Input(Input::KeyUp { key, mods }) => wit::Event::KeyUp(wit::KeyPress {
            key: key_in(key),
            mods: mods_in(mods),
        }),
        Event::Input(Input::Mods(mods)) => wit::Event::Mods(mods_in(mods)),
        Event::Focus(field) => wit::Event::Focus(field.map(id_out)),
        Event::MenuPick(index) => wit::Event::MenuPick(u32::try_from(index).unwrap_or(u32::MAX)),
        Event::Command(id) => wit::Event::Command(id_out(id)),
        Event::Files { at, paths } => wit::Event::Files(wit::DroppedFiles {
            at: point_in(at),
            paths,
        }),
        Event::Scrolled(boxes) => wit::Event::Scrolled(
            boxes
                .into_iter()
                .map(|(id, offset)| wit::Scrolled {
                    id: id_out(id),
                    offset: point_in(offset),
                })
                .collect(),
        ),
        Event::Pointer { input, place } => {
            let place = wit::Place {
                editor: id_out(place.editor),
                at: place_in(place.at),
            };
            let input = match input {
                Input::Press {
                    at,
                    button,
                    mods,
                    count,
                } => wit::PointerInput::Press(wit::Press {
                    at: point_in(at),
                    button: button_in(button),
                    mods: mods_in(mods),
                    count,
                }),
                Input::Release { at, button } => wit::PointerInput::Release(wit::Release {
                    at: point_in(at),
                    button: button_in(button),
                }),
                Input::Move(at) => wit::PointerInput::Move(point_in(at)),
                // Nothing else of the pointer's has a place: it is the page's as it is.
                other => return event_in(Event::Input(other)),
            };
            wit::Event::EditorPointer(wit::EditorPointer { input, place })
        }
        Event::Edit { editor, edited } => wit::Event::Edit(wit::Edited {
            editor: id_out(editor),
            version: edited.version,
            changes: edited
                .changes
                .into_iter()
                .map(|change| wit::Change {
                    from: place_in(change.from),
                    to: place_in(change.to),
                    insert: change.insert,
                })
                .collect(),
            origin: match edited.origin {
                Origin::Person => wit::Origin::Person,
                Origin::Compose => wit::Origin::Compose,
                Origin::Page => wit::Origin::Page,
                Origin::Undo => wit::Origin::Undo,
                Origin::Redo => wit::Origin::Redo,
                Origin::Load => wit::Origin::Load,
            },
        }),
        Event::Selection { editor, selection } => wit::Event::Selection(wit::EditorSelection {
            editor: id_out(editor),
            version: selection.version,
            ranges: selection
                .ranges
                .into_iter()
                .map(|range| wit::TextRange {
                    anchor: place_in(range.anchor),
                    head: place_in(range.head),
                })
                .collect(),
            main: place_in(selection.main),
            composing: selection.composing,
        }),
        Event::Viewport {
            editor,
            first,
            last,
        } => wit::Event::Viewport(wit::Viewport {
            editor: id_out(editor),
            first: place_in(first),
            last: place_in(last),
        }),
        Event::EditorRefused { editor, refusal } => wit::Event::EditorRefused(wit::EditorRefused {
            editor: id_out(editor),
            reason: match refusal {
                Refusal::Reversed => wit::Refusal::Reversed,
                Refusal::OutOfDocument => wit::Refusal::OutOfDocument,
                Refusal::Overlapping => wit::Refusal::Overlapping,
                Refusal::StaleVersion => wit::Refusal::StaleVersion,
                Refusal::FutureVersion => wit::Refusal::FutureVersion,
                Refusal::TooLarge => wit::Refusal::TooLarge,
                Refusal::TooManyDecorations => wit::Refusal::TooManyDecorations,
                Refusal::OutsideRange => wit::Refusal::OutsideRange,
                Refusal::BadPattern => wit::Refusal::BadPattern,
                Refusal::NoEditor => wit::Refusal::NoEditor,
            },
        }),
        Event::Found { editor, found } => wit::Event::Found(wit::Found {
            editor: id_out(editor),
            version: found.version,
            count: place_in(found.count),
            current: found.current.map(place_in),
            more: found.more,
        }),
    }
}

/// A place (a count of characters) as the interface carries it: a document holds fewer than four
/// billion of them.
fn place_in(at: usize) -> u32 {
    u32::try_from(at).unwrap_or(u32::MAX)
}

fn place_out(at: u32) -> usize {
    usize::try_from(at).unwrap_or(usize::MAX)
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

fn source_out(source: &wit::Source) -> Source {
    match source {
        wit::Source::Role(role) => Source::Role(role_out(role)),
        wit::Source::Rgba(forms) => Source::Rgba {
            light: rgba_out(&forms.light),
            dark: forms.dark.as_ref().map(rgba_out),
        },
    }
}

/// A share from the module, `0.0..=1.0`: one that is not a number is `otherwise`.
fn unit(value: f32, otherwise: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        otherwise
    }
}

fn color_out(color: &wit::Color) -> Color {
    Color {
        source: source_out(&color.source),
        // A mix by an amount that is not one is no mix.
        mix: color.mix.as_ref().map(|mix| Mix {
            toward: source_out(&mix.toward),
            amount: unit(mix.amount, 0.0),
        }),
        alpha: unit(color.alpha, 1.0),
    }
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
    spans: &'a [wit::Span],
    paths: &'a [wit::Path],
    segments: &'a [wit::Segment],
    commands: &'a [wit::Command],
    /// Bytes of parts read so far.
    read: usize,
    /// The most bytes a frame may say.
    most: usize,
}

impl<'a> Lists<'a> {
    /// The lists of a frame, if they are no larger together than a frame may be.
    fn new(frame: &'a wit::Frame, most: usize) -> Result<Self, Fault> {
        let lists = Self {
            text: &frame.text,
            numbers: &frame.numbers,
            tracks: &frame.tracks,
            spans: &frame.spans,
            paths: &frame.paths,
            segments: &frame.segments,
            commands: &frame.commands,
            read: 0,
            most,
        };
        let held = lists
            .text
            .len()
            .saturating_add(size_of_val(lists.numbers))
            .saturating_add(lists.tracks.len().saturating_mul(size_of::<Track>()))
            .saturating_add(size_of_val(lists.spans))
            .saturating_add(size_of_val(lists.paths))
            .saturating_add(size_of_val(lists.segments))
            .saturating_add(size_of_val(lists.commands));
        if held > most {
            return Err(Fault::Malformed(format!(
                "a page that says {held} bytes (at most {most})"
            )));
        }
        Ok(lists)
    }

    /// The lists a menu says its titles in: its titles alone.
    fn titles(text: &'a str, most: usize) -> Result<Self, Fault> {
        if text.len() > most {
            return Err(Fault::Malformed(format!(
                "a menu that says {} bytes (at most {most})",
                text.len()
            )));
        }
        Ok(Self {
            text,
            numbers: &[],
            tracks: &[],
            spans: &[],
            paths: &[],
            segments: &[],
            commands: &[],
            read: 0,
            most,
        })
    }

    /// The items `part` names of `list`, no more than `most`, each counted at its size.
    fn items<T>(
        &mut self,
        list: &'a [T],
        part: &wit::Slice,
        most: usize,
        what: &str,
    ) -> Result<&'a [T], Fault> {
        let range = Self::within(part, list.len(), what)?;
        if range.len() > most {
            return Err(Fault::Malformed(format!(
                "{what} of {} items (at most {most})",
                range.len()
            )));
        }
        self.charge(range.len().saturating_mul(size_of::<T>()))?;
        Ok(list.get(range).unwrap_or_default())
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
        italic: text.italic,
        underline: text.underline,
        strike: text.strike,
        color: text.color.as_ref().map(color_out),
        align: text.align.map(align_out),
    }
}

fn side_out(side: wit::Side) -> Side {
    match side {
        wit::Side::Below => Side::Below,
        wit::Side::Above => Side::Above,
        wit::Side::Right => Side::Right,
        wit::Side::Left => Side::Left,
    }
}

/// An angle of a gradient from the module, in degrees: one that is not a number is the default of
/// a style sheet (from the top down).
fn angle_out(angle: f32) -> f32 {
    if angle.is_finite() {
        angle.rem_euclid(360.0)
    } else {
        180.0
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
        overlay: style.overlay,
        anchor: style.anchor.map(|anchor| Anchor {
            to: Id::new(anchor.to),
            side: side_out(anchor.side),
            align: align_out(anchor.align),
            gap: pt(anchor.gap),
        }),
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
        // How far a box is scrolled is the application's: put in when it lays the page out.
        scroll: (0.0, 0.0),
        fill: style.fill.as_ref().map(color_out),
        gradient: style.gradient.as_ref().map(|gradient| Gradient {
            angle: angle_out(gradient.angle),
            from: color_out(&gradient.start),
            to: color_out(&gradient.end),
        }),
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
        // Not a share of an element that is not one: all of it shows.
        opacity: Some(unit(style.opacity, 1.0)).filter(|opacity| *opacity < 1.0),
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
        }),
        wit::Content::Rich(part) => {
            let spans = lists.items(lists.spans, part, limits.parts, "a text of many looks")?;
            let mut out = Vec::with_capacity(spans.len());
            for span in spans {
                out.push(span_out(span, limits, lists)?);
            }
            Kind::Rich(out)
        }
        wit::Content::Canvas(part) => {
            let paths = lists.items(lists.paths, part, limits.parts, "a canvas")?;
            let mut out = Vec::with_capacity(paths.len());
            for path in paths {
                let segments =
                    lists.items(lists.segments, &path.segments, limits.values, "a path")?;
                out.push(Path {
                    segments: segments.iter().map(segment_out).collect(),
                    fill: path.fill.as_ref().map(color_out),
                    stroke: path.stroke.as_ref().map(|stroke| Stroke {
                        width: share(stroke.width),
                        color: color_out(&stroke.color),
                    }),
                });
            }
            Kind::Canvas(out)
        }
    })
}

fn span_out(span: &wit::Span, limits: &ModuleLimits, lists: &mut Lists<'_>) -> Result<Span, Fault> {
    Ok(Span {
        text: lists
            .words(&span.text, limits.text, "a part of a text")?
            .to_owned(),
        color: span.color.as_ref().map(color_out),
        // Weights in fifties, as an element's are: each opens a font on the application's thread.
        weight: span
            .weight
            .map(|weight| (weight.clamp(100, 900) + 25) / 50 * 50),
        italic: span.italic,
        underline: span.underline,
        strike: span.strike,
        family: span.family.map(family_out),
        background: span.background.as_ref().map(color_out),
        link: span.link.map(Id::new),
    })
}

fn segment_out(segment: &wit::Segment) -> Segment {
    let at = |point: &wit::Point| Point::new(pt(point.x), pt(point.y));
    match segment {
        wit::Segment::MoveTo(point) => Segment::Move(at(point)),
        wit::Segment::LineTo(point) => Segment::Line(at(point)),
        wit::Segment::QuadTo(quad) => Segment::Quad {
            control: at(&quad.control),
            to: at(&quad.to),
        },
        wit::Segment::CubicTo(cubic) => Segment::Cubic {
            a: at(&cubic.a),
            b: at(&cubic.b),
            to: at(&cubic.to),
        },
        wit::Segment::Close => Segment::Close,
    }
}

fn accessible_role_out(role: wit::AccessibleRole) -> AccessibleRole {
    match role {
        wit::AccessibleRole::Button => AccessibleRole::Button,
        wit::AccessibleRole::Text => AccessibleRole::Text,
        wit::AccessibleRole::Link => AccessibleRole::Link,
        wit::AccessibleRole::Checkbox => AccessibleRole::Checkbox,
        wit::AccessibleRole::Switch => AccessibleRole::Switch,
        wit::AccessibleRole::Radio => AccessibleRole::Radio,
        wit::AccessibleRole::Tab => AccessibleRole::Tab,
        wit::AccessibleRole::MenuItem => AccessibleRole::MenuItem,
        // A heading is of a level from 1 to 6, as a style sheet's.
        wit::AccessibleRole::Heading(level) => AccessibleRole::Heading(level.clamp(1, 6)),
        wit::AccessibleRole::Image => AccessibleRole::Image,
        wit::AccessibleRole::Progress => AccessibleRole::Progress,
        wit::AccessibleRole::Slider => AccessibleRole::Slider,
    }
}

fn key_out(key: wit::Key) -> Key {
    match key {
        wit::Key::Up => Key::Up,
        wit::Key::Down => Key::Down,
        wit::Key::Left => Key::Left,
        wit::Key::Right => Key::Right,
        wit::Key::PageUp => Key::PageUp,
        wit::Key::PageDown => Key::PageDown,
        wit::Key::Home => Key::Home,
        wit::Key::End => Key::End,
        wit::Key::Enter => Key::Enter,
        wit::Key::Escape => Key::Escape,
        wit::Key::Tab => Key::Tab,
        wit::Key::Backspace => Key::Backspace,
        wit::Key::Delete => Key::Delete,
        // A function key past the twentieth is no key a keyboard has: the twentieth.
        wit::Key::Function(n) => Key::Function(n.clamp(1, 20)),
        wit::Key::Character(c) => Key::Character(c),
    }
}

fn commands_out(
    part: &wit::Slice,
    limits: &ModuleLimits,
    lists: &mut Lists<'_>,
) -> Result<Vec<Command>, Fault> {
    let commands = lists.items(
        lists.commands,
        part,
        limits.parts,
        "the commands of an element",
    )?;
    let mut out = Vec::with_capacity(commands.len());
    for command in commands {
        out.push(Command {
            id: Id::new(command.id),
            title: lists
                .words(&command.title, limits.text, "a command's title")?
                .to_owned(),
            keys: command.keys.map(|keys| Shortcut {
                key: key_out(keys.key),
                mods: Mods {
                    shift: keys.mods.shift,
                    control: keys.mods.control,
                    alt: keys.mods.alt,
                    command: keys.mods.command,
                },
            }),
            enabled: command.enabled,
        });
    }
    Ok(out)
}

/// The longest a move may take, seconds: a page asks for a change to be seen, not for the
/// application to keep drawing it.
const MOTION_SECONDS: f32 = 2.0;

/// A time of a move, held to what a move may take (one that is not a number is none).
fn seconds_out(seconds: f32) -> f32 {
    if seconds.is_finite() {
        seconds.clamp(0.0, MOTION_SECONDS)
    } else {
        0.0
    }
}

fn shift_out(shift: wit::Shift) -> Shift {
    let finite = |value: f32| if value.is_finite() { value } else { 0.0 };
    Shift {
        opacity: finite(shift.opacity).clamp(0.0, 1.0),
        dx: finite(shift.dx),
        dy: finite(shift.dy),
        seconds: seconds_out(shift.seconds),
    }
}

fn motion_out(motion: wit::Motion) -> Motion {
    Motion {
        seconds: seconds_out(motion.seconds),
        ease: match motion.ease {
            wit::Ease::Linear => Ease::Linear,
            wit::Ease::Out => Ease::Out,
            wit::Ease::InOut => Ease::InOut,
            wit::Ease::Spring => Ease::Spring,
        },
        color: motion.color,
        opacity: motion.opacity,
        size: motion.size,
        place: motion.place,
        enter: motion.enter.map(shift_out),
        exit: motion.exit.map(shift_out),
    }
}

fn node_out(node: &wit::Node, limits: &ModuleLimits, lists: &mut Lists<'_>) -> Result<Node, Fault> {
    Ok(Node {
        style: style_out(&node.style, limits, lists)?,
        kind: kind_out(&node.content, limits, lists)?,
        meta: Meta {
            id: node.id.map(Id::new),
            pressable: node.pressable,
            focusable: node.focusable,
            field: node.field.map(|kind| match kind {
                wit::FieldKind::Line => FieldKind::Line,
                wit::FieldKind::Lines => FieldKind::Lines,
                wit::FieldKind::Secret => FieldKind::Secret,
            }),
            editor: node
                .editor
                .as_ref()
                .map(|options| editor_options_out(options, limits, lists))
                .transpose()?,
            label: node
                .label
                .as_ref()
                .map(|part| lists.words(part, limits.text, "a label").map(str::to_owned))
                .transpose()?,
            motion: node.motion.map(motion_out),
            tip: node
                .tip
                .as_ref()
                .map(|tip| -> Result<Tip, Fault> {
                    Ok(Tip {
                        words: lists.words(&tip.words, limits.text, "a tip")?.to_owned(),
                        keys: tip
                            .keys
                            .as_ref()
                            .map(|keys| lists.words(keys, limits.text, "a tip").map(str::to_owned))
                            .transpose()?,
                    })
                })
                .transpose()?,
            role: node.role.map(accessible_role_out),
            state: State {
                checked: node.state.checked,
                selected: node.state.selected,
                expanded: node.state.expanded,
                disabled: node.state.disabled,
                value: node
                    .state
                    .value
                    .as_ref()
                    .map(|part| lists.words(part, limits.text, "a value").map(str::to_owned))
                    .transpose()?,
            },
            commands: commands_out(&node.commands, limits, lists)?,
            focused: node.focused,
            draggable: node.draggable,
            drop_target: node.drop_target,
        },
        children: node.children,
    })
}

/// The tree a module handed out, if it is one within the limits (and the rules of its editors'
/// indent are regular expressions: `patterns` are those known to be).
fn frame_out(
    frame: wit::Frame,
    limits: &ModuleLimits,
    patterns: &mut Patterns,
) -> Result<Frame, Fault> {
    if frame.nodes.len() > limits.nodes {
        return Err(Fault::Malformed(format!(
            "a page of {} elements (at most {})",
            frame.nodes.len(),
            limits.nodes
        )));
    }
    let mut lists = Lists::new(&frame, limits.frame_bytes)?;
    let nodes = frame
        .nodes
        .iter()
        .map(|node| node_out(node, limits, &mut lists))
        .collect::<Result<Vec<_>, _>>()?;
    for options in nodes.iter().filter_map(|node| node.meta.editor.as_ref()) {
        patterns.check(&options.indent)?;
    }
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
            let mut titles = Lists::titles(&menu.titles, limits.frame_bytes)?;
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
    // As many texts as a page may have fields of (one for each element it may have), each
    // within what a text in a reply may be.
    if reply.fill.len() > limits.nodes {
        return Err(Fault::Malformed(format!(
            "texts for {} fields in a reply (at most {})",
            reply.fill.len(),
            limits.nodes
        )));
    }
    let fill = reply
        .fill
        .into_iter()
        .map(|said| Ok((Id::new(said.field), text(said.text)?)))
        .collect::<Result<Vec<_>, Fault>>()?;
    Ok(Reply {
        redraw: reply.redraw,
        close: reply.close,
        menu,
        keyboard: reply.keyboard.map(|keyboard| match keyboard {
            wit::Keyboard::Page => Keyboard::Page,
            wit::Keyboard::Field(id) => Keyboard::Field(Id::new(id)),
        }),
        fill,
        copy: reply.copy.map(text).transpose()?,
        editor: editor_asks_out(reply.editor, limits)?,
        cursor: match reply.cursor {
            wit::Cursor::Arrow => Cursor::Arrow,
            wit::Cursor::Pointer => Cursor::Pointer,
            wit::Cursor::Text => Cursor::Text,
            wit::Cursor::ResizeHorizontal => Cursor::ResizeHorizontal,
            wit::Cursor::ResizeVertical => Cursor::ResizeVertical,
            wit::Cursor::NotAllowed => Cursor::NotAllowed,
            wit::Cursor::Grab => Cursor::Grab,
            wit::Cursor::Grabbing => Cursor::Grabbing,
            wit::Cursor::Crosshair => Cursor::Crosshair,
        },
    })
}

// --- What a page says of its editors -----------------------------------------------------

/// The rules of an editor's indent that a page has said and that are regular expressions, each
/// compiled once as the editor compiles it, not again at every frame that says it: a page says its
/// options every frame. A page that says ever new ones has those it said long ago compiled again.
#[derive(Default)]
pub(crate) struct Patterns {
    known: HashSet<String>,
}

impl Patterns {
    /// How many patterns are known at most.
    const MOST: usize = 64;

    /// Whether `rules` are regular expressions: a page that says one that is not fails, the words
    /// naming the option, as a page that names an option an editor has not.
    fn check(&mut self, rules: &IndentRules) -> Result<(), Fault> {
        for (name, pattern) in [("increase", &rules.increase), ("decrease", &rules.decrease)] {
            let Some(pattern) = pattern else {
                continue;
            };
            if self.known.contains(pattern) {
                continue;
            }
            regex::RegexBuilder::new(pattern)
                .size_limit(limits::PATTERN_SIZE)
                .build()
                .map_err(|why| {
                    Fault::Malformed(format!(
                        "the indent.{name} of an editor, {pattern:?}, is not a regular expression \
                         (the syntax of Rust's regex): {why}"
                    ))
                })?;
            if self.known.len() >= Self::MOST {
                self.known.clear();
            }
            self.known.insert(pattern.clone());
        }
        Ok(())
    }
}

/// The options of an editor, read: its hint, its comments, its pairs and the rules of its indent
/// are parts of the frame's text; pairs are characters two by two (an odd last one is no pair).
fn editor_options_out(
    options: &wit::EditorOptions,
    limits: &ModuleLimits,
    lists: &mut Lists<'_>,
) -> Result<EditorOptions, Fault> {
    let mut words = |part: &Option<wit::Slice>, what: &str| -> Result<Option<String>, Fault> {
        part.as_ref()
            .map(|part| lists.words(part, limits.text, what).map(str::to_owned))
            .transpose()
    };
    let default = EditorOptions::default();
    let hint = words(&options.hint, "the hint of an editor")?;
    let line = words(&options.comment.line, "the line comment of an editor")?;
    let block = match &options.comment.block {
        None => None,
        Some(block) => {
            let open = words(&Some(block.open), "the block comment of an editor")?;
            let close = words(&Some(block.close), "the block comment of an editor")?;
            open.zip(close)
        }
    };
    let increase = words(&options.indent.increase, "the indent.increase of an editor")?;
    let decrease = words(&options.indent.decrease, "the indent.decrease of an editor")?;
    let pairs: Vec<char> = words(&options.pairs, "the pairs of an editor")?
        .unwrap_or_default()
        .chars()
        .collect();
    if pairs.len() > limits.parts.saturating_mul(2) {
        return Err(Fault::Malformed(format!(
            "an editor of {} pairs (at most {})",
            pairs.len() / 2,
            limits.parts
        )));
    }
    Ok(EditorOptions {
        wrap: options.wrap,
        tab_width: options.tab_width.clamp(1, 16),
        hard_tabs: options.hard_tabs,
        gutter: options.gutter,
        read_only: options.read_only,
        keys: match options.keys {
            wit::EditorKeys::Editor => Keys::Editor,
            wit::EditorKeys::Page => Keys::Page,
        },
        hint,
        comment: CommentTokens { line, block },
        pairs: pairs
            .chunks_exact(2)
            .filter_map(|pair| Some((*pair.first()?, *pair.get(1)?)))
            .collect(),
        indent: IndentRules { increase, decrease },
        folds: match options.folds {
            wit::EditorFolds::Page => Folds::Page,
            wit::EditorFolds::Indent => Folds::Indent,
        },
        line_height: options.line_height.unwrap_or(default.line_height),
        padding: options.padding.unwrap_or(default.padding),
        scroll_margin: options.scroll_margin.unwrap_or(default.scroll_margin),
        numbers: match options.numbers {
            wit::LineNumbers::Absolute => Numbers::Absolute,
            wit::LineNumbers::Relative => Numbers::Relative,
            wit::LineNumbers::Hidden => Numbers::Hidden,
        },
        whitespace: match options.whitespace {
            wit::EditorWhitespace::None => Whitespace::None,
            wit::EditorWhitespace::Boundary => Whitespace::Boundary,
            wit::EditorWhitespace::Selection => Whitespace::Selection,
            wit::EditorWhitespace::Trailing => Whitespace::Trailing,
            wit::EditorWhitespace::All => Whitespace::All,
        },
        indent_guides: options.indent_guides,
        caret: Caret {
            shape: match options.caret.shape {
                wit::CaretShape::Bar => CaretShape::Bar,
                wit::CaretShape::Block => CaretShape::Block,
                wit::CaretShape::Underline => CaretShape::Underline,
            },
            width: options.caret.width.unwrap_or(default.caret.width),
            color: tint_out(&options.caret.color),
            text: tint_out(&options.caret.text),
        },
        colors: editor_colors_out(&options.colors),
    }
    .held())
}

fn tint_out(tint: &wit::EditorTint) -> Tint {
    match tint {
        wit::EditorTint::Theme => Tint::Theme,
        wit::EditorTint::Off => Tint::Off,
        wit::EditorTint::Color(color) => Tint::Color(color_out(color)),
    }
}

fn editor_colors_out(colors: &wit::EditorColors) -> EditorColors {
    EditorColors {
        current_line: tint_out(&colors.current_line),
        selection: tint_out(&colors.selection),
        selection_blurred: tint_out(&colors.selection_blurred),
        found: tint_out(&colors.found),
        brackets: tint_out(&colors.brackets),
        numbers: tint_out(&colors.numbers),
        current_number: tint_out(&colors.current_number),
        gutter: tint_out(&colors.gutter),
        folds: tint_out(&colors.folds),
        folded: tint_out(&colors.folded),
        hint: tint_out(&colors.hint),
        whitespace: tint_out(&colors.whitespace),
        indent_guides: tint_out(&colors.indent_guides),
        active_indent_guide: tint_out(&colors.active_indent_guide),
    }
}

/// What a reply asks of its editors, held to what one reply may hand them: text, decorations,
/// and asks (and the changes, ranges and decorations of each) by count. A reply past them is not
/// one; what an ask means for the document is the editor's to check, and to refuse.
fn editor_asks_out(
    asks: Vec<wit::EditorAsk>,
    limits: &ModuleLimits,
) -> Result<Vec<(Id, EditorAsk)>, Fault> {
    let mut text = 0_usize;
    let mut decorations = 0_usize;
    let mut items = asks.len();
    let mut out = Vec::with_capacity(asks.len());
    let too_many = |what: &str, count: usize, most: usize| {
        Fault::Malformed(format!("{what} of {count} in a reply (at most {most})"))
    };
    for ask in asks {
        let editor = Id::new(ask.editor);
        let ask = match ask.ask {
            wit::EditorAskKind::Load(load) => {
                text = text.saturating_add(load.text.len());
                EditorAsk::Load {
                    text: load.text,
                    more: load.more,
                }
            }
            wit::EditorAskKind::Change(set) => {
                items = items.saturating_add(set.changes.len());
                EditorAsk::Change {
                    base: set.base,
                    changes: set
                        .changes
                        .into_iter()
                        .map(|change| {
                            text = text.saturating_add(change.insert.len());
                            Change {
                                from: place_out(change.from),
                                to: place_out(change.to),
                                insert: change.insert,
                            }
                        })
                        .collect(),
                }
            }
            wit::EditorAskKind::Select(select) => {
                items = items.saturating_add(select.ranges.len());
                EditorAsk::Select {
                    version: select.version,
                    ranges: select
                        .ranges
                        .into_iter()
                        .map(|range| Range::new(place_out(range.anchor), place_out(range.head)))
                        .collect(),
                    main: place_out(select.main),
                }
            }
            wit::EditorAskKind::Reveal(reveal) => EditorAsk::Reveal {
                version: reveal.version,
                at: place_out(reveal.at),
            },
            wit::EditorAskKind::Command(command) => {
                let command = command_out(command);
                if let EditCommand::Insert(words)
                | EditCommand::Find { pattern: words, .. }
                | EditCommand::Replace(words)
                | EditCommand::ReplaceAll(words) = &command
                {
                    text = text.saturating_add(words.len());
                }
                EditorAsk::Command(command)
            }
            wit::EditorAskKind::Decorate(decorate) => {
                text = text.saturating_add(decorate.layer.len());
                decorations = decorations.saturating_add(decorate.decorations.len());
                if decorations > limits.editor_decorations {
                    return Err(too_many(
                        "decorations",
                        decorations,
                        limits.editor_decorations,
                    ));
                }
                EditorAsk::Decorate(Decorate {
                    layer: decorate.layer,
                    version: decorate.version,
                    from: place_out(decorate.from),
                    to: place_out(decorate.to),
                    decorations: decorate
                        .decorations
                        .iter()
                        .map(|item| Decoration {
                            from: place_out(item.from),
                            to: place_out(item.to),
                            mark: mark_out(&item.mark),
                        })
                        .collect(),
                })
            }
        };
        if items > limits.editor_asks {
            return Err(too_many(
                "asks of editors and their changes and ranges",
                items,
                limits.editor_asks,
            ));
        }
        if text > limits.editor_text {
            return Err(Fault::Malformed(format!(
                "{text} bytes of text for editors in a reply (at most {})",
                limits.editor_text
            )));
        }
        out.push((editor, ask));
    }
    Ok(out)
}

fn mark_out(mark: &wit::Mark) -> Mark {
    match mark {
        wit::Mark::Style(look) => Mark::Style(Look {
            color: look.color.as_ref().map(color_out),
            background: look.background.as_ref().map(color_out),
            weight: look.weight.map(|weight| weight.clamp(100, 900)),
            italic: look.italic,
            underline: look.underline.as_ref().map(|line| match line {
                wit::Underline::Straight(color) => Underline::Straight(color_out(color)),
                wit::Underline::Wavy(color) => Underline::Wavy(color_out(color)),
            }),
            strike: look.strike,
        }),
        wit::Mark::Line(color) => Mark::Line(color_out(color)),
        wit::Mark::Gutter(color) => Mark::Gutter(color_out(color)),
        wit::Mark::Fold => Mark::Fold,
    }
}

fn command_out(command: wit::EditorCommand) -> EditCommand {
    use wit::EditorCommand as C;
    match command {
        C::Move(by) => EditCommand::Move {
            movement: match by.movement {
                wit::Movement::Left => Movement::Left,
                wit::Movement::Right => Movement::Right,
                wit::Movement::WordLeft => Movement::WordLeft,
                wit::Movement::WordRight => Movement::WordRight,
                wit::Movement::Up => Movement::Up,
                wit::Movement::Down => Movement::Down,
                wit::Movement::PageUp => Movement::PageUp,
                wit::Movement::PageDown => Movement::PageDown,
                wit::Movement::LineStart => Movement::LineStart,
                wit::Movement::LineEnd => Movement::LineEnd,
                wit::Movement::DocumentStart => Movement::DocumentStart,
                wit::Movement::DocumentEnd => Movement::DocumentEnd,
                wit::Movement::Bracket => Movement::Bracket,
            },
            extend: by.extend,
        },
        C::Insert(text) => EditCommand::Insert(text),
        C::Newline => EditCommand::Newline,
        C::DeleteBackward => EditCommand::DeleteBackward,
        C::DeleteForward => EditCommand::DeleteForward,
        C::DeleteWordBackward => EditCommand::DeleteWordBackward,
        C::DeleteWordForward => EditCommand::DeleteWordForward,
        C::Indent => EditCommand::Indent,
        C::Outdent => EditCommand::Outdent,
        C::MoveLinesUp => EditCommand::MoveLinesUp,
        C::MoveLinesDown => EditCommand::MoveLinesDown,
        C::DuplicateLines => EditCommand::DuplicateLines,
        C::ToggleComment => EditCommand::ToggleComment,
        C::SelectAll => EditCommand::SelectAll,
        C::AddCaretAbove => EditCommand::AddCaretAbove,
        C::AddCaretBelow => EditCommand::AddCaretBelow,
        C::SelectNext => EditCommand::SelectNext,
        C::SingleSelection => EditCommand::SingleSelection,
        C::Undo => EditCommand::Undo,
        C::Redo => EditCommand::Redo,
        C::Fold => EditCommand::Fold,
        C::Unfold => EditCommand::Unfold,
        C::ToggleFold => EditCommand::ToggleFold,
        C::UnfoldAll => EditCommand::UnfoldAll,
        C::Find(find) => EditCommand::Find {
            pattern: find.pattern,
            regex: find.regex,
            case: find.matching_case,
            word: find.word,
        },
        C::FindNext => EditCommand::FindNext,
        C::FindPrevious => EditCommand::FindPrevious,
        C::StopFinding => EditCommand::StopFinding,
        C::Replace(text) => EditCommand::Replace(text),
        C::ReplaceAll(text) => EditCommand::ReplaceAll(text),
        C::SelectFound => EditCommand::SelectFound,
        C::SelectOccurrences => EditCommand::SelectOccurrences,
        C::ToggleBlockComment => EditCommand::ToggleBlockComment,
    }
}

// --- Compiling and the version of the interface -------------------------------------------

/// The interface this crate speaks: the package of `wit/view.wit`.
const INTERFACE: &str = "pencere:view@2.0.0";

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
    patterns: Patterns,
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
            patterns: Patterns::default(),
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
        frame_out(frame, &self.limits, &mut self.patterns)
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

    fn take_asks(&mut self) -> Vec<ScrollTo> {
        std::mem::take(&mut self.store.data_mut().asks)
    }

    fn lay_out(&mut self, laid: Arc<Laid>) {
        self.store.data_mut().laid = Some(laid);
    }

    fn texts(&mut self, texts: Vec<(Id, Arc<dyn pencere_view::edit::TextSnapshot>)>) {
        self.store.data_mut().texts = texts.into_iter().collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::Unbounded;

    /// A frame read by a page that has said no rules of an editor's indent before.
    fn frame_out(frame: wit::Frame, limits: &ModuleLimits) -> Result<Frame, Fault> {
        super::frame_out(frame, limits, &mut Patterns::default())
    }

    #[test]
    fn a_rule_of_an_editors_indent_that_is_no_regular_expression_fails_the_page_by_its_name() {
        let mut patterns = Patterns::default();
        let rules = |increase: &str, decrease: &str| IndentRules {
            increase: Some(increase.to_owned()),
            decrease: Some(decrease.to_owned()),
        };
        assert!(patterns.check(&rules(r"\bthen\s*$", r"^\s*end\b")).is_ok());
        assert!(patterns.check(&IndentRules::default()).is_ok());
        let fault = patterns
            .check(&rules(r"\bthen\s*$", "^(end"))
            .expect_err("no pattern");
        assert!(
            matches!(&fault, Fault::Malformed(why) if why.contains("indent.decrease") && why.contains("^(end")),
            "{fault:?}"
        );
        // Look-around is no pattern of Rust's.
        assert!(patterns.check(&rules("(?=x)", "x")).is_err());
        // What is known is compiled once; a page that says ever new ones is held to what is kept.
        for n in 0..200 {
            assert!(patterns.check(&rules(&format!("a{{{n}}}"), "b")).is_ok());
        }
        assert!(patterns.known.len() <= Patterns::MOST);
    }

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
            overlay: false,
            anchor: None,
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
            fill: None,
            gradient: None,
            border: None,
            radii: wit::Corners {
                top_left: 0.0,
                top_right: 0.0,
                bottom_right: 0.0,
                bottom_left: 0.0,
            },
            shadow: None,
            opacity: 1.0,
            text: wit::TextStyle {
                family: None,
                size: None,
                weight: None,
                italic: None,
                underline: None,
                strike: None,
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
            field: None,
            editor: None,
            label: None,
            motion: None,
            tip: None,
            role: None,
            state: wit::State {
                checked: None,
                selected: false,
                expanded: None,
                disabled: false,
                value: None,
            },
            commands: nothing(),
            focused: false,
            draggable: false,
            drop_target: false,
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
        spans: Vec<wit::Span>,
        paths: Vec<wit::Path>,
        segments: Vec<wit::Segment>,
        commands: Vec<wit::Command>,
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
                spans: self.spans,
                paths: self.paths,
                segments: self.segments,
                commands: self.commands,
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
            mix: None,
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
            mix: None,
            source: wit::Source::Role(wit::Role::Accent),
            alpha: f32::NAN,
        });
        odd.style.shadow = Some(wit::Shadow {
            dx: f32::NAN,
            dy: 1.0e30,
            blur: -5.0,
            color: wit::Color {
                mix: None,
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
            mix: None,
            source: wit::Source::Role(tone),
            alpha,
        };
        assert_eq!(
            color_out(&role(W::BrightRed, 0.25)),
            Role::BrightRed.alpha(0.25)
        );
        // A value comes with both its forms, untouched, and its own opacity on top.
        let forms = |light: wit::Rgba, dark| wit::Color {
            mix: None,
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
                mix: None,
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
                    mix: None,
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

    /// A text an editor posted, as plain characters.
    #[derive(Debug)]
    struct Plain(Vec<char>);

    impl pencere_view::edit::TextSnapshot for Plain {
        fn version(&self) -> u64 {
            4
        }
        fn len_chars(&self) -> usize {
            self.0.len()
        }
        fn len_lines(&self) -> usize {
            self.0.iter().filter(|c| **c == '\n').count() + 1
        }
        fn slice(&self, from: usize, to: usize) -> Option<String> {
            self.0.get(from..to).map(|chars| chars.iter().collect())
        }
        fn line_start(&self, line: usize) -> Option<usize> {
            if line == 0 {
                return Some(0);
            }
            self.0
                .iter()
                .enumerate()
                .filter(|(_, c)| **c == '\n')
                .nth(line - 1)
                .map(|(at, _)| at + 1)
        }
        fn line_of(&self, at: usize) -> Option<usize> {
            (at <= self.0.len()).then(|| self.0.iter().take(at).filter(|c| **c == '\n').count())
        }
    }

    #[test]
    fn a_module_reads_the_text_of_an_editor_from_what_the_application_posted() {
        use pencere::view::layout::Host as _;
        let limits = ModuleLimits {
            editor_text: 6,
            ..ModuleLimits::default()
        };
        let mut host = Host::new(&limits, Arc::new(Unbounded));
        assert_eq!(host.editor_version(3), None);
        host.texts
            .insert(Id::new(3), Arc::new(Plain("ğü\nselect".chars().collect())));
        assert_eq!(host.editor_version(3), Some(4));
        assert_eq!(host.editor_length(3), Some(9));
        assert_eq!(host.editor_lines(3), Some(2));
        assert_eq!(host.editor_text(3, 0, 2).as_deref(), Some("ğü"));
        assert_eq!(host.line_start(3, 1), Some(3));
        assert_eq!(host.line_of(3, 5), Some(1));
        // Past the end, the wrong way round, or more than a query answers with: none.
        assert_eq!(host.editor_text(3, 5, 99), None);
        assert_eq!(host.editor_text(3, 4, 2), None);
        assert_eq!(host.editor_text(3, 0, 9), None);
        assert_eq!(host.editor_text(4, 0, 1), None);
    }

    #[test]
    fn what_an_editor_tells_reaches_the_module_as_it_was() {
        let edited = event_in(Event::Edit {
            editor: Id::new(3),
            edited: pencere_view::edit::Edited {
                version: 5,
                changes: vec![Change::new(1, 2, "ğ")],
                origin: Origin::Undo,
            },
        });
        let wit::Event::Edit(edited) = edited else {
            panic!("an edit, not {edited:?}");
        };
        assert_eq!((edited.editor, edited.version), (3, 5));
        assert!(matches!(edited.origin, wit::Origin::Undo));
        let change = &edited.changes[0];
        assert_eq!(
            (change.from, change.to, change.insert.as_str()),
            (1, 2, "ğ")
        );
        let pressed = event_in(Event::Pointer {
            input: Input::Press {
                at: Point::new(1.0, 2.0),
                button: Button::Left,
                mods: Mods::default(),
                count: 2,
            },
            place: pencere_view::Place {
                editor: Id::new(3),
                at: 7,
            },
        });
        let wit::Event::EditorPointer(pointer) = pressed else {
            panic!("an editor's pointer event, not {pressed:?}");
        };
        assert_eq!((pointer.place.editor, pointer.place.at), (3, 7));
        assert!(matches!(
            pointer.input,
            wit::PointerInput::Press(wit::Press { count: 2, .. })
        ));
        let refused = event_in(Event::EditorRefused {
            editor: Id::new(3),
            refusal: Refusal::StaleVersion,
        });
        assert!(matches!(
            refused,
            wit::Event::EditorRefused(wit::EditorRefused {
                editor: 3,
                reason: wit::Refusal::StaleVersion,
            })
        ));
        let found = event_in(Event::Found {
            editor: Id::new(3),
            found: pencere_view::edit::Found {
                version: 9,
                count: 12,
                current: Some(3),
                more: true,
            },
        });
        assert!(matches!(
            found,
            wit::Event::Found(wit::Found {
                editor: 3,
                version: 9,
                count: 12,
                current: Some(3),
                more: true,
            })
        ));
    }

    #[test]
    fn what_a_reply_asks_of_editors_is_read_and_held_to_what_a_reply_may_hand_them() {
        let limits = ModuleLimits {
            editor_text: 8,
            editor_decorations: 2,
            ..ModuleLimits::default()
        };
        let reply = |editor: Vec<wit::EditorAsk>| wit::Reply {
            redraw: false,
            close: false,
            menu: None,
            keyboard: None,
            fill: Vec::new(),
            copy: None,
            cursor: wit::Cursor::Arrow,
            editor,
        };
        let ask = |ask: wit::EditorAskKind| wit::EditorAsk { editor: 3, ask };
        let fold = || wit::Decoration {
            from: 0,
            to: 4,
            mark: wit::Mark::Fold,
        };
        let read = reply_out(
            reply(vec![
                ask(wit::EditorAskKind::Load(wit::Load {
                    text: "abc".into(),
                    more: true,
                })),
                ask(wit::EditorAskKind::Change(wit::ChangeSet {
                    base: 2,
                    changes: vec![wit::Change {
                        from: 0,
                        to: 1,
                        insert: "x".into(),
                    }],
                })),
                ask(wit::EditorAskKind::Command(wit::EditorCommand::Undo)),
                ask(wit::EditorAskKind::Decorate(wit::Decorate {
                    layer: "f".into(),
                    version: 2,
                    from: 0,
                    to: 9,
                    decorations: vec![fold()],
                })),
            ]),
            &limits,
        )
        .expect("within the limits");
        assert_eq!(
            read.editor,
            vec![
                (
                    Id::new(3),
                    EditorAsk::Load {
                        text: "abc".into(),
                        more: true
                    }
                ),
                (
                    Id::new(3),
                    EditorAsk::Change {
                        base: 2,
                        changes: vec![Change::new(0, 1, "x")]
                    }
                ),
                (Id::new(3), EditorAsk::Command(EditCommand::Undo)),
                (
                    Id::new(3),
                    EditorAsk::Decorate(Decorate {
                        layer: "f".into(),
                        version: 2,
                        from: 0,
                        to: 9,
                        decorations: vec![Decoration::new(0, 4, Mark::Fold)],
                    })
                ),
            ]
        );
        // More text than a reply may hand the editors, together, is not a reply.
        let much = reply(vec![
            ask(wit::EditorAskKind::Load(wit::Load {
                text: "abcde".into(),
                more: true,
            })),
            ask(wit::EditorAskKind::Command(wit::EditorCommand::Insert(
                "1234".into(),
            ))),
        ]);
        assert!(matches!(reply_out(much, &limits), Err(Fault::Malformed(_))));
        // What a replacement puts in is text it hands them too.
        let replaced = reply(vec![
            ask(wit::EditorAskKind::Command(wit::EditorCommand::Replace(
                "abcde".into(),
            ))),
            ask(wit::EditorAskKind::Command(wit::EditorCommand::ReplaceAll(
                "1234".into(),
            ))),
        ]);
        assert!(matches!(
            reply_out(replaced, &limits),
            Err(Fault::Malformed(_))
        ));
        let many = reply(vec![ask(wit::EditorAskKind::Decorate(wit::Decorate {
            layer: "f".into(),
            version: 2,
            from: 0,
            to: 9,
            decorations: vec![fold(), fold(), fold()],
        }))]);
        assert!(matches!(reply_out(many, &limits), Err(Fault::Malformed(_))));
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
            keyboard: Some(wit::Keyboard::Field(7)),
            fill: vec![wit::FieldText {
                field: 7,
                text: String::new(),
            }],
            copy,
            cursor: wit::Cursor::Pointer,
            editor: Vec::new(),
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
        assert_eq!(read.keyboard, Some(Keyboard::Field(Id::new(7))));
        assert_eq!(read.fill, [(Id::new(7), String::new())]);
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
                count: 2,
            })),
            wit::Event::Press(wit::Press {
                button: wit::Button::Right,
                count: 2,
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
            event_in(Event::Text { field: Id::new(7), text: "api".into() }),
            wit::Event::Text(wit::FieldText { field: 7, text }) if text == "api"
        ));
        assert!(matches!(
            event_in(Event::Focus(Some(Id::new(7)))),
            wit::Event::Focus(Some(7))
        ));
        assert!(matches!(
            event_in(Event::Input(Input::Scroll {
                at,
                dx: 5.0,
                dy: -2.0
            })),
            wit::Event::Scroll(wit::Scroll { dx, dy, .. }) if (dx, dy) == (5.0, -2.0)
        ));
        assert!(matches!(
            event_in(Event::Input(Input::Key {
                key: Key::Function(12),
                mods: Mods::default()
            })),
            wit::Event::Key(wit::KeyPress {
                key: wit::Key::Function(12),
                ..
            })
        ));
        assert!(matches!(
            event_in(Event::MenuPick(usize::MAX)),
            wit::Event::MenuPick(u32::MAX)
        ));
        assert!(matches!(
            event_in(Event::Input(Input::Release {
                at,
                button: Button::Middle,
            })),
            wit::Event::Release(wit::Release {
                button: wit::Button::Middle,
                ..
            })
        ));
        assert!(matches!(
            event_in(Event::Input(Input::KeyUp {
                key: Key::Character('a'),
                mods: Mods::default()
            })),
            wit::Event::KeyUp(wit::KeyPress {
                key: wit::Key::Character('a'),
                ..
            })
        ));
        assert!(matches!(
            event_in(Event::Input(Input::Mods(Mods {
                alt: true,
                ..Mods::default()
            }))),
            wit::Event::Mods(wit::Mods { alt: true, .. })
        ));
        assert!(matches!(
            event_in(Event::Command(Id::new(9))),
            wit::Event::Command(9)
        ));
        assert!(matches!(
            event_in(Event::Files { at, paths: vec!["/tmp/a b.png".into()] }),
            wit::Event::Files(wit::DroppedFiles { paths, .. }) if paths == ["/tmp/a b.png"]
        ));
    }

    fn accent() -> wit::Color {
        wit::Color {
            mix: None,
            source: wit::Source::Role(wit::Role::Accent),
            alpha: 1.0,
        }
    }

    #[test]
    fn texts_of_many_looks_canvases_commands_and_what_a_page_says_of_itself_are_read() {
        let mut lists = Lists::default();
        let one = lists.words("run ");
        let two = lists.words("make");
        let start = lists.spans.len() as u32;
        lists.spans.push(wit::Span {
            text: one,
            color: None,
            weight: None,
            italic: None,
            underline: None,
            strike: None,
            family: None,
            background: None,
            link: None,
        });
        lists.spans.push(wit::Span {
            text: two,
            color: Some(accent()),
            weight: Some(1000),
            italic: Some(true),
            underline: None,
            strike: None,
            family: Some(wit::Family::Terminal),
            background: None,
            link: Some(5),
        });
        let rich = node(wit::Content::Rich(wit::Slice { start, len: 2 }), 0);

        lists.segments.extend([
            wit::Segment::MoveTo(wit::Point { x: 0.0, y: 0.0 }),
            wit::Segment::LineTo(wit::Point {
                x: f32::NAN,
                y: 4.0,
            }),
            wit::Segment::Close,
        ]);
        lists.paths.push(wit::Path {
            segments: wit::Slice { start: 0, len: 3 },
            fill: None,
            stroke: Some(wit::Stroke {
                width: -2.0,
                color: accent(),
            }),
        });
        let mut canvas = node(wit::Content::Canvas(wit::Slice { start: 0, len: 1 }), 0);
        canvas.style.overlay = true;
        canvas.style.anchor = Some(wit::Anchor {
            to: 3,
            side: wit::Side::Above,
            align: wit::Align::End,
            gap: 4.0,
        });
        canvas.style.gradient = Some(wit::Gradient {
            angle: f32::NAN,
            start: accent(),
            end: accent(),
        });

        let title = lists.words("Find");
        lists.commands.push(wit::Command {
            id: 11,
            title,
            keys: Some(wit::Shortcut {
                key: wit::Key::Function(99),
                mods: wit::Mods {
                    shift: false,
                    control: false,
                    alt: false,
                    command: true,
                },
            }),
            enabled: false,
        });
        let mut root = container(2);
        root.commands = wit::Slice { start: 0, len: 1 };
        root.focused = true;
        root.draggable = true;
        root.drop_target = true;
        root.role = Some(wit::AccessibleRole::Heading(9));
        let value = lists.words("40%");
        root.state.value = Some(value);
        root.state.checked = Some(true);

        let made = frame_out(
            lists.frame(vec![root, rich, canvas]),
            &ModuleLimits::default(),
        )
        .expect("a page");
        let nodes = made.root.fold();
        let meta = &nodes[0].meta;
        assert_eq!(meta.role, Some(AccessibleRole::Heading(6)));
        assert_eq!(meta.state.value.as_deref(), Some("40%"));
        assert_eq!(meta.state.checked, Some(true));
        assert!(meta.focused && meta.draggable && meta.drop_target);
        assert_eq!(meta.commands.len(), 1);
        let command = &meta.commands[0];
        assert_eq!((command.id, command.title.as_str()), (Id::new(11), "Find"));
        assert!(!command.enabled);
        let keys = command.keys.expect("keys");
        assert_eq!(keys.key, Key::Function(20));
        assert!(keys.mods.command);

        let Kind::Rich(spans) = &nodes[1].kind else {
            panic!("a text of many looks");
        };
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].text, "run ");
        assert_eq!(spans[1].text, "make");
        assert_eq!(spans[1].weight, Some(900));
        assert_eq!(spans[1].link, Some(Id::new(5)));
        assert_eq!(spans[1].family, Some(Family::Terminal));

        let Kind::Canvas(paths) = &nodes[2].kind else {
            panic!("a canvas");
        };
        assert_eq!(
            paths[0].segments,
            vec![
                Segment::Move(Point::new(0.0, 0.0)),
                Segment::Line(Point::new(0.0, 4.0)),
                Segment::Close
            ]
        );
        assert_eq!(paths[0].stroke.map(|stroke| stroke.width), Some(0.0));
        let style = &nodes[2].style;
        assert!(style.overlay);
        let anchor = style.anchor.expect("an anchor");
        assert_eq!(
            (anchor.to, anchor.side, anchor.align, anchor.gap),
            (Id::new(3), Side::Above, Align::End, 4.0)
        );
        assert_eq!(style.gradient.map(|gradient| gradient.angle), Some(180.0));
    }

    #[test]
    fn a_text_of_more_parts_or_a_path_of_more_steps_than_the_limits_is_not_a_page() {
        let limits = ModuleLimits {
            parts: 2,
            values: 2,
            ..ModuleLimits::default()
        };
        let mut lists = Lists::default();
        for _ in 0..3 {
            let words = lists.words("a");
            lists.spans.push(wit::Span {
                text: words,
                color: None,
                weight: None,
                italic: None,
                underline: None,
                strike: None,
                family: None,
                background: None,
                link: None,
            });
        }
        let rich = node(wit::Content::Rich(wit::Slice { start: 0, len: 3 }), 0);
        assert!(matches!(
            frame_out(lists.frame(vec![rich]), &limits),
            Err(Fault::Malformed(why)) if why.contains("a text of many looks of 3 items")
        ));

        let mut lists = Lists::default();
        lists.segments.extend([
            wit::Segment::Close,
            wit::Segment::Close,
            wit::Segment::Close,
        ]);
        lists.paths.push(wit::Path {
            segments: wit::Slice { start: 0, len: 3 },
            fill: None,
            stroke: None,
        });
        let canvas = node(wit::Content::Canvas(wit::Slice { start: 0, len: 1 }), 0);
        assert!(matches!(
            frame_out(lists.frame(vec![canvas]), &limits),
            Err(Fault::Malformed(why)) if why.contains("a path of 3 items")
        ));

        // A part past the end of its list is no part.
        let canvas = node(wit::Content::Canvas(wit::Slice { start: 0, len: 1 }), 0);
        assert!(matches!(
            frame_out(frame(vec![canvas]), &limits),
            Err(Fault::Malformed(_))
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
            mix: None,
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
        for version in ["0.5.0", "1.0.0"] {
            let old = format!(
                "(component (instance $page) (export \"pencere:view/page@{version}\" (instance $page)))"
            );
            let refused = World::load(old.as_bytes(), ModuleLimits::default(), Arc::new(Unbounded));
            match refused.err() {
                Some(Fault::Load(why)) => {
                    assert!(
                        why.contains(&format!("written against `pencere:view@{version}`")),
                        "{why}"
                    );
                    assert!(why.contains("opens `pencere:view@2.0.0`"), "{why}");
                }
                other => panic!("{other:?}"),
            }
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
