//! What the screen shows over the page, and the person answers before the page is told anything: a
//! menu the page asked for, the line of a panel (the path of a file to give the page, or of a place
//! to save to) and the address of a link.
//!
//! They are laid out on the cells of the window and drawn over the page as pictures; while one is up
//! the keys and the pointer are its own. None of it is the page's to draw, and the page is told only
//! what the person chose.

use std::path::{Path, PathBuf};

use pencere_view::{
    Border, Color, FieldKind, FileEntry, Fill, Key, Menu, MenuEntry, Mods, Paint, Panel, Quad,
    Rect, Role, Run, Saving, Shadow, Shortcut,
};
use unicode_width::UnicodeWidthChar;

use crate::chrome::Paper;
use crate::field::TextField;
use crate::files::Handles;

// --- The label of a shortcut ---------------------------------------------------------------

/// A shortcut as a menu writes it at the end of a line: the modifiers and the key, `Ctrl+Shift+K`.
pub(crate) fn label(shortcut: &Shortcut) -> String {
    let Shortcut { key, mods } = shortcut;
    let mut text = String::new();
    for (held, name) in [
        (mods.control, "Ctrl"),
        (mods.alt, "Alt"),
        (mods.shift, "Shift"),
        (mods.command, "Super"),
    ] {
        if held {
            text.push_str(name);
            text.push('+');
        }
    }
    match key {
        Key::Up => text.push_str("Up"),
        Key::Down => text.push_str("Down"),
        Key::Left => text.push_str("Left"),
        Key::Right => text.push_str("Right"),
        Key::PageUp => text.push_str("PageUp"),
        Key::PageDown => text.push_str("PageDown"),
        Key::Home => text.push_str("Home"),
        Key::End => text.push_str("End"),
        Key::Enter => text.push_str("Enter"),
        Key::Escape => text.push_str("Esc"),
        Key::Tab => text.push_str("Tab"),
        Key::Backspace => text.push_str("Backspace"),
        Key::Delete => text.push_str("Delete"),
        Key::Function(n) => {
            text.push('F');
            text.push_str(&n.to_string());
        }
        Key::Character(c) => text.extend(c.to_uppercase()),
    }
    text
}

/// Whether `url` is one a link may show: an `http` or `https` address of printable ASCII, not more
/// than 8192 bytes (the rule every host holds a link to; the page's own sandbox applies it too, and
/// a control byte must never reach the terminal, where it would be a sequence).
pub(crate) fn openable(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    (lower.starts_with("http://") || lower.starts_with("https://"))
        && url.len() <= 8192
        && url.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
}

fn width_of(text: &str) -> usize {
    text.chars()
        .map(|c| UnicodeWidthChar::width(c).unwrap_or(0))
        .sum()
}

/// The rectangle of cells `col`, `row`, `cols`, `rows`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Area {
    pub col: u16,
    pub row: u16,
    pub cols: u16,
    pub rows: u16,
}

impl Area {
    pub fn contains(&self, col: u16, row: u16) -> bool {
        col >= self.col
            && col < self.col.saturating_add(self.cols)
            && row >= self.row
            && row < self.row.saturating_add(self.rows)
    }
}

// --- The menu ------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
enum Line {
    Item {
        title: String,
        keys: String,
        enabled: bool,
    },
    Separator,
}

impl Line {
    fn picks(&self) -> bool {
        matches!(self, Self::Item { enabled: true, .. })
    }
}

/// What a key or a press did to a menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Chose {
    /// Nothing the menu cares about: it is up still.
    Nothing,
    /// The entry of this index (separators counted) was picked.
    Pick(usize),
    /// The person turned it away.
    Dismissed,
}

/// A menu the page asked for, over the page, at the cells the page said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MenuBox {
    serial: u64,
    lines: Vec<Line>,
    /// The entry of the page's menu each line is (a page picks by the index of its own entries,
    /// which the lines are not when one of them is not shown).
    entries: Vec<usize>,
    area: Area,
    selected: Option<usize>,
}

