//! The images of a page: files of the bundle the view came in, decoded off the thread that draws,
//! once each, and handed to whoever draws them.
//!
//! A page names an image by the path of a file of its bundle ([`pencere_view::image`]); the host
//! gives the paths' bytes through [`Assets`], and [`Images`] turns them into what the host needs
//! of an image — its size, for the layout ([`Images::size`], which the host's
//! [`pencere_view::Measure::image`] answers from), and its pixels, for the draw list's
//! [`pencere_view::Picture`] ([`Images::bitmap`]). The formats are PNG, JPEG and SVG, and the
//! limits are [`ImageLimits`], the same in every terminal that uses this crate: a view that works
//! in one works in all.
//!
//! **Nothing here waits.** The first time a path is asked for, a worker thread of its own reads
//! the file (an SVG it parses, a PNG or a JPEG it reads the header of), and the answer is *not
//! yet* until it is done; then the host is woken — once for everything that arrived since it
//! last looked — and told what [`Images::poll`] says: that a size is known and the page is to be
//! laid out again, that pixels arrived and it is to be drawn, and why an image was refused (the
//! page's diagnostics). Each path is read once for its size (the result, a refusal as well, is
//! kept for the life of the [`Images`]), and drawn once for each size it is asked at (a few
//! sizes at most, each its own raster): a host at rest is asked for nothing, and so wakes for
//! nothing.
//!
//! **What the view holds is bounded, and what it holds is what is drawn.** An SVG is drawn at the
//! size it is asked at, and so is a PNG or a JPEG: the file is decoded at its own size on the
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
//! later), the oldest first; a drawing that still does not fit is not refused for good, it waits,
//! and is tried again when it is asked for after that time, which is when something may have been
//! let go.

use std::collections::{HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use pencere_view::Size;

use crate::decode::{self, Failure, Format, Raster, Vector};

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
    /// A view of `lua` `2.1` or `wasm` `3.0` also names files beside its bundle: `sha256:` and a
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
    /// Different sizes an image is drawn at, each its own raster; a size past them is shown as
    /// the nearest of the others.
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
    pixels: Arc<Vec<u8>>,
}

