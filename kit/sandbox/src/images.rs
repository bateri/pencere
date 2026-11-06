//! The images of a page: files of the bundle the view came in, decoded off the thread that draws,
//! once each, and handed to whoever draws them.
//!
//! A page names an image by the path of a file of its bundle ([`pencere_view::image`]); the host
//! gives the paths' bytes through [`Assets`], and [`Images`] turns them into what the host needs
//! of an image — its size, for the layout ([`Images::size`], which the host's
//! [`pencere_view::Measure::image`] answers from), and its pixels, for the draw list's
//! [`pencere_view::Picture`] ([`Images::picture`], [`Images::bitmap`]). The formats are PNG, JPEG, SVG and PDF, and the
//! limits are [`ImageLimits`], the same in every terminal that uses this crate: a view that works
//! in one works in all. A PDF is a file with pages: an element names the page it shows, and a
//! page is asked for by the path and its number, each with its own size and its own drawings;
//! it is still one path, which is one of the paths a view may name.
//!
//! **Nothing here waits.** The first time a path is asked for, a worker thread of its own reads
//! the file (an SVG it parses, a PNG or a JPEG it reads the header of), and the answer is *not
//! yet* until it is done; then the host is woken — once for everything that arrived since it
//! last looked — and told what [`Images::poll`] says: that a size is known and the page is to be
//! laid out again, that pixels arrived and it is to be drawn, and why an image was refused (the
//! page's diagnostics). Each path is read once for its size (a PDF for the sizes of all its pages) (the result, a refusal as well, is
//! kept for the life of the [`Images`]), and drawn once for each size it is asked at (a few
//! sizes at most, each its own raster; a page of a PDF has the few of its own): a host at rest is asked for nothing, and so wakes for
//! nothing.
//!
//! **What the view holds is bounded, and what it holds is what is drawn.** An SVG or a page of a
//! PDF is drawn at the size it is asked at, and so is a PNG or a JPEG: the file is decoded at its own size on the
//! worker, shrunk to the size that is drawn (never made larger than the file; each pixel is the
//! average of the ones it covers) and only that is kept, so that a screenshot of 2560 by 1600
//! pixels shown 780 points wide holds the pixels of the 780 and not the 4 million. The size is
//! rounded up to a step of [`STEP`] pixels of the long side, so that a pane that is dragged larger
//! asks for a new decoding once for each step and not for each pixel. A size that is larger than
//! the ones held is decoded again from the bytes the host has ([`Assets::read`]); until it is
//! there the nearest there is is what there is (the host draws it larger), so that a box that
//! had pixels is never empty. A size that is smaller than one held is that one (the host draws
//! it smaller): nothing is decoded again for it.
//!
//! The pixels of the images (and the parsed drawings) are held to [`ImageLimits::pixel_bytes`] in
//! all, the bytes of what is *kept*. When a new drawing needs room, the pixels of the ones nothing
//! has asked for in [`ImageLimits::idle`] are let go (they are drawn again if they are asked for
//! later), the oldest first, and only where that makes room enough: pixels are not let go for a
//! drawing that would not fit after all. A drawing that still does not fit is not refused for good,
//! it waits, and is tried again when it is asked for after that time, which is when something may
//! have been let go. **A box is not left empty for it:** an SVG or a page of a PDF that has no
//! pixels at any size is drawn at a half, a quarter, … of the size, the first that fits, and that
//! stands in (the host draws it larger) until the size asked for is drawn. And the host is woken
//! when there may be room — when the pixels of the pictures it no longer asks for have gone unasked
//! for [`ImageLimits::idle`] and letting them go would make room enough — so that the size asked
//! for is drawn with nothing more done: a host at rest is not woken for a drawing whose room is held
//! by what it shows (a host asks only for what it shows: [`pencere_view::Picture`]).
//!
//! **A large SVG or page is drawn where it is seen** ([`Images::picture`]). A page whose whole is
//! more than [`SMALL`] pixels at the size it is shown at is drawn in a *part* of that size: what is
//! seen ([`pencere_view::Picture::shown`]) and [`PAY`] pixels more on each side, rounded out to a
//! grid of [`GRID`] pixels, and at most a side of [`ImageLimits::side`]; the pixels held come to the
//! screen and not to the page, and the page may be longer than a side. A part that holds what is
//! seen and a [`MARGIN`] more is the one that is given, so that a page scrolled a little is drawn
//! no more; past it the part of the page that is asked for is drawn, and the pixels that were
//! given, or else the whole page at a small size ([`BASE`], drawn first), stand in until it is: a
//! box is not empty. The size itself (the scale) goes in steps for a large one, so that a pane that
//! is dragged larger is drawn once for each step. The pixels that were not asked for at the host's
//! last frame ([`RECENT`] before the last it asked) are let go of when room is needed, without
//! waiting out [`ImageLimits::idle`]; and where there is no other room a drawing may take the
//! place of the pixels of the same page that stand in for it. A PNG or a JPEG is held whole, as
//! it was.

use std::collections::{HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use pencere_view::{Picture, Rect, Size};

use crate::decode::{self, Failure, Format, Part, Raster, Vector};
use crate::pdf::Document;

/// The files of the view's bundle, as the host has them: the one door the images have to them.
pub trait Assets: Send + Sync + 'static {
    /// The bytes of the file at `path`, as the manifest of the bundle names it, or `None` where the
    /// bundle has no such file. Called on the images' worker thread, or on a script's own (the
    /// scripts it requires), never the one that draws; it may take the time reading a file does.
    ///
    /// **`path` is text a page wrote, and is not to be trusted**: it can be `../../secrets`, an
    /// absolute path, a name with a NUL in it or any other string a script can make. A host
    /// looks it up among the files of its *verified* manifest (a map from the manifest's own
    /// paths to the files they name) and answers `None` for what is not there; it never joins it
    /// to a directory, or opens it by itself, or lists a directory with it. Whether the file at a
    /// path of the manifest is within the limits of a bundle was settled when the bundle was
    /// taken in, and the images hold what they read to their own besides ([`ImageLimits`]).
    ///
    /// A view of `lua` also names files beside its bundle: `sha256:` and a
    /// digest (a file its program attached, specification §8.8) and `file:` and a handle (one the
    /// person gave it, §8.9). A host answers these from its own tables — the view's attached files,
    /// the handles it made — and `None` for one that is in neither; a manifest path has no `:`, so
    /// the names never meet.
    fn read(&self, path: &str) -> Option<Arc<[u8]>>;

    /// The media type the manifest gives the file at `path` (`type/subtype`, lower case, as it
    /// is listed), or `None` where the bundle has no such file. What a script may load of the
    /// bundle is decided by it: a file typed `text/x-lua` is a script, whatever the file is
    /// called. Answered from the manifest and not by reading the file, since a script asks for
    /// it of every path it might require; the caution about `path` in [`Assets::read`] is the
    /// same.
    fn media_type(&self, path: &str) -> Option<String>;
}

/// What the images of one view are held to.
#[derive(Clone, Debug)]
pub struct ImageLimits {
    /// The most pixels an image may be wide or tall (so the most a drawn SVG is).
    pub side: u32,
    /// Bytes of pixels the view may hold in all, over its images as they are kept (4 bytes a
    /// pixel: a PNG or a JPEG at the sizes it is drawn at, not the file's own), and of the parsed
    /// drawings of its SVGs. An image whose own pixels are more than this is refused, since it is
    /// decoded whole before it is shrunk.
    pub pixel_bytes: usize,
    /// Bytes the file of one image may be: the most any file of a bundle may be.
    pub file_bytes: usize,
    /// Different paths a view may name, as many as a bundle may have files; the ones after that
    /// are refused without being read.
    pub paths: usize,
    /// Bytes of memory one image may be worked in, besides the pixels it comes to: what a decoder
    /// holds while it decodes, what a drawing holds in the layers it is drawn on. An image is
    /// worked on at a time.
    pub work_bytes: usize,
    /// Different sizes an image is drawn at, each its own raster (for a PDF, each page's); a size
    /// past them is shown as the nearest of the others.
    pub sizes: usize,
    /// How long the pixels of an image that nothing asks for are kept, if their room is needed.
    pub idle: Duration,
}

impl Default for ImageLimits {
    fn default() -> Self {
        Self {
            side: 4096,
            pixel_bytes: 64 << 20,
            file_bytes: 8 << 20,
            paths: 1024,
            work_bytes: 192 << 20,
            sizes: 8,
            idle: Duration::from_secs(2),
        }
    }
}

/// The pixels of an image: sRGB, eight bits a channel, **premultiplied** by the opacity, in rows from
/// the top, `4 * width * height` bytes.
#[derive(Clone, Debug)]
pub struct Bitmap {
    /// Names these pixels and no other: every new size an image is drawn at is another `id`. What the host keeps of an image (its texture) is kept by it.
    pub id: u64,
    pub width: u32,
    pub height: u32,
    /// What these pixels hold of the **whole** image, as shares of it like [`Picture::crop`]: all of
    /// it, `(0, 0, 1, 1)`, for a PNG or a JPEG and for a vector image that is small; the part that
    /// is seen, and a margin, for a page of a PDF or an SVG that is large ([`Images::picture`]).
    pub part: Rect,
    pixels: Arc<Vec<u8>>,
}

impl Bitmap {
    pub(crate) fn new(id: u64, width: u32, height: u32, pixels: Vec<u8>) -> Self {
        Self {
            id,
            width,
            height,
            part: Rect::new(0.0, 0.0, 1.0, 1.0),
            pixels: Arc::new(pixels),
        }
    }

