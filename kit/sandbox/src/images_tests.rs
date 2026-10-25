//! The guards of the images: that a file is read and decoded off the thread that asks, once, and
//! that the host is woken once for what arrived and never at rest; the formats; the limits; and
//! that an SVG reaches nothing but itself.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use pencere_view::{Fit, Font, Measure, Metrics, Paint, Size, div, image, layout};

use crate::images::{Arrival, Assets, Bitmap, ImageLimits, Images};

// --- The files and the host ---------------------------------------------------------------------

/// A door that is shut until the test opens it.
#[derive(Default)]
struct Gate {
    open: Mutex<bool>,
    opened: Condvar,
}

impl Gate {
    fn wait(&self) {
        let mut open = self.open.lock().unwrap();
        while !*open {
            open = self.opened.wait(open).unwrap();
        }
    }

    fn open(&self) {
        *self.open.lock().unwrap() = true;
        self.opened.notify_all();
    }
}

/// The files of a bundle, that remembers which it was asked for, and can keep the asking waiting.
struct Bundle {
    files: HashMap<String, Arc<[u8]>>,
    reads: Mutex<Vec<String>>,
    gate: Option<Arc<Gate>>,
}

impl Assets for Bundle {
    fn read(&self, path: &str) -> Option<Arc<[u8]>> {
        if let Some(gate) = &self.gate {
            gate.wait();
        }
        self.reads.lock().unwrap().push(path.to_owned());
        self.files.get(path).cloned()
    }

    fn media_type(&self, path: &str) -> Option<String> {
        self.files
            .contains_key(path)
            .then(|| "application/octet-stream".to_owned())
    }
}

struct Rig {
    images: Images,
    woke: Receiver<()>,
    bundle: Arc<Bundle>,
}

fn rig_with(files: Vec<(&str, Vec<u8>)>, limits: ImageLimits, gate: Option<Arc<Gate>>) -> Rig {
    let bundle = Arc::new(Bundle {
        files: files
            .into_iter()
            .map(|(path, bytes)| (path.to_owned(), Arc::from(bytes)))
            .collect(),
        reads: Mutex::new(Vec::new()),
        gate,
    });
    let (woken, woke) = channel();
    let woken = Mutex::new(woken);
    let images = Images::new(Arc::clone(&bundle) as Arc<dyn Assets>, limits, move || {
        let _ = woken.lock().unwrap().send(());
    });
    Rig {
        images,
        woke,
        bundle,
    }
}

fn rig(files: Vec<(&str, Vec<u8>)>) -> Rig {
    rig_with(files, ImageLimits::default(), None)
}

impl Rig {
    /// Waits to be woken, and takes what arrived.
    fn wait(&self) -> Arrival {
        self.woke
            .recv_timeout(Duration::from_secs(20))
            .expect("the images woke the host");
        self.images.poll()
    }

    /// Waits until what has arrived, all together, is what `done` says is enough.
    fn until(&self, done: impl Fn(&Arrival) -> bool) -> Arrival {
        let mut total = Arrival::default();
        while !done(&total) {
            let more = self.wait();
            total.relayout |= more.relayout;
            total.redraw |= more.redraw;
            total.diagnostics.extend(more.diagnostics);
        }
        total
    }

    /// Nothing wakes the host for a while.
    fn quiet(&self) {
        assert_eq!(
            self.woke.recv_timeout(Duration::from_millis(250)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout),
            "the host was woken at rest"
        );
        assert!(self.images.poll().is_empty());
    }

    fn reads(&self) -> Vec<String> {
        self.bundle.reads.lock().unwrap().clone()
    }
}

// --- The files themselves -----------------------------------------------------------------------

fn png_of(
    width: u32,
    height: u32,
    color: png::ColorType,
    depth: png::BitDepth,
    data: &[u8],
) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(color);
    encoder.set_depth(depth);
    if color == png::ColorType::Indexed {
        encoder.set_palette(vec![255, 0, 0, 0, 0, 255]);
    }
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(data).unwrap();
    writer.finish().unwrap();
    out
}

/// A PNG of one colour, RGBA.
fn solid(width: u32, height: u32, pixel: [u8; 4]) -> Vec<u8> {
    let data: Vec<u8> = pixel
        .iter()
        .copied()
        .cycle()
        .take((width * height * 4) as usize)
        .collect();
    png_of(
        width,
        height,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &data,
    )
}

const GREEN_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10" viewBox="0 0 20 10"><rect width="20" height="10" fill="#00ff00"/></svg>"##;

const RED_JPEG: &[u8] = include_bytes!("fixtures/red-16x8.jpg");

fn pixel(bitmap: &Bitmap, x: u32, y: u32) -> [u8; 4] {
    let at = ((y * bitmap.width + x) * 4) as usize;
    bitmap.pixels()[at..at + 4].try_into().unwrap()
}

fn whole(w: f32, h: f32) -> Size {
    Size::new(w, h)
}

// --- The size arrives, and the page is laid out again once -------------------------------------

/// What a host's measuring is: the text is not under test, and an image's size is what the
/// images know.
struct Host<'a> {
    images: &'a Images,
}

impl Measure for Host<'_> {
    fn measure(&mut self, _: &Font, _: &str) -> Metrics {
        Metrics::default()
    }

    fn image(&mut self, src: &str) -> Option<Size> {
        self.images.size(src)
    }
}