impl MenuBox {
    /// The box of `menu`, where the page asked (`at`, a cell from 0) as far as the screen of `cols`
    /// by `rows` lets it: moved left and up to fit, and no taller than the screen. A `Paste` has no
    /// line (a terminal's pasteboard is read by the person's own paste, which the terminal does, so
    /// the line could never be picked), and a separator it leaves at an edge or beside another goes
    /// with it.
    pub fn new(menu: &Menu, at: (u16, u16), screen: (u16, u16)) -> Self {
        let mut lines: Vec<Line> = Vec::new();
        let mut entries: Vec<usize> = Vec::new();
        let mut skipped = false;
        for (index, entry) in menu.entries.iter().enumerate() {
            let line = match entry {
                MenuEntry::Item {
                    title,
                    keys,
                    enabled,
                } => Line::Item {
                    title: title.clone(),
                    keys: keys.as_ref().map(label).unwrap_or_default(),
                    enabled: *enabled,
                },
                MenuEntry::Separator => Line::Separator,
                MenuEntry::Paste { .. } => {
                    skipped = true;
                    continue;
                }
            };
            lines.push(line);
            entries.push(index);
        }
        if skipped {
            // Only what the missing line left alone: the page's own separators are as it wrote them.
            let mut kept: Vec<(Line, usize)> = Vec::new();
            for (line, entry) in lines.into_iter().zip(entries) {
                let lone = line == Line::Separator
                    && kept
                        .last()
                        .is_none_or(|(before, _)| *before == Line::Separator);
                if !lone {
                    kept.push((line, entry));
                }
            }
            while kept
                .last()
                .is_some_and(|(line, _)| *line == Line::Separator)
            {
                kept.pop();
            }
            (lines, entries) = kept.into_iter().unzip();
        }
        let widest = lines
            .iter()
            .map(|line| match line {
                Line::Item { title, keys, .. } => {
                    width_of(title)
                        + if keys.is_empty() {
                            0
                        } else {
                            2 + width_of(keys)
                        }
                }
                Line::Separator => 0,
            })
            .max()
            .unwrap_or(0);
        let (screen_cols, screen_rows) = screen;
        // A space on each side of a line, and the border.
        let cols = u16::try_from(widest + 4)
            .unwrap_or(u16::MAX)
            .min(screen_cols);
        let rows = u16::try_from(lines.len() + 2)
            .unwrap_or(u16::MAX)
            .min(screen_rows);
        let col = at.0.min(screen_cols.saturating_sub(cols));
        let row = at.1.min(screen_rows.saturating_sub(rows));
        Self {
            serial: menu.serial,
            lines,
            entries,
            area: Area {
                col,
                row,
                cols,
                rows,
            },
            selected: None,
        }
    }

    pub fn serial(&self) -> u64 {
        self.serial
    }

    #[cfg(test)]
    pub fn area(&self) -> Area {
        self.area
    }

    /// The line a cell is on, if it is on one.
    fn line_at(&self, col: u16, row: u16) -> Option<usize> {
        let inside = col > self.area.col
            && col + 1 < self.area.col + self.area.cols
            && row > self.area.row
            && row + 1 < self.area.row + self.area.rows;
        inside
            .then(|| usize::from(row - self.area.row - 1))
            .filter(|index| *index < self.lines.len())
    }

    /// The pointer is over `col`, `row`: whether the line that is chosen changed.
    pub fn hover(&mut self, col: u16, row: u16) -> bool {
        let over = self
            .line_at(col, row)
            .filter(|index| self.lines.get(*index).is_some_and(Line::picks));
        let changed = over != self.selected;
        self.selected = over;
        changed
    }

    /// The pick of the line `line`: of the entry of the page's menu it is.
    fn pick(&self, line: usize) -> Chose {
        self.entries
            .get(line)
            .map_or(Chose::Nothing, |entry| Chose::Pick(*entry))
    }

    /// A press of the main button at `col`, `row`.
    pub fn press(&mut self, col: u16, row: u16) -> Chose {
        if !self.area.contains(col, row) {
            return Chose::Dismissed;
        }
        match self.line_at(col, row) {
            Some(index) if self.lines.get(index).is_some_and(Line::picks) => self.pick(index),
            _ => Chose::Nothing,
        }
    }

    /// A key: the arrows and Home and End choose, Enter picks, Escape turns the menu away; the
    /// rest is the menu's, and does nothing. The second is whether the menu looks other than it did.
    pub fn key(&mut self, key: Key, mods: Mods) -> (Chose, bool) {
        if mods.control || mods.alt || mods.command {
            return (Chose::Nothing, false);
        }
        let before = self.selected;
        let count = self.lines.len();
        let step = |from: Option<usize>, down: bool| -> Option<usize> {
            if count == 0 {
                return None;
            }
            let start = match (from, down) {
                (Some(at), true) => at + 1,
                (Some(at), false) => at + count - 1,
                (None, true) => 0,
                (None, false) => count - 1,
            };
            (0..count)
                .map(|offset| {
                    if down {
                        (start + offset) % count
                    } else {
                        (start + count - offset % count) % count
                    }
                })
                .find(|index| self.lines.get(*index).is_some_and(Line::picks))
        };
        match key {
            Key::Down => self.selected = step(self.selected, true),
            Key::Up => self.selected = step(self.selected, false),
            Key::Home => self.selected = step(None, true),
            Key::End => self.selected = step(None, false),
            Key::Enter => {
                return (
                    self.selected
                        .map_or(Chose::Nothing, |index| self.pick(index)),
                    false,
                );
            }
            Key::Escape => return (Chose::Dismissed, false),
            _ => {}
        }
        (Chose::Nothing, before != self.selected)
    }

    /// Whether there is nothing in it to choose or to see.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// The titles of the lines, for a test.
    #[cfg(test)]
    pub fn titles(&self) -> Vec<String> {
        self.lines
            .iter()
            .map(|line| match line {
                Line::Item { title, .. } => title.clone(),
                Line::Separator => "-".to_owned(),
            })
            .collect()
    }

