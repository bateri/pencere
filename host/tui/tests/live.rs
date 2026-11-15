//! A program whose terminal hosts no view, on a real pty: where the terminal draws pictures it draws
//! the view it carries as a picture of the window, takes the keys and the mouse, follows the window's
//! size, and gives the terminal back as it was; where it does not, the program has its fallback.
//!
//! The program is the example written against the Rust library (`examples/containers-rust`, which
//! `make check-heavy` builds first), and the app `pencere build` makes of `examples/containers-lua`
//! with each kind of runner (the command, built with and without its terminal, which `make
//! check-heavy` builds first too); the terminal is a pty read by a small emulator that follows
//! what is written to it as far as these tests look (where the cursor is put, what is written, what
//! is cleared, the colours that are asked for, the modes). Heavy: each run compiles the interpreter
//! the page runs in.
//!
//! The terminal here is the weakest there is: `TERM=xterm-256color`, no true colour, and no answer
//! to anything but the device attributes — what a terminal that knows nothing of pencere, of the
//! Kitty protocols or of colour queries does. A test that wants a better one says so.

#![allow(unsafe_code)]
#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::indexing_slicing
)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use pencere_fake_host::pty::{Output, Pty, Size, wait};

#[path = "common/window.rs"]
mod window;
use window::Window;

/// A program built beside the tests (in the directory of the target the tests are in).
fn built(name: &str, how: &str) -> PathBuf {
    let exe = std::env::current_exe().expect("this test's path");
    let dir = exe
        .parent()
        .and_then(std::path::Path::parent)
        .expect("the target directory");
    let found: PathBuf = dir.join(name);
    assert!(
        found.is_file(),
        "{} is missing: `{how}` first (make check-heavy does)",
        found.display()
    );
    found
}

/// The example program, built beside the tests.
fn program() -> String {
    built("containers-rust", "cargo build -p containers-rust")
        .to_string_lossy()
        .into_owned()
}

/// What a terminal that follows the stream knows of the screen.
struct Emulator {
    cols: usize,
    rows: usize,
    cells: Vec<Vec<char>>,
    row: usize,
    col: usize,
    alternate: bool,
    cursor_shown: bool,
    true_color: bool,
    palette: bool,
    /// The private modes that are on.
    modes: std::collections::BTreeSet<u32>,
    /// What came and is not a whole sequence or character yet.
    pending: Vec<u8>,
}

impl Emulator {
    fn new(cols: usize, rows: usize) -> Self {
        Self {
            cols,
            rows,
            cells: vec![vec![' '; cols]; rows],
            row: 0,
            col: 0,
            alternate: false,
            cursor_shown: true,
            true_color: false,
            palette: false,
            modes: std::collections::BTreeSet::new(),
            pending: Vec::new(),
        }
    }

    fn resize(&mut self, cols: usize, rows: usize) {
        *self = Self {
            modes: std::mem::take(&mut self.modes),
            alternate: self.alternate,
            true_color: self.true_color,
            palette: self.palette,
            ..Self::new(cols, rows)
        };
    }

    fn text(&self) -> String {
        self.cells
            .iter()
            .map(|row| row.iter().collect::<String>().trim_end().to_owned())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Where `needle` is on the screen (row, column), from 0.
    fn find(&self, needle: &str) -> Option<(usize, usize)> {
        let wanted: Vec<char> = needle.chars().collect();
        self.cells.iter().enumerate().find_map(|(row, cells)| {
            cells
                .windows(wanted.len())
                .position(|window| window == wanted)
                .map(|col| (row, col))
        })
    }

    fn put(&mut self, c: char) {
        if self.col < self.cols && self.row < self.rows {
            self.cells[self.row][self.col] = c;
        }
        self.col = (self.col + 1).min(self.cols);
    }

    fn feed(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);
        let data = std::mem::take(&mut self.pending);
        let mut at = 0;
        while at < data.len() {
            let byte = data[at];
            match byte {
                0x1b => match self.escape(&data[at..]) {
                    Some(used) => at += used,
                    None => break,
                },
                b'\r' => {
                    self.col = 0;
                    at += 1;
                }
                b'\n' => {
                    self.row = (self.row + 1).min(self.rows - 1);
                    at += 1;
                }
                0..=0x1f => at += 1,
                _ => {
                    let length = match byte {
                        0..=0x7f => 1,
                        0xc0..=0xdf => 2,
                        0xe0..=0xef => 3,
                        _ => 4,
                    };
                    // A character that was cut where the program was stopped (its write ended
                    // before the whole of it) is followed by an escape, which begins a sequence
                    // of its own: it is not part of the character.
                    let cut = data
                        .get(at + 1..(at + length).min(data.len()))
                        .is_some_and(|rest| rest.iter().any(|byte| !(0x80..=0xbf).contains(byte)));
                    if cut || (0x80..=0xbf).contains(&byte) {
                        self.put('\u{fffd}');
                        at += 1;
                        continue;
                    }
                    let Some(bytes) = data.get(at..at + length) else {
                        break;
                    };
                    for c in String::from_utf8_lossy(bytes).chars() {
                        self.put(c);
                    }
                    at += length;
                }
            }
        }
        self.pending = data[at..].to_vec();
    }

    /// The escape sequence at the front of `data`: how long, or `None` where it has not all come.
    fn escape(&mut self, data: &[u8]) -> Option<usize> {
        let second = *data.get(1)?;
        match second {
            b'[' => {
                let end = data
                    .iter()
                    .enumerate()
                    .skip(2)
                    .find(|(_, byte)| (0x40..=0x7e).contains(*byte))?
                    .0;
                let body = std::str::from_utf8(&data[2..end]).unwrap_or("");
                self.csi(body, data[end]);
                Some(end + 1)
            }
            b']' | b'_' | b'P' | b'^' | b'X' => {
                let mut at = 2;
                while at < data.len() {
                    if second == b']' && data[at] == 7 {
                        return Some(at + 1);
                    }
                    if data[at] == 0x1b {
                        if at + 1 >= data.len() {
                            return None;
                        }
                        if data[at + 1] == b'\\' {
                            return Some(at + 2);
                        }
                    }
                    at += 1;
                }
                None
            }
            _ => Some(2),
        }
    }

    fn csi(&mut self, body: &str, last: u8) {
        let private = body.starts_with(['?', '<', '>', '=']);
        let numbers: Vec<usize> = body
            .trim_start_matches(['?', '<', '>', '='])
            .split([';', ':'])
            .filter(|part| !part.is_empty())
            .map(|part| part.parse().unwrap_or(0))
            .collect();
        let first = |default: usize| {
            numbers
                .first()
                .copied()
                .filter(|n| *n > 0)
                .unwrap_or(default)
        };
        if private {
            if matches!(last, b'h' | b'l') {
                for mode in &numbers {
                    let on = last == b'h';
                    match mode {
                        1049 => self.alternate = on,
                        25 => self.cursor_shown = on,
                        _ => {}
                    }
                    if on {
                        self.modes.insert(*mode as u32);
                    } else {
                        self.modes.remove(&(*mode as u32));
                    }
                }
            }
            return;
        }
        match last {
            b'H' | b'f' => {
                self.row = (first(1) - 1).min(self.rows - 1);
                self.col = (numbers.get(1).copied().filter(|n| *n > 0).unwrap_or(1) - 1)
                    .min(self.cols - 1);
            }
            b'J' => {
                if numbers.first().copied().unwrap_or(0) == 2 {
                    self.cells = vec![vec![' '; self.cols]; self.rows];
                }
            }
            b'K' => {
                for col in self.col..self.cols {
                    self.cells[self.row][col] = ' ';
                }
            }
            b'm' => {
                let mut at = 0;
                while at < numbers.len() {
                    match (numbers[at], numbers.get(at + 1)) {
                        (38 | 48, Some(2)) => {
                            self.true_color = true;
                            at += 5;
                        }
                        (38 | 48, Some(5)) => {
                            self.palette = true;
                            at += 3;
                        }
                        _ => at += 1,
                    }
                }
            }
            b'A' => self.row = self.row.saturating_sub(first(1)),
            b'B' => self.row = (self.row + first(1)).min(self.rows - 1),
            b'C' => self.col = (self.col + first(1)).min(self.cols - 1),
            b'D' => self.col = self.col.saturating_sub(first(1)),
            _ => {}
        }
    }
}

/// A program on a pty, and what it has drawn.
struct Terminal {
    pty: Pty,
    screen: Emulator,
    /// Everything the program wrote.
    raw: Vec<u8>,
    /// The questions answered so far: how many of each the output had when it was last looked at.
    answered_attributes: usize,
    answered_graphics: usize,
    answered_keyboard: usize,
    answered_names: usize,
    answered_secondary: usize,
    answered_cells: usize,
    answered_packed: usize,
    answered_window: usize,
    answered_pointer: usize,
    answered_edits: usize,
    answered_copies: usize,
    /// What the terminal answers besides the device attributes.
    knows: Knows,
}

