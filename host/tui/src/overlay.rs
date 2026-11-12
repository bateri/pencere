//! What the screen shows over the page, and the person answers before the page is told anything: a
//! menu the page asked for, the line of a panel (the path of a file to give the page, or of a place
//! to save to) and the address of a link.
//!
//! They are drawn into the cells of the frame, after the page, so that they are one more thing the
//! diff of the screen writes; while one is up the keys and the pointer are its own. None of it is
//! the page's to draw, and the page is told only what the person chose.

use std::collections::HashMap;
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use pencere_view::{
    FieldKind, FileEntry, Fill, Key, Menu, MenuEntry, Mods, Panel, Role, Saving, Shortcut,
};
use unicode_width::UnicodeWidthChar;

use crate::field::{Look, TextField};
use crate::grid::{Attrs, Bg, Grid};
use crate::theme::Theme;

/// The most bytes of a file the page is given to draw by its handle.
const LONGEST_FILE: u64 = 64 * 1024 * 1024;

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

// --- Writing into the grid -----------------------------------------------------------------

/// What a run of characters is drawn in.
#[derive(Clone, Copy)]
struct Ink {
    fg: crate::color::Rgb,
    bg: crate::color::Rgb,
    attrs: Attrs,
}

/// `text` from `col` on row `row`, no further than `until` (a column), in `ink`; the column after it.
fn write(grid: &mut Grid, col: u16, row: u16, until: u16, text: &str, ink: Ink) -> u16 {
    let mut at = col;
    for c in text.chars() {
        let w = UnicodeWidthChar::width(c).unwrap_or(0) as u16;
        if w == 0 {
            continue;
        }
        if at + w > until {
            break;
        }
        if let Some(cell) = grid.cell_mut(at, row) {
            cell.ch = c;
            cell.fg = ink.fg;
            cell.bg = Bg::Rgb(ink.bg);
            cell.attrs = ink.attrs;
            cell.tail = false;
        }
        if w == 2
            && let Some(cell) = grid.cell_mut(at + 1, row)
        {
            cell.ch = ' ';
            cell.fg = ink.fg;
            cell.bg = Bg::Rgb(ink.bg);
            cell.attrs = ink.attrs;
            cell.tail = true;
        }
        at += w;
    }
    at
}