    /// The menu as items of the page's draw list: a card on the cells the box has (half a cell
    /// in from their edge, so that the lines are in whole rows), the line the pointer or the arrows
    /// are on on a ground of its own, a hairline where there is a separator, each line's words and
    /// its shortcut at the right, dim.
    pub(crate) fn paint(&self, paper: &Paper<'_>) -> Vec<Paint> {
        let Area {
            col,
            row,
            cols,
            rows,
        } = self.area;
        if cols < 3 || rows < 2 {
            return Vec::new();
        }
        let whole = paper.cells(col, row, cols, rows);
        let (cw, ch) = paper.cell;
        let card = Rect::new(
            whole.x + cw / 2.0,
            whole.y + ch / 2.0,
            whole.w - cw,
            whole.h - ch,
        );
        let hair = paper.hairline();
        let mut items = vec![Paint::Quad(Quad {
            rect: card,
            radii: [cw * 0.5; 4],
            fill: Some(Color::from(Role::Raised(1))),
            gradient: None,
            border: Some(Border {
                width: hair,
                color: Color::from(Role::Border),
            }),
            shadow: Some(Shadow {
                dx: 0.0,
                dy: ch * 0.2,
                blur: ch * 0.8,
                color: Color::from(Role::Shadow).faded(0.5),
            }),
            clip: paper.window,
        })];
        let font = paper.face(false);
        let inner = paper.cells(col + 1, row + 1, cols - 2, rows - 2);
        for (index, line) in self.lines.iter().enumerate() {
            let at = row + 1 + u16::try_from(index).unwrap_or(0);
            if at + 1 >= row + rows {
                break;
            }
            let band = paper.cells(col + 1, at, cols - 2, 1);
            match line {
                Line::Separator => {
                    let rule = Rect::new(band.x, band.y + (ch - hair) / 2.0, band.w, hair);
                    items.push(paper.quad(rule, Role::Border, inner));
                }
                Line::Item {
                    title,
                    keys,
                    enabled,
                } => {
                    if self.selected == Some(index) {
                        items.push(paper.quad(band, Role::Selection, inner));
                    }
                    let top = paper.centred(&font, band.y, ch);
                    let color = if *enabled {
                        Role::Foreground
                    } else {
                        Role::Dim
                    };
                    items.push(paper.text((band.x + cw, top), title, font, color, inner));
                    if !keys.is_empty() {
                        #[allow(clippy::cast_precision_loss)]
                        let width = width_of(keys) as f32 * cw;
                        let x = (band.right() - cw - width).max(band.x + cw);
                        items.push(paper.text((x, top), keys, font, Role::Dim, inner));
                    }
                }
            }
        }
        items
    }
}

// --- The line of the bottom row ------------------------------------------------------------

/// The address of a link, shown on the bottom row until a key or a press turns it away: that is the
/// question put to the person, and Enter or a press on the row is the answer that follows the
/// link (where it goes is the screen's: the browser where the person is, the pasteboard where they
/// are not).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LinkBar {
    url: String,
    /// What following it does is to copy it, and the row says so.
    copies: bool,
}

impl LinkBar {
    pub fn new(url: String, copies: bool) -> Self {
        Self { url, copies }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// The bar as items of the page's draw list: the row at the foot of the window, on the ground of
    /// a bar, the address of the link underlined as a link is, and what its keys do at the right.
    pub(crate) fn paint(&self, paper: &Paper<'_>, screen: (u16, u16)) -> Vec<Paint> {
        let (cols, rows) = screen;
        if cols < 8 || rows == 0 {
            return Vec::new();
        }
        let area = self.area(screen);
        let bar = paper.cells(area.col, area.row, area.cols, 1);
        let font = paper.face(false);
        let bold = paper.face(true);
        let top = paper.centred(&font, bar.y, bar.h);
        let cw = paper.cell.0;
        let label = " Link ";
        let note = if self.copies {
            " Enter copies  Esc closes "
        } else {
            " Enter opens  Esc closes "
        };
        let after = width_of(label);
        let note_width = width_of(note);
        let room = usize::from(cols).saturating_sub(after + note_width + 1);
        let shown = shorten(&self.url, room);
        #[allow(clippy::cast_precision_loss)]
        let (url_x, end) = (
            bar.x + after as f32 * cw,
            bar.x + (after + width_of(&shown)) as f32 * cw,
        );
        let ground = paper.over_foot(bar);
        let mut items = vec![paper.quad(ground, Role::Raised(3), ground)];
        items.push(paper.text((bar.x, top), label, bold, Role::Foreground, bar));
        if let Paint::Text(run) = paper.text((url_x, top), &shown, font, Role::Accent, bar) {
            items.push(Paint::Text(Run {
                underline: true,
                ..run
            }));
        }
        #[allow(clippy::cast_precision_loss)]
        let note_x = bar.right() - note_width as f32 * cw;
        if note_x > end {
            items.push(paper.text((note_x, top), note, font, Role::Dim, bar));
        }
        items
    }

    pub fn area(&self, screen: (u16, u16)) -> Area {
        Area {
            col: 0,
            row: screen.1.saturating_sub(1),
            cols: screen.0,
            rows: 1,
        }
    }
}

/// `text` cut to `room` columns, with an ellipsis where it was cut.
fn shorten(text: &str, room: usize) -> String {
    if width_of(text) <= room {
        return text.to_owned();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = UnicodeWidthChar::width(c).unwrap_or(0);
        if used + w + 1 > room {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push('…');
    out
}

/// A few words the screen says of what it did, on the bottom row, where the page was not asked and is
/// not told: it takes no key and no press, and the screen takes it away after a while.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NoticeBar {
    text: String,
}

impl NoticeBar {
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_owned(),
        }
    }