/// What a terminal answers besides the device attributes.
#[derive(Clone, Copy, Default)]
struct Knows {
    kitty_graphics: bool,
    kitty_keyboard: bool,
    /// What it answers to `CSI > q`: its name and version.
    name: Option<&'static str>,
    /// What it answers to `CSI > c` (the secondary attributes), between `CSI >` and `c`.
    secondary: Option<&'static str>,
    /// How many pixels a cell is, across and down, as it answers to `CSI 16 t`.
    cell: Option<(u32, u32)>,
    /// Whether it takes pixels that are packed (the question with `o=z`).
    packed: bool,
    /// How many pixels its window is, across and down, as it answers to `CSI 14 t`.
    window: Option<(u32, u32)>,
    /// Whether it knows the pointer in pixels (the answer to the question of mode 1016).
    pixel_mouse: bool,
    /// Whether it changes a part of a picture in place (the edit of a frame, which is asked once).
    edits: bool,
    /// Whether it copies a part of a picture onto another part of it (asked once, like the edit).
    copies: bool,
}

fn count(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
}

impl Terminal {
    fn start(cols: u16, rows: u16, env: &[(&str, &str)], knows: Knows) -> Self {
        Self::start_with(&[], cols, rows, env, knows)
    }

    /// As [`Terminal::start`], with `args` for the program.
    fn start_with(args: &[&str], cols: u16, rows: u16, env: &[(&str, &str)], knows: Knows) -> Self {
        let mut command = vec![program()];
        command.extend(args.iter().map(|arg| (*arg).to_owned()));
        Self::start_command(command, cols, rows, env, knows)
    }

    /// A program that is not the example, with its arguments.
    fn start_command(
        command: Vec<String>,
        cols: u16,
        rows: u16,
        env: &[(&str, &str)],
        knows: Knows,
    ) -> Self {
        let mut variables: Vec<(String, String)> = vec![
            ("TERM".to_owned(), "xterm-256color".to_owned()),
            // Neither a terminal that says it has true colour, nor a screen forced.
            ("COLORTERM".to_owned(), String::new()),
            ("PENCERE_TUI".to_owned(), String::new()),
            ("PENCERE_TUI_THEME".to_owned(), String::new()),
        ];
        for (name, value) in env {
            variables.retain(|(have, _)| have != name);
            variables.push(((*name).to_owned(), (*value).to_owned()));
        }
        let pty = Pty::spawn_in(&command, Size { cols, rows }, &variables, None)
            .expect("the program starts on a pty");
        Self {
            pty,
            screen: Emulator::new(usize::from(cols), usize::from(rows)),
            raw: Vec::new(),
            answered_attributes: 0,
            answered_graphics: 0,
            answered_keyboard: 0,
            answered_names: 0,
            answered_secondary: 0,
            answered_cells: 0,
            answered_packed: 0,
            answered_window: 0,
            answered_pointer: 0,
            answered_edits: 0,
            answered_copies: 0,
            knows,
        }
    }

    /// Reads what the program wrote, and answers what it asked as this terminal does.
    fn pump(&mut self) -> bool {
        let mut buffer = [0_u8; 64 * 1024];
        let mut any = false;
        while let Ok(Output::Bytes(got)) = self.pty.read(&mut buffer) {
            any = true;
            self.raw.extend_from_slice(&buffer[..got]);
            self.screen.feed(&buffer[..got]);
        }
        let attributes = count(&self.raw, b"\x1b[c");
        let graphics = count(&self.raw, b"\x1b_Gi=31,");
        let keyboard = count(&self.raw, b"\x1b[?u");
        let names = count(&self.raw, b"\x1b[>q");
        let secondary = count(&self.raw, b"\x1b[>c");
        let cells = count(&self.raw, b"\x1b[16t");
        let packed = count(&self.raw, b"\x1b_Gi=32,");
        let window = count(&self.raw, b"\x1b[14t");
        let pointer = count(&self.raw, b"\x1b[?1016$p");
        let edits = count(&self.raw, b"\x1b_Ga=f,i=33,");
        let copies = count(&self.raw, b"\x1b_Ga=c,i=34,");
        let mut answer = Vec::new();
        for _ in self.answered_names..names {
            if let Some(name) = self.knows.name {
                answer.extend_from_slice(format!("\x1bP>|{name}\x1b\\").as_bytes());
            }
        }
        for _ in self.answered_secondary..secondary {
            if let Some(attributes) = self.knows.secondary {
                answer.extend_from_slice(format!("\x1b[>{attributes}c").as_bytes());
            }
        }
        for _ in self.answered_cells..cells {
            if let Some((width, height)) = self.knows.cell {
                answer.extend_from_slice(format!("\x1b[6;{height};{width}t").as_bytes());
            }
        }
        for _ in self.answered_packed..packed {
            if self.knows.packed {
                answer.extend_from_slice(b"\x1b_Gi=32;OK\x1b\\");
            }
        }
        for _ in self.answered_window..window {
            if let Some((width, height)) = self.knows.window {
                answer.extend_from_slice(format!("\x1b[4;{height};{width}t").as_bytes());
            }
        }
        for _ in self.answered_pointer..pointer {
            answer.extend_from_slice(if self.knows.pixel_mouse {
                b"\x1b[?1016;2$y"
            } else {
                b"\x1b[?1016;0$y"
            });
        }
        for _ in self.answered_edits..edits {
            if self.knows.edits {
                answer.extend_from_slice(b"\x1b_Gi=33;OK\x1b\\");
            }
        }
        for _ in self.answered_copies..copies {
            if self.knows.copies {
                answer.extend_from_slice(b"\x1b_Gi=34;OK\x1b\\");
            }
        }
        for _ in self.answered_graphics..graphics {
            if self.knows.kitty_graphics {
                answer.extend_from_slice(b"\x1b_Gi=31;OK\x1b\\");
            }
        }
        for _ in self.answered_keyboard..keyboard {
            if self.knows.kitty_keyboard {
                answer.extend_from_slice(b"\x1b[?0u");
            }
        }
        // The device attributes last: the fence the others come before.
        for _ in self.answered_attributes..attributes {
            answer.extend_from_slice(b"\x1b[?1;2c");
        }
        (
            self.answered_attributes,
            self.answered_graphics,
            self.answered_keyboard,
            self.answered_names,
            self.answered_secondary,
        ) = (attributes, graphics, keyboard, names, secondary);
        (self.answered_cells, self.answered_packed) = (cells, packed);
        (
            self.answered_window,
            self.answered_pointer,
            self.answered_edits,
            self.answered_copies,
        ) = (window, pointer, edits, copies);
        if !answer.is_empty() {
            self.type_bytes(&answer);
        }
        any
    }

    fn type_bytes(&self, bytes: &[u8]) {
        let mut sent = 0;
        let began = Instant::now();
        while sent < bytes.len() {
            sent += self.pty.write(&bytes[sent..]).expect("typing");
            assert!(
                began.elapsed() < Duration::from_secs(5),
                "the tty took no input"
            );
        }
    }

    /// Waits until the screen has `text`.
    fn until(&mut self, text: &str) {
        self.until_with(&format!("{text:?} on the screen"), |screen| {
            screen.find(text).is_some()
        });
    }

    fn until_with(&mut self, what: &str, done: impl Fn(&Emulator) -> bool) {
        let began = Instant::now();
        loop {
            self.pump();
            if done(&self.screen) {
                return;
            }
            assert!(
                began.elapsed() < Duration::from_secs(40),
                "no {what}; the screen is\n{}",
                self.screen.text()
            );
            wait(&self.pty, false, false, 20);
        }
    }

    /// Lets the program have `millis` to say what it is going to say.
    fn settle(&mut self, millis: u64) {
        let until = Instant::now() + Duration::from_millis(millis);
        while Instant::now() < until {
            self.pump();
            wait(&self.pty, false, false, 20);
        }
    }

    /// Waits for the program to end, and how.
    fn ended(&mut self) -> std::process::ExitStatus {
        let began = Instant::now();
        loop {
            self.pump();
            if let Some(status) = self.pty.finished() {
                // What it wrote last may not be read yet, with the machine busy: all of it is read.
                while self.pump() {}
                wait(&self.pty, false, false, 20);
                while self.pump() {}
                return status;
            }
            assert!(
                began.elapsed() < Duration::from_secs(20),
                "the program did not end; the screen is\n{}",
                self.screen.text()
            );
            wait(&self.pty, false, false, 20);
        }
    }

    /// Resizes the window, as the person does: the pty's size, and the program is told.
    fn resize(&mut self, cols: u16, rows: u16) {
        let size = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: the master descriptor is ours and open; the size outlives the call.
        let done =
            unsafe { libc::ioctl(self.pty.master_fd(), libc::TIOCSWINSZ as _, &raw const size) };
        assert_eq!(done, 0, "the window's size is set");
        self.screen.resize(usize::from(cols), usize::from(rows));
    }

    /// Waits until the program has written `bytes`, `times` times in all.
    fn until_written(&mut self, bytes: &[u8], times: usize) {
        let what = format!("{:?} written {times} times", String::from_utf8_lossy(bytes));
        let began = Instant::now();
        loop {
            self.pump();
            if count(&self.raw, bytes) >= times {
                return;
            }
            assert!(
                began.elapsed() < Duration::from_secs(20),
                "no {what}; the screen is\n{}",
                self.screen.text()
            );
            wait(&self.pty, false, false, 20);
        }
    }
}

