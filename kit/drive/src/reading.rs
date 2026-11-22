//! What an agent reads of a page: the elements that are on it with a name each can be told by, its
//! fields, the text its editors show and what is selected in them, the boxes that scroll, what it
//! can be told to do, where the keyboard is, and whether it is at rest.
//!
//! The reading is plain data ([`Reading`]) with two writers, one for a person or a model to read
//! ([`Reading::text`]) and one for a program ([`Reading::json`]). It knows no platform: a page
//! under a terminal's host, a page under the headless host of a test, and a page in a web view
//! whose reading a script made are all read, and written, the same way. The page's own part is
//! [`Page::reading`](crate::Page::reading); a host that has no such page builds the parts itself.
//!
//! **A name is six letters** ([`reference`]): the element's [`Id`], mixed, in base 32. It is the same
//! at every reading of a page that keeps the element, needs no memory of the last reading and
//! is the same for whoever reads. Two elements of a reading that come to the same name are both marked
//! ([`Reading::find`] refuses it) and each can still be named by its whole id ([`full`]). An
//! element no [`Id`] names (a line of plain text) has none.
//!
//! **What a page puts in the reading is words, not orders.** Everything that comes of the page is
//! cleaned the same way in both writers ([`clean`]): control characters are `?`, long words are cut
//! (a `…` says so), line breaks stay. A secret field's value is not in the reading at all.

use std::collections::{BTreeSet, HashMap, HashSet};

use pencere_view::{AccessibleRole, FieldKind, Id, Key, Mods, Point, Rect, Shortcut, State};
use pencere_wire::messages::numbered;
use serde_json::{Map, Value, json};

use crate::Page;

/// The longest an element's words are in a reading, in characters.
pub const WORDS_LIMIT: usize = 200;
/// The longest what a field holds is in a reading, in characters.
pub const VALUE_LIMIT: usize = 400;
/// The longest a line of an editor is in a reading, in characters.
pub const LINE_LIMIT: usize = 200;
/// The most lines of an editor a reading shows.
pub const LINES_LIMIT: usize = 200;

/// Where the keyboard is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Focus {
    /// On the page: its keys and its commands.
    #[default]
    Page,
    /// In the host's text field of this name.
    Field(Id),
    /// In the page's editor of this name.
    Editor(Id),
    /// In the host's browser of this name: its web page has the keys the page does not keep.
    Browser(Id),
}

/// What is still going on after what a page was told: what an agent waits for before it reads.
/// Motion the host draws (a transition, a box that glides, a caret) is told and is not waited for,
/// so a page that always moves can be at rest. A page whose own program makes a new page at every
/// frame (`animating`) is not: it is always taking in a page, and an agent waits the time it is
/// given.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rest {
    /// The module has more to do of what it was told.
    pub unprocessed: bool,
    /// How many calls the page made to its program have no answer yet. A subscription is not a
    /// call, and neither is what the program pushes.
    pub calls: usize,
    /// An image is still being read or drawn.
    pub images: bool,
    /// The page has not been drawn for what it last was (a page the module made, or an image that
    /// came, that no frame has laid out; a frame the host was asked for and has not drawn), or its
    /// last layout asked for another. Wider than [`Page::unsettled`], which is the last of these
    /// alone.
    pub unsettled: bool,
    /// Something moves (a transition, a gauge): not waited for.
    pub moving: bool,
}

impl Rest {
    /// Whether nothing is left to wait for: the page says all it will, to what it was told. Motion
    /// does not count.
    pub fn resting(&self) -> bool {
        !self.unprocessed && self.calls == 0 && !self.images && !self.unsettled
    }

    /// What is going on, in words, or `None` where nothing is.
    pub fn says(&self) -> Option<String> {
        let mut parts: Vec<String> = Vec::new();
        if self.unprocessed {
            parts.push("the page is still taking in what it was told".to_owned());
        }
        match self.calls {
            0 => {}
            1 => parts.push("1 call is waiting for an answer".to_owned()),
            n => parts.push(format!("{n} calls are waiting for an answer")),
        }
        if self.images {
            parts.push("images are loading".to_owned());
        }
        if self.unsettled {
            parts.push("the page is still settling".to_owned());
        }
        if self.moving {
            parts.push("motion".to_owned());
        }
        (!parts.is_empty()).then(|| parts.join("; "))
    }
}