#[test]
fn a_page_is_laid_out_before_its_image_is_decoded_and_once_more_after_and_never_at_rest() {
    let gate = Arc::new(Gate::default());
    let rig = rig_with(
        vec![("logo.png", solid(6, 3, [10, 20, 30, 255]))],
        ImageLimits::default(),
        Some(Arc::clone(&gate)),
    );
    let page = || div().items_start().child(image("logo.png", Fit::Contain));

    // The first layout: the file is being read, the image has no size, the box takes no room.
    let mut host = Host {
        images: &rig.images,
    };
    let first = layout(page(), Size::new(100.0, 100.0), 1.0, &mut host);
    assert_eq!(first.rects().nth(1).map(|r| (r.w, r.h)), Some((0.0, 0.0)));
    assert!(first.paint(&mut host).items.is_empty());
    // The layout again, while the file is still being read, starts nothing again and has nothing new.
    let again = layout(page(), Size::new(100.0, 100.0), 1.0, &mut host);
    assert_eq!(again.rects().nth(1).map(|r| (r.w, r.h)), Some((0.0, 0.0)));

    // The decoding ends: the host is woken once, and told that the page is to be laid out again.
    gate.open();
    let arrived = rig.wait();
    assert!(arrived.relayout && arrived.diagnostics.is_empty());
    let second = layout(page(), Size::new(100.0, 100.0), 1.0, &mut host);
    assert_eq!(second.rects().nth(1).map(|r| (r.w, r.h)), Some((6.0, 3.0)));
    let list = second.paint(&mut host);
    let Some(Paint::Image(picture)) = list.items.first() else {
        panic!("the picture, got {list:?}");
    };
    let bitmap = rig
        .images
        .bitmap(&picture.src, picture.full())
        .expect("pixels");
    assert_eq!((bitmap.width, bitmap.height), (6, 3));
    assert_eq!(pixel(&bitmap, 0, 0), [10, 20, 30, 255]);

    // And at rest it costs nothing: asking again and again reads nothing, decodes nothing, wakes
    // nothing — the host that lays out and draws every frame it likes does not start the work again.
    for _ in 0..100 {
        let laid = layout(page(), Size::new(100.0, 100.0), 1.0, &mut host);
        assert!(!laid.paint(&mut host).items.is_empty());
        assert!(rig.images.bitmap("logo.png", whole(6.0, 3.0)).is_some());
    }
    rig.quiet();
    assert_eq!(rig.reads(), ["logo.png"]);
}

// --- The formats ------------------------------------------------------------------------------

#[test]
fn a_png_is_decoded_to_premultiplied_pixels_whatever_its_colour_type() {
    let rig = rig(vec![
        ("rgba.png", solid(2, 1, [200, 100, 50, 128])),
        (
            "grey.png",
            png_of(
                2,
                1,
                png::ColorType::Grayscale,
                png::BitDepth::Eight,
                &[7, 250],
            ),
        ),
        (
            "palette.png",
            png_of(2, 1, png::ColorType::Indexed, png::BitDepth::Eight, &[1, 0]),
        ),
        (
            "deep.png",
            // 16 bits a channel: the high byte is what is kept.
            png_of(
                1,
                1,
                png::ColorType::Rgb,
                png::BitDepth::Sixteen,
                &[0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc],
            ),
        ),
    ]);
    for name in ["rgba.png", "grey.png", "palette.png", "deep.png"] {
        assert_eq!(rig.images.size(name), None, "not yet");
    }
    let arrived = rig.until(|a| a.relayout);
    assert!(arrived.diagnostics.is_empty(), "{:?}", arrived.diagnostics);
    // They arrive one after the other; wait for the last.
    while rig.images.size("deep.png").is_none() {
        rig.wait();
    }
    let get = |name| rig.images.bitmap(name, whole(2.0, 1.0)).expect(name);
    // Premultiplied: the colour is the colour times the opacity.
    assert_eq!(pixel(&get("rgba.png"), 1, 0), [100, 50, 25, 128]);
    assert_eq!(pixel(&get("grey.png"), 0, 0), [7, 7, 7, 255]);
    assert_eq!(pixel(&get("grey.png"), 1, 0), [250, 250, 250, 255]);
    assert_eq!(pixel(&get("palette.png"), 0, 0), [0, 0, 255, 255]);
    assert_eq!(pixel(&get("palette.png"), 1, 0), [255, 0, 0, 255]);
    assert_eq!(pixel(&get("deep.png"), 0, 0), [0x12, 0x56, 0x9a, 255]);
}

#[test]
fn a_jpeg_is_decoded_and_is_solid() {
    let rig = rig(vec![("photo.jpg", RED_JPEG.to_vec())]);
    assert_eq!(rig.images.size("photo.jpg"), None);
    let arrived = rig.wait();
    assert!(
        arrived.relayout && arrived.diagnostics.is_empty(),
        "{arrived:?}"
    );
    assert_eq!(rig.images.size("photo.jpg"), Some(Size::new(16.0, 8.0)));
    let bitmap = rig.images.bitmap("photo.jpg", whole(16.0, 8.0)).unwrap();
    let [r, g, b, a] = pixel(&bitmap, 8, 4);
    assert!(r > 190 && g < 70 && b < 70, "red: {r} {g} {b}");
    assert_eq!(a, 255);
}

