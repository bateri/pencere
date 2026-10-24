//! The two crossings of the page interface on the page's side: the tree and the reply the
//! page hands out, put into the interface's types, and the event the application tells, read
//! into the engine's. The application's side of the same crossings is
//! `crates/pencere-sandbox/src/world.rs`.

use pencere_view::wire::{Direction, Display, Items, Justify, Kind, Length, Node, Overflow, Style};
use pencere_view::{
    Align, Button, Color, Cursor, Event, Family, Fit, Input, Key, Keyboard, MenuEntry, Mods, Point,
    Reply, Rgba, Role, Source, Track,
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

fn color(color: Color) -> wit::Color {
    let source = match color.source {
        Source::Role(inner) => wit::Source::Role(role(inner)),
        Source::Rgba { light, dark } => wit::Source::Rgba(wit::RgbaForms {
            light: rgba(light),
            dark: dark.map(rgba),
        }),
    };
    wit::Color {
        source,
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
        scroll_x: style.scroll.0,
        scroll_y: style.scroll.1,
        fill: style.fill.map(color),
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
        text: wit::TextStyle {
            family: style.text.family.map(|family| match family {
                Family::System => wit::Family::System,
                Family::Terminal => wit::Family::Terminal,
            }),
            size: style.text.size,
            weight: style.text.weight,
            color: style.text.color.map(color),
            align: style.text.align.map(|align| match align {
                Align::Start => wit::Align::Start,
                Align::Center => wit::Align::Center,
                Align::End => wit::Align::End,
            }),
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
}

fn node(node: Node, lists: &mut Lists) -> wit::Node {
    wit::Node {
        content: match node.kind {
            Kind::Box => wit::Content::Box,
            Kind::Text(text) => wit::Content::Text(lists.words(&text)),
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
                opacity: image.opacity,
            }),
        },
        style: style(node.style, lists),
        id: node.meta.id.map(|id| id.number()),
        pressable: node.meta.pressable,
        focusable: node.meta.focusable,
        text_field: node.meta.text_field,
        children: node.children,
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
            Keyboard::Field => wit::Keyboard::Field,
        }),
        copy: reply.copy,
        cursor: match reply.cursor {
            Cursor::Arrow => wit::Cursor::Arrow,
            Cursor::Pointer => wit::Cursor::Pointer,
            Cursor::Text => wit::Cursor::Text,
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
        }),
        wit::Event::Release(release) => Event::Input(Input::Release {
            at: point(release.at),
            button: button(release.button),
        }),
        wit::Event::Scroll(scroll) => Event::Input(Input::Scroll {
            at: point(scroll.at),
            dy: scroll.dy,
        }),
        wit::Event::Key(pressed) => Event::Input(Input::Key {
            key: key(pressed.key),
            mods: mods(pressed.mods),
        }),
        wit::Event::Text(text) => Event::Text(text),
        wit::Event::MenuPick(index) => Event::MenuPick(index as usize),
    }
}