/// An element of the page as an agent reads it: what it is, its name, its words and where it is.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// What it is: `button`, `text`, `link`, `checkbox`, `switch`, `radio`, `tab`, `menuitem`,
    /// `heading1`…`heading6`, `image`, `progress`, `slider`; `field` and `editor` where a host that
    /// reads them apart does not.
    pub role: String,
    /// The element's [`Id`], where it has one: what it is told by.
    pub id: Option<Id>,
    pub words: String,
    /// Checked, chosen, open, of no use, and the value it shows.
    pub state: State,
    /// Where it stands, in the host's points.
    pub rect: Rect,
    /// A press on it does something, whatever it says it is ([`pencere_view::Accessible::pressable`]).
    pub pressable: bool,
}

/// A text field the host puts over the page.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldReading {
    pub id: Id,
    pub kind: FieldKind,
    pub label: Option<String>,
    pub hint: Option<String>,
    /// What it holds, as far as the host says: `None` for a secret, whose value is never read, and
    /// for a field the host does not know.
    pub value: Option<String>,
    /// Something the page draws lies over it.
    pub covered: bool,
    pub rect: Rect,
}

/// A place in an editor's text: line and column, from 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    pub line: usize,
    pub column: usize,
}

/// What an editor has selected: from where to where, and the text (cut where long).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selected {
    pub from: Place,
    pub to: Place,
    pub text: String,
}

/// An editor of the page as an agent reads it: the lines that show, where the caret is and what is
/// selected.
#[derive(Clone, Debug, PartialEq)]
pub struct EditorReading {
    pub id: Id,
    pub label: Option<String>,
    pub focused: bool,
    /// The number of the first line shown, from 1; 0 where no line shows (the editor is covered).
    pub first: usize,
    /// The lines that show, in order, without their line breaks.
    pub lines: Vec<String>,
    /// How many lines the document has.
    pub total: usize,
    pub caret: Place,
    pub selection: Option<Selected>,
    pub rect: Rect,
}

/// A box of the page that scrolls.
#[derive(Clone, Debug, PartialEq)]
pub struct ScrollReading {
    pub id: Id,
    /// How far down it is scrolled, of the most it can be, in percent.
    pub percent: u8,
    /// There is more above what shows.
    pub above: bool,
    /// There is more below.
    pub below: bool,
    pub rect: Rect,
}

/// What the page can be told to do.
#[derive(Clone, Debug, PartialEq)]
pub struct CommandReading {
    pub id: Id,
    pub title: String,
    /// The keys that do it, written as a person writes them (`cmd+k`, `f2`).
    pub keys: Vec<String>,
    pub enabled: bool,
}

/// A line of the menu an agent asked the page for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuLine {
    pub title: String,
    pub enabled: bool,
}

/// What the host gives the page's reading that only it has: the texts of its fields, where its
/// keyboard is, and the menu it holds for the agent.
#[derive(Default)]
pub struct ReadingInput<'a> {
    /// What the host's field `id` holds, as a text; `None` for a field it does not know. Never
    /// asked of a secret field.
    pub field_text: Option<&'a dyn Fn(Id) -> Option<String>>,
    /// The field the keyboard is in, where it is in one; or the browser, which takes the keyboard as
    /// a field does ([`pencere_view::Keyboard::Field`]).
    pub field_focus: Option<Id>,
    /// The lines of the menu held for the agent, where one is.
    pub menu: Option<Vec<MenuLine>>,
}

/// A reading written, and the line each of its items, fields and editors begins at.
#[derive(Default)]
pub(crate) struct Written {
    pub text: String,
    pub items: Vec<usize>,
    pub fields: Vec<usize>,
    pub editors: Vec<usize>,
}

/// What a page is to an agent.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reading {
    pub focus: Focus,
    pub rest: Rest,
    pub items: Vec<Item>,
    pub fields: Vec<FieldReading>,
    pub editors: Vec<EditorReading>,
    pub scrolls: Vec<ScrollReading>,
    pub commands: Vec<CommandReading>,
    /// The menu held for the agent, where there is one.
    pub menu: Option<Vec<MenuLine>>,
    /// What this reading cannot say of this page, each a sentence.
    pub notes: Vec<String>,
}

/// What [`Reading::find`] came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Found {
    /// No element of the reading is named so.
    Nothing,
    /// More than one is: the whole id ([`full`]) names one.
    Clash,
    /// The element.
    One(Id),
}

const ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