#[test]
fn the_format_is_told_from_the_bytes_and_not_from_the_name() {
    let rig = rig(vec![
        ("looks-like.png", GREEN_SVG.as_bytes().to_vec()),
        ("looks-like.svg", solid(3, 3, [1, 2, 3, 255])),
        (
            "declared.svg",
            format!("\u{feff}<?xml version=\"1.0\"?>\n<!-- a drawing -->\n{GREEN_SVG}")
                .into_bytes(),
        ),
    ]);
    for name in ["looks-like.png", "looks-like.svg", "declared.svg"] {
        rig.images.size(name);
    }
    rig.until(|a| a.relayout);
    while ["looks-like.png", "looks-like.svg", "declared.svg"]
        .iter()
        .any(|name| rig.images.size(name).is_none())
    {
        rig.wait();
    }
    assert_eq!(
        rig.images.size("looks-like.png"),
        Some(Size::new(20.0, 10.0))
    );
    assert_eq!(rig.images.size("looks-like.svg"), Some(Size::new(3.0, 3.0)));
    assert_eq!(rig.images.size("declared.svg"), Some(Size::new(20.0, 10.0)));
}

// --- SVG: drawn at the size it is wanted at ----------------------------------------------------

#[test]
fn an_svg_is_drawn_at_the_size_it_is_asked_and_again_when_that_changes() {
    let rig = rig(vec![("icon.svg", GREEN_SVG.as_bytes().to_vec())]);
    assert_eq!(rig.images.size("icon.svg"), None);
    // Its size is known at once it is parsed: the page is laid out again, nothing drawn yet.
    let arrived = rig.wait();
    assert!(arrived.relayout && !arrived.redraw);
    assert_eq!(rig.images.size("icon.svg"), Some(Size::new(20.0, 10.0)));

    // Drawn at what the page wants: not at once, and the host is woken for the pixels (a redraw,
    // not another layout).
    assert_eq!(rig.images.bitmap("icon.svg", whole(40.0, 20.0)), None);
    let arrived = rig.wait();
    assert!(arrived.redraw && !arrived.relayout, "{arrived:?}");
    let small = rig.images.bitmap("icon.svg", whole(40.0, 20.0)).unwrap();
    assert_eq!((small.width, small.height), (40, 20));
    assert!(
        small
            .pixels()
            .chunks_exact(4)
            .all(|px| px == [0, 255, 0, 255])
    );

    // The same size again is the same pixels, and costs nothing.
    assert_eq!(
        rig.images.bitmap("icon.svg", whole(40.0, 20.0)),
        Some(small.clone())
    );
    rig.quiet();

    // A new size is a new drawing; until it is done what there is is the one before.
    let held = rig.images.bitmap("icon.svg", whole(80.0, 40.0)).unwrap();
    assert_eq!(held, small);
    let arrived = rig.wait();
    assert!(arrived.redraw && !arrived.relayout);
    let big = rig.images.bitmap("icon.svg", whole(80.0, 40.0)).unwrap();
    assert_eq!((big.width, big.height), (80, 40));
    assert_ne!(big.id, small.id, "a new drawing is new pixels");
    rig.quiet();
    assert_eq!(rig.reads(), ["icon.svg"], "the file is read once");
}

#[test]
fn an_svg_is_never_drawn_larger_than_an_image_may_be() {
    let limits = ImageLimits {
        side: 16,
        ..ImageLimits::default()
    };
    let rig = rig_with(
        vec![("icon.svg", GREEN_SVG.as_bytes().to_vec())],
        limits,
        None,
    );
    rig.images.size("icon.svg");
    rig.wait();
    rig.images.bitmap("icon.svg", whole(1000.0, 500.0));
    rig.wait();
    let bitmap = rig.images.bitmap("icon.svg", whole(1000.0, 500.0)).unwrap();
    assert_eq!((bitmap.width, bitmap.height), (16, 8));
}

#[test]
fn an_svg_is_drawn_at_a_few_sizes_and_a_size_nothing_asks_for_any_more_gives_its_place_up() {
    let rig = rig(vec![("icon.svg", GREEN_SVG.as_bytes().to_vec())]);
    rig.images.size("icon.svg");
    rig.wait();
    // A pane being dragged larger asks a new size at every frame, many of them in the time one
    // frame's passes take.
    for width in 20..60 {
        rig.images.bitmap("icon.svg", whole(width as f32, 10.0));
    }
    // Eight are drawn (as many as a drawing has places), and the page is told once that the rest
    // are the nearest of them.
    let mut told = Vec::new();
    loop {
        told.extend(rig.wait().diagnostics);
        if rig
            .images
            .bitmap("icon.svg", whole(27.0, 10.0))
            .is_some_and(|bitmap| bitmap.width == 27)
        {
            break;
        }
    }
    let [one] = told.as_slice() else {
        panic!("told once, got {told:?}");
    };
    assert!(one.message.contains("more than 8 sizes"), "{one}");
    let nearest = rig.images.bitmap("icon.svg", whole(59.0, 10.0)).unwrap();
    assert_eq!(nearest.width, 27);
    rig.quiet();

    // The sizes of the drag are not asked for any more: a size that is wanted now takes the place of
    // the one that has gone unasked the longest, and is drawn.
    std::thread::sleep(Duration::from_millis(150));
    assert!(rig.images.bitmap("icon.svg", whole(59.0, 10.0)).is_some());
    rig.wait();
    let last = rig.images.bitmap("icon.svg", whole(59.0, 10.0)).unwrap();
    assert_eq!((last.width, last.height), (59, 10));
    rig.quiet();
}