// --- A terminal that keeps its window as pictures -------------------------------------------

/// The cells of the terminals that draw pictures here (`pictures()`), in pixels.
const CELL: (u32, u32) = (10, 20);

/// What sets the terminal's own ground to the page's (the kit's dark ground, which a terminal that
/// says nothing of its colours is drawn in).
const GROUND: &[u8] = b"\x1b]11;rgb:0d/11/17\x1b\\";

impl Terminal {
    /// Waits until the window has been sent, and the program has said all it is going to say (the
    /// page is made while the first picture is the window of the ground alone).
    fn until_picture(&mut self) {
        self.until_written(b"\x1b_Ga=T,", 1);
        self.quiet_for(2500);
    }

    /// Waits until the program has written nothing for a second: what a key or a press made of the
    /// window is drawn by then.
    fn quiet(&mut self) {
        self.quiet_for(1000);
    }

    fn quiet_for(&mut self, millis: u64) {
        let began = Instant::now();
        let mut last = Instant::now();
        loop {
            if self.pump() {
                last = Instant::now();
            }
            if last.elapsed() > Duration::from_millis(millis) {
                return;
            }
            assert!(
                began.elapsed() < Duration::from_secs(40),
                "the program never went quiet"
            );
            wait(&self.pty, false, false, 20);
        }
    }

    /// The window as a terminal that follows the stream has it now: its pixels, across and down.
    fn pixels(&self, cols: u16, rows: u16) -> Vec<u8> {
        let mut window = Window::default();
        let end = complete(&self.raw);
        window.feed(&self.raw[..end]);
        window.shows((u32::from(cols) * CELL.0, u32::from(rows) * CELL.1), CELL)
    }

    /// The person presses the left button at the pixel `x`, `y` and lets go (the mouse in pixels).
    fn click_at(&mut self, x: u32, y: u32) {
        self.type_bytes(format!("\x1b[<0;{x};{y}M\x1b[<0;{x};{y}m").as_bytes());
    }
}

/// How much of `raw` is whole sequences of pictures: what is cut off at the end is not read yet.
fn complete(raw: &[u8]) -> usize {
    let open = raw
        .windows(3)
        .rposition(|w| w == b"\x1b_G")
        .filter(|at| !raw[*at..].windows(2).any(|w| w == b"\x1b\\"));
    open.unwrap_or(raw.len())
}

/// The smallest box of pixels (left, top, right, bottom, past the last) in which `a` and `b`, two
/// windows `width` across, differ; `None` where they are the same.
fn differing(a: &[u8], b: &[u8], width: u32) -> Option<(u32, u32, u32, u32)> {
    let mut found: Option<(u32, u32, u32, u32)> = None;
    for (n, (x, y)) in a.chunks(4).zip(b.chunks(4)).enumerate() {
        if x != y {
            let n = u32::try_from(n).expect("a pixel");
            let (px, py) = (n % width, n / width);
            found = Some(match found {
                None => (px, py, px + 1, py + 1),
                Some((l, t, r, bo)) => (l.min(px), t.min(py), r.max(px + 1), bo.max(py + 1)),
            });
        }
    }
    found
}

#[test]
#[ignore = "heavy: starts a page, which compiles the interpreter"]
fn a_field_that_is_pressed_and_typed_in_is_drawn_in_its_room_and_a_key_is_an_edit_of_the_picture() {
    let mut terminal =
        Terminal::start_with(&["--script", &page("field.lua")], 100, 30, &[], pictures());
    terminal.until_picture();
    let width = 100 * CELL.0;
    let resting = terminal.pixels(100, 30);
    // A press on the room of the field, at a pixel, gives it the keyboard: its caret is drawn.
    terminal.click_at(60, 30);
    terminal.quiet();
    let focused = terminal.pixels(100, 30);
    let (left, top, right, bottom) =
        differing(&resting, &focused, width).expect("the caret of the field is drawn");
    assert!(
        right - left < 12 && bottom - top < 60,
        "a caret: {left} {top} {right} {bottom}"
    );
    // A letter typed is in the room, and is an edit of the picture, not a new one.
    let (windows, edits) = (
        count(&terminal.raw, b"\x1b_Ga=T"),
        count(&terminal.raw, b"\x1b_Ga=f,i=100,"),
    );
    terminal.type_bytes(b"hello");
    terminal.until_written(b"\x1b_Ga=f,i=100,", edits + 1);
    terminal.quiet();
    assert_eq!(count(&terminal.raw, b"\x1b_Ga=T"), windows, "no new window");
    let typed = terminal.pixels(100, 30);
    let (left, top, right, bottom) =
        differing(&focused, &typed, width).expect("the words are drawn");
    assert!(
        right > left + 20 && bottom < 120 && top < 80,
        "the words are in the room at the top left: {left} {top} {right} {bottom}"
    );
    assert!(
        u64::from(right - left) * u64::from(bottom - top) * 8 < u64::from(width) * 600,
        "a few words are a small part of the window"
    );
    terminal.type_bytes(b"\x03");
    let status = terminal.ended();
    assert_eq!(status.code(), Some(130), "{status:?}");
}

#[test]
#[ignore = "heavy: starts a page, which compiles the interpreter"]
fn a_menu_a_link_and_a_panel_are_drawn_over_the_picture_and_the_picture_is_as_it_was_when_they_go()
{
    let mut terminal = Terminal::start_with(
        &["--script", &page("overlays.lua")],
        100,
        30,
        &[],
        pictures(),
    );
    terminal.until_picture();
    let width = 100 * CELL.0;
    let foot = 29 * CELL.1;
    let resting = terminal.pixels(100, 30);
    // The menu is a card at the cells the page asked for, and takes no more of the window.
    terminal.type_bytes(F1);
    terminal.quiet();
    let menu = terminal.pixels(100, 30);
    let (left, top, right, bottom) = differing(&resting, &menu, width).expect("a menu is drawn");
    assert!(
        left >= 60 && top >= 40 && right - left < 500 && bottom - top < 400,
        "a card near where the page asked: {left} {top} {right} {bottom}"
    );
    terminal.type_bytes(ESCAPE);
    terminal.quiet();
    assert_eq!(
        terminal.pixels(100, 30),
        resting,
        "the window is as it was when the menu is turned away"
    );
    // A link is a row at the foot of the window.
    terminal.type_bytes(F2);
    terminal.quiet();
    let link = terminal.pixels(100, 30);
    let (left, top, right, bottom) = differing(&resting, &link, width).expect("a bar is drawn");
    assert!(
        top >= foot && bottom <= 30 * CELL.1 && left == 0 && right == width,
        "a row at the foot: {left} {top} {right} {bottom}"
    );
    terminal.type_bytes(ESCAPE);
    terminal.quiet();
    assert_eq!(terminal.pixels(100, 30), resting, "and it goes");
    // The line of a panel is a row at the foot too, with the caret of its line; what is pasted into
    // it is drawn.
    terminal.type_bytes(F4);
    terminal.quiet();
    let panel = terminal.pixels(100, 30);
    let (left, top, right, bottom) = differing(&resting, &panel, width).expect("a panel is drawn");
    assert!(
        top >= foot && bottom <= 30 * CELL.1 && left == 0 && right == width,
        "a row at the foot: {left} {top} {right} {bottom}"
    );
    terminal.type_bytes(b"/tmp");
    terminal.quiet();
    let typed = terminal.pixels(100, 30);
    let (left, top, _, bottom) = differing(&panel, &typed, width).expect("the path is drawn");
    assert!(
        top >= foot && bottom <= 30 * CELL.1 && left > 0,
        "{left} {top} {bottom}"
    );
    terminal.type_bytes(ESCAPE);
    terminal.quiet();
    // It goes, and the page says what became of it (in its words at the top), so that the window
    // is not what it was above the foot, and is what it was at the foot.
    let after = terminal.pixels(100, 30);
    assert!(
        differing(&resting, &after, width).is_none_or(|(_, _, _, bottom)| bottom <= foot),
        "nothing of the panel is left at the foot"
    );
    terminal.type_bytes(b"\x03");
    let status = terminal.ended();
    assert_eq!(status.code(), Some(130), "{status:?}");
}