/// One round of splitmix64: the bits of a name mixed so that ids that are small numbers (a page
/// that names its elements by number) do not come to the same names the way their low bits would.
fn mix(number: u64) -> u64 {
    let mut z = number.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// The name of `id`: six letters and digits (base 32, no `i`, `l`, `o`, `u`) of the id mixed,
/// 30 bits of it.
pub fn reference(id: Id) -> String {
    let bits = mix(id.number()) >> 34;
    (0..6)
        .rev()
        .map(|place| {
            let digit = usize::try_from((bits >> (place * 5)) & 31).unwrap_or(0);
            char::from(ALPHABET.get(digit).copied().unwrap_or(b'0'))
        })
        .collect()
}

/// The whole id, 16 hexadecimal digits: what names an element whose [`reference`] two share.
pub fn full(id: Id) -> String {
    format!("{:016x}", id.number())
}

/// Whether `c` is a character that reads as nothing or as another: a control character, one that
/// formats text (a zero-width space, a soft hyphen, the marks that turn it round, the tags that
/// write text a person does not see) or one that ends a line or a paragraph in a way a line break
/// of the reading does not.
fn hidden(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{ad}' | '\u{61c}' | '\u{180e}' | '\u{200b}'..='\u{200f}'
                | '\u{2028}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{feff}'
                | '\u{fff9}'..='\u{fffb}' | '\u{e0000}'..='\u{e007f}'
        )
}

/// What a page said, as it is put in a reading: a control character is `?` (a tab is a space), a
/// line break stays, and what is longer than `limit` characters is cut and ends in `…`. The two
/// writers clean the same way.
pub fn clean(text: &str, limit: usize) -> String {
    let mut out = String::new();
    for (count, c) in text.chars().enumerate() {
        if count == limit {
            out.push('…');
            break;
        }
        out.push(match c {
            '\n' => '\n',
            '\t' => ' ',
            c if hidden(c) => '?',
            c => c,
        });
    }
    out
}

fn role_of(role: AccessibleRole) -> String {
    match role {
        AccessibleRole::Button => "button".to_owned(),
        AccessibleRole::Field(_) => "field".to_owned(),
        AccessibleRole::Editor => "editor".to_owned(),
        AccessibleRole::Text => "text".to_owned(),
        AccessibleRole::Link => "link".to_owned(),
        AccessibleRole::Checkbox => "checkbox".to_owned(),
        AccessibleRole::Switch => "switch".to_owned(),
        AccessibleRole::Radio => "radio".to_owned(),
        AccessibleRole::Tab => "tab".to_owned(),
        AccessibleRole::MenuItem => "menuitem".to_owned(),
        AccessibleRole::Heading(level) => format!("heading{level}"),
        AccessibleRole::Image => "image".to_owned(),
        AccessibleRole::Progress => "progress".to_owned(),
        AccessibleRole::Slider => "slider".to_owned(),
        AccessibleRole::Browser => "browser".to_owned(),
    }
}

impl Item {
    /// The item of what a screen reader is told of an element, which is what the page says of it.
    pub fn of(item: &pencere_view::Accessible) -> Self {
        Self {
            role: role_of(item.role),
            id: item.id,
            words: item.words.clone(),
            state: item.state.clone(),
            rect: item.rect,
            pressable: item.pressable,
        }
    }
}

/// The keys of a shortcut, written as a person writes them and [`Shortcut::parse`] reads them.
pub fn keys_of(shortcut: Shortcut) -> String {
    let Shortcut { key, mods } = shortcut;
    let Mods {
        shift,
        control,
        alt,
        command,
    } = mods;
    let mut parts: Vec<String> = Vec::new();
    for (held, name) in [
        (control, "ctrl"),
        (alt, "alt"),
        (shift, "shift"),
        (command, "cmd"),
    ] {
        if held {
            parts.push(name.to_owned());
        }
    }
    parts.push(match key {
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
        Key::Function(number) => format!("f{number}"),
        Key::Character(' ') => "space".to_owned(),
        Key::Character(c) => c.to_lowercase().collect(),
    });
    parts.join("+")
}

fn rounded(rect: Rect) -> Value {
    json!([
        rect.x.round(),
        rect.y.round(),
        rect.w.round(),
        rect.h.round()
    ])
}

impl Reading {
    /// Every element of the reading that has an [`Id`], once, in the order they are told.
    pub fn ids(&self) -> Vec<Id> {
        let mut seen = HashSet::new();
        let mut ids = Vec::new();
        let all = self
            .items
            .iter()
            .filter_map(|item| item.id)
            .chain(self.fields.iter().map(|field| field.id))
            .chain(self.editors.iter().map(|editor| editor.id))
            .chain(self.scrolls.iter().map(|scroll| scroll.id));
        for id in all {
            if seen.insert(id) {
                ids.push(id);
            }
        }
        ids
    }