    /// What `picture`'s rectangle shows of these pixels, as shares of them: [`Picture::crop`] taken
    /// from the whole image to the part these pixels hold. A host draws the pixels as a texture
    /// with this for its crop. It reaches past the pixels (below 0 or above 1) only where the
    /// rectangle is cut off by its clip, which is not drawn.
    pub fn crop_of(&self, picture: &Picture) -> Rect {
        let (part, crop) = (self.part, picture.crop);
        Rect::new(
            (crop.x - part.x) / part.w.max(f32::EPSILON),
            (crop.y - part.y) / part.h.max(f32::EPSILON),
            crop.w / part.w.max(f32::EPSILON),
            crop.h / part.h.max(f32::EPSILON),
        )
    }

    /// The pixels, `4 * width * height` bytes.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}

impl PartialEq for Bitmap {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

/// Why an image is not on the page, for the person who wrote it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// The path the page named.
    pub path: String,
    /// What is wrong with it, a sentence that goes after the path.
    pub message: String,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "image {:?} {}", self.path, self.message)
    }
}

/// What came of the images since the host last looked ([`Images::poll`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Arrival {
    /// A size is known that was not: the page is laid out again, because the room an image takes
    /// is not what it was.
    pub relayout: bool,
    /// Pixels are there that were not: the page is drawn again.
    pub redraw: bool,
    /// Images that were refused, each once.
    pub diagnostics: Vec<Diagnostic>,
}

impl Arrival {
    /// Nothing came.
    pub fn is_empty(&self) -> bool {
        !self.relayout && !self.redraw && self.diagnostics.is_empty()
    }
}

enum Job {
    /// Read the file at the path: an SVG or a PDF is parsed, a PNG or a JPEG has its header read.
    Decode(String),
    /// Draw the image at the path at this size: an SVG or a PDF's page is drawn, a PNG or a JPEG
    /// decoded and shrunk.
    Draw(String, At),
    /// Let go of the parsed SVG or PDF at the path.
    Forget(String),
    /// Look at what waits for room again: the host asked for it, and may show other things than it
    /// did ([`Shared::wake_due`]).
    Look,
}

/// Pixels the view holds, and when something last asked for them.
struct Kept {
    bitmap: Bitmap,
    asked: Instant,
}

/// What is known of one path.
enum Entry {
    /// Being read.
    Pending,
    /// An image: its size, and the pixels it was drawn into at each size it is asked at.
    Image(Drawing),
    /// Refused; the diagnostic was told.
    Refused,
    /// There was no room for it, and there may be later: tried again when it is asked for after
    /// [`ImageLimits::idle`].
    Waiting { since: Instant },
}

/// Where an image is drawn: which page of the file, and the size. A file that has no pages is
/// drawn as page 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct At {
    page: u32,
    /// The size all of the image is drawn at, which the pixels are made as parts of.
    width: u32,
    height: u32,
    /// The part of that size the pixels are of: all of it for a PNG or a JPEG, and for a vector
    /// image whose whole is small.
    part: Part,
}

impl At {
    /// All of the image at a size.
    fn whole(page: u32, width: u32, height: u32) -> Self {
        Self {
            page,
            width,
            height,
            part: Part::whole(width, height),
        }
    }

    /// Whether the pixels are all of the image.
    fn is_whole(self) -> bool {
        self.part == Part::whole(self.width, self.height)
    }

    /// The size alone, as a PNG's or a JPEG's drawn sizes are compared.
    fn covers(self, other: Self) -> bool {
        self.width >= other.width && self.height >= other.height
    }

    fn area(self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    /// The bytes of the pixels.
    fn bytes(self) -> usize {
        decode::pixel_bytes(self.part.w, self.part.h).unwrap_or(usize::MAX)
    }

    /// Whether the part is what a picture's `shown` is inside, the pixels of the part being
    /// within half a pixel of it.
    fn shows(self, shown: Rect) -> bool {
        let (w, h) = (f64::from(self.width), f64::from(self.height));
        let reach = |from: f32, len: f32, total: f64| {
            (f64::from(from) * total, f64::from(from + len) * total)
        };
        let (x0, x1) = reach(shown.x, shown.w, w);
        let (y0, y1) = reach(shown.y, shown.h, h);
        f64::from(self.part.x) <= x0 + 0.5
            && f64::from(self.part.x) + f64::from(self.part.w) + 0.5 >= x1
            && f64::from(self.part.y) <= y0 + 0.5
            && f64::from(self.part.y) + f64::from(self.part.h) + 0.5 >= y1
    }

    /// The part as shares of the whole.
    fn share(self) -> Rect {
        let (w, h) = (self.width.max(1) as f32, self.height.max(1) as f32);
        Rect::new(
            self.part.x as f32 / w,
            self.part.y as f32 / h,
            self.part.w as f32 / w,
            self.part.h as f32 / h,
        )
    }
}

/// An image: its size, and the pixels it was drawn into at each size it is asked at.
struct Drawing {
    /// Its own size: an SVG's, or the file's of a PNG or a JPEG (as it is shown, turned upright),
    /// or the first page's of a PDF.
    size: Size,
    /// A PNG or a JPEG, decoded again from its file for each size, never larger than the file;
    /// not an SVG or a PDF, which are drawn at any.
    raster: bool,
    /// The own size of each page of a PDF; empty for a file that has no pages.
    pages: Vec<Size>,
    drawn: HashMap<At, Drawn>,
    /// The parts of each page that were asked for last, the newest last, no more than [`WANTED`]:
    /// a job for a part that is none of them any more is not done ([`draw`]). More than one, as
    /// the page may be shown in more places than one (at more sizes, or at a part each).
    latest: HashMap<u32, Vec<At>>,
}

impl Drawing {
    /// The own size of the page, from 1: the only page of a file that has no pages is its size.
    fn size_of(&self, page: u32) -> Option<Size> {
        let index = usize::try_from(page).ok()?.checked_sub(1)?;
        if self.pages.is_empty() {
            return (index == 0).then_some(self.size);
        }
        self.pages.get(index).copied()
    }

    /// How many pages the file has.
    fn page_count(&self) -> usize {
        self.pages.len().max(1)
    }
}

/// One size of an image.
enum Drawn {
    Pending,
    Ready(Kept),
    /// There was no room for it at `since`; asked for last at `asked`. The host is to be woken at
    /// `wake` for it to be asked again, where there may be room by then: worked out
    /// ([`Shared::wake_for`]) from what the host showed when it last asked for pixels, `planned`.
    Waiting {
        since: Instant,
        asked: Instant,
        wake: Option<Instant>,
        planned: Option<Instant>,
    },
    Refused,
}

#[derive(Default)]
struct State {
    entries: HashMap<String, Entry>,
    /// Bytes held, over every bitmap in `entries` and every parsed drawing.
    held: usize,
    arrived: Arrival,
    /// The host has been woken and has not looked since.
    woken: bool,
    /// The next `Bitmap::id`.
    next: u64,
    /// When the host last asked for pixels: what it asked for about then is what it shows now
    /// ([`RECENT`]).
    last_ask: Option<Instant>,
    /// When the worker was last told to look at what waits for room ([`Job::Look`]).
    looked: Option<Instant>,
    /// Told that the view names more paths than it may.
    told_paths: bool,
    /// What was told to be out of room (a path, or a path and a size): once each.
    told_room: HashSet<String>,
    /// Told that an SVG, or a page of a PDF, is asked at more sizes than it is drawn at.
    told_sizes: HashSet<String>,
    /// Told that a page is asked for that a file does not have: the path and the page.
    told_pages: HashSet<(String, u32)>,
}

struct Shared {
    assets: Arc<dyn Assets>,
    limits: ImageLimits,
    wake: Box<dyn Fn() + Send + Sync>,
    state: Mutex<State>,
    /// The `Images` is gone: nobody is to be woken, and nothing more is worked on.
    gone: AtomicBool,
}

/// The images of one view.
pub struct Images {
    shared: Arc<Shared>,
    /// The worker's mailbox; the worker starts with the first image asked for. `None` for a thread
    /// that could not be started.
    worker: OnceLock<Option<Sender<Job>>>,
}

impl Images {
    /// The images of a view whose files `assets` has, held to `limits`. `wake` is called, from the
    /// worker's thread, when something arrived that [`Images::poll`] will tell: it hops to wherever the
    /// host listens and returns. Starts nothing until an image is asked for.
    pub fn new(
        assets: Arc<dyn Assets>,
        limits: ImageLimits,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self {
            shared: Arc::new(Shared {
                assets,
                limits,
                wake: Box::new(wake),
                state: Mutex::new(State::default()),
                gone: AtomicBool::new(false),
            }),
            worker: OnceLock::new(),
        }
    }

    /// The size of the image at `src` in pixels of the image itself (an SVG's own size, the file's
    /// of a PNG or a JPEG, which is not the size of the pixels held: see [`Images::bitmap`]; a
    /// page's of a PDF, in points), if it is known. `page` is the page of the file, from 1: a
    /// file that has none has the one (1), and a page it does not have is `None` and told once
    /// to the page's diagnostics. The first time a path is asked for it is not known: the file
    /// starts to be read (a PNG or a JPEG only for its header), and the host is woken when it is
    /// (see [`Images::poll`]). A host's [`pencere_view::Measure::image`] is this.
    pub fn size(&self, src: &str, page: u32) -> Option<Size> {
        let mut state = self.lock();
        let mut wake = false;
        let size = match state.entries.get(src) {
            Some(Entry::Image(drawing)) => {
                let size = drawing.size_of(page);
                if size.is_none() {
                    let pages = drawing.page_count();
                    wake = self.shared.no_such_page(&mut state, src, page, pages);
                }
                size
            }
            Some(Entry::Pending | Entry::Refused) => None,
            Some(Entry::Waiting { since }) => {
                let again = since.elapsed() >= self.shared.limits.idle;
                if again {
                    wake = self.retry(&mut state, src);
                }
                None
            }
            None => {
                wake = self.begin(&mut state, src);
                None
            }
        };
        drop(state);
        self.wake_if(wake);
        size
    }