#[test]
fn an_svg_used_at_two_sizes_on_one_page_is_drawn_once_at_each_and_the_page_settles() {
    let rig = rig(vec![("icon.svg", GREEN_SVG.as_bytes().to_vec())]);
    rig.images.size("icon.svg");
    rig.wait();
    let (small, large) = (whole(16.0, 8.0), whole(32.0, 16.0));
    // A host that builds the page again, and asks for both sizes each time, for as long as it is
    // woken: the drawing of one must not undo the drawing of the other.
    let mut wakes = 0;
    let mut rounds = 0;
    loop {
        rig.images.bitmap("icon.svg", small);
        rig.images.bitmap("icon.svg", large);
        rounds += 1;
        assert!(rounds < 20, "the page never settled: {wakes} wakes");
        if rig.woke.recv_timeout(Duration::from_millis(400)).is_err() {
            break;
        }
        wakes += 1;
        rig.images.poll();
    }
    assert!((1..=3).contains(&wakes), "{wakes} wakes");
    // Each use has the raster of its own size, and they stay the same pixels.
    let (a, b) = (
        rig.images.bitmap("icon.svg", small).unwrap(),
        rig.images.bitmap("icon.svg", large).unwrap(),
    );
    assert_eq!((a.width, a.height), (16, 8));
    assert_eq!((b.width, b.height), (32, 16));
    assert_ne!(a.id, b.id);
    for _ in 0..50 {
        assert_eq!(rig.images.bitmap("icon.svg", small).unwrap().id, a.id);
        assert_eq!(rig.images.bitmap("icon.svg", large).unwrap().id, b.id);
    }
    rig.quiet();
    assert_eq!(rig.reads(), ["icon.svg"]);
}

// --- What an SVG cannot reach ---------------------------------------------------------------------

/// A directory of the test's own in the temporary directory, removed at the end by name.
struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("pencere-images-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn write(&self, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if let Ok(entries) = std::fs::read_dir(&self.0) {
            for entry in entries.flatten() {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        let _ = std::fs::remove_dir(&self.0);
    }
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0_u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[test]
fn an_svg_reaches_for_no_file_no_url_and_no_data_it_names_and_draws_no_words() {
    let scratch = Scratch::new("external");
    // A green drawing on the disk: the default of the SVG library is to draw it where an SVG names it.
    let green = scratch.write("green.svg", br##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8" fill="#00ff00"/></svg>"##);
    let nested = base64(br##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8" fill="#00ff00"/></svg>"##);
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="8" height="8" viewBox="0 0 8 8">
  <rect width="8" height="8" fill="#ff0000"/>
  <image width="8" height="8" xlink:href="file://{path}"/>
  <image width="8" height="8" href="{path}"/>
  <image width="8" height="8" href="data:image/svg+xml;base64,{nested}"/>
  <text x="0" y="8" font-size="8" fill="#0000ff">hello</text>
</svg>"##,
        path = green.display()
    );

    // The premise: the library, left to its defaults, draws what the SVG names.
    {
        use resvg::{tiny_skia, usvg};
        let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).unwrap();
        let mut pixmap = tiny_skia::Pixmap::new(8, 8).unwrap();
        resvg::render(
            &tree,
            tiny_skia::Transform::identity(),
            &mut pixmap.as_mut(),
        );
        let (r, g) = (pixmap.data()[0], pixmap.data()[1]);
        assert!(
            g > 200 && r < 50,
            "the premise: left to its defaults, it draws the file ({r} {g})"
        );
    }

    let rig = rig(vec![("trap.svg", svg.into_bytes())]);
    rig.images.size("trap.svg");
    rig.wait();
    rig.images.bitmap("trap.svg", whole(8.0, 8.0));
    rig.wait();
    let bitmap = rig.images.bitmap("trap.svg", whole(8.0, 8.0)).unwrap();
    // Red, all of it: neither the files, nor the data, nor the words.
    assert!(
        bitmap
            .pixels()
            .chunks_exact(4)
            .all(|px| px == [255, 0, 0, 255]),
        "something besides the SVG's own shapes was drawn"
    );
    // And nothing but the SVG itself was asked of the bundle.
    assert_eq!(rig.reads(), ["trap.svg"]);
}

#[test]
fn the_words_of_an_svg_are_not_drawn() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="12"><text x="0" y="10" font-size="12" font-family="Helvetica" fill="#000000">Words words words</text></svg>"##;
    let rig = rig(vec![("words.svg", svg.as_bytes().to_vec())]);
    rig.images.size("words.svg");
    rig.wait();
    rig.images.bitmap("words.svg", whole(40.0, 12.0));
    rig.wait();
    let bitmap = rig.images.bitmap("words.svg", whole(40.0, 12.0)).unwrap();
    assert!(
        bitmap.pixels().iter().all(|byte| *byte == 0),
        "something was drawn"
    );
}

// --- The limits and the refusals -------------------------------------------------------------------