    /// The names that more than one element of the reading comes to.
    fn clashes(&self) -> HashSet<String> {
        let mut made: HashMap<String, usize> = HashMap::new();
        for id in self.ids() {
            *made.entry(reference(id)).or_default() += 1;
        }
        made.into_iter()
            .filter(|(_, count)| *count > 1)
            .map(|(name, _)| name)
            .collect()
    }

    /// The element `word` names: a [`reference`] (in any case) or a whole id ([`full`]).
    pub fn find(&self, word: &str) -> Found {
        let word = word.trim().to_ascii_lowercase();
        let ids = self.ids();
        if word.len() == 16
            && let Ok(number) = u64::from_str_radix(&word, 16)
        {
            let id = Id::new(number);
            return if ids.contains(&id) {
                Found::One(id)
            } else {
                Found::Nothing
            };
        }
        let mut named = ids.into_iter().filter(|id| reference(*id) == word);
        match (named.next(), named.next()) {
            (None, _) => Found::Nothing,
            (Some(id), None) => Found::One(id),
            (Some(_), Some(_)) => Found::Clash,
        }
    }

    /// Where the keyboard is, in words: `page`, `field k3f9qa`, `editor k3f9qa`, `browser k3f9qa`.
    fn focus_words(&self) -> String {
        match self.focus {
            Focus::Page => "page".to_owned(),
            Focus::Field(id) => format!("field {}", reference(id)),
            Focus::Editor(id) => format!("editor {}", reference(id)),
            Focus::Browser(id) => format!("browser {}", reference(id)),
        }
    }

    /// The reading written for a person or a model: one line to an element, its name first.
    pub fn text(&self) -> String {
        self.written().text
    }

    /// [`Reading::text`], and the line each item, field and editor of it begins at (from 1), which
    /// the audit tells a flaw by.
    pub(crate) fn written(&self) -> Written {
        let mut at = Written::default();
        let clashes = self.clashes();
        let name = |id: Option<Id>| match id {
            Some(id) => {
                let mark = if clashes.contains(&reference(id)) {
                    "!"
                } else {
                    " "
                };
                format!("{}{mark}", reference(id))
            }
            None => "       ".to_owned(),
        };
        let mut out = String::new();
        out.push_str(&format!("focus: {}\n", self.focus_words()));
        if let Some(says) = self.rest.says() {
            out.push_str(&format!("rest: {says}\n"));
        }
        for note in &self.notes {
            out.push_str(&format!("note: {}\n", clean(note, WORDS_LIMIT)));
        }
        if !self.items.is_empty() {
            out.push_str("items:\n");
        }
        let mut row = out.matches('\n').count();
        for item in &self.items {
            let line = format!(
                "{} {} {}{}\n",
                name(item.id),
                clean(&item.role, WORDS_LIMIT),
                quoted(&clean(&item.words, WORDS_LIMIT)),
                state_words(&item.state)
            );
            at.items.push(row + 1);
            row += line.matches('\n').count();
            out.push_str(&line);
        }
        if !self.fields.is_empty() {
            out.push_str("fields:\n");
        }
        for field in &self.fields {
            at.fields.push(out.matches('\n').count() + 1);
            let kind = match field.kind {
                FieldKind::Line => "field",
                FieldKind::Lines => "field (lines)",
                FieldKind::Secret => "field (secret)",
            };
            let label = field
                .label
                .as_deref()
                .or(field.hint.as_deref())
                .unwrap_or_default();
            out.push_str(&format!(
                "{} {kind} {}",
                name(Some(field.id)),
                quoted(&clean(label, WORDS_LIMIT))
            ));
            // A secret's value is in no reading, whatever the host that built it held.
            if let Some(value) = field
                .value
                .as_ref()
                .filter(|_| field.kind != FieldKind::Secret)
            {
                out.push_str(&format!(" = {}", quoted(&clean(value, VALUE_LIMIT))));
            }
            if field.covered {
                out.push_str(" (covered)");
            }
            out.push('\n');
        }
        if !self.editors.is_empty() {
            out.push_str("editors:\n");
        }
        for editor in &self.editors {
            at.editors.push(out.matches('\n').count() + 1);
            self.write_editor(&mut out, editor, &name(Some(editor.id)));
        }
        if !self.scrolls.is_empty() {
            out.push_str("scrolls:\n");
        }
        for scroll in &self.scrolls {
            let place = match (scroll.above, scroll.below) {
                (false, _) => "at the top",
                (true, false) => "at the bottom",
                (true, true) => "more above and below",
            };
            out.push_str(&format!(
                "{} scroll {}% ({place})\n",
                name(Some(scroll.id)),
                scroll.percent
            ));
        }
        if !self.commands.is_empty() {
            out.push_str("commands:\n");
        }
        for command in &self.commands {
            out.push_str(&format!(
                "  {}",
                quoted(&clean(&command.title, WORDS_LIMIT))
            ));
            if !command.keys.is_empty() {
                let keys: Vec<String> = command
                    .keys
                    .iter()
                    .map(|key| clean(key, WORDS_LIMIT))
                    .collect();
                out.push_str(&format!(" [{}]", keys.join(", ")));
            }
            if !command.enabled {
                out.push_str(" (disabled)");
            }
            out.push('\n');
        }
        if let Some(menu) = &self.menu {
            out.push_str("menu:\n");
            for line in menu {
                out.push_str(&format!("  {}", quoted(&clean(&line.title, WORDS_LIMIT))));
                if !line.enabled {
                    out.push_str(" (disabled)");
                }
                out.push('\n');
            }
        }
        at.text = out;
        at
    }