    /// The pixels of the image at `src` (its page `page`, from 1), to be drawn so that the whole
    /// image is `whole` pixels ([`pencere_view::Picture::full`]), all of it. An image is drawn at the
    /// size, once for each size it is asked at (a few at most: a size past them is the nearest of
    /// the others), and until the drawing at a new size is done the nearest there is of another is
    /// what there is (none the first time). A PNG or a JPEG is held at the size it is drawn at,
    /// rounded up to a step of [`STEP`] pixels of its long side and no larger than the file: a
    /// larger size than the ones held is decoded again from the file, and a smaller one is a held
    /// one that is larger (the host draws the pixels it is given at the size the page gave them).
    /// The host keeps the texture of a bitmap by its [`Bitmap::id`].
    ///
    /// A host that draws a picture asks with [`Images::picture`], which gives only what is seen of
    /// a large SVG or page.
    pub fn bitmap(&self, src: &str, page: u32, whole: Size) -> Option<Bitmap> {
        self.pixels(src, page, whole, Rect::new(0.0, 0.0, 1.0, 1.0))
    }

    /// The pixels for `picture`, as a host draws it: the same as [`Images::bitmap`] for a PNG, a
    /// JPEG and a vector image whose whole is small, and for a large SVG or page of a PDF only
    /// **the part of it that is seen** ([`pencere_view::Picture::shown`]), with a margin that is
    /// rounded out to a grid, so that a page scrolled a little asks for nothing more. What the
    /// bitmap holds of the whole image is [`Bitmap::part`], and what the picture's rectangle shows
    /// of it [`Bitmap::crop_of`]. The pixels held are those of what is seen (a screenful at the
    /// resolution of the display, at most), whatever the size of the page; and while the sharp part
    /// of a page is drawn, the whole page at a small size stands in, so that the box is not empty.
    pub fn picture(&self, picture: &Picture) -> Option<Bitmap> {
        self.pixels(&picture.src, picture.page, picture.full(), picture.shown)
    }

    fn pixels(&self, src: &str, page: u32, whole: Size, shown: Rect) -> Option<Bitmap> {
        let mut state = self.lock();
        let now = Instant::now();
        state.last_ask = Some(now);
        let mut wake = false;
        let found = match state.entries.get_mut(src) {
            Some(Entry::Image(drawing)) if drawing.size_of(page).is_none() => None,
            Some(Entry::Image(_)) => {
                let (found, jobs, told) = self.image_at(&mut state, src, page, whole, shown, now);
                for job in jobs {
                    wake |= self.post(&mut state, job);
                }
                if told {
                    wake |= self.shared.announce(&mut state);
                }
                found
            }
            Some(Entry::Pending | Entry::Refused) => None,
            Some(Entry::Waiting { since }) => {
                if since.elapsed() >= self.shared.limits.idle {
                    wake = self.retry(&mut state, src);
                }
                None
            }
            None => {
                wake = self.begin(&mut state, src);
                None
            }
        };
        drop(state);
        self.wake_if(wake);
        found
    }

    /// What an image has for a whole drawn at `whole`, of which `shown` is seen: its pixels if they
    /// are drawn, else the nearest it has; the drawings to be started if it is not drawn or being,
    /// and whether something is to be told. A PNG or a JPEG is asked at its step ([`stepped`]) and a
    /// held size that covers it is it.
    fn image_at(
        &self,
        state: &mut State,
        src: &str,
        page: u32,
        whole: Size,
        shown: Rect,
        now: Instant,
    ) -> (Option<Bitmap>, Vec<Job>, bool) {
        let limits = &self.shared.limits;
        let Some(Entry::Image(drawing)) = state.entries.get_mut(src) else {
            return (None, Vec::new(), false);
        };
        let (width, height) = drawn_size(whole, limits.side);
        let mut wanted = At::whole(page, width, height);
        let mut part = None;
        if drawing.raster {
            let natural = (drawing.size.w as u32, drawing.size.h as u32);
            let (width, height) = stepped(natural, (width, height));
            wanted = At::whole(page, width, height);
            if let Some(kept) = covering(drawing, wanted) {
                kept.asked = now;
                return (Some(kept.bitmap.clone()), Vec::new(), false);
            }
            // One that covers it is on its way: no other is started for it.
            if covered_by_pending(drawing, wanted) {
                return (stand_in(drawing, wanted, shown, now), Vec::new(), false);
            }
        } else if u64::from(width) * u64::from(height) > SMALL {
            // Large: it is drawn where it is seen, at the size it comes to, in steps.
            let (width, height) = vector_scale(whole, shown, limits.side);
            let seen = seen_part(shown, width, height);
            let at = At {
                part: wanted_part(seen, width, height, limits.side),
                ..At::whole(page, width, height)
            };
            wanted = at;
            part = Some(grow(seen, MARGIN, width, height));
        }
        let mut jobs = Vec::new();
        let mut told = false;
        let mut freed = 0;
        if let Some(margin) = part {
            let wants = drawing.latest.entry(page).or_default();
            wants.retain(|at| *at != wanted);
            wants.push(wanted);
            if wants.len() > WANTED {
                wants.remove(0);
            }
            // Where it is not all of the page that is drawn, the whole page at a small size is there
            // to stand in, and is asked for as long as the page is, a sharp part found or not: let
            // go for room while the page is in view, it would leave a page that is jumped to with
            // nothing to show.
            let base = (!wanted.is_whole()).then(|| {
                let natural = drawing.size_of(page).unwrap_or(Size::new(1.0, 1.0));
                base_of(page, natural)
            });
            if let Some(Drawn::Ready(kept)) = base.and_then(|base| drawing.drawn.get_mut(&base)) {
                kept.asked = now;
            }
            // A sharp one that holds what is seen, with a margin, is the one.
            if let Some(kept) = newest_holding(drawing, wanted, margin) {
                kept.asked = now;
                return (Some(kept.bitmap.clone()), Vec::new(), false);
            }
            // The small one is drawn first, as it is quick.
            if let Some(base) = base {
                // One that waited for room is tried again as the rest are ([`RECENT`]).
                let again = matches!(
                    drawing.drawn.get(&base),
                    Some(Drawn::Waiting { since, .. }) if since.elapsed() >= RECENT
                );
                match drawing.drawn.get(&base) {
                    Some(Drawn::Ready(_)) => {}
                    Some(_) if !again => {}
                    _ => {
                        drawing.drawn.insert(base, Drawn::Pending);
                        jobs.push(Job::Draw(src.to_owned(), base));
                    }
                }
            }
        }
        match drawing.drawn.get_mut(&wanted) {
            Some(Drawn::Ready(kept)) => {
                kept.asked = now;
                return (Some(kept.bitmap.clone()), jobs, false);
            }
            Some(Drawn::Pending | Drawn::Refused) => {}
            Some(Drawn::Waiting { since, asked, .. }) => {
                *asked = now;
                // The room of an SVG or a page comes as soon as the pixels that are no longer shown
                // may go ([`RECENT`]); a PNG's or a JPEG's when they have gone unasked for a time.
                let retry = if drawing.raster { limits.idle } else { RECENT };
                if since.elapsed() >= retry {
                    drawing.drawn.insert(wanted, Drawn::Pending);
                    jobs.push(Job::Draw(src.to_owned(), wanted));
                } else if state
                    .looked
                    .is_none_or(|looked| now.saturating_duration_since(looked) >= RECENT)
                {
                    // Still shown, and the rest of what is shown may have changed: the worker
                    // plans its wake again once this frame is asked for (no oftener than that).
                    state.looked = Some(now);
                    jobs.push(Job::Look);
                }
            }
            None => {
                // At the most sizes a drawing is drawn at, a size nothing has asked for lately
                // (a pane that was dragged larger asked one at every frame) gives its place up. A
                // page of a PDF has the sizes of its own.
                let sizes_of_page = |drawing: &Drawing| {
                    drawing
                        .drawn
                        .keys()
                        .filter(|at| at.page == wanted.page)
                        .map(|at| (at.width, at.height))
                        .collect::<HashSet<_>>()
                };
                let known = sizes_of_page(drawing).contains(&(wanted.width, wanted.height));
                if !known && sizes_of_page(drawing).len() >= limits.sizes {
                    freed = recycle(drawing, wanted.page, now);
                }
                if known || sizes_of_page(drawing).len() < limits.sizes {
                    drawing.drawn.insert(wanted, Drawn::Pending);
                    jobs.push(Job::Draw(src.to_owned(), wanted));
                } else if state.told_sizes.insert(src.to_owned()) {
                    // One more size than a drawing is drawn at, and every one is in use: no job,
                    // so a page that asks for sizes without end asks for nothing without end.
                    state.arrived.diagnostics.push(Diagnostic {
                        path: src.to_owned(),
                        message: format!(
                            "is drawn at more than {} sizes at once{}; the nearest of them is used",
                            limits.sizes,
                            if drawing.pages.is_empty() {
                                String::new()
                            } else {
                                format!(" (page {})", wanted.page)
                            }
                        ),
                    });
                    told = true;
                }
            }
        }
        state.held = state.held.saturating_sub(freed);
        // The nearest drawn size stands in until this one is.
        let Some(Entry::Image(drawing)) = state.entries.get_mut(src) else {
            return (None, jobs, told);
        };
        (stand_in(drawing, wanted, shown, now), jobs, told)
    }