    /// The row at the foot of the window, on the ground of a bar, with the words in it.
    pub(crate) fn paint(&self, paper: &Paper<'_>, screen: (u16, u16)) -> Vec<Paint> {
        let (cols, rows) = screen;
        if cols < 8 || rows == 0 {
            return Vec::new();
        }
        let bar = paper.cells(0, rows - 1, cols, 1);
        let bold = paper.face(true);
        let top = paper.centred(&bold, bar.y, bar.h);
        let words = format!(" {} ", self.text);
        let ground = paper.over_foot(bar);
        vec![
            paper.quad(ground, Role::Raised(3), ground),
            paper.text((bar.x, top), &words, bold, Role::Foreground, bar),
        ]
    }
}

// --- The panel -----------------------------------------------------------------------------

/// What the person did with the line of a panel.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Answered {
    /// It is up still.
    Open,
    /// The person turned it away.
    Turned,
    /// What the person gave the page.
    Gave(Vec<FileEntry>),
    /// The place the person chose to save to.
    Chose(Saving),
}

/// The columns of its line that a panel keeps for the path before it says what its keys do.
const PATH_ROOM: u16 = 24;

/// A panel for the person to give the page files or a place to save in, as one line at the foot of
/// the screen: the path, which the person types (a Tab completes it), Enter takes and Escape turns
/// away.
pub(crate) struct PanelBar {
    panel: Panel,
    field: TextField,
    /// What is said of the path under the line: why it is not taken, or what a Tab found.
    note: Option<String>,
    /// The file that is there, which the person was told Enter replaces: a second Enter at the same
    /// place does.
    replacing: Option<PathBuf>,
}

impl PanelBar {
    pub fn panel(&self) -> &Panel {
        &self.panel
    }

    pub fn new(panel: Panel) -> Self {
        let mut field = TextField::new(FieldKind::Line);
        if let Panel::Save { name, .. } = &panel {
            field.fill(&Fill::text(name.clone()).select_all());
        }
        Self {
            panel,
            field,
            note: None,
            replacing: None,
        }
    }

    /// The line's text, for a test.
    #[cfg(test)]
    pub fn text(&self) -> &str {
        self.field.text()
    }

