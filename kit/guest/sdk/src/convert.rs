//! The two crossings of the page interface on the page's side: the tree and the reply the
//! page hands out, put into the interface's types, and the event the application tells, read
//! into the engine's. The application's side of the same crossings is
//! `crates/pencere-sandbox/src/world.rs`.

use pencere_view::wire::{Direction, Display, Items, Justify, Kind, Length, Node, Overflow, Style};
use pencere_view::{
    AccessibleRole, Align, Button, Color, Command, Cursor, Ease, Event, Family, FieldKind, Fit, Id,
    Input, Key, Keyboard, MenuEntry, Mods, Path, Point, Reply, Rgba, Role, Segment, Shift, Side,
    Source, Span, Track,
};

use crate::Frame;
use crate::bindings::pencere::view::types as wit;

// --- What the page hands out ---------------------------------------------------------------

fn role(role: Role) -> wit::Role {
    match role {
        Role::Background => wit::Role::Background,
        Role::Foreground => wit::Role::Foreground,
        Role::Dim => wit::Role::Dim,
        Role::Accent => wit::Role::Accent,
        Role::Cursor => wit::Role::Cursor,
        Role::Success => wit::Role::Success,
        Role::Error => wit::Role::Error,
        Role::Info => wit::Role::Info,
        Role::Warning => wit::Role::Warning,
        Role::Raised(steps) => wit::Role::Raised(steps),
        Role::Quiet => wit::Role::Quiet,
        Role::Border => wit::Role::Border,
        Role::Shadow => wit::Role::Shadow,
        Role::Selection => wit::Role::Selection,
        Role::Black => wit::Role::Black,
        Role::Red => wit::Role::Red,
        Role::Green => wit::Role::Green,
        Role::Yellow => wit::Role::Yellow,
        Role::Blue => wit::Role::Blue,
        Role::Magenta => wit::Role::Magenta,
        Role::Cyan => wit::Role::Cyan,
        Role::White => wit::Role::White,
        Role::BrightBlack => wit::Role::BrightBlack,
        Role::BrightRed => wit::Role::BrightRed,
        Role::BrightGreen => wit::Role::BrightGreen,
        Role::BrightYellow => wit::Role::BrightYellow,
        Role::BrightBlue => wit::Role::BrightBlue,
        Role::BrightMagenta => wit::Role::BrightMagenta,
        Role::BrightCyan => wit::Role::BrightCyan,
        Role::BrightWhite => wit::Role::BrightWhite,
    }
}

fn rgba(value: Rgba) -> wit::Rgba {
    wit::Rgba {
        r: value.r,
        g: value.g,
        b: value.b,
        a: value.a,
    }
}

fn source(source: Source) -> wit::Source {
    match source {
        Source::Role(inner) => wit::Source::Role(role(inner)),
        Source::Rgba { light, dark } => wit::Source::Rgba(wit::RgbaForms {
            light: rgba(light),
            dark: dark.map(rgba),
        }),
    }
}

fn color(color: Color) -> wit::Color {
    wit::Color {
        source: source(color.source),
        mix: color.mix.map(|mix| wit::Mix {
            toward: source(mix.toward),
            amount: mix.amount,
        }),
        alpha: color.alpha,
    }
}

fn length(length: Length) -> wit::Length {
    match length {
        Length::Auto => wit::Length::Auto,
        Length::Px(points) => wit::Length::Px(points),
        Length::Percent(percent) => wit::Length::Percent(percent),
    }
}

fn track(track: &Track) -> wit::Track {
    match *track {
        Track::Px(points) => wit::Track::Px(points),
        Track::Fr(fr) => wit::Track::Fr(fr),
        Track::Range { min, fr } => wit::Track::Range(wit::Range { min, fr }),
        Track::Auto => wit::Track::Auto,
    }
}

fn edges(edges: [f32; 4]) -> wit::Edges {
    wit::Edges {
        top: edges[0],
        right: edges[1],
        bottom: edges[2],
        left: edges[3],
    }
}

fn family(family: Family) -> wit::Family {
    match family {
        Family::System => wit::Family::System,
        Family::Terminal => wit::Family::Terminal,
    }
}

