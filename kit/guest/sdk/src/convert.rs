//! The two crossings of the page interface on the page's side: the tree and the reply the
//! page hands out, put into the interface's types, and the event the application tells, read
//! into the engine's. The application's side of the same crossings is
//! `crates/pencere-sandbox/src/world.rs`.

use pencere_view::edit::{
    CaretHeight, CaretShape, Change, EditCommand, Edited, EditorAsk, EditorColors, EditorOptions,
    Folds, Found, GutterEvent, GutterHit, GutterKind, GutterShape, HoverEvent, HoverHit, HoverKind,
    HoverOver, KeyAction, Keys, LineHighlight, Mark, Matches, Movement, Numbers, Origin, Pressing,
    Range, Refusal, RevealAlign, RulerLane, Selection, TargetEvent, TargetKind, TargetSource, Tint,
    Underline, Whitespace, WidgetShow, WidgetSide, ZoneSide,
};
use pencere_view::wire::{Direction, Display, Items, Justify, Kind, Length, Node, Overflow, Style};
use pencere_view::{
    AccessibleRole, Align, Button, Color, Command, Cursor, Ease, Event, Family, FieldKind, Fit, Id,
    Input, Key, Keyboard, MenuEntry, Mods, Path, Place, Point, Rect, Reply, Rgba, Role, Segment,
    Shift, Side, Source, Span, Track,
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
                keys: command.keys.iter().map(shortcut_out).collect(),
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
        // A field and an editor are ones by what the element is, not by its role.
        AccessibleRole::Field(_) | AccessibleRole::Editor => return None,
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

fn shortcut_out(keys: &pencere_view::Shortcut) -> wit::Shortcut {
    wit::Shortcut {
        key: key_out(keys.key),
        mods: mods_out(keys.mods),
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
        editor: node
            .meta
            .editor
            .as_ref()
            .map(|options| editor_options(options, lists)),
        label: node.meta.label.as_deref().map(|label| lists.words(label)),
        zone: node.meta.zone.map(|id| id.number()),
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
            spin: motion.spin,
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
        wake: frame.wake,
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
                        keys,
                        enabled,
                    } => wit::MenuEntry::Item(wit::MenuItem {
                        title: lists.words(&title),
                        keys: keys.as_ref().map(shortcut_out),
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
            .map(|(field, fill)| wit::FieldFill {
                field: field.number(),
                text: fill.text,
                // A place past what the interface counts is past the end of any text: its end.
                select: fill.select.map(|range| wit::CharRange {
                    from: u32::try_from(range.start).unwrap_or(u32::MAX),
                    to: u32::try_from(range.end).unwrap_or(u32::MAX),
                }),
            })
            .collect(),
        copy: reply.copy,
        editor: reply.editor.into_iter().map(editor_ask).collect(),
        link: reply.link,
        wake: reply.wake,
        cursor: cursor(reply.cursor),
    }
}

fn cursor(cursor: Cursor) -> wit::Cursor {
    match cursor {
        Cursor::Arrow => wit::Cursor::Arrow,
        Cursor::Pointer => wit::Cursor::Pointer,
        Cursor::Text => wit::Cursor::Text,
        Cursor::ResizeHorizontal => wit::Cursor::ResizeHorizontal,
        Cursor::ResizeVertical => wit::Cursor::ResizeVertical,
        Cursor::NotAllowed => wit::Cursor::NotAllowed,
        Cursor::Grab => wit::Cursor::Grab,
        Cursor::Grabbing => wit::Cursor::Grabbing,
        Cursor::Crosshair => wit::Cursor::Crosshair,
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
        editor @ (wit::Event::EditorPointer(_)
        | wit::Event::Edit(_)
        | wit::Event::Selection(_)
        | wit::Event::Viewport(_)
        | wit::Event::EditorRefused(_)
        | wit::Event::Found(_)
        | wit::Event::EditorGutter(_)
        | wit::Event::EditorHover(_)
        | wit::Event::EditorTarget(_)
        | wit::Event::KeymapPending(_)) => editor_event(editor),
        wit::Event::Wake(now) => Event::Wake { now },
        wit::Event::EditorReady(ready) => Event::EditorReady {
            editor: Id::new(ready.editor),
            version: ready.version,
        },
    }
}
// --- An editor's options and what the page asks of its editors -----------------------------

fn editor_options(options: &EditorOptions, lists: &mut Lists) -> wit::EditorOptions {
    let pairs: String = options
        .pairs
        .iter()
        .flat_map(|(open, close)| [*open, *close])
        .collect();
    wit::EditorOptions {
        wrap: options.wrap,
        tab_width: options.tab_width,
        hard_tabs: options.hard_tabs,
        gutter: options.gutter,
        read_only: options.read_only,
        keys: match options.keys {
            Keys::Editor => wit::EditorKeys::Editor,
            Keys::Page => wit::EditorKeys::Page,
        },
        hint: options.hint.as_deref().map(|hint| lists.words(hint)),
        comment: wit::CommentTokens {
            line: options
                .comment
                .line
                .as_deref()
                .map(|line| lists.words(line)),
            block: options
                .comment
                .block
                .as_ref()
                .map(|(open, close)| wit::BlockComment {
                    open: lists.words(open),
                    close: lists.words(close),
                }),
        },
        pairs: (!pairs.is_empty()).then(|| lists.words(&pairs)),
        indent: wit::IndentRules {
            increase: options
                .indent
                .increase
                .as_deref()
                .map(|rule| lists.words(rule)),
            decrease: options
                .indent
                .decrease
                .as_deref()
                .map(|rule| lists.words(rule)),
        },
        folds: match options.folds {
            Folds::Page => wit::EditorFolds::Page,
            Folds::Indent => wit::EditorFolds::Indent,
        },
        line_height: Some(options.line_height),
        padding: Some(options.padding),
        scroll_margin: Some(options.scroll_margin),
        numbers: match options.numbers {
            Numbers::Absolute => wit::LineNumbers::Absolute,
            Numbers::Relative => wit::LineNumbers::Relative,
            Numbers::Hidden => wit::LineNumbers::Hidden,
        },
        current_line: match options.current_line {
            LineHighlight::Text => wit::LineHighlight::Text,
            LineHighlight::Gutter => wit::LineHighlight::Gutter,
            LineHighlight::All => wit::LineHighlight::All,
        },
        caret: wit::EditorCaret {
            shape: match options.caret.shape {
                CaretShape::Bar => wit::CaretShape::Bar,
                CaretShape::Block => wit::CaretShape::Block,
                CaretShape::Underline => wit::CaretShape::Underline,
            },
            height: match options.caret.height {
                CaretHeight::Row => wit::CaretHeight::Row,
                CaretHeight::Text => wit::CaretHeight::Text,
            },
            width: Some(options.caret.width),
            color: tint(options.caret.color),
            text: tint(options.caret.text),
        },
        colors: editor_colors(&options.colors),
        whitespace: match options.whitespace {
            Whitespace::None => wit::EditorWhitespace::None,
            Whitespace::Boundary => wit::EditorWhitespace::Boundary,
            Whitespace::Selection => wit::EditorWhitespace::Selection,
            Whitespace::Trailing => wit::EditorWhitespace::Trailing,
            Whitespace::All => wit::EditorWhitespace::All,
        },
        indent_guides: options.indent_guides,
        matches: match options.matches {
            Matches::None => wit::EditorMatches::None,
            Matches::Selection => wit::EditorMatches::Selection,
            Matches::Word => wit::EditorMatches::Word,
        },
        gutters: options
            .gutters
            .iter()
            .map(|column| wit::GutterColumn {
                name: column.name.clone(),
                width: column.width,
                events: column.events.map(|events| wit::ColumnEvents {
                    press: events.press,
                    menu: events.menu,
                    hover: events.hover,
                }),
                hover: column.hover.as_ref().map(|preview| wit::GutterPreview {
                    shape: gutter_shape(&preview.shape),
                    color: color(preview.color),
                }),
                pad: column.pad,
            })
            .collect(),
        selection: wit::SelectionLook {
            radius: Some(options.selection.radius),
        },
        marks: wit::MarkLooks {
            bar: Some(options.marks.bar),
            hover: Some(options.marks.hover),
            ruler: Some(options.marks.ruler),
        },
        events: wit::EditorEvents {
            hover: options.events.hover,
            target: options.events.target,
        },
        hover: wit::HoverOptions {
            delay: Some(options.hover.delay),
        },
        gestures: options
            .gestures
            .iter()
            .map(|(gesture, pressing)| wit::GestureBinding {
                button: match gesture.button {
                    Button::Left => wit::Button::Left,
                    Button::Right => wit::Button::Right,
                    Button::Middle => wit::Button::Middle,
                },
                mods: mods_out(gesture.mods),
                count: gesture.count,
                pressing: match pressing {
                    Pressing::Place => wit::Pressing::Place,
                    Pressing::Extend => wit::Pressing::Extend,
                    Pressing::AddCaret => wit::Pressing::AddCaret,
                    Pressing::Column => wit::Pressing::Column,
                    Pressing::Word => wit::Pressing::Word,
                    Pressing::Line => wit::Pressing::Line,
                    Pressing::Target => wit::Pressing::Target,
                    Pressing::Nothing => wit::Pressing::Nothing,
                },
            })
            .collect(),
        keymap: options
            .keymap
            .iter()
            .map(|binding| wit::KeyBinding {
                keys: binding.keys.iter().map(shortcut_words).collect(),
                action: binding.action.as_ref().map(|action| match action {
                    KeyAction::Command(command) => {
                        wit::KeyAction::Command(editor_command(command.clone()))
                    }
                    KeyAction::Tab => wit::KeyAction::Tab,
                    KeyAction::Copy => wit::KeyAction::Copy,
                    KeyAction::Cut => wit::KeyAction::Cut,
                    KeyAction::Paste => wit::KeyAction::Paste,
                    KeyAction::Escape => wit::KeyAction::Escape,
                    KeyAction::ToLineStart => wit::KeyAction::ToLineStart,
                    KeyAction::ToLineEnd => wit::KeyAction::ToLineEnd,
                }),
            })
            .collect(),
        links: wit::LinkOptions {
            delay: Some(options.links.delay),
        },
        chord_timeout: Some(options.chord_timeout),
        widgets: options
            .widgets
            .iter()
            .map(|(name, group)| wit::WidgetGroup {
                name: name.clone(),
                show: match group.show {
                    WidgetShow::On => wit::WidgetShow::On,
                    WidgetShow::Off => wit::WidgetShow::Off,
                    WidgetShow::UnlessHeld => wit::WidgetShow::UnlessHeld,
                    WidgetShow::WhenHeld => wit::WidgetShow::WhenHeld,
                },
                hold: Some(mods_out(group.hold)),
                size: Some(group.size),
                color: group.color.map(color),
                background: group.background.map(color),
                radius: Some(group.radius),
                pad: Some(group.pad),
                gap: Some(group.gap),
                margin: Some(group.margin),
            })
            .collect(),
    }
}

/// The keys as a page writes them (what `Shortcut::parse` reads).
fn shortcut_words(shortcut: &pencere_view::Shortcut) -> String {
    let mut out = String::new();
    for (held, word) in [
        (shortcut.mods.control, "ctrl+"),
        (shortcut.mods.alt, "alt+"),
        (shortcut.mods.shift, "shift+"),
        (shortcut.mods.command, "cmd+"),
    ] {
        if held {
            out.push_str(word);
        }
    }
    out.push_str(&match shortcut.key {
        Key::Up => "up".to_owned(),
        Key::Down => "down".to_owned(),
        Key::Left => "left".to_owned(),
        Key::Right => "right".to_owned(),
        Key::PageUp => "page_up".to_owned(),
        Key::PageDown => "page_down".to_owned(),
        Key::Home => "home".to_owned(),
        Key::End => "end".to_owned(),
        Key::Enter => "enter".to_owned(),
        Key::Escape => "escape".to_owned(),
        Key::Tab => "tab".to_owned(),
        Key::Backspace => "backspace".to_owned(),
        Key::Delete => "delete".to_owned(),
        Key::Function(n) => format!("f{n}"),
        Key::Character(' ') => "space".to_owned(),
        Key::Character(c) => c.to_string(),
    });
    out
}

fn tint(tint: Tint) -> wit::EditorTint {
    match tint {
        Tint::Theme => wit::EditorTint::Theme,
        Tint::Off => wit::EditorTint::Off,
        Tint::Color(value) => wit::EditorTint::Color(color(value)),
    }
}

fn editor_colors(colors: &EditorColors) -> wit::EditorColors {
    wit::EditorColors {
        current_line: tint(colors.current_line),
        selection: tint(colors.selection),
        selection_blurred: tint(colors.selection_blurred),
        found: tint(colors.found),
        brackets: tint(colors.brackets),
        numbers: tint(colors.numbers),
        current_number: tint(colors.current_number),
        gutter: tint(colors.gutter),
        folds: tint(colors.folds),
        folded: tint(colors.folded),
        hint: tint(colors.hint),
        whitespace: tint(colors.whitespace),
        indent_guides: tint(colors.indent_guides),
        active_indent_guide: tint(colors.active_indent_guide),
        matches: tint(colors.matches),
        mark_hover: tint(colors.mark_hover),
    }
}

/// A place as the interface carries it.
fn place(at: usize) -> u32 {
    u32::try_from(at).unwrap_or(u32::MAX)
}

fn change(change: Change) -> wit::Change {
    wit::Change {
        from: place(change.from),
        to: place(change.to),
        insert: change.insert,
    }
}

fn mark(mark: Mark) -> wit::Mark {
    match mark {
        Mark::Style(look) => wit::Mark::Style(wit::Look {
            color: look.color.map(color),
            background: look.background.map(color),
            weight: look.weight,
            italic: look.italic,
            underline: look.underline.map(|line| match line {
                Underline::Straight(c) => wit::Underline::Straight(color(c)),
                Underline::Wavy(c) => wit::Underline::Wavy(color(c)),
            }),
            strike: look.strike,
            id: look.id,
            held: look.held.map(mods_out),
            over: look.over,
            cursor: look.cursor.map(cursor),
        }),
        Mark::Line(c) => wit::Mark::Line(color(c)),
        Mark::Gutter(gutter) => wit::Mark::Gutter(wit::GutterMark {
            column: gutter.column,
            shape: gutter_shape(&gutter.shape),
            color: color(gutter.color),
            id: gutter.id,
            label: gutter.label,
            cursor: gutter.cursor.map(cursor),
            hover: gutter.hover.map(color),
            pressed: gutter.pressed.map(color),
        }),
        Mark::Fold => wit::Mark::Fold,
        Mark::Ruler(ruler) => wit::Mark::Ruler(wit::RulerMark {
            color: color(ruler.color),
            lane: match ruler.lane {
                RulerLane::Full => wit::RulerLane::Full,
                RulerLane::Left => wit::RulerLane::Left,
                RulerLane::Center => wit::RulerLane::Center,
                RulerLane::Right => wit::RulerLane::Right,
            },
        }),
        Mark::Zone(zone) => wit::Mark::Zone(wit::Zone {
            id: zone.id.number(),
            height: zone.height,
            side: match zone.side {
                ZoneSide::Below => wit::ZoneSide::Below,
                ZoneSide::Above => wit::ZoneSide::Above,
            },
        }),
        Mark::Anchor(name) => wit::Mark::Anchor(name),
        Mark::Widget(widget) => wit::Mark::Widget(wit::Widget {
            parts: widget
                .parts
                .into_iter()
                .map(|part| wit::WidgetPart {
                    text: part.text,
                    color: part.color.map(color),
                    background: part.background.map(color),
                    weight: part.weight,
                    italic: part.italic,
                    id: part.id,
                })
                .collect(),
            side: match widget.side {
                WidgetSide::Before => wit::WidgetSide::Before,
                WidgetSide::After => wit::WidgetSide::After,
                WidgetSide::End => wit::WidgetSide::End,
            },
            group: widget.group,
            id: widget.id,
            label: widget.label,
            cursor: widget.cursor.map(cursor),
            anchor: widget.anchor,
            color: widget.color.map(color),
            background: widget.background.map(color),
            pad_left: widget.pad_left,
            pad_right: widget.pad_right,
            margin_left: widget.margin_left,
            margin_right: widget.margin_right,
        }),
    }
}

fn gutter_shape(shape: &GutterShape) -> wit::GutterShape {
    match shape {
        GutterShape::Dot => wit::GutterShape::Dot,
        GutterShape::Bar => wit::GutterShape::Bar,
        GutterShape::Above => wit::GutterShape::Above,
        GutterShape::Below => wit::GutterShape::Below,
        GutterShape::Image(src) => wit::GutterShape::Image(src.clone()),
    }
}

fn editor_command(command: EditCommand) -> wit::EditorCommand {
    use wit::EditorCommand as C;
    match command {
        EditCommand::Move { movement, extend } => C::Move(wit::MoveBy {
            movement: match movement {
                Movement::Left => wit::Movement::Left,
                Movement::Right => wit::Movement::Right,
                Movement::WordLeft => wit::Movement::WordLeft,
                Movement::WordRight => wit::Movement::WordRight,
                Movement::Up => wit::Movement::Up,
                Movement::Down => wit::Movement::Down,
                Movement::PageUp => wit::Movement::PageUp,
                Movement::PageDown => wit::Movement::PageDown,
                Movement::LineStart => wit::Movement::LineStart,
                Movement::LineEnd => wit::Movement::LineEnd,
                Movement::DocumentStart => wit::Movement::DocumentStart,
                Movement::DocumentEnd => wit::Movement::DocumentEnd,
                Movement::Bracket => wit::Movement::Bracket,
            },
            extend,
        }),
        EditCommand::Insert(text) => C::Insert(text),
        EditCommand::Newline => C::Newline,
        EditCommand::DeleteBackward => C::DeleteBackward,
        EditCommand::DeleteForward => C::DeleteForward,
        EditCommand::DeleteWordBackward => C::DeleteWordBackward,
        EditCommand::DeleteWordForward => C::DeleteWordForward,
        EditCommand::Indent => C::Indent,
        EditCommand::Outdent => C::Outdent,
        EditCommand::MoveLinesUp => C::MoveLinesUp,
        EditCommand::MoveLinesDown => C::MoveLinesDown,
        EditCommand::DuplicateLines => C::DuplicateLines,
        EditCommand::ToggleComment => C::ToggleComment,
        EditCommand::SelectAll => C::SelectAll,
        EditCommand::AddCaretAbove => C::AddCaretAbove,
        EditCommand::AddCaretBelow => C::AddCaretBelow,
        EditCommand::SelectNext => C::SelectNext,
        EditCommand::SingleSelection => C::SingleSelection,
        EditCommand::Undo => C::Undo,
        EditCommand::Redo => C::Redo,
        EditCommand::Fold => C::Fold,
        EditCommand::Unfold => C::Unfold,
        EditCommand::ToggleFold => C::ToggleFold,
        EditCommand::UnfoldAll => C::UnfoldAll,
        EditCommand::Find {
            pattern,
            regex,
            case,
            word,
        } => C::Find(wit::Find {
            pattern,
            regex,
            matching_case: case,
            word,
        }),
        EditCommand::FindNext => C::FindNext,
        EditCommand::FindPrevious => C::FindPrevious,
        EditCommand::StopFinding => C::StopFinding,
        EditCommand::Replace(text) => C::Replace(text),
        EditCommand::ReplaceAll(text) => C::ReplaceAll(text),
        EditCommand::SelectFound => C::SelectFound,
        EditCommand::SelectOccurrences => C::SelectOccurrences,
        EditCommand::ToggleBlockComment => C::ToggleBlockComment,
        EditCommand::FoldAll => C::FoldAll,
        EditCommand::DeleteLines => C::DeleteLines,
        EditCommand::SelectLine => C::SelectLine,
        EditCommand::DuplicateLinesUp => C::DuplicateLinesUp,
        EditCommand::JoinLines => C::JoinLines,
        EditCommand::InsertBlankLine => C::InsertBlankLine,
        EditCommand::SplitLine => C::SplitLine,
        EditCommand::TransposeChars => C::TransposeChars,
        EditCommand::DeleteToLineStart => C::DeleteToLineStart,
        EditCommand::DeleteToLineEnd => C::DeleteToLineEnd,
    }
}

fn editor_ask((editor, ask): (Id, EditorAsk)) -> wit::EditorAsk {
    let ask = match ask {
        EditorAsk::Load { text, more } => wit::EditorAskKind::Load(wit::Load { text, more }),
        EditorAsk::Change {
            base,
            changes,
            history,
            tag,
        } => wit::EditorAskKind::Change(wit::ChangeSet {
            base,
            changes: changes.into_iter().map(change).collect(),
            history,
            tag,
        }),
        EditorAsk::Select {
            version,
            ranges,
            main,
        } => wit::EditorAskKind::Select(wit::SelectRanges {
            version,
            ranges: ranges
                .into_iter()
                .map(|range| wit::TextRange {
                    anchor: place(range.anchor),
                    head: place(range.head),
                })
                .collect(),
            main: place(main),
        }),
        EditorAsk::Reveal { version, at, align } => wit::EditorAskKind::Reveal(wit::Reveal {
            version,
            at: place(at),
            align: match align {
                RevealAlign::Nearest => wit::RevealAlign::Nearest,
                RevealAlign::Start => wit::RevealAlign::Start,
                RevealAlign::Center => wit::RevealAlign::Center,
                RevealAlign::End => wit::RevealAlign::End,
            },
        }),
        EditorAsk::Command(command) => wit::EditorAskKind::Command(editor_command(command)),
        EditorAsk::Decorate(decorate) => wit::EditorAskKind::Decorate(wit::Decorate {
            layer: decorate.layer,
            version: decorate.version,
            from: place(decorate.from),
            to: place(decorate.to),
            decorations: decorate
                .decorations
                .into_iter()
                .map(|item| wit::Decoration {
                    from: place(item.from),
                    to: place(item.to),
                    mark: mark(item.mark),
                })
                .collect(),
        }),
    };
    wit::EditorAsk {
        editor: editor.number(),
        ask,
    }
}

fn unplace(at: u32) -> usize {
    at as usize
}

/// An event of an editor's, as the engine's.
fn editor_event(event: wit::Event) -> Event {
    match event {
        wit::Event::EditorGutter(gutter) => Event::Gutter(GutterEvent {
            editor: Id::new(gutter.editor),
            column: gutter.column,
            line: unplace(gutter.line),
            kind: match gutter.kind {
                wit::GutterKind::Press => GutterKind::Press,
                wit::GutterKind::Enter => GutterKind::Enter,
                wit::GutterKind::Leave => GutterKind::Leave,
            },
            button: button(gutter.button),
            count: gutter.count,
            mods: mods(gutter.mods),
            marker: gutter.marker.map(|hit| GutterHit {
                id: hit.id,
                layer: hit.layer,
                from: unplace(hit.from),
                to: unplace(hit.to),
            }),
            rect: Rect::new(gutter.cell.x, gutter.cell.y, gutter.cell.w, gutter.cell.h),
        }),
        wit::Event::EditorHover(hover) => Event::Hover(HoverEvent {
            editor: Id::new(hover.editor),
            kind: match hover.kind {
                wit::HoverKind::Dwell => HoverKind::Dwell,
                wit::HoverKind::Leave => HoverKind::Leave,
            },
            over: match hover.over {
                wit::HoverOver::Text => HoverOver::Text,
                wit::HoverOver::Widget => HoverOver::Widget,
            },
            id: hover.id,
            at: unplace(hover.at),
            from: unplace(hover.from),
            to: unplace(hover.to),
            line: unplace(hover.line),
            hits: hover
                .hits
                .into_iter()
                .map(|hit| HoverHit {
                    layer: hit.layer,
                    id: hit.id,
                    from: unplace(hit.from),
                    to: unplace(hit.to),
                })
                .collect(),
            rect: Rect::new(hover.word.x, hover.word.y, hover.word.w, hover.word.h),
            mods: mods(hover.mods),
            version: hover.version,
        }),
        wit::Event::EditorTarget(target) => Event::Target(TargetEvent {
            editor: Id::new(target.editor),
            kind: match target.kind {
                wit::TargetKind::Enter => TargetKind::Enter,
                wit::TargetKind::Leave => TargetKind::Leave,
                wit::TargetKind::Press => TargetKind::Press,
            },
            source: match target.source {
                wit::TargetSource::Word => TargetSource::Word,
                wit::TargetSource::Link => TargetSource::Link,
                wit::TargetSource::Widget => TargetSource::Widget,
            },
            at: unplace(target.at),
            from: unplace(target.from),
            to: unplace(target.to),
            id: target.link,
            rect: Rect::new(target.word.x, target.word.y, target.word.w, target.word.h),
            mods: mods(target.mods),
            button: button(target.button),
            count: target.count,
            version: target.version,
        }),
        wit::Event::KeymapPending(pending) => Event::KeymapPending {
            editor: Id::new(pending.editor),
            keys: pending.keys,
        },
        wit::Event::EditorPointer(pointer) => Event::Pointer {
            input: match pointer.input {
                wit::PointerInput::Move(at) => Input::Move(point(at)),
                wit::PointerInput::Press(press) => Input::Press {
                    at: point(press.at),
                    button: button(press.button),
                    mods: mods(press.mods),
                    count: press.count,
                },
                wit::PointerInput::Release(release) => Input::Release {
                    at: point(release.at),
                    button: button(release.button),
                },
            },
            place: Place {
                editor: Id::new(pointer.place.editor),
                at: unplace(pointer.place.at),
            },
        },
        wit::Event::Edit(edited) => Event::Edit {
            editor: Id::new(edited.editor),
            edited: Edited {
                version: edited.version,
                changes: edited
                    .changes
                    .into_iter()
                    .map(|c| Change::new(unplace(c.from), unplace(c.to), c.insert))
                    .collect(),
                origin: match edited.origin {
                    wit::Origin::Person => Origin::Person,
                    wit::Origin::Compose => Origin::Compose,
                    wit::Origin::Page => Origin::Page,
                    wit::Origin::Undo => Origin::Undo,
                    wit::Origin::Redo => Origin::Redo,
                    wit::Origin::Load => Origin::Load,
                },
                tag: edited.tag,
            },
        },
        wit::Event::Selection(selection) => Event::Selection {
            editor: Id::new(selection.editor),
            selection: Selection {
                ranges: selection
                    .ranges
                    .into_iter()
                    .map(|r| Range::new(unplace(r.anchor), unplace(r.head)))
                    .collect(),
                main: unplace(selection.main),
                version: selection.version,
                composing: selection.composing,
            },
        },
        wit::Event::Viewport(viewport) => Event::Viewport {
            editor: Id::new(viewport.editor),
            first: unplace(viewport.first),
            last: unplace(viewport.last),
        },
        wit::Event::EditorRefused(refused) => Event::EditorRefused {
            editor: Id::new(refused.editor),
            refusal: match refused.reason {
                wit::Refusal::Reversed => Refusal::Reversed,
                wit::Refusal::OutOfDocument => Refusal::OutOfDocument,
                wit::Refusal::Overlapping => Refusal::Overlapping,
                wit::Refusal::StaleVersion => Refusal::StaleVersion,
                wit::Refusal::FutureVersion => Refusal::FutureVersion,
                wit::Refusal::TooLarge => Refusal::TooLarge,
                wit::Refusal::TooManyDecorations => Refusal::TooManyDecorations,
                wit::Refusal::OutsideRange => Refusal::OutsideRange,
                wit::Refusal::BadPattern => Refusal::BadPattern,
                wit::Refusal::NoEditor => Refusal::NoEditor,
            },
        },
        wit::Event::Found(found) => Event::Found {
            editor: Id::new(found.editor),
            found: Found {
                version: found.version,
                count: unplace(found.count),
                current: found.current.map(unplace),
                more: found.more,
            },
        },
        // Every other event is the engine's own, read above.
        other => from_wit_event(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pencere_view::edit::{CommentTokens, Decorate, Decoration, IndentRules, Look};
    use pencere_view::{Element, div};

    #[test]
    fn a_link_crosses_as_the_page_said_it_and_the_application_holds_it_to_the_rule() {
        // The page says the address as it is: which ones are opened is the application's to rule
        // (a web page's `link`, the protocol's §9.7), so one it will drop crosses all the same.
        let reply = to_wit_reply(
            Reply::redraw()
                .with_cursor(Cursor::Pointer)
                .with_link("https://example.com/docs"),
        );
        assert_eq!(reply.link.as_deref(), Some("https://example.com/docs"));
        assert!(reply.redraw);
        assert!(matches!(reply.cursor, wit::Cursor::Pointer));
        let reply = to_wit_reply(Reply::none().with_link("file:///etc/passwd"));
        assert_eq!(reply.link.as_deref(), Some("file:///etc/passwd"));
        assert_eq!(to_wit_reply(Reply::none()).link, None);
    }

    #[test]
    fn a_wake_crosses_as_the_page_asked_it_and_the_time_it_comes_at_crosses_back() {
        // Asked twice, the earlier stands; asked by a frame, it crosses with the frame.
        let reply = to_wit_reply(Reply::redraw().with_wake(2.0).with_wake(0.5));
        assert_eq!(reply.wake, Some(0.5));
        assert!(reply.redraw);
        assert_eq!(to_wit_reply(Reply::none()).wake, None);
        let frame = to_wit_frame(crate::Frame {
            wake: Some(3.0),
            ..crate::Frame::still(div())
        });
        assert_eq!(frame.wake, Some(3.0));
        assert_eq!(to_wit_frame(crate::Frame::still(div())).wake, None);
        assert_eq!(
            from_wit_event(wit::Event::Wake(41.25)),
            Event::Wake { now: 41.25 }
        );
    }

    #[test]
    fn how_an_editor_looks_crosses_as_the_page_said_it() {
        use pencere_view::edit::{Caret, CaretShape};
        let options = EditorOptions {
            line_height: 1.8,
            padding: 9.0,
            scroll_margin: 3,
            numbers: Numbers::Relative,
            current_line: LineHighlight::All,
            caret: Caret {
                shape: CaretShape::Block,
                height: CaretHeight::Text,
                width: 2.0,
                color: Tint::Color(Color::from(Role::Accent)),
                text: Tint::Off,
            },
            whitespace: Whitespace::Boundary,
            indent_guides: true,
            colors: EditorColors {
                current_line: Tint::Off,
                gutter: Tint::Color(Color::from(Role::Raised(1))),
                whitespace: Tint::Color(Color::from(Role::Dim)),
                active_indent_guide: Tint::Off,
                ..EditorColors::default()
            },
            ..EditorOptions::default()
        };
        let frame = to_wit_frame(Frame::still(div().editor(Id::new(5), options)));
        let editor = frame.nodes[0]
            .editor
            .as_ref()
            .expect("the node is an editor");
        assert_eq!(
            (editor.line_height, editor.padding, editor.scroll_margin),
            (Some(1.8), Some(9.0), Some(3))
        );
        assert!(matches!(editor.numbers, wit::LineNumbers::Relative));
        assert!(matches!(editor.current_line, wit::LineHighlight::All));
        assert!(matches!(editor.caret.shape, wit::CaretShape::Block));
        assert!(matches!(editor.caret.height, wit::CaretHeight::Text));
        assert_eq!(editor.caret.width, Some(2.0));
        assert!(matches!(&editor.caret.color, wit::EditorTint::Color(_)));
        assert!(matches!(editor.caret.text, wit::EditorTint::Off));
        assert!(matches!(editor.colors.current_line, wit::EditorTint::Off));
        assert!(matches!(&editor.colors.gutter, wit::EditorTint::Color(_)));
        assert!(matches!(editor.colors.selection, wit::EditorTint::Theme));
        assert!(matches!(editor.whitespace, wit::EditorWhitespace::Boundary));
        assert!(editor.indent_guides);
        assert!(matches!(
            &editor.colors.whitespace,
            wit::EditorTint::Color(_)
        ));
        assert!(matches!(
            editor.colors.indent_guides,
            wit::EditorTint::Theme
        ));
        assert!(matches!(
            editor.colors.active_indent_guide,
            wit::EditorTint::Off
        ));
    }

    #[test]
    fn an_editor_and_what_the_page_asks_of_it_cross_as_they_are() {
        let options = EditorOptions {
            hint: Some("SQL…".into()),
            comment: CommentTokens {
                line: Some("--".into()),
                block: Some(("/*".into(), "*/".into())),
            },
            pairs: vec![('(', ')'), ('\'', '\'')],
            indent: IndentRules {
                increase: Some(r"\bthen\s*$".into()),
                decrease: None,
            },
            folds: Folds::Indent,
            ..EditorOptions::default()
        };
        let root: Element = div().editor(Id::new(5), options);
        let frame = to_wit_frame(Frame::still(root));
        let editor = frame.nodes[0]
            .editor
            .as_ref()
            .expect("the node is an editor");
        let words = |part: &wit::Slice| {
            frame
                .text
                .get(part.start as usize..(part.start + part.len) as usize)
                .unwrap_or_default()
                .to_owned()
        };
        assert_eq!(editor.hint.as_ref().map(words).as_deref(), Some("SQL…"));
        assert_eq!(editor.pairs.as_ref().map(words).as_deref(), Some("()''"));
        assert_eq!(
            editor.comment.line.as_ref().map(words).as_deref(),
            Some("--")
        );
        let block = editor.comment.block.as_ref().expect("a block comment");
        assert_eq!(
            (words(&block.open), words(&block.close)),
            ("/*".into(), "*/".into())
        );
        assert_eq!(
            editor.indent.increase.as_ref().map(words).as_deref(),
            Some(r"\bthen\s*$")
        );
        assert!(editor.indent.decrease.is_none());
        assert!(matches!(editor.folds, wit::EditorFolds::Indent));
        let reply = Reply {
            editor: vec![
                (
                    Id::new(5),
                    EditorAsk::Change {
                        base: 3,
                        changes: vec![Change::new(0, 2, "ğ")],
                        history: false,
                        tag: Some("disk".into()),
                    },
                ),
                (
                    Id::new(5),
                    EditorAsk::Decorate(Decorate {
                        layer: "syntax".into(),
                        version: 3,
                        from: 0,
                        to: 4,
                        decorations: vec![Decoration::new(
                            0,
                            2,
                            Mark::Style(Look {
                                weight: Some(700),
                                ..Look::default()
                            }),
                        )],
                    }),
                ),
            ],
            ..Reply::none()
        };
        let asked = to_wit_reply(reply).editor;
        assert_eq!(asked.len(), 2);
        assert!(matches!(
            &asked[0].ask,
            wit::EditorAskKind::Change(set) if set.base == 3 && set.changes[0].insert == "ğ"
                && !set.history && set.tag.as_deref() == Some("disk")
        ));
        let event = from_wit_event(wit::Event::Edit(wit::Edited {
            editor: 5,
            version: 4,
            changes: vec![wit::Change {
                from: 1,
                to: 1,
                insert: "x".into(),
            }],
            origin: wit::Origin::Page,
            tag: Some("format".into()),
        }));
        assert_eq!(
            event,
            Event::Edit {
                editor: Id::new(5),
                edited: Edited {
                    version: 4,
                    changes: vec![Change::new(1, 1, "x")],
                    origin: Origin::Page,
                    tag: Some("format".into()),
                },
            }
        );
        assert_eq!(
            from_wit_event(wit::Event::EditorReady(wit::EditorReady {
                editor: 5,
                version: 0,
            })),
            Event::EditorReady {
                editor: Id::new(5),
                version: 0,
            }
        );
        let event = from_wit_event(wit::Event::Found(wit::Found {
            editor: 5,
            version: 4,
            count: 12,
            current: Some(3),
            more: false,
        }));
        assert_eq!(
            event,
            Event::Found {
                editor: Id::new(5),
                found: Found {
                    version: 4,
                    count: 12,
                    current: Some(3),
                    more: false,
                },
            }
        );
        let replace = Reply {
            editor: vec![(
                Id::new(5),
                EditorAsk::Command(EditCommand::ReplaceAll("${1}x".into())),
            )],
            ..Reply::none()
        };
        assert!(matches!(
            &to_wit_reply(replace).editor[0].ask,
            wit::EditorAskKind::Command(wit::EditorCommand::ReplaceAll(text)) if text == "${1}x"
        ));
    }
}
