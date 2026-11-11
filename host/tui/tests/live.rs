//! A program whose terminal hosts no view, on a real pty: it draws the view it carries in the cells
//! of the terminal, takes the keys and the mouse, follows the window's size, and gives the terminal
//! back as it was.
//!
//! The program is the example written against the Rust library (`examples/containers-rust`, which
//! `make check-heavy` builds first); the terminal is a pty read by a small emulator that follows
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

/// The example program, built beside the tests.
fn program() -> String {
    let exe = std::env::current_exe().expect("this test's path");
    let dir = exe
        .parent()
        .and_then(std::path::Path::parent)
        .expect("the target directory");
    let found: PathBuf = dir.join("containers-rust");
    assert!(
        found.is_file(),
        "{} is missing: `cargo build -p containers-rust` first (make check-heavy does)",
        found.display()
    );
    found.to_string_lossy().into_owned()
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
    /// What the terminal answers besides the device attributes.
    knows: Knows,
}

/// What a terminal answers besides the device attributes.
#[derive(Clone, Copy, Default)]
struct Knows {
    kitty_graphics: bool,
    kitty_keyboard: bool,
}

fn count(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
}

impl Terminal {
    fn start(cols: u16, rows: u16, env: &[(&str, &str)], knows: Knows) -> Self {
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
        let pty = Pty::spawn_in(&[program()], Size { cols, rows }, &variables, None)
            .expect("the program starts on a pty");
        Self {
            pty,
            screen: Emulator::new(usize::from(cols), usize::from(rows)),
            raw: Vec::new(),
            answered_attributes: 0,
            answered_graphics: 0,
            answered_keyboard: 0,
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
        let mut answer = Vec::new();
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
        ) = (attributes, graphics, keyboard);
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
                self.pump();
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

    /// The person presses the left button over `needle` and lets go (the mouse in the long form).
    fn click_on(&mut self, needle: &str) {
        let (row, col) = self
            .screen
            .find(needle)
            .unwrap_or_else(|| panic!("no {needle:?} to click:\n{}", self.screen.text()));
        let (col, row) = (col + 2, row + 1);
        self.type_bytes(format!("\x1b[<0;{col};{row}M\x1b[<0;{col};{row}m").as_bytes());
    }

    /// The person moves the pointer over `needle`, with no button down.
    fn move_over(&mut self, needle: &str) {
        let (row, col) = self
            .screen
            .find(needle)
            .unwrap_or_else(|| panic!("no {needle:?} to move over:\n{}", self.screen.text()));
        let (col, row) = (col + 2, row + 1);
        self.type_bytes(format!("\x1b[<35;{col};{row}M").as_bytes());
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

/// The page is up: its list of containers is drawn.
fn up(terminal: &mut Terminal) {
    terminal.until("Containers");
    terminal.until("worker");
}

#[test]
#[ignore = "heavy: starts a page, which compiles the interpreter"]
fn a_terminal_that_hosts_no_view_gets_the_view_drawn_in_its_cells() {
    let mut terminal = Terminal::start(100, 30, &[], Knows::default());
    up(&mut terminal);
    // The screen is the alternate one, with no cursor, the mouse and the pastes switched on, and
    // the colours the 256 of the palette: the terminal said nothing of true colour.
    assert!(terminal.screen.alternate);
    assert!(!terminal.screen.cursor_shown);
    for mode in [1002, 1003, 1006, 2004] {
        assert!(
            terminal.screen.modes.contains(&mode),
            "mode {mode} is on: {:?}",
            terminal.screen.modes
        );
    }
    assert!(terminal.screen.palette && !terminal.screen.true_color);
    // The list and the page's own chrome, in cells.
    // What an empty field says: its hint, though nobody has touched the field (it comes with the
    // frame after the one that brings the field).
    terminal.until("Filter by name");
    for word in ["Running", "Stopped", "api", "mailer", "commands"] {
        assert!(
            terminal.screen.find(word).is_some(),
            "{word:?} is drawn:\n{}",
            terminal.screen.text()
        );
    }
    // This terminal never said it has the Kitty graphics protocol: no picture command is sent it.
    assert_eq!(count(&terminal.raw, b"\x1b_Ga=T"), 0);
    terminal.type_bytes(b"\x03");
    let status = terminal.ended();
    assert_eq!(status.code(), Some(130), "{status:?}");
}

#[test]
#[ignore = "heavy: starts a page, which compiles the interpreter"]
fn the_keys_and_the_mouse_reach_the_page_and_nothing_stops_the_program() {
    let mut terminal = Terminal::start(100, 30, &[], Knows::default());
    up(&mut terminal);
    // A press on a filter chip: the list is the stopped containers.
    terminal.click_on("Stopped");
    terminal.until_with("only the stopped containers", |screen| {
        screen.find("mailer").is_some() && screen.find("worker").is_none()
    });
    terminal.click_on("All");
    terminal.until("worker");
    // ⌃Z and ⌃S are keys for the page, not the tty's suspend and stop: the program goes on and
    // what is typed after them is read.
    terminal.type_bytes(b"\x1a\x13");
    terminal.type_bytes(b"/web");
    terminal.until_with("the filter", |screen| {
        screen.find("web").is_some() && screen.find("worker").is_none()
    });
    assert!(terminal.pty.finished().is_none());
    // The wheel over the list, in the old encoding of the mouse as well as the long one.
    let (row, col) = terminal.screen.find("web").expect("web is drawn");
    terminal.type_bytes(format!("\x1b[<65;{};{}M", col + 1, row + 1).as_bytes());
    terminal.type_bytes(&[0x1b, b'[', b'M', 32 + 64, 33 + col as u8, 33 + row as u8]);
    terminal.settle(300);
    assert!(terminal.pty.finished().is_none());
    terminal.type_bytes(b"\x03");
    assert_eq!(terminal.ended().code(), Some(130));
}

#[test]
#[ignore = "heavy: starts a page, which compiles the interpreter"]
fn the_page_follows_the_size_of_the_window_and_waits_for_it_before_it_draws() {
    // A window that opens small and grows, as one does: the first frame is drawn for the size the
    // terminal says, and a bigger one comes with the signal.
    let mut terminal = Terminal::start(60, 20, &[], Knows::default());
    terminal.until("Containers");
    terminal.resize(120, 40);
    terminal.until_with(
        "the page at its new size",
        |screen| matches!(screen.find("commands"), Some((row, _)) if row >= 36),
    );
    // The screen is cleared for the new size and the page is drawn again, for 70 by 24.
    terminal.resize(70, 24);
    terminal.until("Containers");
    terminal.type_bytes(b"\x03");
    assert_eq!(terminal.ended().code(), Some(130));
}

#[test]
#[ignore = "heavy: starts a page, which compiles the interpreter"]
fn a_terminal_that_has_the_kitty_protocols_gets_them_and_gives_them_back() {
    let knows = Knows {
        kitty_graphics: true,
        kitty_keyboard: true,
    };
    let mut terminal = Terminal::start(100, 30, &[], knows);
    up(&mut terminal);
    terminal.settle(300);
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
    let mut terminal = Terminal::start(100, 30, &[], Knows::default());
    let before = terminal.pty.modes().expect("the tty's modes");
    up(&mut terminal);
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
    for mode in [1002, 1003, 1004, 1006, 2004] {
        assert!(
            !screen.modes.contains(&mode),
            "mode {mode} is off: {:?}",
            screen.modes
        );
    }
    // The line discipline is the shell's again.
    let after = terminal.pty.modes();
    assert!(
        after.is_none_or(|after| after == before),
        "{before:?} before, {after:?} after"
    );
}

#[test]
#[ignore = "heavy: starts a page, which compiles the interpreter"]
fn the_pointer_takes_the_shape_the_page_asks_for_and_gives_it_back() {
    let mut terminal = Terminal::start(100, 30, &[], Knows::default());
    up(&mut terminal);
    terminal.until("Filter by name");
    // Nothing has asked for a shape yet: the terminal's own is as it was.
    assert_eq!(count(&terminal.raw, b"\x1b]22;"), 0);
    // Over something that answers a click: a hand. Over the page's title: the arrow again. Written
    // once for each change, not for each move.
    terminal.move_over("Stopped");
    terminal.until_written(b"\x1b]22;pointer\x1b\\", 1);
    terminal.move_over("Stopped");
    terminal.move_over("Containers");
    terminal.until_written(b"\x1b]22;default\x1b\\", 1);
    terminal.settle(200);
    assert_eq!(count(&terminal.raw, b"\x1b]22;pointer\x1b\\"), 1);
    assert_eq!(count(&terminal.raw, b"\x1b]22;default\x1b\\"), 1);
    // Over the room of a field of the host's, which the page does not draw: the I-beam.
    terminal.move_over("Filter by name");
    terminal.until_written(b"\x1b]22;text\x1b\\", 1);
    terminal.type_bytes(b"\x03");
    let status = terminal.ended();
    assert_eq!(status.code(), Some(130), "{status:?}");
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