    /// The bytes of pixels the view holds, and of the parsed drawings it keeps.
    #[cfg(test)]
    pub(crate) fn held(&self) -> usize {
        self.lock().held
    }

    /// When the small whole page that stands in for the parts of `page` of `src` was last asked for,
    /// where it is drawn.
    #[cfg(test)]
    pub(crate) fn small_asked(&self, src: &str, page: u32) -> Option<Instant> {
        let state = self.lock();
        let Some(Entry::Image(drawing)) = state.entries.get(src) else {
            return None;
        };
        match drawing.drawn.get(&base_of(page, drawing.size_of(page)?)) {
            Some(Drawn::Ready(kept)) => Some(kept.asked),
            _ => None,
        }
    }

    /// What arrived since the last look, and the host is not woken again for it. A host that is
    /// woken looks; one that looks at no time is only not told.
    pub fn poll(&self) -> Arrival {
        let mut state = self.lock();
        state.woken = false;
        std::mem::take(&mut state.arrived)
    }

    /// Whether a file is still being read, or an image drawn at a size it was asked at:
    /// the host will be woken for it. What has arrived is [`Images::poll`]'s once this says no.
    ///
    /// **Not for a frame**, which never waits for an image: for a host that wants the page whole
    /// before it looks (a test, a picture of the page) and waits for the wake while this says so.
    /// A file that waits for room (the view's pixels at their budget) is not on its way: it is
    /// tried again when it is asked for after [`ImageLimits::idle`], on the system's clock.
    pub fn busy(&self) -> bool {
        self.lock().entries.values().any(|entry| match entry {
            Entry::Pending => true,
            Entry::Image(drawing) => drawing
                .drawn
                .values()
                .any(|drawn| matches!(drawn, Drawn::Pending)),
            Entry::Refused | Entry::Waiting { .. } => false,
        })
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.shared
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn wake_if(&self, wake: bool) {
        if wake {
            (self.shared.wake)();
        }
    }

    /// Starts on a path nobody has asked for; whether the host is to be woken (for a path that is
    /// refused at once).
    fn begin(&self, state: &mut State, src: &str) -> bool {
        if state.entries.len() >= self.shared.limits.paths {
            if !state.told_paths {
                state.told_paths = true;
                state.arrived.diagnostics.push(Diagnostic {
                    path: src.to_owned(),
                    message: Failure::Paths {
                        most: self.shared.limits.paths,
                    }
                    .to_string(),
                });
                return self.shared.announce(state);
            }
            return false;
        }
        state.entries.insert(src.to_owned(), Entry::Pending);
        self.post(state, Job::Decode(src.to_owned()))
    }

    /// Starts again on a path that had no room.
    fn retry(&self, state: &mut State, src: &str) -> bool {
        state.entries.insert(src.to_owned(), Entry::Pending);
        self.post(state, Job::Decode(src.to_owned()))
    }

    /// Forgets the images at `srcs` and what they held: the files beside the bundle the program let
    /// go of (`sha256:` names, specification §8.8). A name asked for again is read again, and one
    /// the host no longer has is an image that is not there.
    pub fn forget(&self, srcs: &[String]) {
        let mut state = self.lock();
        let mut freed = 0;
        let mut drawings = Vec::new();
        for src in srcs {
            if let Some(Entry::Image(drawing)) = state.entries.remove(src.as_str()) {
                for drawn in drawing.drawn.into_values() {
                    if let Drawn::Ready(kept) = drawn {
                        freed += kept.bitmap.pixels.len();
                    }
                }
                // Only an SVG or a PDF has a parsed drawing for the worker to let go of.
                if !drawing.raster {
                    drawings.push(src.clone());
                }
            }
            state
                .told_room
                .retain(|told| !told.starts_with(src.as_str()));
            state.told_sizes.remove(src.as_str());
            state.told_pages.retain(|(told, _)| told != src);
        }
        state.held = state.held.saturating_sub(freed);
        for src in drawings {
            self.post(&mut state, Job::Forget(src));
        }
        state.arrived.relayout = true;
    }

    /// Hands `job` to the worker, which is started for the first one; whether the host is to be
    /// woken (there is no worker, and what waited for it is refused).
    fn post(&self, state: &mut State, job: Job) -> bool {
        let worker = self.worker.get_or_init(|| {
            let (jobs, inbox) = channel();
            let shared = Arc::clone(&self.shared);
            let started = std::thread::Builder::new()
                .name("view-images".into())
                .spawn(move || serve(&shared, &inbox));
            started.is_ok().then_some(jobs)
        });
        let sent = worker.as_ref().is_some_and(|jobs| jobs.send(job).is_ok());
        if sent {
            return false;
        }
        // No worker: whatever was asked for is refused, and the page is told why.
        let paths: Vec<String> = state
            .entries
            .iter()
            .filter(|(_, entry)| matches!(entry, Entry::Pending))
            .map(|(path, _)| path.clone())
            .collect();
        for path in paths {
            self.shared.refuse(
                state,
                &path,
                &Failure::Broken("no thread to decode it on".into()),
            );
        }
        self.shared.announce(state)
    }
}

impl Drop for Images {
    fn drop(&mut self) {
        self.shared.gone.store(true, Ordering::SeqCst);
    }
}

/// The nearest of the sizes an image (a page of it) is drawn at to `wanted`, in the proportion of
/// their areas, to stand in until it is: of those that hold all of `shown`, the part that is seen;
/// none where nothing is drawn yet, or nothing holds what is seen. Another page's is not the page.
fn stand_in(drawing: &mut Drawing, wanted: At, shown: Rect, now: Instant) -> Option<Bitmap> {
    let area = |width: u32, height: u32| f64::from(width) * f64::from(height);
    let target = area(wanted.width, wanted.height);
    let nearest = drawing
        .drawn
        .iter_mut()
        .filter_map(|(at, drawn)| match drawn {
            Drawn::Ready(kept) if at.page == wanted.page && at.shows(shown) => Some((*at, kept)),
            _ => None,
        })
        .min_by(|(a, kept_a), (b, kept_b)| {
            let far = |at: &At| (area(at.width, at.height) / target).ln().abs();
            // The nearer size; of the same, the newer pixels.
            far(a)
                .total_cmp(&far(b))
                .then(kept_b.bitmap.id.cmp(&kept_a.bitmap.id))
        });
    nearest.map(|(_, kept)| {
        kept.asked = now;
        kept.bitmap.clone()
    })
}

/// The newest pixels of the page of `wanted`, at its size, that hold `margin` (what is seen and a
/// little more): a part that need not be drawn again.
fn newest_holding(drawing: &mut Drawing, wanted: At, margin: Part) -> Option<&mut Kept> {
    drawing
        .drawn
        .iter_mut()
        .filter_map(|(at, drawn)| match drawn {
            Drawn::Ready(kept)
                if at.page == wanted.page
                    && (at.width, at.height) == (wanted.width, wanted.height)
                    && at.part.contains(margin) =>
            {
                Some(kept)
            }
            _ => None,
        })
        .max_by_key(|kept| kept.bitmap.id)
}

/// The drawn size of a PNG or a JPEG that is at least `wanted` on both sides and the least
/// larger of those that are: what is drawn from it is not larger than its pixels.
fn covering(drawing: &mut Drawing, wanted: At) -> Option<&mut Kept> {
    drawing
        .drawn
        .iter_mut()
        .filter_map(|(at, drawn)| match drawn {
            Drawn::Ready(kept) if at.covers(wanted) => Some((*at, kept)),
            _ => None,
        })
        .min_by_key(|(at, _)| at.area())
        .map(|(_, kept)| kept)
}

/// Whether a drawing that covers `wanted` is being made.
fn covered_by_pending(drawing: &Drawing, wanted: At) -> bool {
    drawing
        .drawn
        .iter()
        .any(|(at, drawn)| matches!(drawn, Drawn::Pending) && at.covers(wanted))
}

/// How many pixels of the long side a PNG or a JPEG is kept in steps of: the size it is asked at
/// is rounded up to a multiple of this, so that a pane that is dragged larger decodes the file
/// again once for each step and not for each pixel.
pub(crate) const STEP: u32 = 256;

/// The size a PNG or a JPEG of `natural` pixels is kept at for a whole image drawn at `wanted`
/// pixels: the long side, as much as the side that asks for more needs, rounded up to a multiple of
/// [`STEP`], and not past the file's own; the proportions are the file's.
pub(crate) fn stepped(natural: (u32, u32), wanted: (u32, u32)) -> (u32, u32) {
    let (width, height) = (u64::from(natural.0.max(1)), u64::from(natural.1.max(1)));
    let long = width.max(height);
    // The long side that makes the file as large as the page draws it on the side that needs most.
    let need = (u64::from(wanted.0) * long)
        .div_ceil(width)
        .max((u64::from(wanted.1) * long).div_ceil(height));
    let step = u64::from(STEP);
    let asked = need.div_ceil(step).max(1) * step;
    if asked >= long {
        return (natural.0.max(1), natural.1.max(1));
    }
    let at =
        |side: u64| u32::try_from((side * asked + long / 2) / long).map_or(1, |side| side.max(1));
    (at(width), at(height))
}

/// The pixels that have gone unasked for the longest: when they were asked, the path they are of,
/// and the page and size they were drawn at.
type Oldest = (Instant, String, At);

/// How long a drawn size of an SVG must have gone unasked to be given up for a new one when the
/// drawing is at its most sizes: longer than the passes of one frame of a page take, which ask for
/// every size in use. And so what the host asked for this long before the last it asked for is
/// what it showed then, and what it asked for before that is what it no longer shows.
const RECENT: Duration = Duration::from_millis(100);

/// How long before the last time the host asked for pixels another ask may be and still be of the
/// same frame: a frame asks for every picture it shows in one pass, and the frame before it is
/// further back than this.
const FRAME: Duration = Duration::from_millis(30);

/// Gives up the size of `page` that nothing has asked for the longest, with all its parts, if that is
/// for [`RECENT`]; the bytes of pixels it held.
fn recycle(drawing: &mut Drawing, page: u32, now: Instant) -> usize {
    // When each size was last asked for: when its pixels that were asked for last were.
    let mut last: HashMap<(u32, u32), Instant> = HashMap::new();
    // A size with a part on its way is not given up: it would stay a size, and its room go for
    // nothing.
    let mut busy: HashSet<(u32, u32)> = HashSet::new();
    for (at, drawn) in &drawing.drawn {
        if at.page != page {
            continue;
        }
        match drawn {
            Drawn::Ready(kept) => {
                let seen = last.entry((at.width, at.height)).or_insert(kept.asked);
                *seen = (*seen).max(kept.asked);
            }
            Drawn::Pending => {
                busy.insert((at.width, at.height));
            }
            _ => {}
        }
    }
    let oldest = last
        .into_iter()
        .filter(|(size, _)| !busy.contains(size))
        .filter(|(_, asked)| now.saturating_duration_since(*asked) >= RECENT)
        .min_by_key(|(_, asked)| *asked);
    let Some(((width, height), _)) = oldest else {
        return 0;
    };
    let mut freed = 0;
    drawing.drawn.retain(|at, drawn| {
        if at.page != page || (at.width, at.height) != (width, height) {
            return true;
        }
        match drawn {
            Drawn::Pending => true,
            Drawn::Ready(kept) => {
                freed += kept.bitmap.pixels.len();
                false
            }
            _ => false,
        }
    });
    freed
}

/// The size an SVG is drawn at for a whole image of `whole` pixels: whole pixels, at least one and
/// no more than `side` on either side (the proportions kept).
fn drawn_size(whole: Size, side: u32) -> (u32, u32) {
    let side = side.max(1) as f32;
    let fit = |value: f32| {
        if value.is_finite() {
            value.round().max(1.0)
        } else {
            1.0
        }
    };
    let (w, h) = (fit(whole.w), fit(whole.h));
    let shrink = (side / w.max(h)).min(1.0);
    let at = |value: f32| ((value * shrink).round().clamp(1.0, side)) as u32;
    (at(w), at(h))
}

/// How many more pixels than this a vector image's whole is, it is drawn where it is seen and not
/// all: a part of a smaller one is no saving.
const SMALL: u64 = 512 * 512;

/// The grid, pixels, that the part of an image that is drawn is rounded out to, so that the part
/// asked for is the same for a little scrolling.
const GRID: u32 = 128;

/// How far beyond what is seen the part that is drawn reaches (before it is rounded out), pixels.
const PAY: u32 = 128;

/// How near the edge of the part that is held what is seen may come before a new part is drawn.
const MARGIN: u32 = 64;

/// The long side the whole page that stands in is drawn at, pixels.
const BASE: u32 = 512;

/// How many parts of a page at one size are kept: the one that is seen and the one before it.
const PARTS: usize = 2;

/// How many parts of a page are kept as the ones asked for last ([`Drawing::latest`]): those of
/// as many places as the page is likely shown in at once.
const WANTED: usize = 4;

/// How many sides ([`ImageLimits::side`]) the whole of a vector image may be: it is drawn in parts,
/// each at most a side, so the whole is no longer held to one.
const WHOLES: u32 = 16;

/// From how long a long side the size a vector image is drawn at goes in steps (a long side that is
/// shorter is the size asked for): a pane that is dragged larger asks for a new drawing once for each
/// step and not for each pixel.
const STEPPED_FROM: f64 = 1024.0;

/// The size all of a large SVG or page is drawn at when `shown` of it is seen and it is shown as
/// `whole`: that, no more than [`WHOLES`] sides long, smaller where what is seen would not be one
/// raster, and its long side in steps (of a thirty-second of its power of two, at most) so that
/// a pane dragged larger draws it again for each step.
fn vector_scale(whole: Size, shown: Rect, side: u32) -> (u32, u32) {
    let finite = |value: f32| {
        if value.is_finite() {
            f64::from(value).max(1.0)
        } else {
            1.0
        }
    };
    let (mut w, mut h) = (finite(whole.w), finite(whole.h));
    let most = f64::from(side) * f64::from(WHOLES);
    let shrink = (most / w.max(h)).min(1.0);
    (w, h) = (w * shrink, h * shrink);
    let share = |value: f32| f64::from(value).clamp(0.0, 1.0);
    let seen = (share(shown.w) * w).max(share(shown.h) * h);
    // A little under a side, so that the step up that follows does not pass it.
    let room = f64::from(side) * 31.0 / 32.0;
    if seen > room {
        (w, h) = (w * room / seen, h * room / seen);
    }
    let long = w.max(h);
    if long >= STEPPED_FROM {
        let step = f64::from(1_u32 << (long.log2().floor() as u32 - 5));
        let to = (long / step).ceil() * step;
        (w, h) = (w * to / long, h * to / long);
    }
    (w.round().max(1.0) as u32, h.round().max(1.0) as u32)
}

/// The pixels of an image drawn `width` by `height` that `shown` (shares of the whole) is within,
/// rounded out to whole pixels, at least one.
fn seen_part(shown: Rect, width: u32, height: u32) -> Part {
    let axis = |from: f32, len: f32, total: u32| {
        let total = f64::from(total);
        let from = f64::from(from).clamp(0.0, 1.0);
        let to = (from + f64::from(len)).clamp(0.0, 1.0);
        let start = (from * total).floor().clamp(0.0, total - 1.0);
        let end = (to * total).ceil().clamp(start + 1.0, total);
        (start as u32, end as u32)
    };
    let (x, right) = axis(shown.x, shown.w, width);
    let (y, bottom) = axis(shown.y, shown.h, height);
    Part {
        x,
        y,
        w: right - x,
        h: bottom - y,
    }
}

/// The part of an image drawn `width` by `height` that is drawn for `seen`: [`PAY`] more on each
/// side, rounded out to the [`GRID`], and no more than a side of the pixels: where that is more,
/// a part of a side that holds `seen` as far as it can.
fn wanted_part(seen: Part, width: u32, height: u32, side: u32) -> Part {
    let axis = |from: u32, len: u32, total: u32| {
        let to = from.saturating_add(len);
        let start = from.saturating_sub(PAY) / GRID * GRID;
        let end = to
            .saturating_add(PAY)
            .div_ceil(GRID)
            .saturating_mul(GRID)
            .min(total);
        if end - start <= side {
            return (start, end);
        }
        let start = (from + len / 2)
            .saturating_sub(side / 2)
            .min(total.saturating_sub(side));
        (start, start.saturating_add(side).min(total))
    };
    let (x, right) = axis(seen.x, seen.w, width);
    let (y, bottom) = axis(seen.y, seen.h, height);
    Part {
        x,
        y,
        w: right - x,
        h: bottom - y,
    }
}

/// `part` grown by `by` pixels on each side, not past an image drawn `width` by `height`.
fn grow(part: Part, by: u32, width: u32, height: u32) -> Part {
    let (x, y) = (part.x.saturating_sub(by), part.y.saturating_sub(by));
    Part {
        x,
        y,
        w: part.x.saturating_add(part.w).saturating_add(by).min(width) - x,
        h: part.y.saturating_add(part.h).saturating_add(by).min(height) - y,
    }
}

/// Where the whole of the page `page`, of the own size `natural`, is drawn small, to stand in for its
/// parts at every size: [`BASE`] pixels on its long side, in its proportions.
fn base_of(page: u32, natural: Size) -> At {
    let long = natural.w.max(natural.h).max(f32::EPSILON);
    let side = |value: f32| {
        (value.max(f32::EPSILON) / long * BASE as f32)
            .round()
            .max(1.0) as u32
    };
    At::whole(page, side(natural.w), side(natural.h))
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether the host is to be woken: something is waiting for it, it has not been woken for it
    /// already, and the images are not gone. The caller makes the wake once the lock is let go, so
    /// that a host that looks at once finds it free.
    fn announce(&self, state: &mut State) -> bool {
        let wake = !state.woken && !state.arrived.is_empty() && !self.gone.load(Ordering::SeqCst);
        state.woken |= wake;
        wake
    }

    /// Lets go of the state and wakes the host if `announce` said to.
    fn finish(&self, state: MutexGuard<'_, State>, wake: bool) {
        drop(state);
        if wake {
            (self.wake)();
        }
    }

    /// The file has no page `page`: its box stays empty, and the page is told once, how many it has.
    /// Whether the host is to be woken.
    fn no_such_page(&self, state: &mut State, path: &str, page: u32, pages: usize) -> bool {
        if !state.told_pages.insert((path.to_owned(), page)) {
            return false;
        }
        let has = match pages {
            1 => "has no page but the first".to_owned(),
            count => format!("has {count} pages"),
        };
        state.arrived.diagnostics.push(Diagnostic {
            path: path.to_owned(),
            message: format!("{has}, and no page {page}"),
        });
        self.announce(state)
    }

    /// The path is not an image the view can have: it stays empty, and the page is told once.
    fn refuse(&self, state: &mut State, path: &str, failure: &Failure) {
        state.entries.insert(path.to_owned(), Entry::Refused);
        state.arrived.diagnostics.push(Diagnostic {
            path: path.to_owned(),
            message: failure.to_string(),
        });
    }

    /// The path could not be had: for good, unless it was only that there was no room, which may
    /// not be so later.
    fn fail(&self, state: &mut State, path: &str, failure: &Failure) {
        let Failure::Budget { .. } = failure else {
            self.refuse(state, path, failure);
            return;
        };
        state.entries.insert(
            path.to_owned(),
            Entry::Waiting {
                since: Instant::now(),
            },
        );
        if state.told_room.insert(path.to_owned()) {
            state.arrived.diagnostics.push(Diagnostic {
                path: path.to_owned(),
                message: failure.to_string(),
            });
        }
    }

    /// Whether the pixels of `kept` (of the path, drawn at `at`) may be let go of now, for room: they
    /// have gone unasked for [`ImageLimits::idle`]; or, for an SVG or a page, they were not asked
    /// for at the host's last frame, that is they are not what it shows ([`RECENT`] before the
    /// last it asked for pixels), which is not waited out; or they are the pixels of the same
    /// page, at another size or of another part, than the ones `replacing` draws (not the small
    /// whole page that stands in), which is the last room there is: what stands in for a drawing
    /// is asked for at every frame, and would hold the room of what is to replace it for good.
    fn may_go(
        &self,
        state: &State,
        now: Instant,
        (path, kept, at, raster): (&str, &Kept, At, bool),
        replacing: Option<(&str, At)>,
    ) -> bool {
        let unasked = now.saturating_duration_since(kept.asked) >= self.limits.idle;
        let out_of_view = !raster
            && state
                .last_ask
                .is_some_and(|last| kept.asked < last.checked_sub(RECENT).unwrap_or(last));
        let replaced = replacing.is_some_and(|(of, new)| {
            of == path && at != new && at.page == new.page && at.width.max(at.height) > BASE
        });
        unasked || out_of_view || replaced
    }

    /// Makes room for `bytes` more: lets go of the pixels that may go ([`Shared::may_go`]), the
    /// longest unasked first, until they fit; `Budget` if they do not, and then nothing is let go.
    /// `replacing` is the part of a page that the room is for.
    fn make_room(
        &self,
        state: &mut State,
        bytes: usize,
        replacing: Option<(&str, At)>,
    ) -> Result<(), Failure> {
        let most = self.limits.pixel_bytes;
        if bytes > most {
            return Err(Failure::Whole { bytes, most });
        }
        let now = Instant::now();
        let free: usize = kept(state)
            .filter(|(path, kept, at, raster)| {
                self.may_go(state, now, (path, kept, *at, *raster), replacing)
            })
            .map(|(_, kept, ..)| kept.bitmap.pixels.len())
            .sum();
        if state.held.saturating_add(bytes) > most.saturating_add(free) {
            return Err(Failure::Budget { most });
        }
        while state.held.saturating_add(bytes) > most {
            if !self.release_one(state, replacing) {
                return Err(Failure::Budget { most });
            }
        }
        Ok(())
    }

    /// When the host is to be woken for a drawing of `bytes` that had no room at `since`, so that
    /// it asks for it again: once the pixels it no longer asks for (those it did not ask for with
    /// the last it did ask for, `shown_from` on: the pictures it does not show) have gone unasked for
    /// [`ImageLimits::idle`], the longest unasked first, as many as make room enough; and never
    /// before the drawing may be tried again. None where they would not: the room is held by what
    /// the host shows, which is had again only when it shows something else, at a frame that asks
    /// for the drawing anyway.
    fn wake_for(
        &self,
        state: &State,
        bytes: usize,
        (since, retry): (Instant, Duration),
        shown_from: Instant,
    ) -> Option<Instant> {
        let most = self.limits.pixel_bytes;
        let idle = self.limits.idle;
        let mut need = state.held.saturating_add(bytes).saturating_sub(most);
        let mut unseen: Vec<(Instant, usize, bool)> = kept(state)
            .filter(|(_, kept, ..)| kept.asked < shown_from)
            .map(|(_, kept, _, raster)| (kept.asked, kept.bitmap.pixels.len(), raster))
            .collect();
        unseen.sort_unstable_by_key(|(asked, ..)| *asked);
        let mut by = since + retry;
        for (asked, bytes, raster) in unseen {
            if need == 0 {
                break;
            }
            need = need.saturating_sub(bytes);
            // The pixels of an SVG or a page that are not shown may go at once; a PNG's, a JPEG's
            // when they have gone unasked for [`ImageLimits::idle`].
            if raster {
                by = by.max(asked + idle);
            }
        }
        (need == 0).then_some(by)
    }

    /// The drawings that wait for room, on the worker: each is planned a wake ([`Shared::wake_for`])
    /// once the host has done asking for the pixels of a frame ([`RECENT`] after the last it asked
    /// for), from what it showed then, and again after it asks for more; the host is woken for
    /// those whose time has come, each once. When to look again, if there is anything to look at.
    fn wake_due(&self) -> Option<Instant> {
        let mut state = self.lock();
        let now = Instant::now();
        let last = state.last_ask?;
        let settled = last + RECENT;
        // The host has been quiet since `last`: what it asked for in that frame (within [`FRAME`] of
        // the last ask) is what it shows, and the rest is not, though it was asked a frame before.
        let shown_from = last.checked_sub(FRAME).unwrap_or(last);
        // What waits, where it is, and whether it is planned for the last asking.
        let mut plans = Vec::new();
        let mut next: Option<Instant> = None;
        let mut sooner = |at: Instant| next = Some(next.map_or(at, |next: Instant| next.min(at)));
        for (path, entry) in &state.entries {
            let Entry::Image(drawing) = entry else {
                continue;
            };
            let retry = if drawing.raster {
                self.limits.idle
            } else {
                RECENT
            };
            for (at, drawn) in &drawing.drawn {
                if let Drawn::Waiting {
                    since,
                    asked,
                    wake,
                    planned,
                } = drawn
                {
                    if *planned != Some(last) {
                        if now < settled {
                            sooner(settled);
                        } else {
                            // Not shown at the last frame: not worth a wake.
                            let shown = *asked >= shown_from;
                            let bytes = at.bytes();
                            let wake = shown
                                .then(|| self.wake_for(&state, bytes, (*since, retry), shown_from))
                                .flatten();
                            plans.push((path.clone(), *at, wake));
                            if let Some(wake) = wake {
                                sooner(wake);
                            }
                        }
                    } else if let Some(wake) = wake {
                        sooner(*wake);
                    }
                }
            }
        }
        for (path, at, planned_wake) in plans {
            if let Some(Entry::Image(drawing)) = state.entries.get_mut(&path)
                && let Some(Drawn::Waiting { wake, planned, .. }) = drawing.drawn.get_mut(&at)
            {
                *wake = planned_wake;
                *planned = Some(last);
            }
        }
        let mut due = false;
        for entry in state.entries.values_mut() {
            let Entry::Image(drawing) = entry else {
                continue;
            };
            for drawn in drawing.drawn.values_mut() {
                if let Drawn::Waiting { wake, .. } = drawn
                    && wake.is_some_and(|at| at <= now)
                {
                    *wake = None;
                    due = true;
                }
            }
        }
        if due {
            state.arrived.redraw = true;
            let wake = self.announce(&mut state);
            self.finish(state, wake);
            return self.wake_due();
        }
        next
    }

    /// Lets go of the pixels that have been unasked for the longest, of those that may go
    /// ([`Shared::may_go`]); whether it did.
    fn release_one(&self, state: &mut State, replacing: Option<(&str, At)>) -> bool {
        let now = Instant::now();
        let mut oldest: Option<Oldest> = None;
        for (path, kept, at, raster) in kept(state) {
            if self.may_go(state, now, (path, kept, at, raster), replacing)
                && oldest
                    .as_ref()
                    .is_none_or(|(before, ..)| kept.asked < *before)
            {
                oldest = Some((kept.asked, path.clone(), at));
            }
        }
        let Some((_, path, at)) = oldest else {
            return false;
        };
        let freed = match state.entries.get_mut(&path) {
            Some(Entry::Image(drawing)) => match drawing.drawn.remove(&at) {
                Some(Drawn::Ready(kept)) => kept.bitmap.pixels.len(),
                _ => 0,
            },
            _ => 0,
        };
        state.held = state.held.saturating_sub(freed);
        true
    }

    /// The side and the memory approve an image of this size from its header: its pixels, which
    /// are made whole before they are shrunk, within what the view may hold at all, and what the
    /// decoder works in within what one image may be worked in. The room for what is kept is made
    /// when it is drawn, at the size it is kept at.
    fn approves(&self, width: u32, height: u32, working: usize) -> Result<(), Failure> {
        let side = self.limits.side;
        if width > side || height > side {
            return Err(Failure::TooLarge {
                width,
                height,
                side,
            });
        }
        if working > self.limits.work_bytes {
            return Err(Failure::Work {
                bytes: working,
                most: self.limits.work_bytes,
            });
        }
        let bytes = decode::pixel_bytes(width, height).unwrap_or(usize::MAX);
        let most = self.limits.pixel_bytes;
        if bytes > most {
            return Err(Failure::Whole { bytes, most });
        }
        Ok(())
    }

    /// The bitmap of the pixels drawn at `at`, held from now on.
    fn bitmap(&self, state: &mut State, raster: Raster, at: At) -> Bitmap {
        state.next += 1;
        state.held = state.held.saturating_add(raster.pixels.len());
        Bitmap {
            id: state.next,
            width: raster.width,
            height: raster.height,
            part: at.share(),
            pixels: Arc::new(raster.pixels),
        }
    }
}

/// The worker: reads and decodes what it is asked, one at a time, until the images are gone; and
/// between, wakes the host when a drawing that had no room may have it ([`Shared::wake_due`]).
fn serve(shared: &Arc<Shared>, inbox: &Receiver<Job>) {
    // The parsed SVGs and PDFs, which only this thread draws.
    let mut sources: HashMap<String, Source> = HashMap::new();
    loop {
        let job = match shared.wake_due() {
            None => inbox.recv().ok(),
            Some(at) => match inbox.recv_timeout(at.saturating_duration_since(Instant::now())) {
                Ok(job) => Some(job),
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => None,
            },
        };
        let Some(job) = job else {
            return;
        };
        if shared.gone.load(Ordering::SeqCst) {
            return;
        }
        let (path, at) = match &job {
            Job::Look => continue,
            Job::Decode(path) => (path.clone(), None),
            Job::Draw(path, at) => (path.clone(), Some(*at)),
            Job::Forget(path) => {
                if let Some(source) = sources.remove(path) {
                    let mut state = shared.lock();
                    state.held = state.held.saturating_sub(source.bytes());
                }
                continue;
            }
        };
        // A decoder that gives up by panicking costs the image and not the worker.
        let ran = catch_unwind(AssertUnwindSafe(|| match job {
            Job::Decode(path) => decode_path(shared, &mut sources, &path),
            Job::Draw(path, at) => draw(shared, &sources, &path, at),
            Job::Forget(_) | Job::Look => {}
        }));
        if ran.is_err() {
            let mut state = shared.lock();
            match at {
                // A file that panicked when decoded is refused.
                None => {
                    if matches!(state.entries.get(&path), Some(Entry::Pending)) {
                        let gave_up = Failure::Broken("the decoder gave up".into());
                        shared.refuse(&mut state, &path, &gave_up);
                    }
                }
                // An image that panicked when drawn stays what it was, and is not drawn at that
                // size again; the page is told.
                Some(at) => {
                    if let Some(Entry::Image(drawing)) = state.entries.get_mut(&path)
                        && matches!(drawing.drawn.get(&at), Some(Drawn::Pending))
                    {
                        let who = if drawing.raster {
                            "decoder"
                        } else {
                            "renderer"
                        };
                        let place = place(drawing, at);
                        drawing.drawn.insert(at, Drawn::Refused);
                        state.arrived.diagnostics.push(Diagnostic {
                            path: path.clone(),
                            message: format!("could not be drawn: the {who} gave up ({place})"),
                        });
                    }
                }
            }
            let wake = shared.announce(&mut state);
            shared.finish(state, wake);
        }
    }
}

/// What the worker keeps of a file it draws from what it parsed of it: an SVG or a PDF.
enum Source {
    Vector(Box<Vector>),
    Document(Box<Document>),
}

impl Source {
    /// What the parsed file takes in memory, which is paid for from the view's room.
    fn bytes(&self) -> usize {
        match self {
            Self::Vector(vector) => vector.weight().bytes,
            Self::Document(document) => document.bytes(),
        }
    }
}

/// Where a drawing was made, as the page is told it: the size, and the page where the file has
/// pages.
fn place(drawing: &Drawing, at: At) -> String {
    let mut size = format!("drawn at {}\u{d7}{}", at.width, at.height);
    if !at.is_whole() {
        size = format!("{size}, a part of {}\u{d7}{}", at.part.w, at.part.h);
    }
    if drawing.pages.is_empty() {
        size
    } else {
        format!("page {}, {size}", at.page)
    }
}

/// Reads the file at `path`: an SVG or a PDF is parsed, a PNG or a JPEG has its size read from the
/// header (its pixels are decoded when they are drawn, at the size they are asked at).
fn decode_path(shared: &Arc<Shared>, sources: &mut HashMap<String, Source>, path: &str) {
    let outcome = read_and_open(shared, path);
    let mut state = shared.lock();
    match outcome {
        Ok(Opened::Raster(width, height)) => {
            state.entries.insert(
                path.to_owned(),
                Entry::Image(Drawing {
                    size: Size::new(width as f32, height as f32),
                    raster: true,
                    pages: Vec::new(),
                    drawn: HashMap::new(),
                    latest: HashMap::new(),
                }),
            );
            state.arrived.relayout = true;
        }
        Ok(Opened::Vector(vector)) => {
            // The parsed drawing is held for as long as the view is, and is paid for.
            let kept = vector.weight().bytes;
            match shared.make_room(&mut state, kept, None) {
                Ok(()) => {
                    state.held = state.held.saturating_add(kept);
                    let (w, h) = vector.natural();
                    sources.insert(path.to_owned(), Source::Vector(vector));
                    state.entries.insert(
                        path.to_owned(),
                        Entry::Image(Drawing {
                            size: Size::new(w, h),
                            raster: false,
                            pages: Vec::new(),
                            drawn: HashMap::new(),
                            latest: HashMap::new(),
                        }),
                    );
                    state.arrived.relayout = true;
                }
                Err(failure) => shared.fail(&mut state, path, &failure),
            }
        }
        Ok(Opened::Document(document)) => {
            // The parsed document is held for as long as the view is, and is paid for, as an
            // SVG's drawing is.
            let kept = document.bytes();
            match shared.make_room(&mut state, kept, None) {
                Ok(()) => {
                    state.held = state.held.saturating_add(kept);
                    let pages = document.sizes().to_vec();
                    let size = pages.first().copied().unwrap_or(Size::new(1.0, 1.0));
                    sources.insert(path.to_owned(), Source::Document(document));
                    state.entries.insert(
                        path.to_owned(),
                        Entry::Image(Drawing {
                            size,
                            raster: false,
                            pages,
                            drawn: HashMap::new(),
                            latest: HashMap::new(),
                        }),
                    );
                    state.arrived.relayout = true;
                }
                Err(failure) => shared.fail(&mut state, path, &failure),
            }
        }
        Err(failure) => shared.fail(&mut state, path, &failure),
    }
    let wake = shared.announce(&mut state);
    shared.finish(state, wake);
}

enum Opened {
    /// A PNG or a JPEG, with its size.
    Raster(u32, u32),
    Vector(Box<Vector>),
    Document(Box<Document>),
}

/// The bytes of the file at `path`, within what a file may be, and their format.
fn read_file(shared: &Shared, path: &str) -> Result<(Arc<[u8]>, Format), Failure> {
    let bytes = shared.assets.read(path).ok_or(Failure::Missing)?;
    if bytes.len() > shared.limits.file_bytes {
        return Err(Failure::Source {
            bytes: bytes.len(),
            most: shared.limits.file_bytes,
        });
    }
    let format = decode::sniff(&bytes).ok_or(Failure::Unknown)?;
    Ok((bytes, format))
}

fn read_and_open(shared: &Shared, path: &str) -> Result<Opened, Failure> {
    let (bytes, format) = read_file(shared, path)?;
    match format {
        Format::Svg => Vector::parse(&bytes).map(|vector| Opened::Vector(Box::new(vector))),
        Format::Pdf => Document::parse(&bytes).map(|document| Opened::Document(Box::new(document))),
        format => {
            let header = decode::header(&bytes, format)?;
            shared.approves(header.width, header.height, header.working)?;
            Ok(Opened::Raster(header.width, header.height))
        }
    }
}

/// The PNG or JPEG at `path`, decoded at its own size and shrunk to `width` by `height` (which is
/// not past it). The whole image is made on this thread and let go of when this returns.
fn read_and_shrink(
    shared: &Shared,
    path: &str,
    width: u32,
    height: u32,
) -> Result<Raster, Failure> {
    let (bytes, format) = read_file(shared, path)?;
    let whole = decode::raster(&bytes, format, &mut |w, h, working| {
        shared.approves(w, h, working)
    })?;
    Ok(decode::shrink(whole, width, height))
}

/// Draws the image at `path` at `at`, if that is still wanted of it.
fn draw(shared: &Arc<Shared>, sources: &HashMap<String, Source>, path: &str, at: At) {
    let (width, height) = (at.width, at.height);
    let raster = {
        let mut state = shared.lock();
        let Some(Entry::Image(drawing)) = state.entries.get_mut(path) else {
            return;
        };
        if !matches!(drawing.drawn.get(&at), Some(Drawn::Pending)) {
            return;
        }
        // A larger one came first: this is no longer wanted of a file.
        if drawing.raster && covering(drawing, at).is_some() {
            drawing.drawn.remove(&at);
            return;
        }
        // A part of a page that is not the one that is wanted now (the page was scrolled on while
        // this waited its turn) is not drawn: there is no use for it, and the worker is for the
        // one there is.
        if !at.is_whole()
            && !drawing
                .latest
                .get(&at.page)
                .is_some_and(|wants| wants.contains(&at))
        {
            drawing.drawn.remove(&at);
            return;
        }
        drawing.raster
    };
    // Whether the page had pixels before this drawing, which stand in while it waits.
    let had_pixels = !raster && has_pixels(shared, path, at.page);
    let outcome = if raster {
        // The room for what is kept is made before the file is decoded, at the size it is kept at.
        decode::pixel_bytes(width, height)
            .ok_or(Failure::Budget {
                most: shared.limits.pixel_bytes,
            })
            .and_then(|bytes| shared.make_room(&mut shared.lock(), bytes, None))
            .and_then(|()| read_and_shrink(shared, path, width, height))
    } else {
        let Some(source) = sources.get(path) else {
            return;
        };
        let outcome = draw_vector(shared, path, source, at);
        // No room at the size asked: where the picture has no pixels at any size, it is drawn
        // smaller, so that its box is not empty while it waits. (A page that is drawn in parts has
        // the whole of it small to stand in, which is drawn first.)
        let smaller = match &outcome {
            Err(Failure::Budget { .. } | Failure::Whole { .. }) if at.is_whole() && !had_pixels => {
                draw_smaller(shared, path, source, at)
            }
            _ => None,
        };
        if let Some((smaller, raster)) = smaller {
            let mut state = shared.lock();
            let now = Instant::now();
            if let Some(Entry::Image(drawing)) = state.entries.get(path)
                && !drawing.drawn.contains_key(&smaller)
            {
                let bitmap = shared.bitmap(&mut state, raster, smaller);
                state.arrived.redraw = true;
                if let Some(Entry::Image(drawing)) = state.entries.get_mut(path) {
                    drawing
                        .drawn
                        .insert(smaller, Drawn::Ready(Kept { bitmap, asked: now }));
                }
            }
        }
        outcome
    };
    let mut state = shared.lock();
    let now = Instant::now();
    let mut told = None;
    let drawn = match outcome {
        Ok(raster) => {
            let bitmap = shared.bitmap(&mut state, raster, at);
            state.arrived.redraw = true;
            Drawn::Ready(Kept { bitmap, asked: now })
        }
        // No room, and perhaps later: the host is woken when there may be. Told, once for a size,
        // unless the page has pixels at another size or of another part that stand in: the room
        // that comes when the pixels of what was scrolled away from go is not the author's to know.
        Err(failure @ Failure::Budget { .. }) => {
            if !had_pixels
                && state
                    .told_room
                    .insert(format!("{path}@{}x{width}x{height}", at.page))
            {
                told = Some(failure);
            }
            Drawn::Waiting {
                since: now,
                asked: now,
                wake: None,
                planned: None,
            }
        }
        // The file is not what its header said, or is gone: the image is refused for good, and
        // its box is empty as one that cannot be read is.
        Err(failure) if raster => {
            let freed = pixels_held(&mut state, path);
            state.held = state.held.saturating_sub(freed);
            shared.refuse(&mut state, path, &failure);
            state.arrived.relayout = true;
            let wake = shared.announce(&mut state);
            shared.finish(state, wake);
            return;
        }
        // For good at this size: the shapes it was drawn as before stay what there is.
        Err(failure) => {
            told = Some(failure);
            Drawn::Refused
        }
    };
    let mut freed = 0;
    if let Some(Entry::Image(drawing)) = state.entries.get_mut(path) {
        // What a larger drawing covers is not asked for again: it is the one that is given.
        if raster && matches!(drawn, Drawn::Ready(_)) {
            freed = give_up_covered(drawing, at);
        }
        let ready = matches!(drawn, Drawn::Ready(_));
        drawing.drawn.insert(at, drawn);
        // Of the parts of a page at a size, the one drawn now and the one before it are kept.
        if !raster && ready && !at.is_whole() {
            freed += give_up_parts(drawing, at);
        }
    } else if let Drawn::Ready(kept) = drawn {
        freed += kept.bitmap.pixels.len();
    }
    state.held = state.held.saturating_sub(freed);
    if let Some(failure) = told {
        let place = match state.entries.get(path) {
            Some(Entry::Image(drawing)) => place(drawing, at),
            _ => format!("drawn at {width}\u{d7}{height}"),
        };
        state.arrived.diagnostics.push(Diagnostic {
            path: path.to_owned(),
            message: format!("{failure} ({place})"),
        });
    }
    let wake = shared.announce(&mut state);
    shared.finish(state, wake);
}

/// The SVG or the page of a PDF in `source` drawn at `at`: the pixels it makes, and the layers it is
/// drawn on while it makes them, approved before the first is.
fn draw_vector(shared: &Shared, path: &str, source: &Source, at: At) -> Result<Raster, Failure> {
    let (width, height, part) = (at.width, at.height, at.part);
    let bytes = decode::pixel_bytes(part.w, part.h).ok_or(Failure::Budget {
        most: shared.limits.pixel_bytes,
    })?;
    let working = match source {
        Source::Vector(vector) => vector.working(part.w, part.h),
        Source::Document(document) => document.working(part.w, part.h),
    };
    if working > shared.limits.work_bytes {
        return Err(Failure::Work {
            bytes: working,
            most: shared.limits.work_bytes,
        });
    }
    // What is drawn may take the room of the pixels that stand in for it, if there is no other: the
    // whole page at a small size stands in for them (and is not drawn this way).
    let replacing = (width.max(height) > BASE).then_some((path, at));
    shared.make_room(&mut shared.lock(), bytes, replacing)?;
    match source {
        Source::Vector(vector) => vector.draw_part(width, height, part),
        Source::Document(document) => document.draw_part(
            usize::try_from(at.page)
                .unwrap_or(usize::MAX)
                .saturating_sub(1),
            width,
            height,
            part,
        ),
    }
}

/// The long side under which a picture is not drawn smaller for want of room: a box that small is
/// not worth the room either.
const SMALLEST: u32 = 16;

/// The SVG or the page of a PDF in `source` drawn at a half of `at`, a quarter, …, the first that
/// there is room for, to stand in until `at` is: where it is drawn, and its pixels. None where
/// nothing down to [`SMALLEST`] fits, or it cannot be drawn.
fn draw_smaller(shared: &Shared, path: &str, source: &Source, at: At) -> Option<(At, Raster)> {
    let mut smaller = at;
    loop {
        smaller = At::whole(
            smaller.page,
            smaller.width.div_ceil(2),
            smaller.height.div_ceil(2),
        );
        if smaller.width.max(smaller.height) < SMALLEST {
            return None;
        }
        match draw_vector(shared, path, source, smaller) {
            Ok(raster) => return Some((smaller, raster)),
            Err(Failure::Budget { .. } | Failure::Whole { .. }) => {}
            Err(_) => return None,
        }
    }
}

/// Whether the page `page` of the image at `path` has pixels at some size, to stand in for another.
fn has_pixels(shared: &Shared, path: &str, page: u32) -> bool {
    match shared.lock().entries.get(path) {
        Some(Entry::Image(drawing)) => drawing
            .drawn
            .iter()
            .any(|(at, drawn)| at.page == page && matches!(drawn, Drawn::Ready(_))),
        _ => false,
    }
}

/// The bytes of pixels the image at `path` holds, which are let go of when the image is replaced by
/// a refusal (nothing is left of it but the diagnostic).
fn pixels_held(state: &mut State, path: &str) -> usize {
    let Some(Entry::Image(drawing)) = state.entries.get(path) else {
        return 0;
    };
    drawing
        .drawn
        .values()
        .map(|drawn| match drawn {
            Drawn::Ready(kept) => kept.bitmap.pixels.len(),
            _ => 0,
        })
        .sum()
}

/// The pixels held, each with the path, the page and the size and part it is drawn at, and whether
/// it is a PNG's or a JPEG's.
fn kept(state: &State) -> impl Iterator<Item = (&String, &Kept, At, bool)> {
    state.entries.iter().flat_map(|(path, entry)| {
        let drawing = match entry {
            Entry::Image(drawing) => Some(drawing),
            _ => None,
        };
        drawing.into_iter().flat_map(move |drawing| {
            drawing
                .drawn
                .iter()
                .filter_map(move |(at, drawn)| match drawn {
                    Drawn::Ready(kept) => Some((path, kept, *at, drawing.raster)),
                    _ => None,
                })
        })
    })
}

/// Lets go of the oldest parts of the page of `at`, at its size, past the [`PARTS`] that are kept
/// (`at` is one of them); the bytes of the pixels they held.
fn give_up_parts(drawing: &mut Drawing, at: At) -> usize {
    let mut parts: Vec<(u64, At)> = drawing
        .drawn
        .iter()
        .filter_map(|(of, drawn)| match drawn {
            Drawn::Ready(kept)
                if (of.page, of.width, of.height) == (at.page, at.width, at.height)
                    && !of.is_whole() =>
            {
                Some((kept.bitmap.id, *of))
            }
            _ => None,
        })
        .collect();
    parts.sort_unstable_by_key(|(id, _)| std::cmp::Reverse(*id));
    let mut freed = 0;
    for (_, old) in parts.into_iter().skip(PARTS) {
        if let Some(Drawn::Ready(kept)) = drawing.drawn.remove(&old) {
            freed += kept.bitmap.pixels.len();
        }
    }
    freed
}

/// Lets go of the sizes of a PNG or a JPEG that the one drawn at `at` covers, being no larger on
/// either side; the bytes of the pixels they held. The host keeps the texture it made of them by
/// their `id` for as long as it likes.
fn give_up_covered(drawing: &mut Drawing, at: At) -> usize {
    let mut freed = 0;
    drawing.drawn.retain(|size, drawn| match drawn {
        Drawn::Ready(kept) if size.page == at.page && at.covers(*size) && *size != at => {
            freed += kept.bitmap.pixels.len();
            false
        }
        _ => true,
    });
    freed
}