impl Bitmap {
    pub(crate) fn new(id: u64, width: u32, height: u32, pixels: Vec<u8>) -> Self {
        Self {
            id,
            width,
            height,
            pixels: Arc::new(pixels),
        }
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
    /// Read the file at the path: an SVG is parsed, a PNG or a JPEG has its header read.
    Decode(String),
    /// Draw the image at the path at this size: an SVG is drawn, a PNG or a JPEG decoded and shrunk.
    Draw(String, (u32, u32)),
    /// Let go of the parsed SVG at the path.
    Forget(String),
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

/// An image: its size, and the pixels it was drawn into at each size it is asked at.
struct Drawing {
    /// Its own size: an SVG's, or the file's of a PNG or a JPEG (as it is shown, turned upright).
    size: Size,
    /// A PNG or a JPEG, decoded again from its file for each size, never larger than the file;
    /// not an SVG, which is drawn at any.
    raster: bool,
    drawn: HashMap<(u32, u32), Drawn>,
}

/// One size of an image.
enum Drawn {
    Pending,
    Ready(Kept),
    Waiting { since: Instant },
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
    /// Told that the view names more paths than it may.
    told_paths: bool,
    /// What was told to be out of room (a path, or a path and a size): once each.
    told_room: HashSet<String>,
    /// Told that an SVG is asked at more sizes than it is drawn at.
    told_sizes: HashSet<String>,
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
    /// of a PNG or a JPEG, which is not the size of the pixels held: see [`Images::bitmap`]), if it
    /// is known. The first time a path is asked for it is not: the file starts to be read (a PNG
    /// or a JPEG only for its header), and the host is woken when it is (see [`Images::poll`]). A host's
    /// [`pencere_view::Measure::image`] is this.
    pub fn size(&self, src: &str) -> Option<Size> {
        let mut state = self.lock();
        let mut wake = false;
        let size = match state.entries.get(src) {
            Some(Entry::Image(drawing)) => Some(drawing.size),
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

    /// The pixels of the image at `src`, to be drawn so that the whole image is `whole` pixels
    /// ([`pencere_view::Picture::full`]). An image is drawn at the size, once for each size it is
    /// asked at (a few at most: a size past them is the nearest of the others), and until the
    /// drawing at a new size is done the nearest there is of another is what there is (none the
    /// first time). A PNG or a JPEG is held at the size it is drawn at, rounded up to a step of
    /// [`STEP`] pixels of its long side and no larger than the file: a larger size than the ones
    /// held is decoded again from the file, and a smaller one is a held one that is larger (the
    /// host draws the pixels it is given at the size the page gave them). The host keeps the
    /// texture of a bitmap by its [`Bitmap::id`].
    pub fn bitmap(&self, src: &str, whole: Size) -> Option<Bitmap> {
        let mut state = self.lock();
        let now = Instant::now();
        let wanted = drawn_size(whole, self.shared.limits.side);
        let mut wake = false;
        let found = match state.entries.get_mut(src) {
            Some(Entry::Image(_)) => {
                let (found, job, told) = self.image_at(&mut state, src, wanted, now);
                if let Some(job) = job {
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

    /// What an image has at `wanted`: its pixels if they are drawn, else the nearest it has; a
    /// drawing to be started if it is not drawn or being, and whether something is to be told. A
    /// PNG or a JPEG is asked at its step ([`stepped`]) and a held size that covers it is it.
    fn image_at(
        &self,
        state: &mut State,
        src: &str,
        wanted: (u32, u32),
        now: Instant,
    ) -> (Option<Bitmap>, Option<Job>, bool) {
        let limits = &self.shared.limits;
        let Some(Entry::Image(drawing)) = state.entries.get_mut(src) else {
            return (None, None, false);
        };
        let wanted = if drawing.raster {
            let natural = (drawing.size.w as u32, drawing.size.h as u32);
            let wanted = stepped(natural, wanted);
            if let Some(kept) = covering(drawing, wanted) {
                kept.asked = now;
                return (Some(kept.bitmap.clone()), None, false);
            }
            // One that covers it is on its way: no other is started for it.
            if covered_by_pending(drawing, wanted) {
                return (stand_in(drawing, wanted, now), None, false);
            }
            wanted
        } else {
            wanted
        };
        let mut job = None;
        let mut told = false;
        let mut freed = 0;
        match drawing.drawn.get_mut(&wanted) {
            Some(Drawn::Ready(kept)) => {
                kept.asked = now;
                return (Some(kept.bitmap.clone()), None, false);
            }
            Some(Drawn::Pending | Drawn::Refused) => {}
            Some(Drawn::Waiting { since }) => {
                if since.elapsed() >= limits.idle {
                    drawing.drawn.insert(wanted, Drawn::Pending);
                    job = Some(Job::Draw(src.to_owned(), wanted));
                }
            }
            None => {
                // At the most sizes a drawing is drawn at, a size nothing has asked for lately
                // (a pane that was dragged larger asked one at every frame) gives its place up.
                if drawing.drawn.len() >= limits.sizes {
                    freed = recycle(drawing, now);
                }
                if drawing.drawn.len() < limits.sizes {
                    drawing.drawn.insert(wanted, Drawn::Pending);
                    job = Some(Job::Draw(src.to_owned(), wanted));
                } else if state.told_sizes.insert(src.to_owned()) {
                    // One more size than a drawing is drawn at, and every one is in use: no job,
                    // so a page that asks for sizes without end asks for nothing without end.
                    state.arrived.diagnostics.push(Diagnostic {
                        path: src.to_owned(),
                        message: format!(
                            "is drawn at more than {} sizes at once; the nearest of them is used",
                            limits.sizes
                        ),
                    });
                    told = true;
                }
            }
        }
        state.held = state.held.saturating_sub(freed);
        // The nearest drawn size stands in until this one is.
        let Some(Entry::Image(drawing)) = state.entries.get_mut(src) else {
            return (None, job, told);
        };
        (stand_in(drawing, wanted, now), job, told)
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
                // Only an SVG has a parsed drawing for the worker to let go of.
                if !drawing.raster {
                    drawings.push(src.clone());
                }
            }
            state
                .told_room
                .retain(|told| !told.starts_with(src.as_str()));
            state.told_sizes.remove(src.as_str());
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

/// The nearest of the sizes an image is drawn at to `wanted`, in the proportion of their areas,
/// to stand in until it is: none where nothing is drawn yet.
fn stand_in(drawing: &mut Drawing, wanted: (u32, u32), now: Instant) -> Option<Bitmap> {
    let area = |width: u32, height: u32| f64::from(width) * f64::from(height);
    let target = area(wanted.0, wanted.1);
    let nearest = drawing
        .drawn
        .values_mut()
        .filter_map(|drawn| match drawn {
            Drawn::Ready(kept) => Some(kept),
            _ => None,
        })
        .min_by(|a, b| {
            let far = |kept: &Kept| {
                (area(kept.bitmap.width, kept.bitmap.height) / target)
                    .ln()
                    .abs()
            };
            far(a).total_cmp(&far(b))
        });
    nearest.map(|kept| {
        kept.asked = now;
        kept.bitmap.clone()
    })
}

/// The drawn size of a PNG or a JPEG that is at least `wanted` on both sides and the least
/// larger of those that are: what is drawn from it is not larger than its pixels.
fn covering(drawing: &mut Drawing, wanted: (u32, u32)) -> Option<&mut Kept> {
    drawing
        .drawn
        .iter_mut()
        .filter_map(|(at, drawn)| match drawn {
            Drawn::Ready(kept) if at.0 >= wanted.0 && at.1 >= wanted.1 => Some((at, kept)),
            _ => None,
        })
        .min_by_key(|(at, _)| u64::from(at.0) * u64::from(at.1))
        .map(|(_, kept)| kept)
}

/// Whether a drawing that covers `wanted` is being made.
fn covered_by_pending(drawing: &Drawing, wanted: (u32, u32)) -> bool {
    drawing
        .drawn
        .iter()
        .any(|(at, drawn)| matches!(drawn, Drawn::Pending) && at.0 >= wanted.0 && at.1 >= wanted.1)
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
/// and the size they were drawn at.
type Oldest = (Instant, String, (u32, u32));

/// How long a drawn size of an SVG must have gone unasked to be given up for a new one when the
/// drawing is at its most sizes: longer than the passes of one frame of a page take, which ask for
/// every size in use.
const RECENT: Duration = Duration::from_millis(100);

/// Gives up the drawn size that nothing has asked for the longest, if that is for [`RECENT`];
/// the bytes of pixels it held.
fn recycle(drawing: &mut Drawing, now: Instant) -> usize {
    let oldest = drawing
        .drawn
        .iter()
        .filter_map(|(at, drawn)| match drawn {
            Drawn::Ready(kept) if now.saturating_duration_since(kept.asked) >= RECENT => {
                Some((kept.asked, *at))
            }
            _ => None,
        })
        .min_by_key(|(asked, _)| *asked);
    let Some((_, at)) = oldest else {
        return 0;
    };
    match drawing.drawn.remove(&at) {
        Some(Drawn::Ready(kept)) => kept.bitmap.pixels.len(),
        _ => 0,
    }
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

    /// Makes room for `bytes` more: lets go of the pixels of the drawings nothing has asked for in
    /// [`ImageLimits::idle`], the longest unasked first, until they fit; `Budget` if they do not.
    fn make_room(&self, state: &mut State, bytes: usize) -> Result<(), Failure> {
        let most = self.limits.pixel_bytes;
        if bytes > most {
            return Err(Failure::Whole { bytes, most });
        }
        while state.held.saturating_add(bytes) > most {
            if !self.release_one(state) {
                return Err(Failure::Budget { most });
            }
        }
        Ok(())
    }

    /// Lets go of the pixels that have been unasked for the longest, if any have been for the time
    /// they are kept; whether it did.
    fn release_one(&self, state: &mut State) -> bool {
        let now = Instant::now();
        let idle = self.limits.idle;
        let mut oldest: Option<Oldest> = None;
        let mut consider = |asked: Instant, path: &String, at: (u32, u32)| {
            if now.saturating_duration_since(asked) >= idle
                && oldest.as_ref().is_none_or(|(before, ..)| asked < *before)
            {
                oldest = Some((asked, path.clone(), at));
            }
        };
        for (path, entry) in &state.entries {
            if let Entry::Image(drawing) = entry {
                for (at, drawn) in &drawing.drawn {
                    if let Drawn::Ready(kept) = drawn {
                        consider(kept.asked, path, *at);
                    }
                }
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

    fn bitmap(&self, state: &mut State, raster: Raster) -> Bitmap {
        state.next += 1;
        state.held = state.held.saturating_add(raster.pixels.len());
        Bitmap {
            id: state.next,
            width: raster.width,
            height: raster.height,
            pixels: Arc::new(raster.pixels),
        }
    }
}

/// The worker: reads and decodes what it is asked, one at a time, until the images are gone.
fn serve(shared: &Arc<Shared>, inbox: &Receiver<Job>) {
    // The parsed SVGs, which only this thread draws.
    let mut vectors: HashMap<String, Vector> = HashMap::new();
    while let Ok(job) = inbox.recv() {
        if shared.gone.load(Ordering::SeqCst) {
            return;
        }
        let (path, at) = match &job {
            Job::Decode(path) => (path.clone(), None),
            Job::Draw(path, at) => (path.clone(), Some(*at)),
            Job::Forget(path) => {
                if let Some(vector) = vectors.remove(path) {
                    let mut state = shared.lock();
                    state.held = state.held.saturating_sub(vector.weight().bytes);
                }
                continue;
            }
        };
        // A decoder that gives up by panicking costs the image and not the worker.
        let ran = catch_unwind(AssertUnwindSafe(|| match job {
            Job::Decode(path) => decode_path(shared, &mut vectors, &path),
            Job::Draw(path, at) => draw(shared, &vectors, &path, at),
            Job::Forget(_) => {}
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
                Some((width, height)) => {
                    if let Some(Entry::Image(drawing)) = state.entries.get_mut(&path)
                        && matches!(drawing.drawn.get(&(width, height)), Some(Drawn::Pending))
                    {
                        let who = if drawing.raster {
                            "decoder"
                        } else {
                            "renderer"
                        };
                        drawing.drawn.insert((width, height), Drawn::Refused);
                        state.arrived.diagnostics.push(Diagnostic {
                            path: path.clone(),
                            message: format!(
                                "could not be drawn: the {who} gave up (drawn at {width}\u{d7}{height})"
                            ),
                        });
                    }
                }
            }
            let wake = shared.announce(&mut state);
            shared.finish(state, wake);
        }
    }
}

/// Reads the file at `path`: an SVG is parsed, a PNG or a JPEG has its size read from the header
/// (its pixels are decoded when they are drawn, at the size they are asked at).
fn decode_path(shared: &Arc<Shared>, vectors: &mut HashMap<String, Vector>, path: &str) {
    let outcome = read_and_open(shared, path);
    let mut state = shared.lock();
    match outcome {
        Ok(Opened::Raster(width, height)) => {
            state.entries.insert(
                path.to_owned(),
                Entry::Image(Drawing {
                    size: Size::new(width as f32, height as f32),
                    raster: true,
                    drawn: HashMap::new(),
                }),
            );
            state.arrived.relayout = true;
        }
        Ok(Opened::Vector(vector)) => {
            // The parsed drawing is held for as long as the view is, and is paid for.
            let kept = vector.weight().bytes;
            match shared.make_room(&mut state, kept) {
                Ok(()) => {
                    state.held = state.held.saturating_add(kept);
                    let (w, h) = vector.natural();
                    vectors.insert(path.to_owned(), *vector);
                    state.entries.insert(
                        path.to_owned(),
                        Entry::Image(Drawing {
                            size: Size::new(w, h),
                            raster: false,
                            drawn: HashMap::new(),
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
fn draw(shared: &Arc<Shared>, vectors: &HashMap<String, Vector>, path: &str, at: (u32, u32)) {
    let (width, height) = at;
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
        drawing.raster
    };
    let outcome = if raster {
        // The room for what is kept is made before the file is decoded, at the size it is kept at.
        decode::pixel_bytes(width, height)
            .ok_or(Failure::Budget {
                most: shared.limits.pixel_bytes,
            })
            .and_then(|bytes| shared.make_room(&mut shared.lock(), bytes))
            .and_then(|()| read_and_shrink(shared, path, width, height))
    } else {
        let Some(vector) = vectors.get(path) else {
            return;
        };
        // The pixels it makes, and the layers it is drawn on while it makes them, are approved
        // before the first is.
        decode::pixel_bytes(width, height)
            .ok_or(Failure::Budget {
                most: shared.limits.pixel_bytes,
            })
            .and_then(|bytes| {
                let working = vector.working(width, height);
                if working > shared.limits.work_bytes {
                    return Err(Failure::Work {
                        bytes: working,
                        most: shared.limits.work_bytes,
                    });
                }
                shared.make_room(&mut shared.lock(), bytes)
            })
            .and_then(|()| vector.draw(width, height))
    };
    let mut state = shared.lock();
    let now = Instant::now();
    let mut told = None;
    let drawn = match outcome {
        Ok(raster) => {
            let bitmap = shared.bitmap(&mut state, raster);
            state.arrived.redraw = true;
            Drawn::Ready(Kept { bitmap, asked: now })
        }
        // No room, and perhaps later.
        Err(failure @ Failure::Budget { .. }) => {
            if state.told_room.insert(format!("{path}@{width}x{height}")) {
                told = Some(failure);
            }
            Drawn::Waiting { since: now }
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
        drawing.drawn.insert(at, drawn);
    } else if let Drawn::Ready(kept) = drawn {
        freed += kept.bitmap.pixels.len();
    }
    state.held = state.held.saturating_sub(freed);
    if let Some(failure) = told {
        state.arrived.diagnostics.push(Diagnostic {
            path: path.to_owned(),
            message: format!("{failure} (drawn at {width}\u{d7}{height})"),
        });
    }
    let wake = shared.announce(&mut state);
    shared.finish(state, wake);
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

/// Lets go of the sizes of a PNG or a JPEG that the one drawn at `at` covers, being no larger on
/// either side; the bytes of the pixels they held. The host keeps the texture it made of them by
/// their `id` for as long as it likes.
fn give_up_covered(drawing: &mut Drawing, at: (u32, u32)) -> usize {
    let mut freed = 0;
    drawing.drawn.retain(|size, drawn| match drawn {
        Drawn::Ready(kept) if size.0 <= at.0 && size.1 <= at.1 && *size != at => {
            freed += kept.bitmap.pixels.len();
            false
        }
        _ => true,
    });
    freed
}
