//! Pictures of the page scripts of a directory, in a dark terminal's colours and in a light one's:
//! what a site shows of the kit. Each `.lua` file is a page. Its lines that start with `--@` say
//! what is done before the picture is taken, in their order:
//!
//! - `--@ size <width> <height>`: the view, in points (640 × 360 unless said);
//! - `--@ click <name>`, `--@ hover <name>`, `--@ focus <editor>`, `--@ press <keys>`,
//!   `--@ type <words>`, `--@ wait <seconds>`: what the person does;
//! - `--@ crop <name>`: the picture is of the element `name` alone, with `--@ pad <points>` of the
//!   page around it.
//!
//! The host's fields are drawn over the page as a host draws them: what one holds, or, while it is
//! empty, what it says. Motion is reduced, so that what opens is where it comes to rest. Each
//! picture is two pixels to the point, and `shots.json` says the size of each in points and what
//! the page was told was wrong.
//!
//! ```sh
//! cargo run -p pencere-drive --features shot --example shots -- <scripts> <out> \
//!     --terminal Mono-Regular.ttf --bold Mono-Bold.ttf --italic Mono-Italic.ttf --system Sans.ttf
//! ```

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use pencere_drive::headless::{Calls, Files, Headless};
use pencere_drive::shot::{Fonts, Palette, png};
use pencere_view::{DrawList, Family, FieldKind, Font, Measure, Paint, Point, Rect, Role, Run};
use tiny_skia::{IntRect, Pixmap};

/// Pixels to a point.
const SCALE: f32 = 2.0;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("shots: {why}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mut places = Vec::new();
    let mut fonts = Fonts::none();
    while let Some(arg) = args.next() {
        let set: fn(Fonts, &[u8]) -> Result<Fonts, pencere_drive::shot::FontError> =
            match arg.as_str() {
                "--terminal" => Fonts::terminal,
                "--bold" => Fonts::bold,
                "--italic" => Fonts::italic,
                "--system" => Fonts::system,
                _ => {
                    places.push(PathBuf::from(arg));
                    continue;
                }
            };
        let path = args.next().ok_or(format!("{arg} takes a font file"))?;
        let bytes = fs::read(&path).map_err(|why| format!("{path}: {why}"))?;
        fonts = set(fonts, &bytes).map_err(|why| format!("{path}: {why}"))?;
    }
    let [scripts, out] = places.as_slice() else {
        return Err(
            "shots <scripts> <out> [--terminal|--bold|--italic|--system <font>]".to_owned(),
        );
    };
    let mut found = Vec::new();
    collect(scripts, &mut found)?;
    found.sort();
    let mut manifest = serde_json::Map::new();
    for script in &found {
        let entry = script
            .strip_prefix(scripts)
            .unwrap_or(script)
            .to_string_lossy()
            .replace('\\', "/");
        let name = entry.strip_suffix(".lua").unwrap_or(&entry).to_owned();
        let shot = picture(scripts, &entry, &fonts)
            .map_err(|why| format!("{}: {why}", script.display()))?;
        for (theme, bytes) in [("dark", &shot.dark), ("light", &shot.light)] {
            let file = out.join(format!("{name}-{theme}.png"));
            if let Some(parent) = file.parent() {
                fs::create_dir_all(parent).map_err(|why| format!("{}: {why}", parent.display()))?;
            }
            fs::write(&file, bytes).map_err(|why| format!("{}: {why}", file.display()))?;
        }
        for words in &shot.diagnostics {
            eprintln!("{name}: {words}");
        }
        manifest.insert(
            name,
            serde_json::json!({
                "width": shot.width,
                "height": shot.height,
                "diagnostics": shot.diagnostics,
            }),
        );
        println!("{}", script.display());
    }
    let file = out.join("shots.json");
    let json = serde_json::to_string_pretty(&manifest).map_err(|why| why.to_string())?;
    fs::write(&file, json + "\n").map_err(|why| format!("{}: {why}", file.display()))
}

/// The page scripts under `dir`, at any depth: a script whose name starts with `_` is one the pages
/// require, and no page.
fn collect(dir: &Path, found: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|why| format!("{}: {why}", dir.display()))?;
    for entry in entries {
        let path = entry.map_err(|why| why.to_string())?.path();
        if path.is_dir() {
            collect(&path, found)?;
        } else if path.extension().is_some_and(|ext| ext == "lua")
            && !path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with('_'))
        {
            found.push(path);
        }
    }
    Ok(())
}

/// A page's two pictures, the size they are of in points, and what the page was told was wrong.
struct Shot {
    width: f32,
    height: f32,
    dark: Vec<u8>,
    light: Vec<u8>,
    diagnostics: Vec<String>,
}