fn align(align: Align) -> wit::Align {
    match align {
        Align::Start => wit::Align::Start,
        Align::Center => wit::Align::Center,
        Align::End => wit::Align::End,
    }
}

fn style(style: Style, lists: &mut Lists) -> wit::Style {
    wit::Style {
        display: match style.display {
            Display::Flex => wit::Display::Flex,
            Display::Grid => wit::Display::Grid,
            Display::None => wit::Display::None,
        },
        direction: match style.direction {
            Direction::Row => wit::Direction::Row,
            Direction::Column => wit::Direction::Column,
        },
        wrap: style.wrap,
        absolute: style.absolute,
        overlay: style.overlay,
        anchor: style.anchor.map(|anchor| wit::Anchor {
            to: anchor.to.number(),
            side: match anchor.side {
                Side::Below => wit::Side::Below,
                Side::Above => wit::Side::Above,
                Side::Right => wit::Side::Right,
                Side::Left => wit::Side::Left,
            },
            align: align(anchor.align),
            gap: anchor.gap,
        }),
        inset: wit::Inset {
            top: style.inset[0],
            right: style.inset[1],
            bottom: style.inset[2],
            left: style.inset[3],
        },
        width: length(style.width),
        height: length(style.height),
        min_width: length(style.min_width),
        min_height: length(style.min_height),
        max_width: length(style.max_width),
        max_height: length(style.max_height),
        padding: edges(style.padding),
        margin: edges(style.margin),
        gap_x: style.gap.0,
        gap_y: style.gap.1,
        grow: style.grow,
        shrink: style.shrink,
        items: style.items.map(|items| match items {
            Items::Start => wit::Items::Start,
            Items::Center => wit::Items::Center,
            Items::End => wit::Items::End,
            Items::Stretch => wit::Items::Stretch,
        }),
        justify: style.justify.map(|justify| match justify {
            Justify::Start => wit::Justify::Start,
            Justify::Center => wit::Justify::Center,
            Justify::End => wit::Justify::End,
            Justify::Between => wit::Justify::Between,
        }),
        columns: lists.tracks(style.columns.iter().map(track)),
        rows: lists.tracks(style.rows.iter().map(track)),
        column_span: style.column_span,
        overflow: match style.overflow {
            Overflow::Visible => wit::Overflow::Visible,
            Overflow::Hidden => wit::Overflow::Hidden,
            Overflow::Scroll => wit::Overflow::Scroll,
        },
        fill: style.fill.map(color),
        gradient: style.gradient.map(|gradient| wit::Gradient {
            angle: gradient.angle,
            start: color(gradient.from),
            end: color(gradient.to),
        }),
        border: style.border.map(|border| wit::Border {
            width: border.width,
            color: color(border.color),
        }),
        radii: wit::Corners {
            top_left: style.radii[0],
            top_right: style.radii[1],
            bottom_right: style.radii[2],
            bottom_left: style.radii[3],
        },
        shadow: style.shadow.map(|shadow| wit::Shadow {
            dx: shadow.dx,
            dy: shadow.dy,
            blur: shadow.blur,
            color: color(shadow.color),
        }),
        opacity: style.opacity.unwrap_or(1.0),
        text: wit::TextStyle {
            family: style.text.family.map(family),
            size: style.text.size,
            weight: style.text.weight,
            italic: style.text.italic,
            underline: style.text.underline,
            strike: style.text.strike,
            color: style.text.color.map(color),
            align: style.text.align.map(align),
        },
        truncate: style.truncate,
    }
}

/// The lists a frame names the parts of: the words of its lines of text and the paths of its
/// pictures, the values of its sparklines and the tracks of its grids. Each element appends what
/// it has and keeps the part it took, so a frame says every byte of itself once.
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

/// A part of a list that begins at `start` and has `len` items. A list of a module cannot pass
/// the 4 GiB its memory addresses, so the numbers fit.
fn part(start: usize, len: usize) -> wit::Slice {
    wit::Slice {
        start: u32::try_from(start).unwrap_or(u32::MAX),
        len: u32::try_from(len).unwrap_or(u32::MAX),
    }
}