    fn write_editor(&self, out: &mut String, editor: &EditorReading, name: &str) {
        let label = editor.label.as_deref().unwrap_or_default();
        out.push_str(&format!(
            "{name} editor {}",
            quoted(&clean(label, WORDS_LIMIT))
        ));
        if editor.first > 0 {
            let last = editor.first + editor.lines.len().saturating_sub(1);
            out.push_str(&format!(
                " lines {}-{last} of {}",
                editor.first, editor.total
            ));
        } else {
            out.push_str(&format!(" no lines show of {}", editor.total));
        }
        out.push_str(&format!(
            ", caret {}:{}",
            editor.caret.line, editor.caret.column
        ));
        if let Some(selected) = &editor.selection {
            out.push_str(&format!(
                ", selected {}:{}-{}:{} {}",
                selected.from.line,
                selected.from.column,
                selected.to.line,
                selected.to.column,
                quoted(&clean(&selected.text, LINE_LIMIT))
            ));
        }
        if editor.focused {
            out.push_str(" (keyboard)");
        }
        out.push('\n');
        let width = (editor.first + editor.lines.len()).to_string().len();
        for (at, line) in editor.lines.iter().enumerate() {
            out.push_str(&format!(
                "  {:>width$}| {}\n",
                editor.first + at,
                clean(line, LINE_LIMIT)
            ));
        }
    }