#[test]
fn an_image_past_the_side_limit_is_refused_with_a_diagnostic_and_leaves_its_box_empty() {
    let rig = rig(vec![
        ("wide.png", solid(4097, 1, [0, 0, 0, 255])),
        ("tall.png", solid(1, 4097, [0, 0, 0, 255])),
        ("just.png", solid(4096, 1, [0, 0, 0, 255])),
    ]);
    for name in ["wide.png", "tall.png", "just.png"] {
        rig.images.size(name);
    }
    let arrived = rig.until(|a| a.diagnostics.len() == 2 && a.relayout);
    let words: Vec<String> = arrived
        .diagnostics
        .iter()
        .map(ToString::to_string)
        .collect();
    assert!(
        words.iter().any(|w| w.contains("wide.png")
            && w.contains("4097\u{d7}1")
            && w.contains("4096\u{d7}4096")),
        "{words:?}"
    );
    assert!(
        words
            .iter()
            .any(|w| w.contains("tall.png") && w.contains("1\u{d7}4097")),
        "{words:?}"
    );
    assert_eq!(rig.images.size("wide.png"), None);
    assert_eq!(rig.images.size("tall.png"), None);
    assert_eq!(rig.images.bitmap("wide.png", whole(10.0, 10.0)), None);
    // The side itself is within the limit.
    while rig.images.size("just.png").is_none() {
        rig.wait();
    }
    assert_eq!(rig.images.size("just.png"), Some(Size::new(4096.0, 1.0)));
    // Told once: asking again neither reads nor tells again.
    rig.images.size("wide.png");
    rig.quiet();
    assert_eq!(rig.reads().iter().filter(|p| *p == "wide.png").count(), 1);
}

#[test]
fn the_pixels_of_a_view_are_held_to_a_budget_and_the_image_that_would_pass_it_is_refused() {
    let limits = ImageLimits {
        pixel_bytes: 2 * 4 * 4 * 4,
        ..ImageLimits::default()
    };
    let files = ["a.png", "b.png", "c.png"]
        .map(|name| (name, solid(4, 4, [1, 1, 1, 255])))
        .to_vec();
    let rig = rig_with(files, limits, None);
    // One at a time, so that which one is over is not a race.
    for name in ["a.png", "b.png", "c.png"] {
        rig.images.size(name);
        rig.until(|a| a.relayout || !a.diagnostics.is_empty());
    }
    assert!(rig.images.size("a.png").is_some());
    assert!(rig.images.size("b.png").is_some());
    assert_eq!(rig.images.size("c.png"), None);
    // The refusal was told with the reason.
    rig.images.size("c.png");
    rig.quiet();
}

#[test]
fn the_budget_diagnostic_names_the_image_and_the_budget() {
    let limits = ImageLimits {
        pixel_bytes: 63,
        ..ImageLimits::default()
    };
    let rig = rig_with(vec![("a.png", solid(4, 4, [1, 1, 1, 255]))], limits, None);
    rig.images.size("a.png");
    let arrived = rig.wait();
    let [diagnostic] = arrived.diagnostics.as_slice() else {
        panic!("one diagnostic, got {arrived:?}");
    };
    assert_eq!(diagnostic.path, "a.png");
    assert!(
        diagnostic
            .message
            .contains("past the 0 MiB a view may hold"),
        "{diagnostic}"
    );
    assert!(!arrived.relayout);
}

#[test]
fn a_new_drawing_of_an_svg_takes_the_room_of_one_nothing_asks_for() {
    // Room for the drawing's own shapes (a few hundred bytes) and one drawing of 16 by 8 (512
    // bytes) and not for two: kept pixels are let go for new ones when nothing asks for them. Here
    // that is at once, for the time they are kept is none.
    let limits = ImageLimits {
        pixel_bytes: 16 * 8 * 4 + 400,
        idle: Duration::ZERO,
        ..ImageLimits::default()
    };
    let rig = rig_with(
        vec![("icon.svg", GREEN_SVG.as_bytes().to_vec())],
        limits,
        None,
    );
    rig.images.size("icon.svg");
    rig.wait();
    rig.images.bitmap("icon.svg", whole(16.0, 8.0));
    rig.wait();
    rig.images.bitmap("icon.svg", whole(8.0, 4.0));
    let arrived = rig.wait();
    assert!(arrived.diagnostics.is_empty(), "{arrived:?}");
    assert_eq!(
        rig.images
            .bitmap("icon.svg", whole(8.0, 4.0))
            .map(|b| b.width),
        Some(8)
    );
    rig.images.bitmap("icon.svg", whole(16.0, 8.0));
    let arrived = rig.wait();
    assert!(arrived.diagnostics.is_empty(), "{arrived:?}");
    assert_eq!(
        rig.images
            .bitmap("icon.svg", whole(16.0, 8.0))
            .map(|b| b.width),
        Some(16)
    );
}