impl Lists {
    fn words(&mut self, words: &str) -> wit::Slice {
        let start = self.text.len();
        self.text.push_str(words);
        part(start, words.len())
    }

    fn values(&mut self, values: &[f32]) -> wit::Slice {
        let start = self.numbers.len();
        self.numbers.extend_from_slice(values);
        part(start, values.len())
    }

    fn tracks(&mut self, tracks: impl Iterator<Item = wit::Track>) -> wit::Slice {
        let start = self.tracks.len();
        self.tracks.extend(tracks);
        part(start, self.tracks.len() - start)
    }

    fn spans(&mut self, spans: &[Span]) -> wit::Slice {
        let made: Vec<wit::Span> = spans
            .iter()
            .map(|span| wit::Span {
                text: self.words(&span.text),
                color: span.color.map(color),
                weight: span.weight,
                italic: span.italic,
                underline: span.underline,
                strike: span.strike,
                family: span.family.map(family),
                background: span.background.map(color),
                link: span.link.map(|id| id.number()),
            })
            .collect();
        let start = self.spans.len();
        self.spans.extend(made);
        part(start, spans.len())
    }

    fn paths(&mut self, paths: &[Path]) -> wit::Slice {
        let made: Vec<wit::Path> = paths
            .iter()
            .map(|path| {
                let start = self.segments.len();
                self.segments.extend(path.segments.iter().map(segment));
                wit::Path {
                    segments: part(start, path.segments.len()),
                    fill: path.fill.map(color),
                    stroke: path.stroke.map(|stroke| wit::Stroke {
                        width: stroke.width,
                        color: color(stroke.color),
                    }),
                }
            })
            .collect();
        let start = self.paths.len();
        self.paths.extend(made);
        part(start, paths.len())
    }

    fn commands(&mut self, commands: &[Command]) -> wit::Slice {
        let made: Vec<wit::Command> = commands
            .iter()
            .map(|command| wit::Command {
                id: command.id.number(),
                title: self.words(&command.title),
                keys: command.keys.map(|keys| wit::Shortcut {
                    key: key_out(keys.key),
                    mods: mods_out(keys.mods),
                }),
                enabled: command.enabled,
            })
            .collect();
        let start = self.commands.len();
        self.commands.extend(made);
        part(start, commands.len())
    }
}

fn segment(segment: &Segment) -> wit::Segment {
    let at = |point: Point| wit::Point {
        x: point.x,
        y: point.y,
    };
    match *segment {
        Segment::Move(to) => wit::Segment::MoveTo(at(to)),
        Segment::Line(to) => wit::Segment::LineTo(at(to)),
        Segment::Quad { control, to } => wit::Segment::QuadTo(wit::QuadTo {
            control: at(control),
            to: at(to),
        }),
        Segment::Cubic { a, b, to } => wit::Segment::CubicTo(wit::CubicTo {
            a: at(a),
            b: at(b),
            to: at(to),
        }),
        Segment::Close => wit::Segment::Close,
    }
}

fn accessible_role(role: AccessibleRole) -> Option<wit::AccessibleRole> {
    Some(match role {
        AccessibleRole::Button => wit::AccessibleRole::Button,
        // A field is one by what the element is, not by its role.
        AccessibleRole::Field(_) => return None,
        AccessibleRole::Text => wit::AccessibleRole::Text,
        AccessibleRole::Link => wit::AccessibleRole::Link,
        AccessibleRole::Checkbox => wit::AccessibleRole::Checkbox,
        AccessibleRole::Switch => wit::AccessibleRole::Switch,
        AccessibleRole::Radio => wit::AccessibleRole::Radio,
        AccessibleRole::Tab => wit::AccessibleRole::Tab,
        AccessibleRole::MenuItem => wit::AccessibleRole::MenuItem,
        AccessibleRole::Heading(level) => wit::AccessibleRole::Heading(level),
        AccessibleRole::Image => wit::AccessibleRole::Image,
        AccessibleRole::Progress => wit::AccessibleRole::Progress,
        AccessibleRole::Slider => wit::AccessibleRole::Slider,
    })
}