    fn word(&self) -> &'static str {
        match self.panel {
            Panel::Pick { .. } => "Open",
            Panel::Save { .. } => "Save as",
        }
    }

    /// What the keys do, said at the right of the line as a link's bar says its Escape.
    fn keys(&self) -> &'static str {
        match self.panel {
            Panel::Pick { .. } => " Enter opens  Esc cancels ",
            Panel::Save { .. } => " Enter saves  Esc cancels ",
        }
    }

    /// A key at the line. Where it is taken (Enter) the answer is the person's.
    pub fn key(
        &mut self,
        key: Key,
        mods: Mods,
        typed: Option<&str>,
        handles: &Handles,
    ) -> Answered {
        match key {
            Key::Escape => return Answered::Turned,
            Key::Enter => return self.take(handles),
            Key::Tab if !mods.shift => {
                self.note = None;
                self.complete();
                return Answered::Open;
            }
            _ => {}
        }
        if self.field.state().takes(key, mods) {
            self.note = None;
            self.field.key(key, mods, typed);
        }
        Answered::Open
    }

    /// Text typed at once (a paste): into the line.
    pub fn paste(&mut self, text: &str) {
        self.note = None;
        self.field.paste(text);
    }

    /// A press at the line: the caret goes there.
    pub fn press(&mut self, col: u16, extend: bool) {
        let first = self.word().chars().count() as u16 + 3;
        let cells = usize::from(col.saturating_sub(first));
        self.field.place(cells, u16::MAX, extend);
    }

    fn take(&mut self, handles: &Handles) -> Answered {
        let text = self.field.text().trim().to_owned();
        if text.is_empty() {
            self.note = Some("Type a path first.".to_owned());
            return Answered::Open;
        }
        let path = resolve(&text);
        match &self.panel {
            Panel::Pick { .. } => match entry_of(&path, handles) {
                Ok(entry) => Answered::Gave(vec![entry]),
                Err(why) => {
                    self.note = Some(why);
                    Answered::Open
                }
            },
            Panel::Save { name, .. } => {
                // A folder is where the file goes under the name the page proposed: its last part,
                // which is all that a name of a file is (the page's `../x` or `/x` puts the file
                // where the person did not choose).
                let target = if path.is_dir() {
                    path.join(pencere::safe_name(name))
                } else {
                    path
                };
                let Some(file_name) = target
                    .file_name()
                    .map(|file_name| file_name.to_string_lossy().into_owned())
                else {
                    self.note = Some("That is not a name for a file.".to_owned());
                    return Answered::Open;
                };
                let parent = target
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty());
                if parent.is_some_and(|parent| !parent.is_dir()) {
                    self.note = Some("There is no such folder.".to_owned());
                    return Answered::Open;
                }
                match std::fs::symlink_metadata(&target) {
                    Ok(meta) if meta.is_dir() => {
                        self.note = Some("That is a folder.".to_owned());
                        return Answered::Open;
                    }
                    // A file that is there is the person's: they are told, and a second Enter at the
                    // same place says it may go (a Save panel of the system asks the same).
                    Ok(_) if self.replacing.as_ref() != Some(&target) => {
                        self.note = Some("That file is there: Enter again replaces it.".to_owned());
                        self.replacing = Some(target);
                        return Answered::Open;
                    }
                    _ => {}
                }
                let handle = handles.place(target);
                Answered::Chose(Saving {
                    handle,
                    name: file_name,
                })
            }
        }
    }

    /// Completes the path as far as it goes: the one name that fits, or what the names that fit
    /// have in common (the rest are listed under the line).
    fn complete(&mut self) {
        let text = self.field.text().to_owned();
        let (folder_text, stem) = match text.rfind('/') {
            Some(at) => (&text[..=at], &text[at + 1..]),
            None => ("", text.as_str()),
        };
        let folder = if folder_text.is_empty() {
            PathBuf::from(".")
        } else {
            resolve(folder_text)
        };
        let Ok(listing) = std::fs::read_dir(&folder) else {
            self.note = Some("There is no such folder.".to_owned());
            return;
        };
        let mut names: Vec<(String, bool)> = listing
            .filter_map(Result::ok)
            .filter_map(|found| {
                let name = found.file_name().to_string_lossy().into_owned();
                (name.starts_with(stem) && (stem.starts_with('.') || !name.starts_with('.')))
                    .then(|| (name, found.path().is_dir()))
            })
            .collect();
        names.sort();
        let Some(first) = names.first().cloned() else {
            self.note = Some("Nothing begins like that.".to_owned());
            return;
        };
        let mut common = first.0.clone();
        for (name, _) in &names {
            let shared = common
                .chars()
                .zip(name.chars())
                .take_while(|(a, b)| a == b)
                .count();
            common = common.chars().take(shared).collect();
        }
        let mut completed = format!("{folder_text}{common}");
        if names.len() == 1 && first.1 {
            completed.push('/');
        }
        self.field.fill(&Fill::text(completed));
        if names.len() > 1 {
            let list: Vec<String> = names
                .iter()
                .map(|(name, folder)| {
                    if *folder {
                        format!("{name}/")
                    } else {
                        name.clone()
                    }
                })
                .collect();
            self.note = Some(list.join("  "));
        }
    }

    /// The line as items of the page's draw list: the row at the foot of the window (and the one above
    /// it that says why the path was not taken), on the ground of a bar, what the line is for, the
    /// path with its selection and its caret, and what the keys do at the right where there is room.
    pub(crate) fn paint(&self, paper: &Paper<'_>, screen: (u16, u16)) -> Vec<Paint> {
        let (cols, rows) = screen;
        if cols < 12 || rows == 0 {
            return Vec::new();
        }
        let area = self.area(screen);
        let (cw, ch) = paper.cell;
        let bar = paper.cells(0, rows - 1, cols, 1);
        let font = paper.face(false);
        let top = paper.centred(&font, bar.y, ch);
        let mut items = Vec::new();
        if let Some(note) = &self.note
            && area.rows > 1
        {
            let above = paper.cells(0, rows - 2, cols, 1);
            let ground = paper.over_foot(above);
            items.push(paper.quad(ground, Role::Raised(3), ground));
            let at = paper.centred(&font, above.y, ch);
            items.push(paper.text((above.x + cw, at), note, font, Role::Dim, above));
            items.push(paper.quad(bar, Role::Raised(3), bar));
        } else {
            let ground = paper.over_foot(bar);
            items.push(paper.quad(ground, Role::Raised(3), ground));
        }
        let label = format!(" {}: ", self.word());
        let after = u16::try_from(width_of(&label)).unwrap_or(u16::MAX);
        items.push(paper.text((bar.x, top), &label, paper.face(true), Role::Accent, bar));
        let keys = self.keys();
        let keys_width = u16::try_from(width_of(keys)).unwrap_or(u16::MAX);
        let told = cols.saturating_sub(after + keys_width) >= PATH_ROOM;
        let room = if told {
            cols - after - keys_width
        } else {
            cols - after
        };
        let hint = match (&self.panel, told) {
            (Panel::Pick { .. }, true) => "a path (Tab completes)",
            (Panel::Save { .. }, true) => "where to (Tab completes)",
            (Panel::Pick { .. }, false) => {
                "a path \u{2014} Tab completes, Enter takes, Esc turns away"
            }
            (Panel::Save { .. }, false) => {
                "where to \u{2014} Tab completes, Enter takes, Esc turns away"
            }
        };
        let text = paper.cells(after, rows - 1, room, 1);
        crate::chrome::line(
            paper,
            &mut items,
            &self.field,
            Some(hint),
            text,
            true,
            Role::Foreground,
        );
        if told {
            let x = bar.right() - f32::from(keys_width) * cw;
            items.push(paper.text((x, top), keys, font, Role::Dim, bar));
        }
        items
    }

    pub fn area(&self, screen: (u16, u16)) -> Area {
        let rows = if self.note.is_some() { 2 } else { 1 };
        Area {
            col: 0,
            row: screen.1.saturating_sub(rows),
            cols: screen.0,
            rows: rows.min(screen.1),
        }
    }
}