#[test]
fn an_image_that_does_not_fit_waits_and_is_had_when_the_pixels_of_one_nothing_asks_for_are_let_go()
{
    // Room for two images of 4 by 4 pixels, and three named.
    let limits = ImageLimits {
        pixel_bytes: 2 * 4 * 4 * 4,
        idle: Duration::from_millis(80),
        ..ImageLimits::default()
    };
    let files = ["a.png", "b.png", "c.png"]
        .map(|name| (name, solid(4, 4, [1, 1, 1, 255])))
        .to_vec();
    let rig = rig_with(files, limits, None);
    for name in ["a.png", "b.png"] {
        rig.images.size(name);
        rig.until(|a| a.relayout);
    }
    // Both are in use, so the third has no room: told, once, and not refused for good.
    rig.images.size("a.png");
    rig.images.size("b.png");
    assert_eq!(rig.images.size("c.png"), None);
    let told = rig.until(|a| !a.diagnostics.is_empty());
    let [one] = told.diagnostics.as_slice() else {
        panic!("one diagnostic, got {told:?}");
    };
    assert_eq!(one.path, "c.png");
    assert!(one.message.contains("past"), "{one}");
    // Asked again before the time the others are kept for, it starts nothing.
    assert_eq!(rig.images.size("c.png"), None);
    rig.quiet();
    assert_eq!(rig.reads(), ["a.png", "b.png", "c.png"]);

    // Later, when the other two have gone unasked for that time, it is had: the pixels of the one
    // asked for the longest ago are let go (its size is still known).
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(rig.images.size("c.png"), None);
    let had = rig.until(|a| a.relayout);
    assert!(had.diagnostics.is_empty(), "told again: {had:?}");
    assert_eq!(rig.images.size("c.png"), Some(Size::new(4.0, 4.0)));
    assert_eq!(rig.images.size("a.png"), Some(Size::new(4.0, 4.0)));
    // And its pixels come back when something asks for them, in the room of another that nothing asks for.
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(rig.images.bitmap("a.png", whole(4.0, 4.0)), None);
    rig.until(|a| a.redraw);
    assert!(rig.images.bitmap("a.png", whole(4.0, 4.0)).is_some());
}

#[test]
fn what_is_not_an_image_is_refused_and_told_why_once() {
    let png = solid(4, 4, [1, 1, 1, 255]);
    let rig = rig(vec![
        ("notes.txt", b"hello".to_vec()),
        ("cut.png", png[..png.len() - 20].to_vec()),
        ("bad.svg", b"<svg><rect".to_vec()),
        ("zipped.svgz", vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 3]),
    ]);
    for name in [
        "notes.txt",
        "cut.png",
        "bad.svg",
        "zipped.svgz",
        "absent.png",
    ] {
        rig.images.size(name);
    }
    let arrived = rig.until(|a| a.diagnostics.len() == 5);
    let say = |path: &str| {
        arrived
            .diagnostics
            .iter()
            .find(|d| d.path == path)
            .map(|d| d.message.clone())
            .unwrap_or_default()
    };
    assert!(
        say("notes.txt").contains("is not a PNG, JPEG or SVG"),
        "{}",
        say("notes.txt")
    );
    assert!(say("zipped.svgz").contains("is not a PNG, JPEG or SVG"));
    assert!(
        say("cut.png").contains("could not be decoded"),
        "{}",
        say("cut.png")
    );
    assert!(
        say("bad.svg").contains("could not be decoded"),
        "{}",
        say("bad.svg")
    );
    assert!(
        say("absent.png").contains("not a file of the view's bundle"),
        "{}",
        say("absent.png")
    );
    assert!(!arrived.relayout, "nothing's size is known");
    // Told once, and not read again.
    for name in [
        "notes.txt",
        "cut.png",
        "bad.svg",
        "zipped.svgz",
        "absent.png",
    ] {
        assert_eq!(rig.images.size(name), None);
        assert_eq!(rig.images.bitmap(name, whole(10.0, 10.0)), None);
    }
    rig.quiet();
    assert_eq!(rig.reads().len(), 5);
}

#[test]
fn a_file_larger_than_a_file_may_be_is_refused_unread() {
    let limits = ImageLimits {
        file_bytes: 100,
        ..ImageLimits::default()
    };
    let rig = rig_with(
        vec![("big.png", solid(64, 64, [9, 9, 9, 255]))],
        limits,
        None,
    );
    rig.images.size("big.png");
    let arrived = rig.wait();
    assert!(
        arrived.diagnostics[0]
            .message
            .contains("past the 100 bytes"),
        "{arrived:?}"
    );
}

#[test]
fn a_page_that_names_more_images_than_it_may_is_told_once_and_the_rest_are_not_read() {
    let limits = ImageLimits {
        paths: 2,
        ..ImageLimits::default()
    };
    let rig = rig_with(Vec::new(), limits, None);
    for name in ["a", "b", "c", "d", "e"] {
        rig.images.size(name);
    }
    let arrived = rig.until(|a| a.diagnostics.len() >= 3);
    assert_eq!(
        arrived.diagnostics.len(),
        3,
        "two absent and one for the rest: {arrived:?}"
    );
    assert!(
        arrived
            .diagnostics
            .iter()
            .any(|d| d.message.contains("past the 2 images"))
    );
    rig.images.size("f");
    rig.quiet();
    assert_eq!(rig.reads().len(), 2);
}

#[test]
fn images_that_are_gone_wake_nobody() {
    let gate = Arc::new(Gate::default());
    let rig = rig_with(
        vec![("a.png", solid(2, 2, [1, 2, 3, 255]))],
        ImageLimits::default(),
        Some(Arc::clone(&gate)),
    );
    rig.images.size("a.png");
    let Rig { images, woke, .. } = rig;
    drop(images);
    gate.open();
    assert!(woke.recv_timeout(Duration::from_millis(400)).is_err());
}