#[test]
#[ignore = "heavy: starts a page, which compiles the interpreter"]
fn the_terminal_lent_to_a_command_is_given_back_and_the_picture_is_sent_anew_as_it_was() {
    let mut terminal = Terminal::start_with(
        &["--hand-off", "--script", &page("field.lua")],
        100,
        30,
        &[],
        Knows {
            kitty_keyboard: true,
            ..pictures()
        },
    );
    terminal.until_picture();
    let resting = terminal.pixels(100, 30);
    let before = terminal.raw.len();
    terminal.type_bytes(F1);
    // The screen is entered again when the command has ended: twice in all.
    terminal.until_written(b"\x1b[?1049h", 2);
    terminal.until_written(b"\x1b_Ga=T,", 2);
    terminal.quiet();
    let raw = terminal.raw.clone();
    let left = position_of(&raw, b"\x1b[?1049l", before).expect("the screen is left for it");
    let said = position_of(&raw, b"HANDED-OVER", before).expect("the command wrote its word");
    let back = position_of(&raw, b"\x1b[?1049h", left).expect("the screen is entered again");
    assert!(left < said && said < back, "{left} < {said} < {back}");
    // The pictures are given up and the modes put back before the screen is left, and the pointer is
    // told in pixels again after.
    let freed = position_of(&raw, b"\x1b_Ga=d,d=A", before).expect("the pictures are given up");
    let pixels_off =
        position_of(&raw, b"\x1b[?1016l", before).expect("the pointer's mode is given back");
    assert!(
        freed < left && pixels_off < left,
        "{freed} {pixels_off} < {left}"
    );
    assert!(
        position_of(&raw, b"\x1b[?1016h", back).is_some(),
        "the pointer in pixels again"
    );
    // The terminal's own ground is its own for the command, and the page's again after it.
    let ground_off = position_of(&raw, b"\x1b]111\x1b\\", before)
        .expect("the ground is given back for the command");
    let ground_on = position_of(&raw, GROUND, left).expect("the ground is set again");
    assert!(
        ground_off < left && back < ground_on,
        "{ground_off} < {left} < {back} < {ground_on}"
    );
    // The whole window is sent anew, in the same pixels, in a picture of a new number.
    assert_eq!(count(&raw, b"\x1b_Ga=T,"), 2, "the window is sent anew");
    assert_eq!(
        terminal.pixels(100, 30),
        resting,
        "and it is the picture it was"
    );
    terminal.type_bytes(b"\x1b[99;5u");
    assert_eq!(terminal.ended().code(), Some(130));
}

#[test]
#[ignore = "heavy: starts a page, which compiles the interpreter"]
fn the_terminals_ground_is_given_back_when_a_signal_ends_the_program() {
    for signal in [libc::SIGTERM, libc::SIGHUP] {
        let mut terminal = Terminal::start(100, 30, &[], pictures());
        terminal.until_written(b"\x1b_Ga=T,", 1);
        terminal.settle(600);
        assert_eq!(count(&terminal.raw, GROUND), 1);
        // The program's own child (the pty's): the signal ends the view the way the person's
        // closing of the window would.
        let pid = i32::try_from(terminal.pty.id()).expect("a process id");
        unsafe { libc::kill(pid, signal) };
        terminal.ended();
        let given = terminal
            .raw
            .windows(b"\x1b]111\x1b\\".len())
            .rposition(|w| w == b"\x1b]111\x1b\\")
            .unwrap_or_else(|| panic!("signal {signal}: the ground is given back"));
        let left = terminal
            .raw
            .windows(8)
            .rposition(|w| w == b"\x1b[?1049l")
            .expect("the alternate screen is left");
        assert!(given < left, "signal {signal}: {given} < {left}");
    }
}

/// A terminal that draws pictures and has the Kitty keyboard: the keys of ⌃⇧ and a letter reach the
/// program as keys of their own.
fn with_chords() -> Knows {
    Knows {
        kitty_keyboard: true,
        ..pictures()
    }
}

#[test]
#[ignore = "heavy: starts a page, which compiles the interpreter"]
fn what_is_selected_in_an_editor_is_copied_and_cut_to_the_pasteboard_by_control_shift_c_and_x() {
    let mut terminal = Terminal::start_with(
        &["--script", &page("chords.lua")],
        100,
        30,
        &[],
        with_chords(),
    );
    terminal.until_picture();
    terminal.click_at(60, 50);
    terminal.type_bytes(b"copy me");
    terminal.quiet();
    // Shift+Left twice selects "me". ⌃⇧C copies it, and does not stop the program as ⌃C does.
    terminal.type_bytes(b"\x1b[1;2D\x1b[1;2D");
    terminal.type_bytes(b"\x1b[99;6u");
    terminal.until_written(&pasteboard("bWU="), 1);
    terminal.quiet();
    assert_eq!(count(&terminal.raw, &pasteboard("bWU=")), 1);
    // ⌃⇧X cuts it: it is on the pasteboard, and out of the text, so that the two characters before
    // the caret are now "y " and not "me" again.
    terminal.type_bytes(b"\x1b[120;6u");
    terminal.until_written(&pasteboard("bWU="), 2);
    terminal.quiet();
    terminal.type_bytes(b"\x1b[1;2D\x1b[1;2D");
    terminal.type_bytes(b"\x1b[99;6u");
    terminal.until_written(&pasteboard("eSA="), 1);
    terminal.type_bytes(b"\x1b[99;5u");
    assert_eq!(terminal.ended().code(), Some(130));
}

#[test]
#[ignore = "heavy: starts a page, which compiles the interpreter"]
fn a_command_the_page_binds_to_a_command_key_is_done_by_control_shift_and_the_pages_own_binding_of_that_wins()
 {
    let mut terminal = Terminal::start_with(
        &["--script", &page("chords.lua")],
        100,
        30,
        &[],
        with_chords(),
    );
    terminal.until_picture();
    // The page binds ⌘J: ⌃⇧J does it.
    terminal.type_bytes(b"\x1b[106;6u");
    terminal.quiet();
    terminal.type_bytes(DOWN);
    terminal.type_bytes(ENTER);
    terminal.until_written(&pasteboard("amF5"), 1);
    terminal.quiet();
    // The page binds ⌘K and ⌃⇧K both: ⌃⇧K is the page's own, and ⌘K is not what it does.
    terminal.type_bytes(b"\x1b[107;6u");
    terminal.quiet();
    terminal.type_bytes(DOWN);
    terminal.type_bytes(ENTER);
    terminal.until_written(&pasteboard("a2F5X293bg=="), 1);
    terminal.quiet();
    assert_eq!(count(&terminal.raw, &pasteboard("a2F5")), 0);
    // A ⌘ key a terminal reports is the page's as it was.
    terminal.type_bytes(b"\x1b[107;9u");
    terminal.quiet();
    terminal.type_bytes(DOWN);
    terminal.type_bytes(ENTER);
    terminal.until_written(&pasteboard("a2F5"), 1);
    terminal.type_bytes(b"\x1b[99;5u");
    assert_eq!(terminal.ended().code(), Some(130));
}

#[test]
#[ignore = "heavy: starts a page, which compiles the interpreter"]
fn control_and_the_slash_of_the_kitty_keyboard_reaches_the_page_as_it_is_and_with_shift_as_the_command_key()
 {
    let mut terminal = Terminal::start_with(
        &["--script", &page("slash.lua")],
        100,
        30,
        &[],
        with_chords(),
    );
    terminal.until_picture();
    // Ctrl and the slash (the comment of an editor on a keyboard with no ⌘) is the key it is.
    terminal.type_bytes(b"\x1b[47;5u");
    terminal.quiet();
    terminal.type_bytes(DOWN);
    terminal.type_bytes(ENTER);
    terminal.until_written(&pasteboard("Y3RybC1zbGFzaA=="), 1);
    terminal.quiet();
    // With Shift the page, which binds neither, is told the ⌘ key of the chord, as for any character.
    terminal.type_bytes(b"\x1b[47;6u");
    terminal.quiet();
    terminal.type_bytes(DOWN);
    terminal.type_bytes(ENTER);
    terminal.until_written(&pasteboard("Y21kLXNsYXNo"), 1);
    terminal.type_bytes(b"\x1b[99;5u");
    assert_eq!(terminal.ended().code(), Some(130));
}

#[test]
#[ignore = "heavy: starts a page, which compiles the interpreter"]
fn a_menu_the_page_wrote_for_a_mac_shows_the_keys_a_terminal_sends_and_none_it_does_not() {
    let mut terminal = Terminal::start_with(
        &["--script", &page("chords.lua")],
        100,
        30,
        &[],
        with_chords(),
    );
    terminal.until_picture();
    let resting = terminal.pixels(100, 30);
    // Written for a Mac: ⌘X, ⌘C, ⌘/ (and ⌘K, which the page's own ⌃⇧K took, and ⌘ with an arrow).
    terminal.type_bytes(F1);
    terminal.quiet();
    let for_a_mac = terminal.pixels(100, 30);
    assert!(
        differing(&resting, &for_a_mac, 1000).is_some(),
        "a menu is drawn"
    );
    terminal.type_bytes(ESCAPE);
    terminal.quiet();
    assert_eq!(terminal.pixels(100, 30), resting, "and it goes");
    // Written as the terminal sends them, and with no key on the lines that have none: the same menu,
    // to the pixel (the keys are not `Super`, which would be wider, and the lines with none say none).
    terminal.type_bytes(F3);
    terminal.quiet();
    let as_sent = terminal.pixels(100, 30);
    assert_eq!(
        differing(&for_a_mac, &as_sent, 1000),
        None,
        "the keys of a Mac are the chords of a terminal"
    );
    terminal.type_bytes(b"\x1b[99;5u");
    assert_eq!(terminal.ended().code(), Some(130));
}