fn mods_out(mods: Mods) -> wit::Mods {
    wit::Mods {
        shift: mods.shift,
        control: mods.control,
        alt: mods.alt,
        command: mods.command,
    }
}

fn key_out(key: Key) -> wit::Key {
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

fn node(node: Node, lists: &mut Lists) -> wit::Node {
    wit::Node {
        content: match node.kind {
            Kind::Box => wit::Content::Box,
            Kind::Text(text) => wit::Content::Text(lists.words(&text)),
            Kind::Rich(spans) => wit::Content::Rich(lists.spans(&spans)),
            Kind::Canvas(paths) => wit::Content::Canvas(lists.paths(&paths)),
            Kind::Spark(spark) => wit::Content::Spark(wit::Spark {
                values: lists.values(&spark.values),
                color: color(spark.color),
                peak: spark.peak,
            }),
            Kind::Image(image) => wit::Content::Image(wit::Image {
                src: lists.words(&image.src),
                fit: match image.fit {
                    Fit::Contain => wit::Fit::Contain,
                    Fit::Cover => wit::Fit::Cover,
                },
                tint: image.tint.map(color),
            }),
        },
        style: style(node.style, lists),
        id: node.meta.id.map(|id| id.number()),
        pressable: node.meta.pressable,
        focusable: node.meta.focusable,
        field: node.meta.field.map(|kind| match kind {
            FieldKind::Line => wit::FieldKind::Line,
            FieldKind::Lines => wit::FieldKind::Lines,
            FieldKind::Secret => wit::FieldKind::Secret,
        }),
        label: node.meta.label.as_deref().map(|label| lists.words(label)),
        motion: node.meta.motion.map(|motion| wit::Motion {
            seconds: motion.seconds,
            ease: match motion.ease {
                Ease::Linear => wit::Ease::Linear,
                Ease::Out => wit::Ease::Out,
                Ease::InOut => wit::Ease::InOut,
                Ease::Spring => wit::Ease::Spring,
            },
            color: motion.color,
            opacity: motion.opacity,
            size: motion.size,
            place: motion.place,
            enter: motion.enter.map(shift),
            exit: motion.exit.map(shift),
        }),
        tip: node.meta.tip.as_ref().map(|tip| wit::Tip {
            words: lists.words(&tip.words),
            keys: tip.keys.as_deref().map(|keys| lists.words(keys)),
        }),
        role: node.meta.role.and_then(accessible_role),
        state: wit::State {
            checked: node.meta.state.checked,
            selected: node.meta.state.selected,
            expanded: node.meta.state.expanded,
            disabled: node.meta.state.disabled,
            value: node
                .meta
                .state
                .value
                .as_deref()
                .map(|value| lists.words(value)),
        },
        commands: lists.commands(&node.meta.commands),
        focused: node.meta.focused,
        draggable: node.meta.draggable,
        drop_target: node.meta.drop_target,
        children: node.children,
    }
}

fn shift(shift: Shift) -> wit::Shift {
    wit::Shift {
        opacity: shift.opacity,
        dx: shift.dx,
        dy: shift.dy,
        seconds: shift.seconds,
    }
}

/// The page, as the interface's list of nodes and the lists they name the parts of.
pub fn to_wit_frame(frame: Frame) -> wit::Frame {
    let mut lists = Lists::default();
    let nodes = frame
        .root
        .fold()
        .into_iter()
        .map(|each| node(each, &mut lists))
        .collect();
    wit::Frame {
        nodes,
        text: lists.text,
        numbers: lists.numbers,
        tracks: lists.tracks,
        spans: lists.spans,
        paths: lists.paths,
        segments: lists.segments,
        commands: lists.commands,
        animating: frame.animating,
        wake_at: frame.wake_at,
    }
}

/// The page's answer to an event, as the interface's.
pub fn to_wit_reply(reply: Reply) -> wit::Reply {
    wit::Reply {
        redraw: reply.redraw,
        close: reply.close,
        menu: reply.menu.map(|menu| {
            // The titles of the entries are one string, as the words of a frame are.
            let mut lists = Lists::default();
            let entries = menu
                .entries
                .into_iter()
                .map(|entry| match entry {
                    MenuEntry::Separator => wit::MenuEntry::Separator,
                    MenuEntry::Item {
                        title,
                        key,
                        enabled,
                    } => wit::MenuEntry::Item(wit::MenuItem {
                        title: lists.words(&title),
                        key,
                        enabled,
                    }),
                })
                .collect();
            wit::Menu {
                at: wit::Point {
                    x: menu.at.x,
                    y: menu.at.y,
                },
                entries,
                titles: lists.text,
            }
        }),
        keyboard: reply.keyboard.map(|keyboard| match keyboard {
            Keyboard::Page => wit::Keyboard::Page,
            Keyboard::Field(id) => wit::Keyboard::Field(id.number()),
        }),
        fill: reply
            .fill
            .into_iter()
            .map(|(field, text)| wit::FieldText {
                field: field.number(),
                text,
            })
            .collect(),
        copy: reply.copy,
        cursor: match reply.cursor {
            Cursor::Arrow => wit::Cursor::Arrow,
            Cursor::Pointer => wit::Cursor::Pointer,
            Cursor::Text => wit::Cursor::Text,
            Cursor::ResizeHorizontal => wit::Cursor::ResizeHorizontal,
            Cursor::ResizeVertical => wit::Cursor::ResizeVertical,
            Cursor::NotAllowed => wit::Cursor::NotAllowed,
            Cursor::Grab => wit::Cursor::Grab,
            Cursor::Grabbing => wit::Cursor::Grabbing,
            Cursor::Crosshair => wit::Cursor::Crosshair,
        },
    }
}

// --- What the application tells ------------------------------------------------------------

fn point(at: wit::Point) -> Point {
    Point::new(at.x, at.y)
}

fn button(button: wit::Button) -> Button {
    match button {
        wit::Button::Left => Button::Left,
        wit::Button::Right => Button::Right,
        wit::Button::Middle => Button::Middle,
    }
}

fn mods(mods: wit::Mods) -> Mods {
    Mods {
        shift: mods.shift,
        control: mods.control,
        alt: mods.alt,
        command: mods.command,
    }
}

fn key(key: wit::Key) -> Key {
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
        wit::Key::Function(n) => Key::Function(n),
        wit::Key::Character(c) => Key::Character(c),
    }
}