// --- What decoding and drawing may take ----------------------------------------------------------

#[test]
fn the_memory_a_decoder_works_in_is_approved_before_it_is_used() {
    // The pixels are 64 bytes; the decoders work in more than that besides, and one image may be
    // worked in no more than the limit.
    let limits = ImageLimits {
        work_bytes: 16,
        ..ImageLimits::default()
    };
    let rig = rig_with(
        vec![
            ("a.png", solid(4, 4, [1, 1, 1, 255])),
            ("b.jpg", RED_JPEG.to_vec()),
        ],
        limits,
        None,
    );
    for name in ["a.png", "b.jpg"] {
        rig.images.size(name);
        let arrived = rig.wait();
        let [one] = arrived.diagnostics.as_slice() else {
            panic!("one diagnostic for {name}, got {arrived:?}");
        };
        assert_eq!(one.path, name);
        assert!(one.message.contains("to work in"), "{one}");
        assert_eq!(rig.images.size(name), None);
    }
}

/// `levels` of groups that each use the one before ten times: ten to the `levels` minus one parts
/// in a file of two kilobytes.
fn use_bomb(levels: usize) -> String {
    let mut svg = String::from(
        r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="10"><defs><g id="g0"><rect width="1" height="1"/></g>"##,
    );
    for level in 1..levels {
        svg.push_str(&format!(r##"<g id="g{level}">"##));
        for _ in 0..10 {
            svg.push_str(&format!(r##"<use xlink:href="#g{}"/>"##, level - 1));
        }
        svg.push_str("</g>");
    }
    svg.push_str(&format!(
        r##"</defs><use xlink:href="#g{}"/></svg>"##,
        levels - 1
    ));
    svg
}

#[test]
fn a_drawing_that_unfolds_into_far_more_than_its_file_says_is_refused() {
    // Ten thousand parts are a drawing; a hundred thousand and more are not.
    let rig = rig(vec![
        ("fine.svg", use_bomb(5).into_bytes()),
        ("bomb.svg", use_bomb(7).into_bytes()),
        ("worse.svg", use_bomb(9).into_bytes()),
    ]);
    rig.images.size("fine.svg");
    rig.until(|a| a.relayout);
    assert!(rig.images.size("fine.svg").is_some());
    for name in ["bomb.svg", "worse.svg"] {
        rig.images.size(name);
        let arrived = rig.until(|a| !a.diagnostics.is_empty());
        let [one] = arrived.diagnostics.as_slice() else {
            panic!("one diagnostic for {name}, got {arrived:?}");
        };
        assert_eq!(one.path, name);
        assert_eq!(rig.images.size(name), None);
    }
    // The first is refused for what it comes to, the second by the parser's own limit.
    rig.images.size("bomb.svg");
    rig.quiet();
}

#[test]
fn a_drawing_is_held_to_the_budget_with_its_parsed_shapes() {
    // The pixels of a drawing at any size are a share of the budget, and so are its shapes.
    let limits = ImageLimits {
        pixel_bytes: 1000,
        ..ImageLimits::default()
    };
    let rig = rig_with(vec![("bomb.svg", use_bomb(5).into_bytes())], limits, None);
    rig.images.size("bomb.svg");
    let arrived = rig.wait();
    let [one] = arrived.diagnostics.as_slice() else {
        panic!("one diagnostic, got {arrived:?}");
    };
    assert!(one.message.contains("MiB"), "{one}");
    assert!(!arrived.relayout);
}

#[test]
fn the_layers_a_drawing_is_drawn_on_are_approved_with_its_pixels() {
    // Thirty groups inside one another, each with an opacity, are thirty layers the size of the
    // surface while they are drawn.
    let mut svg = String::from(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64">"##,
    );
    for _ in 0..30 {
        svg.push_str(r##"<g opacity="0.9">"##);
    }
    svg.push_str(r##"<rect width="64" height="64" fill="#00ff00"/>"##);
    for _ in 0..30 {
        svg.push_str("</g>");
    }
    svg.push_str("</svg>");
    // A surface of 64 by 64 is sixteen kilobytes: a few layers are within the limit, thirty are not.
    let limits = ImageLimits {
        work_bytes: 5 * 64 * 64 * 4,
        ..ImageLimits::default()
    };
    let rig = rig_with(vec![("deep.svg", svg.into_bytes())], limits, None);
    rig.images.size("deep.svg");
    rig.wait();
    assert_eq!(rig.images.bitmap("deep.svg", whole(64.0, 64.0)), None);
    let arrived = rig.until(|a| !a.diagnostics.is_empty());
    let [one] = arrived.diagnostics.as_slice() else {
        panic!("one diagnostic, got {arrived:?}");
    };
    assert!(one.message.contains("to work in"), "{one}");
    assert!(one.message.contains("drawn at 64\u{d7}64"), "{one}");
    // Told for that size once, and not asked again at it.
    assert_eq!(rig.images.bitmap("deep.svg", whole(64.0, 64.0)), None);
    rig.quiet();
    // At a size the layers fit, it is drawn.
    assert_eq!(rig.images.bitmap("deep.svg", whole(8.0, 8.0)), None);
    rig.wait();
    assert!(rig.images.bitmap("deep.svg", whole(8.0, 8.0)).is_some());
}

// --- A photograph is held the way it was taken --------------------------------------------------

/// The fixture JPEG with the one EXIF block it has swapped for one that says only `orientation`.
fn jpeg_taken(orientation: u16) -> Vec<u8> {
    let app1 = RED_JPEG
        .windows(2)
        .position(|pair| pair == [0xFF, 0xE1])
        .expect("an EXIF block");
    let length = usize::from(u16::from_be_bytes([RED_JPEG[app1 + 2], RED_JPEG[app1 + 3]]));
    let mut tiff = b"II*\0".to_vec();
    tiff.extend(8_u32.to_le_bytes());
    tiff.extend(1_u16.to_le_bytes());
    // The orientation: tag 0x0112, a short, once.
    tiff.extend(0x0112_u16.to_le_bytes());
    tiff.extend(3_u16.to_le_bytes());
    tiff.extend(1_u32.to_le_bytes());
    tiff.extend(orientation.to_le_bytes());
    tiff.extend([0, 0]);
    tiff.extend(0_u32.to_le_bytes());
    let mut block = vec![0xFF, 0xE1];
    block.extend(((tiff.len() + 8) as u16).to_be_bytes());
    block.extend(b"Exif\0\0");
    block.extend(tiff);
    let mut bytes = RED_JPEG[..app1].to_vec();
    bytes.extend(block);
    bytes.extend(&RED_JPEG[app1 + 2 + length..]);
    bytes
}

#[test]
fn a_photograph_is_shown_upright_as_the_exif_of_its_file_says() {
    let rig = rig(vec![
        ("up.jpg", jpeg_taken(1)),
        ("turned.jpg", jpeg_taken(6)),
        ("mirrored.jpg", jpeg_taken(2)),
        ("odd.jpg", jpeg_taken(9)),
    ]);
    for name in ["up.jpg", "turned.jpg", "mirrored.jpg", "odd.jpg"] {
        rig.images.size(name);
        rig.until(|a| a.relayout);
    }
    // 16 by 8 as it is held; a quarter turn makes it 8 by 16. A mirror, and a value that is none,
    // leave the size.
    assert_eq!(rig.images.size("up.jpg"), Some(Size::new(16.0, 8.0)));
    assert_eq!(rig.images.size("turned.jpg"), Some(Size::new(8.0, 16.0)));
    assert_eq!(rig.images.size("mirrored.jpg"), Some(Size::new(16.0, 8.0)));
    assert_eq!(rig.images.size("odd.jpg"), Some(Size::new(16.0, 8.0)));
    let turned = rig.images.bitmap("turned.jpg", whole(8.0, 16.0)).unwrap();
    assert_eq!((turned.width, turned.height), (8, 16));
    assert_eq!(pixel(&turned, 7, 15)[3], 255);
}

#[test]
fn the_eight_orientations_turn_the_pixels_as_a_web_view_does() {
    use crate::decode::{Raster, Turn, orient};
    // a b c
    // d e f
    let held = |turn| {
        let raster = Raster {
            width: 3,
            height: 2,
            pixels: b"abcdef"
                .iter()
                .flat_map(|letter| [*letter, 0, 0, 255])
                .collect(),
        };
        let turned = orient(raster, turn);
        let letters: String = turned
            .pixels
            .chunks_exact(4)
            .map(|pixel| char::from(pixel[0]))
            .collect();
        (turned.width, turned.height, letters)
    };
    assert_eq!(held(Turn::Upright), (3, 2, "abcdef".into()));
    assert_eq!(held(Turn::Mirrored), (3, 2, "cbafed".into()));
    assert_eq!(held(Turn::Half), (3, 2, "fedcba".into()));
    assert_eq!(held(Turn::Flipped), (3, 2, "defabc".into()));
    assert_eq!(held(Turn::Transposed), (2, 3, "adbecf".into()));
    assert_eq!(held(Turn::Clockwise), (2, 3, "daebfc".into()));
    assert_eq!(held(Turn::Transversed), (2, 3, "fcebda".into()));
    assert_eq!(held(Turn::Anticlockwise), (2, 3, "cfbead".into()));
}

#[test]
fn an_exif_block_that_is_not_whole_says_nothing() {
    use crate::decode::{Turn, orientation};
    let good = {
        let mut tiff = b"MM\0*".to_vec();
        tiff.extend(8_u32.to_be_bytes());
        tiff.extend(1_u16.to_be_bytes());
        tiff.extend(0x0112_u16.to_be_bytes());
        tiff.extend(3_u16.to_be_bytes());
        tiff.extend(1_u32.to_be_bytes());
        tiff.extend(8_u16.to_be_bytes());
        tiff.extend([0, 0]);
        tiff
    };
    assert_eq!(orientation(&good), Turn::Anticlockwise);
    // Every cut of it, and bytes that are not a header, are upright and do not panic.
    for end in 0..good.len() {
        let _ = orientation(&good[..end]);
    }
    assert_eq!(orientation(&good[..19]), Turn::Upright);
    assert_eq!(orientation(b"nonsense"), Turn::Upright);
    // An offset that points past the block.
    let mut far = good.clone();
    far[4..8].copy_from_slice(&u32::MAX.to_be_bytes());
    assert_eq!(orientation(&far), Turn::Upright);
}