#[test]
#[ignore = "heavy: starts a page, which compiles the interpreter"]
fn a_wheel_turned_after_a_chord_scrolls_for_the_command_key_is_not_held_after_it() {
    let mut terminal = Terminal::start_with(
        &["--script", &page("chords.lua")],
        100,
        30,
        &[],
        with_chords(),
    );
    terminal.until_picture();
    // ⌃⇧Q is the page's ⌘Q, which it has no use for. The wheel turns over the box that zooms with
    // the command key, with no move of the pointer first: it scrolls, as it does with none held.
    let resting = terminal.pixels(100, 30);
    terminal.type_bytes(b"\x1b[113;6u");
    terminal.quiet();
    terminal.type_bytes(b"\x1b[<65;200;220M");
    terminal.quiet();
    assert!(
        differing(&resting, &terminal.pixels(100, 30), 1000).is_some(),
        "the box moved"
    );
    terminal.type_bytes(F2);
    terminal.quiet();
    terminal.type_bytes(DOWN);
    terminal.type_bytes(ENTER);
    terminal.until_written(&pasteboard("c2Nyb2xsZWQ="), 1);
    terminal.quiet();
    assert_eq!(
        count(&terminal.raw, &pasteboard("em9vbWVk")),
        0,
        "it is no zoom"
    );
    terminal.type_bytes(b"\x1b[99;5u");
    assert_eq!(terminal.ended().code(), Some(130));
}

#[test]
#[ignore = "heavy: starts a page, which compiles the interpreter"]
fn a_terminal_that_has_the_kitty_protocols_gets_them_and_gives_them_back() {
    let knows = Knows {
        kitty_keyboard: true,
        ..pictures()
    };
    let mut terminal = Terminal::start(100, 30, &[], knows);
    terminal.until_written(b"\x1b_Ga=T,", 1);
    terminal.settle(1500);
    let pushed = terminal
        .raw
        .windows(5)
        .position(|window| window == b"\x1b[>1u")
        .expect("the keyboard protocol is switched on");
    // ⌃C is a key where the keyboard is the Kitty protocol's (no byte for the tty to turn into a
    // signal), and it ends the program all the same.
    terminal.type_bytes(b"\x1b[99;5u");
    let status = terminal.ended();
    assert_eq!(status.code(), Some(130), "{status:?}");
    let popped = terminal
        .raw
        .windows(4)
        .rposition(|window| window == b"\x1b[<u")
        .expect("the keyboard protocol is switched off");
    let left = terminal
        .raw
        .windows(8)
        .rposition(|window| window == b"\x1b[?1049l")
        .expect("the alternate screen is left");
    assert!(pushed < popped && popped < left);
}

#[test]
#[ignore = "heavy: starts a page, which compiles the interpreter"]
fn the_terminal_is_given_back_as_it_was() {
    let mut terminal = Terminal::start(100, 30, &[], pictures());
    let before = terminal.pty.modes().expect("the tty's modes");
    terminal.until_written(b"\x1b_Ga=T,", 1);
    terminal.settle(2000);
    let during = terminal.pty.modes().expect("the tty's modes");
    // The screen has the keys itself: no line editing, no echo.
    assert!(!during.canonical && !during.echo);
    terminal.type_bytes(b"\x03");
    let status = terminal.ended();
    assert_eq!(status.code(), Some(130), "{status:?}");
    // What the person sees: the alternate screen left, the cursor shown, the mouse and the pastes
    // off, and the wrap back on.
    let screen = &terminal.screen;
    assert!(!screen.alternate && screen.cursor_shown);
    for mode in [1002, 1003, 1004, 1006, 1016, 2004] {
        assert!(
            !screen.modes.contains(&mode),
            "mode {mode} is off: {:?}; the tail of what was written: {:?}",
            screen.modes,
            String::from_utf8_lossy(&terminal.raw[terminal.raw.len().saturating_sub(300)..])
        );
    }
    // The line discipline is the shell's again.
    let after = terminal.pty.modes();
    assert!(
        after.is_none_or(|after| after == before),
        "{before:?} before, {after:?} after"
    );
}

// --- What is shown over the page ------------------------------------------------------------

/// A page of the tests (`tests/pages`), run by the example program in place of its own.
fn page(name: &str) -> String {
    format!("{}/tests/pages/{name}", env!("CARGO_MANIFEST_DIR"))
}

const F1: &[u8] = b"\x1bOP";
const F2: &[u8] = b"\x1bOQ";
const F3: &[u8] = b"\x1bOR";
const F4: &[u8] = b"\x1bOS";
const F5: &[u8] = b"\x1b[15~";
const DOWN: &[u8] = b"\x1b[B";
const ENTER: &[u8] = b"\r";
const ESCAPE: &[u8] = b"\x1b";

/// What the program writes to put `base64` on the terminal's pasteboard.
fn pasteboard(base64: &str) -> Vec<u8> {
    format!("\x1b]52;c;{base64}\x1b\\").into_bytes()
}

// --- What a page is told, and what it can ask of the terminal ------------------------------

const F6: &[u8] = b"\x1b[17~";
const F7: &[u8] = b"\x1b[18~";

/// The page of `events.lua` in a terminal that draws pictures: a box that is a hand at the top left, a field
/// of the host's under it, an editor under that, and a log of what the page was told.
fn start_events() -> Terminal {
    let mut terminal =
        Terminal::start_with(&["--script", &page("events.lua")], 100, 30, &[], pictures());
    terminal.until_picture();
    terminal
}