    /// The reading written for a program: one JSON object.
    pub fn json(&self) -> String {
        let clashes = self.clashes();
        let named = |id: Id| {
            let reference = reference(id);
            let clash = clashes.contains(&reference);
            (reference, full(id), clash)
        };
        let items: Vec<Value> = self
            .items
            .iter()
            .map(|item| {
                let mut object = Map::new();
                if let Some(id) = item.id {
                    let (reference, full, clash) = named(id);
                    object.insert("ref".into(), json!(reference));
                    object.insert("id".into(), json!(full));
                    object.insert("clash".into(), json!(clash));
                } else {
                    object.insert("ref".into(), Value::Null);
                    object.insert("id".into(), Value::Null);
                }
                object.insert("role".into(), json!(clean(&item.role, WORDS_LIMIT)));
                object.insert("text".into(), json!(clean(&item.words, WORDS_LIMIT)));
                object.insert("state".into(), state_json(&item.state));
                object.insert("box".into(), rounded(item.rect));
                Value::Object(object)
            })
            .collect();
        let fields: Vec<Value> = self
            .fields
            .iter()
            .map(|field| {
                let (reference, full, clash) = named(field.id);
                json!({
                    "ref": reference,
                    "id": full,
                    "clash": clash,
                    "kind": match field.kind {
                        FieldKind::Line => "line",
                        FieldKind::Lines => "lines",
                        FieldKind::Secret => "secret",
                    },
                    "label": field.label.as_deref().map(|l| clean(l, WORDS_LIMIT)),
                    "hint": field.hint.as_deref().map(|h| clean(h, WORDS_LIMIT)),
                    "value": field
                        .value
                        .as_deref()
                        .filter(|_| field.kind != FieldKind::Secret)
                        .map(|v| clean(v, VALUE_LIMIT)),
                    "covered": field.covered,
                    "box": rounded(field.rect),
                })
            })
            .collect();
        let editors: Vec<Value> = self
            .editors
            .iter()
            .map(|editor| {
                let (reference, full, clash) = named(editor.id);
                let place = |at: Place| json!({ "line": at.line, "column": at.column });
                json!({
                    "ref": reference,
                    "id": full,
                    "clash": clash,
                    "label": editor.label.as_deref().map(|l| clean(l, WORDS_LIMIT)),
                    "focused": editor.focused,
                    "lines": {
                        "from": editor.first,
                        "total": editor.total,
                        "text": editor.lines.iter().map(|line| clean(line, LINE_LIMIT)).collect::<Vec<_>>(),
                    },
                    "caret": place(editor.caret),
                    "selection": editor.selection.as_ref().map(|selected| json!({
                        "from": place(selected.from),
                        "to": place(selected.to),
                        "text": clean(&selected.text, LINE_LIMIT),
                    })),
                    "box": rounded(editor.rect),
                })
            })
            .collect();
        let scrolls: Vec<Value> = self
            .scrolls
            .iter()
            .map(|scroll| {
                let (reference, full, clash) = named(scroll.id);
                json!({
                    "ref": reference,
                    "id": full,
                    "clash": clash,
                    "percent": scroll.percent,
                    "above": scroll.above,
                    "below": scroll.below,
                    "box": rounded(scroll.rect),
                })
            })
            .collect();
        let commands: Vec<Value> = self
            .commands
            .iter()
            .map(|command| {
                json!({
                    "title": clean(&command.title, WORDS_LIMIT),
                    "keys": command.keys.iter().map(|key| clean(key, WORDS_LIMIT)).collect::<Vec<_>>(),
                    "enabled": command.enabled,
                })
            })
            .collect();
        let menu = self.menu.as_ref().map(|lines| {
            lines
                .iter()
                .map(|line| json!({ "title": clean(&line.title, WORDS_LIMIT), "enabled": line.enabled }))
                .collect::<Vec<_>>()
        });
        let focus = match self.focus {
            Focus::Page => json!({ "kind": "page" }),
            Focus::Field(id) => json!({ "kind": "field", "ref": reference(id), "id": full(id) }),
            Focus::Editor(id) => json!({ "kind": "editor", "ref": reference(id), "id": full(id) }),
            Focus::Browser(id) => {
                json!({ "kind": "browser", "ref": reference(id), "id": full(id) })
            }
        };
        let value = json!({
            "focus": focus,
            "rest": {
                "resting": self.rest.resting(),
                "says": self.rest.says(),
                "unprocessed": self.rest.unprocessed,
                "calls": self.rest.calls,
                "images": self.rest.images,
                "unsettled": self.rest.unsettled,
                "moving": self.rest.moving,
            },
            "notes": self.notes.iter().map(|note| clean(note, WORDS_LIMIT)).collect::<Vec<_>>(),
            "items": items,
            "fields": fields,
            "editors": editors,
            "scrolls": scrolls,
            "commands": commands,
            "menu": menu,
        });
        value.to_string()
    }
}

fn quoted(words: &str) -> String {
    // A line break in what is said stays, and what follows it is indented under the line.
    format!("\"{}\"", words.replace('\n', "\n        "))
}

fn state_words(state: &State) -> String {
    let mut parts: Vec<String> = Vec::new();
    match state.checked {
        Some(true) => parts.push("checked".to_owned()),
        Some(false) => parts.push("unchecked".to_owned()),
        None => {}
    }
    if state.selected {
        parts.push("selected".to_owned());
    }
    match state.expanded {
        Some(true) => parts.push("expanded".to_owned()),
        Some(false) => parts.push("collapsed".to_owned()),
        None => {}
    }
    if state.disabled {
        parts.push("disabled".to_owned());
    }
    if let Some(value) = &state.value {
        parts.push(format!("value {}", quoted(&clean(value, WORDS_LIMIT))));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" [{}]", parts.join(", "))
    }
}

fn state_json(state: &State) -> Value {
    let mut object = Map::new();
    if let Some(checked) = state.checked {
        object.insert("checked".into(), json!(checked));
    }
    if state.selected {
        object.insert("selected".into(), json!(true));
    }
    if let Some(expanded) = state.expanded {
        object.insert("expanded".into(), json!(expanded));
    }
    if state.disabled {
        object.insert("disabled".into(), json!(true));
    }
    if let Some(value) = &state.value {
        object.insert("value".into(), json!(clean(value, WORDS_LIMIT)));
    }
    Value::Object(object)
}