/// `count` of `c` from `col`.
fn fill(grid: &mut Grid, col: u16, row: u16, count: u16, c: char, ink: Ink) {
    for at in col..col.saturating_add(count) {
        if let Some(cell) = grid.cell_mut(at, row) {
            cell.ch = c;
            cell.fg = ink.fg;
            cell.bg = Bg::Rgb(ink.bg);
            cell.attrs = ink.attrs;
            cell.tail = false;
        }
    }
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

    /// Whether the cells `col`..`col + cols` and `row`..`row + rows` share any with this.
    pub fn meets(&self, col: u16, row: u16, cols: u16, rows: u16) -> bool {
        col < self.col.saturating_add(self.cols)
            && self.col < col.saturating_add(cols)
            && row < self.row.saturating_add(self.rows)
            && self.row < row.saturating_add(rows)
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
    area: Area,
    selected: Option<usize>,
}

impl MenuBox {
    /// The box of `menu`, where the page asked (`at`, a cell from 0) as far as the screen of `cols`
    /// by `rows` lets it: moved left and up to fit, and no taller than the screen. The line of a
    /// `Paste` is there, and not to be picked: a terminal's pasteboard is read by the person's own
    /// paste, which the terminal does.
    pub fn new(menu: &Menu, at: (u16, u16), screen: (u16, u16)) -> Self {
        let lines: Vec<Line> = menu
            .entries
            .iter()
            .map(|entry| match entry {
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
                MenuEntry::Paste { title, .. } => Line::Item {
                    title: title.clone(),
                    keys: String::new(),
                    enabled: false,
                },
            })
            .collect();
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

    /// A press of the main button at `col`, `row`.
    pub fn press(&mut self, col: u16, row: u16) -> Chose {
        if !self.area.contains(col, row) {
            return Chose::Dismissed;
        }
        match self.line_at(col, row) {
            Some(index) if self.lines.get(index).is_some_and(Line::picks) => Chose::Pick(index),
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
                return (self.selected.map_or(Chose::Nothing, Chose::Pick), false);
            }
            Key::Escape => return (Chose::Dismissed, false),
            _ => {}
        }
        (Chose::Nothing, before != self.selected)
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

    pub fn draw(&self, grid: &mut Grid, theme: &Theme) {
        let Area {
            col,
            row,
            cols,
            rows,
        } = self.area;
        if cols < 3 || rows < 2 {
            return;
        }
        let ground = theme.role(Role::Raised(3));
        let line = theme.role(Role::Border).mix(theme.foreground, 0.35);
        let text = theme.role(Role::Foreground);
        let dim = theme.role(Role::Dim);
        let chosen = theme.role(Role::Selection);
        let edge = Ink {
            fg: line,
            bg: ground,
            attrs: Attrs::NONE,
        };
        // The frame.
        fill(grid, col, row, cols, '─', edge);
        fill(grid, col, row + rows - 1, cols, '─', edge);
        write(grid, col, row, col + 1, "╭", edge);
        write(grid, col + cols - 1, row, col + cols, "╮", edge);
        write(grid, col, row + rows - 1, col + 1, "╰", edge);
        write(grid, col + cols - 1, row + rows - 1, col + cols, "╯", edge);
        for (index, entry) in self.lines.iter().enumerate() {
            let at = row + 1 + u16::try_from(index).unwrap_or(0);
            if at + 1 >= row + rows {
                break;
            }
            write(grid, col, at, col + 1, "│", edge);
            write(grid, col + cols - 1, at, col + cols, "│", edge);
            match entry {
                Line::Separator => {
                    fill(grid, col + 1, at, cols - 2, '─', edge);
                    write(grid, col, at, col + 1, "├", edge);
                    write(grid, col + cols - 1, at, col + cols, "┤", edge);
                }
                Line::Item {
                    title,
                    keys,
                    enabled,
                } => {
                    let ground = if self.selected == Some(index) {
                        chosen
                    } else {
                        ground
                    };
                    let ink = Ink {
                        fg: if *enabled { text } else { dim },
                        bg: ground,
                        attrs: Attrs::NONE,
                    };
                    fill(grid, col + 1, at, cols - 2, ' ', ink);
                    let inner = col + cols - 1;
                    write(grid, col + 2, at, inner.saturating_sub(1), title, ink);
                    if !keys.is_empty() {
                        let start = inner.saturating_sub(1 + width_of(keys) as u16);
                        let dimmer = Ink { fg: dim, ..ink };
                        write(grid, start.max(col + 2), at, inner, keys, dimmer);
                    }
                }
            }
        }
    }
}

// --- The line of the bottom row ------------------------------------------------------------

/// The ink of the bar at the foot of the screen, over the page.
fn bar(theme: &Theme) -> Ink {
    Ink {
        fg: theme.role(Role::Foreground),
        bg: theme.role(Role::Raised(3)),
        attrs: Attrs::NONE,
    }
}

/// The address of a link, shown on the bottom row as a link the terminal can open (OSC 8), until a
/// key or a press turns it away.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LinkBar {
    url: String,
}

impl LinkBar {
    pub fn new(url: String) -> Self {
        Self { url }
    }

    pub fn area(&self, screen: (u16, u16)) -> Area {
        Area {
            col: 0,
            row: screen.1.saturating_sub(1),
            cols: screen.0,
            rows: 1,
        }
    }

    /// Draws the bar; where the link is (the row and its columns), for the screen to write as one.
    pub fn draw(&self, grid: &mut Grid, theme: &Theme) -> Option<crate::paint::Link> {
        let (cols, rows) = (grid.cols(), grid.rows());
        if cols < 8 || rows == 0 {
            return None;
        }
        let row = rows - 1;
        let ink = bar(theme);
        fill(grid, 0, row, cols, ' ', ink);
        let note = " Esc closes ";
        let note_width = width_of(note) as u16;
        let accent = Ink {
            fg: theme.role(Role::Accent),
            attrs: Attrs::UNDERLINE,
            ..ink
        };
        let label = " Link ";
        let after = write(
            grid,
            0,
            row,
            cols,
            label,
            Ink {
                attrs: Attrs::BOLD,
                ..ink
            },
        );
        let room = cols.saturating_sub(after + note_width + 1);
        let shown = shorten(&self.url, usize::from(room));
        let end = write(grid, after, row, cols, &shown, accent);
        if cols > note_width + end {
            let dim = Ink {
                fg: theme.role(Role::Dim),
                ..ink
            };
            write(grid, cols - note_width, row, cols, note, dim);
        }
        Some(crate::paint::Link {
            row,
            from: after,
            to: end,
            url: self.url.clone(),
        })
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

/// A panel for the person to give the page files or a place to save in, as one line at the foot of
/// the screen: the path, which the person types (a Tab completes it), Enter takes and Escape turns
/// away.
pub(crate) struct PanelBar {
    panel: Panel,
    field: TextField,
    /// What is said of the path under the line: why it is not taken, or what a Tab found.
    note: Option<String>,
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
                // A folder is where the file goes under the name the page proposed.
                let target = if path.is_dir() { path.join(name) } else { path };
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
                let handle = handles.make(target);
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

    pub fn area(&self, screen: (u16, u16)) -> Area {
        let rows = if self.note.is_some() { 2 } else { 1 };
        Area {
            col: 0,
            row: screen.1.saturating_sub(rows),
            cols: screen.0,
            rows: rows.min(screen.1),
        }
    }

    /// Draws the line, and gives the cell of the caret.
    pub fn draw(&self, grid: &mut Grid, theme: &Theme) -> Option<(u16, u16)> {
        let (cols, rows) = (grid.cols(), grid.rows());
        if cols < 12 || rows == 0 {
            return None;
        }
        let row = rows - 1;
        let ink = bar(theme);
        fill(grid, 0, row, cols, ' ', ink);
        let label = format!(" {}: ", self.word());
        let after = write(
            grid,
            0,
            row,
            cols,
            &label,
            Ink {
                fg: theme.role(Role::Accent),
                attrs: Attrs::BOLD,
                ..ink
            },
        );
        let look = Look {
            text: ink.fg,
            hint: theme.role(Role::Dim),
            selection: theme.role(Role::Selection),
        };
        let hint = match self.panel {
            Panel::Pick { .. } => "a path — Tab completes, Enter takes, Esc turns away",
            Panel::Save { .. } => "where to — Tab completes, Enter takes, Esc turns away",
        };
        let caret = self
            .field
            .draw(grid, (after, row), (cols - after, 1), look, Some(hint));
        if let Some(note) = &self.note
            && row > 0
        {
            let above = row - 1;
            let warn = Ink {
                fg: theme.role(Role::Dim),
                ..ink
            };
            fill(grid, 0, above, cols, ' ', warn);
            write(grid, 1, above, cols, note, warn);
        }
        caret
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
    let handle = (!folder).then(|| handles.make(path.to_path_buf()));
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

// --- The handles ---------------------------------------------------------------------------

/// The files the person gave the page, each by the handle the page names it by: 32 lowercase hex
/// digits, which are nothing once the screen ends. The page draws one as `file:` and the handle,
/// and is answered from here, never from a path of its own.
#[derive(Debug, Default)]
pub(crate) struct Handles {
    files: Mutex<HashMap<String, PathBuf>>,
}

impl Handles {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// A new handle for `path`.
    pub fn make(&self, path: PathBuf) -> String {
        let mut files = self.files.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            let handle = random_handle();
            if !files.contains_key(&handle) {
                files.insert(handle.clone(), path);
                return handle;
            }
        }
    }

    pub fn path_of(&self, handle: &str) -> Option<PathBuf> {
        self.files
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(handle)
            .cloned()
    }

    /// The bytes of the file under `handle`: none for a handle that was not made, a file that is
    /// gone or one too large to draw.
    pub fn read(&self, handle: &str) -> Option<Vec<u8>> {
        let path = self.path_of(handle)?;
        let meta = std::fs::metadata(&path).ok()?;
        if !meta.is_file() || meta.len() > LONGEST_FILE {
            return None;
        }
        std::fs::read(path).ok()
    }
}

/// 32 hex digits no one chose: the keys of two `RandomState`s (the system's randomness), the time
/// and a count.
fn random_handle() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let hash = |salt: u64| {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u64(salt);
        hasher.finish()
    };
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos() as u64);
    let count = COUNT.fetch_add(1, Ordering::Relaxed);
    format!("{:016x}{:016x}", hash(nanos), hash(count))
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

    #[test]
    fn the_line_of_a_paste_is_there_and_cannot_be_picked() {
        let menu = menu(vec![MenuEntry::Paste {
            title: "Paste".to_owned(),
            enabled: true,
            editor: pencere_view::Id::named("doc"),
        }]);
        let mut open = MenuBox::new(&menu, (0, 0), (40, 10));
        assert_eq!(open.titles(), ["Paste"]);
        assert_eq!(
            open.key(Key::Down, Mods::default()),
            (Chose::Nothing, false)
        );
        assert_eq!(open.press(2, 1), Chose::Nothing);
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
            handles.path_of(&saving.handle),
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
}