/// What `text` is, written in base 64.
fn unbase64(text: &str) -> String {
    let mut out = Vec::new();
    let (mut bits, mut have) = (0_u32, 0_u32);
    for c in text.bytes() {
        let value = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => continue,
        };
        bits = bits << 6 | u32::from(value);
        have += 6;
        if have >= 8 {
            have -= 8;
            out.push(u8::try_from(bits >> have).unwrap_or(0));
            bits &= (1 << have) - 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// What the page of `events.lua` was told, in order, and the text of its editor: the page says it by the
/// terminal's pasteboard (F6 opens a menu of one line, and picking it is the person's own word).
fn told(terminal: &mut Terminal) -> (Vec<String>, String) {
    let had = count(&terminal.raw, b"\x1b]52;c;");
    terminal.type_bytes(F6);
    terminal.quiet();
    terminal.type_bytes(DOWN);
    terminal.type_bytes(ENTER);
    terminal.until_written(b"\x1b]52;c;", had + 1);
    terminal.quiet();
    let at = terminal
        .raw
        .windows(7)
        .rposition(|window| window == b"\x1b]52;c;")
        .expect("the report");
    let end = position_of(&terminal.raw, b"\x1b\\", at).expect("the end of the report");
    let said = unbase64(&String::from_utf8_lossy(&terminal.raw[at + 7..end]));
    let (log, text) = said.split_once('|').expect("a log and a text");
    let log = if log.is_empty() {
        Vec::new()
    } else {
        log.split(';').map(str::to_owned).collect()
    };
    (log, text.to_owned())
}

/// The pointer moves to the pixel `x`, `y`, no button held.
fn move_to(terminal: &mut Terminal, x: u32, y: u32) {
    terminal.type_bytes(format!("\x1b[<35;{x};{y}M").as_bytes());
}

/// Ctrl+C, and the program ended as a signal ends it.
fn end(terminal: &mut Terminal) {
    terminal.type_bytes(b"\x03");
    assert_eq!(terminal.ended().code(), Some(130));
}

/// The press and the release of the left button at the pixel `x`, `y`, and the program quiet again.
fn press_at(terminal: &mut Terminal, x: u32, y: u32) {
    terminal.click_at(x, y);
    terminal.quiet();
}

#[test]
#[ignore = "heavy: starts a page, which compiles the interpreter"]
fn the_keys_and_the_pointer_reach_the_page_and_nothing_stops_the_program() {
    let mut terminal = start_events();
    // A press at a pixel is the page's, in the box or outside it; the wheel at a pixel too.
    press_at(&mut terminal, 100, 50);
    press_at(&mut terminal, 700, 450);
    terminal.type_bytes(b"\x1b[<65;100;50M");
    // Ctrl+Z and Ctrl+S are keys for the page, not the tty's suspend and stop: the program goes on, and
    // what is typed after them is read.
    terminal.type_bytes(b"\x1a\x13");
    terminal.type_bytes(b"/web");
    terminal.quiet();
    assert!(terminal.pty.finished().is_none());
    let (log, text) = told(&mut terminal);
    assert_eq!(
        log,
        [
            "press hand",
            "press away",
            "wheel",
            "key z ctrl",
            "key s ctrl",
            "key /",
            "key w",
            "key e",
            "key b",
        ]
    );
    assert_eq!(text, "");
    end(&mut terminal);
}

#[test]
#[ignore = "heavy: starts a page, which compiles the interpreter"]
fn keys_of_movement_sent_at_once_are_each_told_the_page_in_order() {
    let mut terminal = start_events();
    // Each of the keys of movement, forty of them in one write (a key held down, a test, a paste with no
    // brackets): every one is told the page, and the end and the home after them.
    terminal.type_bytes(&b"\x1b[B".repeat(40));
    terminal.type_bytes(&b"\x1b[6~".repeat(40));
    terminal.type_bytes(b"\x1b[F\x1b[H");
    terminal.quiet();
    let (log, _) = told(&mut terminal);
    let mut wanted = vec!["key down"; 40];
    wanted.extend(vec!["key page_down"; 40]);
    wanted.extend(["key end", "key home"]);
    assert_eq!(log, wanted);
    end(&mut terminal);
}

#[test]
#[ignore = "heavy: starts a page, which compiles the interpreter"]
fn the_pointer_takes_the_shape_the_page_asks_for_and_gives_it_back() {
    let mut terminal = start_events();
    // Nothing has asked for a shape yet: the terminal's own is as it was.
    assert_eq!(count(&terminal.raw, b"\x1b]22;"), 0);
    // Over the box that answers: a hand. Away from it: the arrow again. Written once for each change, not
    // for each move.
    move_to(&mut terminal, 100, 50);
    terminal.until_written(b"\x1b]22;pointer\x1b\\", 1);
    move_to(&mut terminal, 110, 55);
    move_to(&mut terminal, 700, 450);
    terminal.until_written(b"\x1b]22;default\x1b\\", 1);
    terminal.settle(200);
    assert_eq!(count(&terminal.raw, b"\x1b]22;pointer\x1b\\"), 1);
    assert_eq!(count(&terminal.raw, b"\x1b]22;default\x1b\\"), 1);
    // Over the room of a field of the host's, which the page does not draw: the I-beam.
    move_to(&mut terminal, 200, 120);
    terminal.until_written(b"\x1b]22;text\x1b\\", 1);
    end(&mut terminal);
    // The terminal's own shape is given back (an empty request), before the screen is left.
    let reset = terminal
        .raw
        .windows(6)
        .rposition(|window| window == b"\x1b]22;\x1b")
        .expect("the pointer is given back");
    let left = terminal
        .raw
        .windows(8)
        .rposition(|window| window == b"\x1b[?1049l")
        .expect("the alternate screen is left");
    assert!(reset < left);
}

#[test]
#[ignore = "heavy: starts a page, which compiles the interpreter"]
fn a_menu_is_picked_from_with_the_pointer_and_turned_away_with_escape_or_a_press_outside() {
    let mut terminal = start_events();
    let width = 100 * CELL.0;
    let resting = terminal.pixels(100, 30);
    terminal.type_bytes(F7);
    terminal.quiet();
    // The menu is a card over the picture: a press in the middle of it picks its one line.
    let (left, top, right, bottom) =
        differing(&resting, &terminal.pixels(100, 30), width).expect("a menu is drawn");
    press_at(&mut terminal, (left + right) / 2, (top + bottom) / 2);
    assert_eq!(terminal.pixels(100, 30), resting, "the menu went");
    // Escape turns it away, and the page is not told.
    terminal.type_bytes(F7);
    terminal.quiet();
    terminal.type_bytes(ESCAPE);
    terminal.quiet();
    assert_eq!(terminal.pixels(100, 30), resting, "the menu went");
    // A press outside does the same, and the release of it reaches nothing.
    terminal.type_bytes(F7);
    terminal.quiet();
    press_at(&mut terminal, 700, 450);
    assert_eq!(terminal.pixels(100, 30), resting, "the menu went");
    // While a menu is up the keys are its own: what is typed reaches neither page nor editor.
    terminal.type_bytes(F7);
    terminal.quiet();
    terminal.type_bytes(b"xyz");
    terminal.type_bytes(ESCAPE);
    terminal.quiet();
    assert_eq!(terminal.pixels(100, 30), resting, "the menu went");
    let (log, text) = told(&mut terminal);
    assert_eq!(
        log,
        ["picked leave"],
        "the page was told the pick, and only it"
    );
    assert_eq!(text, "");
    end(&mut terminal);
}

#[test]
#[ignore = "heavy: starts a page, which compiles the interpreter"]
fn a_panel_is_a_line_for_a_path_and_what_the_person_gives_is_told_the_page() {
    let dir = std::env::temp_dir().join(format!("pencere-tui-panel-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a folder of our own");
    let file = dir.join("note.txt");
    std::fs::write(&file, b"hello").expect("a file");
    let mut terminal = start_events();
    // Turned away: the page is told that too.
    terminal.type_bytes(F4);
    terminal.quiet();
    terminal.type_bytes(ESCAPE);
    terminal.quiet();
    // A path that is not there is refused, and the line stays: what is typed next is the line's.
    terminal.type_bytes(F4);
    terminal.quiet();
    terminal.type_bytes(format!("{}/gone.txt\r", dir.display()).as_bytes());
    terminal.quiet();
    // A file that is: its name, its size, and where it is (the program reads the person's files).
    // Ctrl+A selects the line, and what is typed replaces it.
    terminal.type_bytes(&[0x01]);
    terminal.type_bytes(format!("{}\r", file.display()).as_bytes());
    terminal.quiet();
    // The place to save to: the page's name is proposed, a folder is given that name.
    terminal.type_bytes(F5);
    terminal.quiet();
    terminal.type_bytes(format!("{}\r", dir.display()).as_bytes());
    terminal.quiet();
    let (log, text) = told(&mut terminal);
    assert_eq!(
        log,
        [
            "given: turned away",
            "given: note.txt 5 path",
            "given: saving out.txt"
        ]
    );
    assert_eq!(text, "", "what was typed in the panel is not the editor's");
    end(&mut terminal);
    std::fs::remove_dir_all(&dir).expect("our folder goes");
}

/// A press in the room of the editor of `events.lua`: it has the keyboard.
fn into_the_editor(terminal: &mut Terminal) {
    press_at(terminal, 200, 200);
}

#[test]
#[ignore = "heavy: starts a page, which compiles the interpreter"]
fn an_editor_takes_typing_a_selection_and_a_paste() {
    let mut terminal = start_events();
    into_the_editor(&mut terminal);
    terminal.type_bytes(b"hello");
    // Shift+Left twice selects "lo", and typing replaces them.
    terminal.type_bytes(b"\x1b[1;2D\x1b[1;2D");
    terminal.type_bytes(b"p");
    // A paste (the person's own terminal's) goes in at the caret, and lines go on past the editor's room.
    terminal.type_bytes(b"\x1b[200~ there\x1b[201~");
    terminal.type_bytes(b"\rtwo\rthree\rfour\rfive\rsix\rseven");
    terminal.quiet();
    let (log, text) = told(&mut terminal);
    assert_eq!(text, "help there\ntwo\nthree\nfour\nfive\nsix\nseven");
    assert!(
        log.iter().all(|entry| !entry.starts_with("key")),
        "the editor's keys are the editor's, and the page is told none: {log:?}"
    );
    end(&mut terminal);
}

#[test]
#[ignore = "heavy: starts a page, which compiles the interpreter"]
fn an_editor_has_the_keys_of_a_system_with_no_command_key() {
    let mut terminal = start_events();
    into_the_editor(&mut terminal);
    terminal.type_bytes(b"one two");
    assert_eq!(told(&mut terminal).1, "one two");
    // Ctrl+A selects all, and typing replaces it; Ctrl+Z undoes that and Ctrl+Y does it again.
    terminal.type_bytes(b"\x01x");
    assert_eq!(told(&mut terminal).1, "x");
    terminal.type_bytes(b"\x1a");
    assert_eq!(told(&mut terminal).1, "one two");
    terminal.type_bytes(b"\x19");
    assert_eq!(told(&mut terminal).1, "x");
    // Ctrl+Left goes back a word, and Ctrl+Shift+Home selects from there to the start of the document:
    // typing replaces "one ".
    terminal.type_bytes(b"\x1a");
    assert_eq!(told(&mut terminal).1, "one two");
    terminal.type_bytes(b"\x1b[1;5D\x1b[1;6H");
    terminal.type_bytes(b"z");
    assert_eq!(told(&mut terminal).1, "ztwo");
    end(&mut terminal);
}

// --- The hand-off --------------------------------------------------------------------------

/// Where `needle` first is in `haystack` at or after `from`.
fn position_of(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    haystack
        .get(from..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|at| at + from)
}

// --- Pictures ------------------------------------------------------------------------------

/// What a bateri pane or a Ghostty window started from a tmux shell has: the variable tmux gave the
/// shell, and the name of the terminal that is bateri's.
const LEAKED: &[(&str, &str)] = &[
    ("TMUX", "/tmp/tmux-501/default,1,0"),
    ("TERM_PROGRAM", "bateri"),
];

#[test]
#[ignore = "heavy: starts a page, which compiles the interpreter"]
fn a_multiplexer_is_not_asked_about_pictures_and_a_terminal_that_is_not_one_is() {
    // tmux answers what it is (and would keep the question of pictures for the title of the pane),
    // and the terminal behind it may draw pictures, which tmux does not pass. What the terminal says
    // of itself, or the variables the multiplexer sets, are what is known; a variable that leaked into
    // a terminal that says it is another is not enough to keep it from being asked.
    let name = |name| Knows {
        name: Some(name),
        ..pictures()
    };
    // What each case is called, what its terminal says, the variables it has, and whether it is asked.
    type Case<'a> = (&'a str, Knows, &'a [(&'a str, &'a str)], bool);
    let cases: [Case; 4] = [
        ("tmux by its name", name("tmux 3.7c"), &[], false),
        (
            "tmux by its name and its variable",
            name("tmux 3.7c"),
            LEAKED,
            false,
        ),
        (
            "a terminal that says what it is",
            name("ghostty 1.2.0"),
            &[],
            true,
        ),
        (
            "a variable that leaked into it",
            name("ghostty 1.2.0"),
            LEAKED,
            true,
        ),
    ];
    for (what, knows, env, asked) in cases {
        let mut terminal = Terminal::start(100, 30, env, knows);
        if asked {
            terminal.until_written(b"\x1b_Ga=T,", 1);
            // The plain question and the one with packed pixels, each once, and the size of a cell
            // asked once.
            assert_eq!(
                count(&terminal.raw, b"i=31,"),
                1,
                "{what}: the question of pictures"
            );
            assert_eq!(
                count(&terminal.raw, b"i=32,"),
                1,
                "{what}: and of packed pixels"
            );
            assert_eq!(
                count(&terminal.raw, b"\x1b[16t"),
                1,
                "{what}: a cell's size"
            );
            terminal.type_bytes(b"\x03");
            assert_eq!(terminal.ended().code(), Some(130), "{what}");
        } else {
            let status = terminal.ended();
            assert_eq!(status.code(), Some(0), "{what}: {status:?}");
            assert_eq!(
                count(&terminal.raw, b"a=q"),
                0,
                "{what}: no question of pictures"
            );
            assert_eq!(
                count(&terminal.raw, b"\x1b_Ga=T"),
                0,
                "{what}: and no picture"
            );
            assert_eq!(
                count(&terminal.raw, b"\x1b[?1049h"),
                0,
                "{what}: no alternate screen"
            );
            assert!(
                terminal.screen.text().contains("worker"),
                "{what}: the fallback's list"
            );
        }
    }
}

#[test]
#[ignore = "heavy: starts a page, which compiles the interpreter"]
fn a_menu_the_page_asks_for_is_drawn_over_the_picture_and_a_pick_is_told_the_page() {
    let mut terminal = Terminal::start_with(
        &["--script", &page("overlays.lua")],
        100,
        30,
        &[],
        pictures(),
    );
    terminal.until_picture();
    terminal.type_bytes(F1);
    terminal.quiet();
    // The arrows go from line to line that can be picked (past the one that cannot, and the line
    // between), Enter picks: the page is told, the menu goes, and a line that copies is the person's
    // word for the terminal's pasteboard.
    terminal.type_bytes(DOWN);
    terminal.type_bytes(DOWN);
    terminal.type_bytes(ENTER);
    terminal.until_written(&pasteboard("ZnJvbSB0aGUgbWVudQ=="), 1);
    // A key that is the page's own word is not the person's: nothing is written for it.
    terminal.type_bytes(F3);
    terminal.settle(600);
    assert_eq!(
        count(&terminal.raw, &pasteboard("dGhlIHBhZ2UncyBvd24gd29yZA==")),
        0
    );
    terminal.type_bytes(b"\x03");
    assert_eq!(terminal.ended().code(), Some(130));
}

// --- An app `pencere build` makes -------------------------------------------------------------

/// The command, as it is built with its terminal (the default) and without it: the second under a
/// name of its own, as `make check-heavy` leaves it.
fn command(terminal: bool) -> PathBuf {
    if terminal {
        built("pencere", "cargo build -p pencere-cli")
    } else {
        built(
            "pencere-no-terminal",
            "cargo build -p pencere-cli --no-default-features, and the executable copied to target/debug/pencere-no-terminal",
        )
    }
}

/// A directory of this test's own, to put apps in, empty.
fn scratch(name: &str) -> PathBuf {
    let exe = std::env::current_exe().expect("this test's path");
    let dir = exe
        .parent()
        .and_then(std::path::Path::parent)
        .expect("the target directory")
        .join(format!("live-apps-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a directory");
    dir
}

/// The example app of Lua, made into one executable by `runner` (`args` after `build <dir>`).
fn app_of(
    runner: &std::path::Path,
    dir: &std::path::Path,
    name: &str,
    source: &str,
    args: &[&str],
) -> PathBuf {
    let out = dir.join(name);
    let made = std::process::Command::new(runner)
        .arg("build")
        .arg(source)
        .args(args)
        .arg("-o")
        .arg(&out)
        .output()
        .expect("the command runs");
    assert!(
        made.status.success(),
        "{}",
        String::from_utf8_lossy(&made.stderr)
    );
    out
}

fn containers_lua() -> String {
    format!(
        "{}/../../examples/containers-lua",
        env!("CARGO_MANIFEST_DIR")
    )
}

/// The page is up: its list of containers is drawn.
#[test]
#[ignore = "heavy: starts a page, which loads the interpreter"]
fn an_app_with_a_terminal_in_it_draws_its_page_in_a_terminal_that_draws_pictures() {
    let dir = scratch("terminal");
    let app = app_of(&command(true), &dir, "with", &containers_lua(), &[]);
    let mut terminal = Terminal::start_command(
        vec![app.to_string_lossy().into_owned()],
        100,
        30,
        &[],
        pictures(),
    );
    // The window is one picture, and no text is written to the cells: the picture is the screen.
    terminal.until_written(b"\x1b_Ga=T,f=32,s=1010,v=620,", 1);
    assert!(terminal.screen.alternate);
    assert!(!terminal.screen.cursor_shown);
    assert!(terminal.screen.find("Containers").is_none());
    // The list is the app's own handler's answer (Lua, on the loop of the app, to a call the page
    // made), and the readings are the topic it feeds: the picture is edited in place as they come,
    // with no key pressed.
    let edits = count(&terminal.raw, b"\x1b_Ga=f,i=");
    terminal.until_written(b"\x1b_Ga=f,i=", edits + 2);
    assert_eq!(
        count(&terminal.raw, b"\x1b_Ga=T"),
        1,
        "what the topic feeds is no new window"
    );
    terminal.type_bytes(b"\x03");
    let status = terminal.ended();
    assert_eq!(status.code(), Some(130), "{status:?}");
    // Not the text of its `fallback`, which is what this app says where nothing shows its page (and
    // says after the screen is given back, so that the whole of what was written is looked at).
    assert_eq!(
        count(&terminal.raw, b"NAME         IMAGE"),
        0,
        "the fallback ran"
    );
    // The terminal is the person's again.
    assert!(!terminal.screen.alternate);
    assert!(terminal.screen.cursor_shown);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
#[ignore = "heavy: needs the command built without its terminal"]
fn an_app_with_no_terminal_in_it_takes_its_fallback_in_a_terminal_that_hosts_no_view() {
    let dir = scratch("no-terminal");
    // The command with no terminal in it makes the app of a copy of itself.
    let plain = command(false);
    let app = app_of(
        &plain,
        &dir,
        "without",
        &containers_lua(),
        &["--no-terminal"],
    );
    let mut terminal = Terminal::start_command(
        vec![app.to_string_lossy().into_owned()],
        100,
        30,
        &[],
        Knows::default(),
    );
    // What the app's `fallback` prints, and the end of the app: no screen was ever drawn.
    terminal.until("NAME");
    terminal.until("worker");
    let status = terminal.ended();
    assert_eq!(status.code(), Some(0), "{status:?}");
    assert_eq!(
        count(&terminal.raw, b"\x1b[?1049h"),
        0,
        "no alternate screen"
    );
    assert!(terminal.screen.find("Filter by name").is_none());
    // It is the smaller of the two.
    let with = app_of(&command(true), &dir, "with", &containers_lua(), &[]);
    let size = |path: &std::path::Path| std::fs::metadata(path).expect("a file").len();
    assert!(
        size(&app) < size(&with),
        "{} bytes, and {} with the terminal",
        size(&app),
        size(&with)
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// An app of one Lua file and a page that draws "drawn", in `dir/source`.
fn small_app(dir: &std::path::Path, app: &str) -> PathBuf {
    let source = dir.join("source");
    std::fs::create_dir_all(&source).expect("a directory");
    std::fs::write(source.join("app.lua"), app).expect("an app");
    std::fs::write(
        source.join("page.lua"),
        "return { render = function() return pencere.div():size_full():child(pencere.text(\"drawn\")) end }",
    )
    .expect("a page");
    source
}

#[test]
#[ignore = "heavy: starts a page, which loads the interpreter"]
fn an_app_that_turns_its_screen_off_takes_its_fallback_with_a_terminal_in_the_runner_too() {
    let dir = scratch("screen-off");
    let source = small_app(
        &dir,
        r#"
        local pencere = require("pencere")
        local outcome = pencere.serve {
            page = "page.lua",
            tui = false,
            fallback = function(why) print("the fallback ran: " .. why.reason) end,
        }
        pencere.exit(outcome.status)
        "#,
    );
    let app = app_of(&command(true), &dir, "app", &source.to_string_lossy(), &[]);
    let mut terminal = Terminal::start_command(
        vec![app.to_string_lossy().into_owned()],
        100,
        30,
        &[],
        Knows::default(),
    );
    terminal.until("the fallback ran: no-host");
    let status = terminal.ended();
    assert_eq!(status.code(), Some(0), "{status:?}");
    assert_eq!(
        count(&terminal.raw, b"\x1b[?1049h"),
        0,
        "no alternate screen"
    );
    assert!(terminal.screen.find("drawn").is_none());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
#[ignore = "heavy: starts a page, which loads the interpreter"]
fn an_app_that_closes_its_view_or_ends_while_its_page_is_drawn_gives_the_terminal_back() {
    let dir = scratch("ends");
    // The page asks the app once it is up, and the app does what it was written to do a second after
    // it was asked: the page is on the screen by then (it takes a while to start).
    let source = |app: &str| {
        let source = small_app(&dir, app);
        std::fs::write(
            source.join("page.lua"),
            "pencere.call(\"ready\", nil, function() return true end)\n\
             return { render = function() return pencere.div():size_full():child(pencere.text(\"drawn\")) end }",
        )
        .expect("a page");
        source
    };
    // The app closes the view itself: `serve` gives how that ended, and the app goes on.
    let source_of_closing = source(
        r#"
        local pencere = require("pencere")
        local outcome = pencere.serve {
            page = "page.lua",
            calls = {
                ready = function()
                    pencere.time.after(1, function() pencere.close_view() end)
                    return {}
                end,
            },
        }
        print("served: " .. tostring(outcome.shown) .. " " .. tostring(outcome.ended))
        "#,
    );
    let app = app_of(
        &command(true),
        &dir,
        "closes",
        &source_of_closing.to_string_lossy(),
        &[],
    );
    let mut terminal = Terminal::start_command(
        vec![app.to_string_lossy().into_owned()],
        100,
        30,
        &[],
        pictures(),
    );
    terminal.until_written(b"\x1b_Ga=T,f=32,", 1);
    terminal.until("served: true program");
    let status = terminal.ended();
    assert_eq!(status.code(), Some(0), "{status:?}");
    assert!(!terminal.screen.alternate && terminal.screen.cursor_shown);
    // The app ends with its status while the page is up: the page goes, and the terminal is back.
    let source_of_ending = source(
        r#"
        local pencere = require("pencere")
        pencere.serve {
            page = "page.lua",
            calls = {
                ready = function()
                    pencere.time.after(1, function() pencere.exit(3) end)
                    return {}
                end,
            },
        }
        "#,
    );
    let app = app_of(
        &command(true),
        &dir,
        "ends",
        &source_of_ending.to_string_lossy(),
        &[],
    );
    let mut terminal = Terminal::start_command(
        vec![app.to_string_lossy().into_owned()],
        100,
        30,
        &[],
        pictures(),
    );
    terminal.until_written(b"\x1b_Ga=T,f=32,", 1);
    let status = terminal.ended();
    assert_eq!(status.code(), Some(3), "{status:?}");
    assert!(!terminal.screen.alternate && terminal.screen.cursor_shown);
    let _ = std::fs::remove_dir_all(dir);
}

/// A terminal that draws pictures, with cells of 10 x 20 pixels, that takes its pixels packed, edits
/// a part of a picture and tells the pointer in pixels.
fn pictures() -> Knows {
    Knows {
        kitty_graphics: true,
        packed: true,
        edits: true,
        copies: true,
        pixel_mouse: true,
        cell: Some((10, 20)),
        window: Some((1000, 600)),
        ..Knows::default()
    }
}

#[test]
#[ignore = "heavy: starts a page, which compiles the interpreter"]
fn a_terminal_that_draws_pictures_is_sent_the_window_as_one_picture_and_what_a_key_changes_as_an_edit_of_it()
 {
    let mut terminal = Terminal::start(100, 30, &[], pictures());
    // The whole window, in its pixels, packed and quiet: one picture.
    terminal.until_written(b"\x1b_Ga=T,f=32,s=1010,v=620,", 1);
    terminal.settle(600);
    assert_eq!(
        count(&terminal.raw, b"\x1b_Ga=T"),
        1,
        "the window is sent once"
    );
    assert!(
        terminal.screen.modes.contains(&1016),
        "the pointer is told in pixels: {:?}",
        terminal.screen.modes
    );
    // No text is written to the cells: the picture is the screen.
    assert!(terminal.screen.find("Containers").is_none());
    let edits = count(&terminal.raw, b"\x1b_Ga=f,i=100,");
    // A key moves the selection: some rows of the picture change, and are edited into it where it
    // stands.
    terminal.type_bytes(b"\x1b[B");
    terminal.until_written(b"\x1b_Ga=f,i=100,", edits + 1);
    assert_eq!(
        count(&terminal.raw, b"\x1b_Ga=T"),
        1,
        "a key is no new window"
    );
    // A press at a pixel is a press of the page.
    let edits = count(&terminal.raw, b"\x1b_Ga=f,i=100,");
    terminal.type_bytes(b"\x1b[<0;120;190M\x1b[<0;120;190m");
    terminal.until_written(b"\x1b_Ga=f,i=100,", edits + 1);
    // The window gets smaller: the terminal is asked what its cells are, and the page is one
    // picture again, in the pixels of the window it has now.
    terminal.resize(80, 24);
    terminal.until_written(b"\x1b_Ga=T,f=32,s=810,v=500,", 1);
    // The old window is given up when the new one has been sent, which is the last of that frame.
    terminal.until_written(b"\x1b_Ga=d,d=I,i=100", 1);
    assert_eq!(
        count(&terminal.raw, b"\x1b_Ga=d,d=I,i=100"),
        1,
        "the old window is given up"
    );
    terminal.type_bytes(b"\x03");
    let status = terminal.ended();
    assert_eq!(status.code(), Some(130), "{status:?}");
    // The pictures are given up, and the modes given back, before the screen is left.
    let freed = terminal
        .raw
        .windows(b"\x1b_Ga=d,d=A".len())
        .rposition(|window| window == b"\x1b_Ga=d,d=A")
        .expect("the pictures are given up");
    let left = terminal
        .raw
        .windows(8)
        .rposition(|window| window == b"\x1b[?1049l")
        .expect("the alternate screen is left");
    assert!(freed < left);
    assert!(count(&terminal.raw, b"\x1b[?1016l") >= 1);
    // The terminal's own ground is the page's while the page is up (the pixels that are not the
    // picture's, left and above it, are that colour), and the terminal's again before it is left.
    let entered = position_of(&terminal.raw, b"\x1b[?1049h", 0).expect("the screen is entered");
    let set = position_of(&terminal.raw, GROUND, 0).expect("the ground is set");
    let given =
        position_of(&terminal.raw, b"\x1b]111\x1b\\", set).expect("the ground is given back");
    assert!(
        entered < set && set < given && given < left,
        "{entered} {set} {given} {left}"
    );
    assert_eq!(count(&terminal.raw, GROUND), 1, "set once");
    assert!(
        terminal
            .screen
            .modes
            .iter()
            .all(|mode| matches!(mode, 7 | 25))
            && !terminal.screen.alternate
    );
}

#[test]
#[ignore = "heavy: starts a page, which compiles the interpreter"]
fn a_terminal_that_answers_nothing_of_pictures_is_left_as_it_is_with_the_fallback_the_screen_off_gets()
 {
    // The screen is forced, as it is where a terminal hosts no view; the terminal says nothing of
    // pictures. The program takes its fallback, and the terminal is left as it is left with the
    // screen off.
    let mut off = Terminal::start(100, 30, &[("PENCERE_TUI", "0")], Knows::default());
    let status = off.ended();
    assert_eq!(status.code(), Some(0), "{status:?}");
    let mut forced = Terminal::start(100, 30, &[("PENCERE_TUI", "1")], Knows::default());
    let status = forced.ended();
    assert_eq!(status.code(), Some(0), "{status:?}");
    assert_eq!(
        forced.screen.text(),
        off.screen.text(),
        "the same words are written"
    );
    assert!(
        off.screen.text().contains("worker"),
        "the list as text:\n{}",
        off.screen.text()
    );
    for terminal in [&off, &forced] {
        // The modes that are on are the ones a terminal starts with (wrap, and the cursor).
        assert!(
            terminal
                .screen
                .modes
                .iter()
                .all(|mode| matches!(mode, 7 | 25)),
            "{:?}",
            terminal.screen.modes
        );
        assert!(!terminal.screen.alternate && terminal.screen.cursor_shown);
    }
    // No picture is sent, and nothing is said of what the terminal could not do.
    assert_eq!(count(&forced.raw, b"\x1b_Ga=T"), 0);
    // The terminal never left its screen for ours: the questions are all it was given.
    assert_eq!(
        count(&forced.raw, b"\x1b[?1049h"),
        0,
        "no alternate screen of a terminal that draws no pictures"
    );
    assert!(!String::from_utf8_lossy(&forced.raw).contains("could not be drawn"));
}

#[test]
#[ignore = "heavy: starts a page, which compiles the interpreter"]
fn a_terminal_that_draws_pictures_but_does_not_say_how_large_its_cells_are_takes_the_fallback() {
    let knows = Knows {
        cell: None,
        window: None,
        ..pictures()
    };
    let mut terminal = Terminal::start(100, 30, &[("PENCERE_TUI", "1")], knows);
    let status = terminal.ended();
    assert_eq!(status.code(), Some(0), "{status:?}");
    assert!(terminal.screen.text().contains("worker"));
    assert_eq!(count(&terminal.raw, b"\x1b_Ga=T"), 0);
    assert_eq!(
        count(&terminal.raw, b"\x1b[?1049h"),
        0,
        "no alternate screen of a terminal that cannot be drawn on"
    );
}