/// The calls a page made to its program that have not been answered, by the number it gave each.
/// Read from the name of a message's one member and its number alone, as a host that carries the
/// page's words reads them: nothing else of the six messages. A subscription is not a call, what
/// the program pushes and how far a call has got answer nothing.
#[derive(Debug, Default)]
pub(crate) struct Owed(BTreeSet<u64>);

impl Owed {
    /// How many calls are waiting for an answer.
    pub(crate) fn count(&self) -> usize {
        self.0.len()
    }

    /// Nothing is waited for: the page's view is over.
    pub(crate) fn clear(&mut self) {
        self.0.clear();
    }

    /// What the page said to its program: a call is waited for, a cancel gives it up.
    pub(crate) fn said(&mut self, text: &str) {
        if !names(text, &["call", "cancel"]) {
            return;
        }
        match numbered(text) {
            Some(("call", id)) => {
                self.0.insert(id);
            }
            Some(("cancel", id)) => {
                self.0.remove(&id);
            }
            _ => {}
        }
    }

    /// What the program said to the page: a response ends the wait for its call.
    pub(crate) fn heard(&mut self, text: &str) {
        if !names(text, &["response"]) {
            return;
        }
        if let Some(("response", id)) = numbered(text) {
            self.0.remove(&id);
        }
    }
}

/// Whether the one member of the JSON object `text` is one of `members`: its name, read where it
/// stands, and nothing of what it holds.
fn names(text: &str, members: &[&str]) -> bool {
    let Some(rest) = text.trim_start().strip_prefix('{') else {
        return false;
    };
    let Some(rest) = rest.trim_start().strip_prefix('"') else {
        return false;
    };
    members.iter().any(|member| {
        rest.strip_prefix(member)
            .is_some_and(|after| after.starts_with('"'))
    })
}