/// The picture of the page `entry` of the bundle that is the directory `root`.
fn picture(root: &Path, entry: &str, fonts: &Fonts) -> Result<Shot, String> {
    let source = fs::read_to_string(root.join(entry)).map_err(|why| why.to_string())?;
    let mut size = (640.0_f32, 360.0_f32);
    let mut steps = Vec::new();
    let mut crop = None;
    let mut pad = 0.0_f32;
    for line in source.lines() {
        let Some(rest) = line.strip_prefix("--@") else {
            continue;
        };
        let rest = rest.trim();
        let (verb, what) = rest
            .split_once(' ')
            .map_or((rest, ""), |(verb, what)| (verb, what.trim()));
        match verb {
            "size" => {
                let mut numbers = what.split_whitespace().map(str::parse::<f32>);
                let (Some(Ok(width)), Some(Ok(height))) = (numbers.next(), numbers.next()) else {
                    return Err(format!("a size is two numbers, not {what:?}"));
                };
                size = (width, height);
            }
            "crop" => crop = Some(what.to_owned()),
            "pad" => {
                pad = what
                    .parse()
                    .map_err(|_| format!("a pad is points, not {what:?}"))?
            }
            "click" | "hover" | "focus" | "press" | "type" | "wait" => {
                steps.push((verb.to_owned(), what.to_owned()));
            }
            _ => return Err(format!("no such step: {verb}")),
        }
    }
    let mut page = Headless::bundle(Files::dir(root), entry)
        .measure(fonts.clone())
        .size(size.0, size.1)
        .scale(f64::from(SCALE))
        .reduce_motion(true)
        .open()
        .map_err(|why| why.to_string())?;
    for (verb, what) in &steps {
        match verb.as_str() {
            "click" => page.click(what),
            "hover" => page.hover(what),
            "focus" => page.focus_editor(what),
            "press" => page.press(what).map(|_| ()),
            "type" => page.type_text(what),
            _ => {
                let seconds = what
                    .parse::<f64>()
                    .map_err(|_| format!("a wait is seconds, not {what:?}"))?;
                page.advance(seconds)
            }
        }
        .map_err(|why| format!("{verb} {what}: {why}"))?;
    }
    let list = with_fields(&page, fonts);
    let view = (pixels(size.0), pixels(size.1));
    let mut dark =
        png(&list, view, fonts, page.images(), Palette::Dark).map_err(|why| why.to_string())?;
    let mut light =
        png(&list, view, fonts, page.images(), Palette::Light).map_err(|why| why.to_string())?;
    let (mut width, mut height) = size;
    if let Some(name) = crop {
        let rect = page
            .bounds(&name)
            .ok_or(format!("no element is named {name} to crop to"))?;
        let x = (rect.x - pad).max(0.0);
        let y = (rect.y - pad).max(0.0);
        width = (rect.x + rect.w + pad).min(size.0) - x;
        height = (rect.y + rect.h + pad).min(size.1) - y;
        let part = IntRect::from_xywh(px_i32(x), px_i32(y), pixels(width), pixels(height))
            .ok_or(format!("{name} has no room to crop to"))?;
        dark = cut(&dark, part)?;
        light = cut(&light, part)?;
    }
    Ok(Shot {
        width,
        height,
        dark,
        light,
        diagnostics: page.diagnostics().to_vec(),
    })
}

/// The last frame's draw list with the host's fields drawn over it: what each holds, or what it
/// says while it is empty, in the size of the page's words.
fn with_fields(page: &Headless<Calls>, fonts: &Fonts) -> DrawList {
    let mut list = page.list().clone();
    let font = Font {
        family: Family::Terminal,
        size: words_size(&list),
        weight: 400,
        italic: false,
    };
    let line = fonts.clone().measure(&font, "Hg").height.max(font.size);
    for field in &page.outline().fields {
        if field.covered {
            continue;
        }
        let held = page.field_of(field.id).filter(|text| !text.is_empty());
        let (words, role) = match (held, &field.hint) {
            (Some(text), _) if field.kind == FieldKind::Secret => {
                ("\u{2022}".repeat(text.chars().count()), Role::Foreground)
            }
            (Some(text), _) => (text.to_owned(), Role::Foreground),
            (None, Some(hint)) => (hint.clone(), Role::Quiet),
            (None, None) => continue,
        };
        let room = Rect::new(
            field.rect.x * SCALE,
            field.rect.y * SCALE,
            field.rect.w * SCALE,
            field.rect.h * SCALE,
        );
        let top = if field.kind == FieldKind::Lines {
            room.y
        } else {
            room.y + ((room.h - line) / 2.0).max(0.0)
        };
        for (index, text) in words.lines().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let y = top + line * index as f32;
            list.items.push(Paint::Text(Run {
                origin: Point::new(room.x, y),
                text: text.to_owned(),
                font,
                color: role.solid(),
                opacity: 1.0,
                grounds: Vec::new(),
                clip: room,
                underline: false,
                strike: false,
            }));
        }
    }
    list
}

/// The size most of the page's words are in, in pixels.
fn words_size(list: &DrawList) -> f32 {
    let mut sizes: Vec<f32> = list
        .items
        .iter()
        .filter_map(|paint| match paint {
            Paint::Text(run) => Some(run.font.size),
            _ => None,
        })
        .collect();
    sizes.sort_by(f32::total_cmp);
    sizes.get(sizes.len() / 2).copied().unwrap_or(13.0 * SCALE)
}

/// `points` in pixels.
fn pixels(points: f32) -> u32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let pixels = (points * SCALE).round().max(0.0) as u32;
    pixels
}

fn px_i32(points: f32) -> i32 {
    #[allow(clippy::cast_possible_truncation)]
    let pixels = (points * SCALE).round() as i32;
    pixels
}

/// The part `part` of the picture `png`.
fn cut(png: &[u8], part: IntRect) -> Result<Vec<u8>, String> {
    let whole = Pixmap::decode_png(png).map_err(|why| why.to_string())?;
    whole
        .clone_rect(part)
        .ok_or("the part is not in the picture".to_owned())?
        .encode_png()
        .map_err(|why| why.to_string())
}