/// What the application told, as the engine's.
pub fn from_wit_event(event: wit::Event) -> Event {
    match event {
        wit::Event::PointerMove(at) => Event::Input(Input::Move(point(at))),
        wit::Event::PointerLeave => Event::Input(Input::Leave),
        wit::Event::Press(press) => Event::Input(Input::Press {
            at: point(press.at),
            button: button(press.button),
            mods: mods(press.mods),
            count: press.count,
        }),
        wit::Event::Release(release) => Event::Input(Input::Release {
            at: point(release.at),
            button: button(release.button),
        }),
        wit::Event::Scroll(scroll) => Event::Input(Input::Scroll {
            at: point(scroll.at),
            dx: scroll.dx,
            dy: scroll.dy,
        }),
        wit::Event::Key(pressed) => Event::Input(Input::Key {
            key: key(pressed.key),
            mods: mods(pressed.mods),
        }),
        wit::Event::Text(said) => Event::Text {
            field: Id::new(said.field),
            text: said.text,
        },
        wit::Event::Focus(field) => Event::Focus(field.map(Id::new)),
        wit::Event::MenuPick(index) => Event::MenuPick(index as usize),
        wit::Event::KeyUp(pressed) => Event::Input(Input::KeyUp {
            key: key(pressed.key),
            mods: mods(pressed.mods),
        }),
        wit::Event::Mods(held) => Event::Input(Input::Mods(mods(held))),
        wit::Event::Command(id) => Event::Command(Id::new(id)),
        wit::Event::Files(dropped) => Event::Files {
            at: point(dropped.at),
            paths: dropped.paths,
        },
        wit::Event::Scrolled(boxes) => Event::Scrolled(
            boxes
                .into_iter()
                .map(|scrolled| (Id::new(scrolled.id), point(scrolled.offset)))
                .collect(),
        ),
    }
}
