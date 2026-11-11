//! Opens an app folder with no screen, lets it run a moment, and prints one frame of it as a
//! terminal draws it: the cells with their colours, the pictures as the Kitty protocol.
//!
//! `frame <app folder> [columns rows] [--blocks | --no-images] [--picture] [--light]`
//!
//! The terminal the output is shown in needs the size of the frame plus one row (the frame ends
//! below its last row), and to understand true colour.

use std::io::Write;
use std::process::ExitCode;

use pencere_drive::headless::Headless;
use pencere_tui::{CanvasMode, Cells, ImageMode, Options, Theme, render};

const COLUMN: f32 = 8.0;
const ROW: f32 = 18.0;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("frame: {why}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut folder = None;
    let mut numbers = Vec::new();
    let mut images = ImageMode::Kitty;
    let mut canvases = CanvasMode::Characters;
    let mut theme = Theme::dark();
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--blocks" => images = ImageMode::Blocks,
            "--no-images" => images = ImageMode::Off,
            "--picture" => canvases = CanvasMode::Picture,
            "--light" => theme = Theme::light(),
            other => match other.parse::<u16>() {
                Ok(number) => numbers.push(number),
                Err(_) if folder.is_none() && !other.starts_with("--") => {
                    folder = Some(other.to_owned());
                }
                Err(_) => return Err(format!("what is {other}?")),
            },
        }
    }
    let folder = folder.ok_or("an app folder is needed")?;
    let (columns, rows) = match numbers.as_slice() {
        [] => (80, 24),
        [columns, rows] => (*columns, *rows),
        _ => return Err("the size is columns and rows".to_owned()),
    };
    let mut page = Headless::app(&folder)
        .size(f32::from(columns) * COLUMN, f32::from(rows) * ROW)
        .measure(Cells::new(COLUMN, ROW))
        .open()
        .map_err(|why| format!("{folder}: {why:?}"))?;
    page.advance(1.0).map_err(|why| format!("{why:?}"))?;
    let options = Options {
        col: COLUMN,
        row: ROW,
        images,
        canvases,
        ..Options::default()
    };
    let mut tries = 0;
    let frame = loop {
        let frame = render(
            page.list(),
            columns,
            rows,
            &theme,
            &page.page().images(),
            &options,
        );
        if !frame.waiting || tries >= 100 {
            break frame;
        }
        tries += 1;
        page.advance(0.1).map_err(|why| format!("{why:?}"))?;
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    std::io::stdout()
        .write_all(&frame.ansi())
        .map_err(|why| why.to_string())
}