/// What an agent reads of `page`: its outline as the last frame told it, with what only the host has
/// (`input`) and the part of the editors the outline does not say.
pub(crate) fn read(page: &mut Page, input: &ReadingInput<'_>, rest: Rest) -> Reading {
    let outline = page.outline().clone();
    let place = page.placement();
    let fields: Vec<FieldReading> = outline
        .fields
        .iter()
        .map(|field| FieldReading {
            id: field.id,
            kind: field.kind,
            label: field.label.clone(),
            hint: field.hint.clone(),
            // A secret's value is not asked for: it is in no reading, whatever the host holds.
            value: match field.kind {
                FieldKind::Secret => None,
                _ => input.field_text.and_then(|text| text(field.id)),
            },
            covered: field.covered,
            rect: field.rect,
        })
        .collect();
    let field_ids: HashSet<Id> = fields.iter().map(|field| field.id).collect();
    let keyboard = page.focused_editor();
    let mut editors = Vec::new();
    let mut items = Vec::new();
    for item in &outline.accessible {
        match (item.role, item.id) {
            (AccessibleRole::Editor, Some(id)) => {
                editors.push(editor_of(page, id, item, keyboard == Some(id)));
            }
            (AccessibleRole::Field(_), Some(id)) if field_ids.contains(&id) => {}
            _ => items.push(Item::of(item)),
        }
    }
    let editor_ids: HashSet<Id> = editors.iter().map(|editor| editor.id).collect();
    let scrolls = page
        .laid()
        .map(|laid| {
            laid.scroll_areas()
                .into_iter()
                .filter(|(id, extent, _)| !editor_ids.contains(id) && extent.max_offset() > 0.5)
                .map(|(id, extent, rect)| {
                    let most = extent.max_offset();
                    let at = extent.offset.y;
                    ScrollReading {
                        id,
                        percent: ((at / most) * 100.0).round().clamp(0.0, 100.0) as u8,
                        above: at > 0.5,
                        below: at < most - 0.5,
                        rect: place.to_host(rect),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let focus = if let Some(id) = keyboard {
        Focus::Editor(id)
    } else {
        match input.field_focus {
            Some(id) if field_ids.contains(&id) => Focus::Field(id),
            Some(id) if page.browsers().iter().any(|browser| browser.id == id) => {
                Focus::Browser(id)
            }
            _ => Focus::Page,
        }
    };
    Reading {
        focus,
        rest,
        items,
        fields,
        editors,
        scrolls,
        commands: outline
            .commands
            .iter()
            .map(|command| CommandReading {
                id: command.id,
                title: command.title.clone(),
                keys: command.keys.iter().map(|keys| keys_of(*keys)).collect(),
                enabled: command.enabled,
            })
            .collect(),
        menu: input.menu.clone(),
        notes: Vec::new(),
    }
}

/// The places in an editor's document at the top and at the bottom of what shows of it: `under`
/// says what is at a height of the page (the document's place, where the editor is there and
/// nothing covers it or scrolls it away). The room from `y` to `y + h` is probed at even steps;
/// the first and the last that find the editor are narrowed to the edge by halving, so that an
/// editor that a card covers in part or a box that scrolls cuts off, or that is taller than the
/// page, gives the lines that show.
pub(crate) fn shown_span(
    mut under: impl FnMut(f32) -> Option<usize>,
    y: f32,
    h: f32,
) -> Option<(usize, usize)> {
    const PROBES: u8 = 24;
    const HALVINGS: u8 = 10;
    let span = (h - 2.0).max(0.0);
    let at = |step: u8| y + 1.0 + span * f32::from(step) / f32::from(PROBES);
    let hits: Vec<Option<usize>> = (0..=PROBES).map(|step| under(at(step))).collect();
    let first = hits.iter().position(Option::is_some)?;
    let last = hits.iter().rposition(Option::is_some)?;
    let mut top = hits.get(first).copied().flatten()?;
    let mut bottom = hits.get(last).copied().flatten()?;
    let (first, last) = (u8::try_from(first).ok()?, u8::try_from(last).ok()?);
    if first > 0 {
        let (mut miss, mut hit) = (at(first - 1), at(first));
        for _ in 0..HALVINGS {
            let middle = f32::midpoint(miss, hit);
            match under(middle) {
                Some(found) => {
                    hit = middle;
                    top = found;
                }
                None => miss = middle,
            }
        }
    }
    if last < PROBES {
        let (mut hit, mut miss) = (at(last), at(last + 1));
        for _ in 0..HALVINGS {
            let middle = f32::midpoint(hit, miss);
            match under(middle) {
                Some(found) => {
                    hit = middle;
                    bottom = found;
                }
                None => miss = middle,
            }
        }
    }
    Some((top, bottom))
}

/// What the page's editor `id` shows: the lines between the place under the top edge of its room
/// and the place under the bottom edge, where the caret is and what is selected. An editor nothing
/// reaches (covered, scrolled out of a box that scrolls) shows no line.
fn editor_of(
    page: &mut Page,
    id: Id,
    item: &pencere_view::Accessible,
    focused: bool,
) -> EditorReading {
    let rect = item.rect;
    let x = rect.x + rect.w / 2.0;
    let shown = shown_span(
        |y| {
            page.place_in_editor(Point::new(x, y))
                .filter(|(found, _)| *found == id)
                .map(|(_, at)| at)
        },
        rect.y,
        rect.h,
    );
    let label = (!item.words.is_empty()).then(|| item.words.clone());
    let Some(buffer) = page.editor(id) else {
        return EditorReading {
            id,
            label,
            focused,
            first: 0,
            lines: Vec::new(),
            total: 0,
            caret: Place { line: 1, column: 1 },
            selection: None,
            rect,
        };
    };
    let place_of = |at: usize| {
        let line = buffer.line_of(at);
        Place {
            line: line + 1,
            column: at.saturating_sub(buffer.line_start(line)) + 1,
        }
    };
    let (first, lines) = match shown {
        Some((top, bottom)) => {
            let (first, last) = (buffer.line_of(top), buffer.line_of(bottom));
            let (first, last) = (first.min(last), first.max(last));
            let last = last.min(first + LINES_LIMIT - 1);
            // As much of a line as a reading holds, not the whole of a line of megabytes.
            let line = |line: usize| {
                let from = buffer.line_start(line);
                let to = buffer.line_end(line).min(from + LINE_LIMIT + 1);
                buffer.slice(from, to).unwrap_or_default()
            };
            (first + 1, (first..=last).map(line).collect())
        }
        None => (0, Vec::new()),
    };
    let main = buffer.main();
    let selection = (!main.is_empty()).then(|| Selected {
        from: place_of(main.start()),
        to: place_of(main.end()),
        text: buffer
            .slice(main.start(), main.end().min(main.start() + LINE_LIMIT + 1))
            .unwrap_or_default(),
    });
    EditorReading {
        id,
        label,
        focused,
        first,
        lines,
        total: buffer.len_lines(),
        caret: place_of(main.head),
        selection,
        rect,
    }
}