/// What the person typed, as a path: `~` is their home, a relative one is from where the program is.
pub(crate) fn resolve(text: &str) -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let path = match (text.strip_prefix("~/"), text == "~", &home) {
        (Some(rest), _, Some(home)) => home.join(rest),
        (_, true, Some(home)) => home.clone(),
        _ => PathBuf::from(text),
    };
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir().map_or(path.clone(), |here| here.join(&path))
    }
}

/// The file at `path`, as the page is given it.
fn entry_of(path: &Path, handles: &Handles) -> Result<FileEntry, String> {
    let meta = std::fs::metadata(path).map_err(|why| match why.kind() {
        std::io::ErrorKind::NotFound => "There is no such file.".to_owned(),
        std::io::ErrorKind::PermissionDenied => "That is not yours to read.".to_owned(),
        _ => format!("It cannot be read: {why}."),
    })?;
    let name = path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let folder = meta.is_dir();
    let handle = (!folder).then(|| handles.give(path, &meta));
    Ok(FileEntry {
        handle,
        name,
        size: if folder { 0 } else { meta.len() },
        media_type: media_type_of(path),
        path: Some(path.to_string_lossy().into_owned()),
        folder,
    })
}

/// A guess at a file's type, from the end of its name.
pub(crate) fn media_type_of(path: &Path) -> String {
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase());
    match extension.as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("svg") => "image/svg+xml",
        Some("pdf") => "application/pdf",
        Some("txt" | "md" | "log") => "text/plain",
        Some("json") => "application/json",
        _ => "application/octet-stream",
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pencere_view::{Point, Shortcut};

    fn item(title: &str, keys: Option<&str>, enabled: bool) -> MenuEntry {
        MenuEntry::Item {
            title: title.to_owned(),
            keys: keys.and_then(Shortcut::parse),
            enabled,
        }
    }

    fn menu(entries: Vec<MenuEntry>) -> Menu {
        Menu {
            at: Point::new(0.0, 0.0),
            entries,
            serial: 7,
        }
    }

    #[test]
    fn a_shortcut_is_written_with_its_modifiers_in_one_order() {
        let shortcut = Shortcut::parse("shift+ctrl+k").unwrap();
        assert_eq!(label(&shortcut), "Ctrl+Shift+K");
        assert_eq!(label(&Shortcut::parse("f2").unwrap()), "F2");
        assert_eq!(label(&Shortcut::parse("alt+enter").unwrap()), "Alt+Enter");
    }

    #[test]
    fn the_arrows_choose_lines_that_can_be_picked_and_wrap_round() {
        let menu = menu(vec![
            item("Cut", None, false),
            item("Copy", None, true),
            MenuEntry::Separator,
            item("Delete", Some("ctrl+d"), true),
        ]);
        let mut open = MenuBox::new(&menu, (2, 2), (40, 10));
        assert_eq!(open.key(Key::Down, Mods::default()), (Chose::Nothing, true));
        assert_eq!(open.selected, Some(1));
        open.key(Key::Down, Mods::default());
        assert_eq!(open.selected, Some(3));
        open.key(Key::Down, Mods::default());
        assert_eq!(open.selected, Some(1), "past the last it is the first");
        open.key(Key::Up, Mods::default());
        assert_eq!(open.selected, Some(3), "before the first it is the last");
        assert_eq!(open.key(Key::Enter, Mods::default()).0, Chose::Pick(3));
        assert_eq!(open.key(Key::Escape, Mods::default()).0, Chose::Dismissed);
    }

    #[test]
    fn a_press_on_a_line_picks_it_and_one_outside_turns_the_menu_away() {
        let menu = menu(vec![
            item("Cut", None, false),
            item("Copy", None, true),
            MenuEntry::Separator,
        ]);
        let mut open = MenuBox::new(&menu, (4, 3), (40, 12));
        let area = open.area();
        // The lines are inside the border: the first is on the row under the top edge.
        assert_eq!(open.press(area.col + 2, area.row + 1), Chose::Nothing);
        assert_eq!(open.press(area.col + 2, area.row + 2), Chose::Pick(1));
        assert_eq!(open.press(area.col + 2, area.row + 3), Chose::Nothing);
        assert_eq!(open.press(0, 0), Chose::Dismissed);
        // A press on the border is the menu's and picks nothing.
        assert_eq!(open.press(area.col, area.row + 2), Chose::Nothing);
    }

    #[test]
    fn a_menu_that_would_leave_the_screen_is_moved_to_fit() {
        let menu = menu(vec![item("A long line of a menu", Some("ctrl+l"), true)]);
        let open = MenuBox::new(&menu, (38, 9), (40, 10));
        let area = open.area();
        assert!(
            area.col + area.cols <= 40 && area.row + area.rows <= 10,
            "{area:?}"
        );
        assert_eq!(open.serial(), 7);
        assert_eq!(open.titles(), ["A long line of a menu"]);
    }

    fn paste() -> MenuEntry {
        MenuEntry::Paste {
            title: "Paste".to_owned(),
            enabled: true,
            editor: pencere_view::Id::named("doc"),
        }
    }

    #[test]
    fn a_paste_has_no_line_and_the_separator_it_leaves_alone_goes_with_it() {
        let sep = || MenuEntry::Separator;
        let titles =
            |entries: Vec<MenuEntry>| MenuBox::new(&menu(entries), (0, 0), (40, 10)).titles();
        // Nothing of it is shown, and the separator before it was the end of the menu.
        assert_eq!(
            titles(vec![
                item("Cut", None, true),
                item("Copy", None, true),
                sep(),
                paste()
            ]),
            ["Cut", "Copy"]
        );
        assert_eq!(
            titles(vec![sep(), paste(), item("Cut", None, true)]),
            ["Cut"]
        );
        // Two separators that met where it was are one.
        assert_eq!(
            titles(vec![
                item("Cut", None, true),
                sep(),
                paste(),
                sep(),
                item("Delete", None, true)
            ]),
            ["Cut", "-", "Delete"]
        );
        // The separators of a menu with no paste are the page's own, as it wrote them.
        assert_eq!(
            titles(vec![sep(), item("Cut", None, true), sep()]),
            ["-", "Cut", "-"]
        );
        assert!(MenuBox::new(&menu(vec![paste()]), (0, 0), (40, 10)).is_empty());
    }

    #[test]
    fn a_pick_is_the_entry_of_the_pages_menu_whatever_lines_went_before_it() {
        let menu = menu(vec![
            item("Cut", None, true),
            paste(),
            MenuEntry::Separator,
            item("Delete", None, true),
        ]);
        let mut open = MenuBox::new(&menu, (0, 0), (40, 10));
        assert_eq!(open.titles(), ["Cut", "-", "Delete"]);
        open.key(Key::End, Mods::default());
        assert_eq!(open.key(Key::Enter, Mods::default()).0, Chose::Pick(3));
        let area = open.area();
        assert_eq!(open.press(area.col + 2, area.row + 3), Chose::Pick(3));
        assert_eq!(open.press(area.col + 2, area.row + 1), Chose::Pick(0));
    }

    #[test]
    fn a_path_is_taken_for_a_file_that_is_there_and_not_for_one_that_is_not() {
        let dir = std::env::temp_dir().join(format!("pencere-tui-panel-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("note.txt");
        std::fs::write(&file, b"hello").unwrap();
        let handles = Handles::new();
        let mut bar = PanelBar::new(Panel::Pick { multiple: false });
        bar.paste(&dir.join("gone.txt").to_string_lossy());
        assert_eq!(
            bar.key(Key::Enter, Mods::default(), None, &handles),
            Answered::Open
        );
        assert_eq!(bar.note.as_deref(), Some("There is no such file."));
        let mut bar = PanelBar::new(Panel::Pick { multiple: false });
        bar.paste(&file.to_string_lossy());
        let Answered::Gave(entries) = bar.key(Key::Enter, Mods::default(), None, &handles) else {
            panic!("the file was not given");
        };
        let entry = entries.first().unwrap();
        assert_eq!(
            (entry.name.as_str(), entry.size, entry.folder),
            ("note.txt", 5, false)
        );
        assert_eq!(entry.media_type, "text/plain");
        let handle = entry.handle.as_deref().unwrap();
        assert_eq!(handle.len(), 32);
        assert!(
            handle
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        assert_eq!(handles.read(handle).as_deref(), Some(&b"hello"[..]));
        assert_eq!(handles.read("0".repeat(32).as_str()), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_tab_completes_a_path_as_far_as_the_names_agree() {
        let dir = std::env::temp_dir().join(format!("pencere-tui-tab-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("source")).unwrap();
        std::fs::write(dir.join("sample.txt"), b"").unwrap();
        std::fs::write(dir.join("sample.md"), b"").unwrap();
        let handles = Handles::new();
        let mut bar = PanelBar::new(Panel::Pick { multiple: false });
        bar.paste(&format!("{}/so", dir.display()));
        bar.key(Key::Tab, Mods::default(), None, &handles);
        assert_eq!(
            bar.text(),
            format!("{}/source/", dir.display()),
            "one name, a folder"
        );
        let mut bar = PanelBar::new(Panel::Pick { multiple: false });
        bar.paste(&format!("{}/sa", dir.display()));
        bar.key(Key::Tab, Mods::default(), None, &handles);
        assert_eq!(
            bar.text(),
            format!("{}/sample.", dir.display()),
            "what the names share"
        );
        assert_eq!(bar.note.as_deref(), Some("sample.md  sample.txt"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_place_to_save_to_is_a_name_in_a_folder_that_is_there() {
        let dir = std::env::temp_dir().join(format!("pencere-tui-save-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let handles = Handles::new();
        let panel = Panel::Save {
            name: "report.txt".to_owned(),
            media_type: None,
        };
        let mut bar = PanelBar::new(panel.clone());
        assert_eq!(bar.text(), "report.txt", "the page's name is proposed");
        bar.paste(&dir.to_string_lossy());
        let Answered::Chose(saving) = bar.key(Key::Enter, Mods::default(), None, &handles) else {
            panic!("no place was chosen");
        };
        assert_eq!(
            saving.name, "report.txt",
            "a folder is given the proposed name"
        );
        assert_eq!(
            handles.place_of(&saving.handle),
            Some(dir.join("report.txt"))
        );
        let mut bar = PanelBar::new(panel);
        bar.paste(&dir.join("nowhere").join("a.txt").to_string_lossy());
        assert_eq!(
            bar.key(Key::Enter, Mods::default(), None, &handles),
            Answered::Open
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_name_the_page_proposes_is_its_last_part_in_the_folder_the_person_types() {
        let dir = std::env::temp_dir().join(format!("pencere-tui-name-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("inside")).unwrap();
        let handles = Handles::new();
        for (proposed, file) in [
            ("../escaped.txt", "escaped.txt"),
            ("/etc/escaped.txt", "escaped.txt"),
            ("a/b/c.txt", "c.txt"),
        ] {
            let mut bar = PanelBar::new(Panel::Save {
                name: proposed.to_owned(),
                media_type: None,
            });
            bar.paste(&dir.join("inside").to_string_lossy());
            let Answered::Chose(saving) = bar.key(Key::Enter, Mods::default(), None, &handles)
            else {
                panic!("no place was chosen for {proposed}");
            };
            assert_eq!(saving.name, file, "{proposed}");
            assert_eq!(
                handles.place_of(&saving.handle),
                Some(dir.join("inside").join(file)),
                "{proposed}"
            );
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_file_that_is_there_is_replaced_at_the_second_enter_and_a_folder_is_not_a_place() {
        let dir = std::env::temp_dir().join(format!("pencere-tui-there-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("folder")).unwrap();
        std::fs::write(dir.join("there.txt"), b"theirs").unwrap();
        let handles = Handles::new();
        let panel = Panel::Save {
            name: "x.txt".to_owned(),
            media_type: None,
        };
        let mut bar = PanelBar::new(panel.clone());
        bar.paste(&dir.join("there.txt").to_string_lossy());
        assert_eq!(
            bar.key(Key::Enter, Mods::default(), None, &handles),
            Answered::Open
        );
        assert_eq!(
            bar.note.as_deref(),
            Some("That file is there: Enter again replaces it.")
        );
        assert!(
            matches!(
                bar.key(Key::Enter, Mods::default(), None, &handles),
                Answered::Chose(_)
            ),
            "the second Enter at the same place"
        );
        // Another place is asked about on its own.
        std::fs::write(dir.join("other.txt"), b"theirs").unwrap();
        let mut bar = PanelBar::new(panel.clone());
        bar.paste(&dir.join("there.txt").to_string_lossy());
        bar.key(Key::Enter, Mods::default(), None, &handles);
        bar.paste(&dir.join("other.txt").to_string_lossy());
        assert_eq!(
            bar.key(Key::Enter, Mods::default(), None, &handles),
            Answered::Open,
            "an Enter at another place is not the answer to the question about this one"
        );
        // A folder that has the name the page proposes is not a place: the name is taken from it.
        std::fs::create_dir_all(dir.join("folder").join("x.txt")).unwrap();
        let mut bar = PanelBar::new(panel);
        bar.paste(&dir.join("folder").to_string_lossy());
        assert_eq!(
            bar.key(Key::Enter, Mods::default(), None, &handles),
            Answered::Open
        );
        assert_eq!(bar.note.as_deref(), Some("That is a folder."));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
