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
//! and decodes the file, and the answer is *not yet* until it is done; then the host is woken
//! — once for everything that arrived since it last looked — and told what [`Images::poll`]
//! says: that a size is known and the page is to be laid out again, that pixels arrived and it is
//! to be drawn, and why an image was refused (the page's diagnostics). Each path is read and
//! decoded once (the result, a refusal as well, is kept for the life of the [`Images`]), and an
//! SVG is drawn once for each size it is asked at (a few sizes at most, each its own raster): a
//! host at rest is asked for nothing, and so wakes for nothing.
//!
//! **What the view holds is bounded, and what it holds is what is drawn.** The pixels of the
//! images (and the parsed drawings) are held to [`ImageLimits::pixel_bytes`] in all. When a new image
//! needs room, the pixels of the ones nothing has asked for in [`ImageLimits::idle`] are let go
//! (they are decoded again if they are asked for later), the oldest first; an image that still
//! does not fit is not refused for good, it waits, and is tried again when it is asked for after
//! that time, which is when something may have been let go.

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
    /// Bytes of decoded pixels the view may hold in all, over its images (4 bytes a pixel), and
    /// of the parsed drawings of its SVGs.
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
    /// Different sizes an SVG is drawn at, each its own raster; a size past them is shown as the
    /// nearest of the others.
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
    /// Names these pixels and no other: a new decoding of a path, and every new size an SVG is drawn
    /// at, is another `id`. What the host keeps of an image (its texture) is kept by it.
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
    /// Read and decode the file at the path.
    Decode(String),
    /// Draw the SVG at the path at this size.
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
    /// Being read and decoded.
    Pending,
    Raster(Kept),
    /// A raster whose pixels were let go for the room (nothing asked for them): its size is still
    /// known, and it is decoded again when its pixels are asked for.
    Released {
        size: Size,
        reloading: bool,
    },
    Vector(Drawing),
    /// Refused; the diagnostic was told.
    Refused,
    /// There was no room for it, and there may be later: tried again when it is asked for after
    /// [`ImageLimits::idle`].
    Waiting {
        since: Instant,
    },
}

/// An SVG: its size, and the pixels it was drawn into at each size it is asked at.
struct Drawing {
    size: Size,
    drawn: HashMap<(u32, u32), Drawn>,
}

/// One size of an SVG.
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

    /// The size of the image at `src` in pixels of the image itself (an SVG's own size), if it is
    /// known. The first time a path is asked for it is not: the image starts to be read and decoded,
    /// and the host is woken when it is (see [`Images::poll`]). A host's
    /// [`pencere_view::Measure::image`] is this.
    pub fn size(&self, src: &str) -> Option<Size> {
        let mut state = self.lock();
        let now = Instant::now();
        let mut wake = false;
        let size = match state.entries.get_mut(src) {
            Some(Entry::Raster(kept)) => {
                kept.asked = now;
                Some(Size::new(
                    kept.bitmap.width as f32,
                    kept.bitmap.height as f32,
                ))
            }
            Some(Entry::Vector(drawing)) => Some(drawing.size),
            Some(Entry::Released { size, .. }) => Some(*size),
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
    /// ([`pencere_view::Picture::full`]). A PNG or a JPEG has its pixels once, whatever the size;
    /// an SVG is drawn at the size, once for each size it is asked at (a few at most: a size past
    /// them is the nearest of the others), and until the drawing at a new size is done the nearest
    /// there is of another is what there is (none the first time). The host keeps the texture of
    /// a bitmap by its [`Bitmap::id`].
    pub fn bitmap(&self, src: &str, whole: Size) -> Option<Bitmap> {
        let mut state = self.lock();
        let now = Instant::now();
        let wanted = drawn_size(whole, self.shared.limits.side);
        let mut wake = false;
        let found = match state.entries.get_mut(src) {
            Some(Entry::Raster(kept)) => {
                kept.asked = now;
                Some(kept.bitmap.clone())
            }
            Some(Entry::Vector(_)) => {
                let (found, job, told) = self.vector_at(&mut state, src, wanted, now);
                if let Some(job) = job {
                    wake |= self.post(&mut state, job);
                }
                if told {
                    wake |= self.shared.announce(&mut state);
                }
                found
            }
            Some(Entry::Released { reloading, .. }) => {
                if !*reloading {
                    *reloading = true;
                    wake = self.post(&mut state, Job::Decode(src.to_owned()));
                }
                None
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

    /// What an SVG has at `wanted`: its pixels if they are drawn, else the nearest it has; a
    /// drawing to be started if it is not drawn or being, and whether something is to be told.
    fn vector_at(
        &self,
        state: &mut State,
        src: &str,
        wanted: (u32, u32),
        now: Instant,
    ) -> (Option<Bitmap>, Option<Job>, bool) {
        let limits = &self.shared.limits;
        let Some(Entry::Vector(drawing)) = state.entries.get_mut(src) else {
            return (None, None, false);
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
        let Some(Entry::Vector(drawing)) = state.entries.get_mut(src) else {
            return (None, job, told);
        };
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
        let stand_in = nearest.map(|kept| {
            kept.asked = now;
            kept.bitmap.clone()
        });
        (stand_in, job, told)
    }

    /// What arrived since the last look, and the host is not woken again for it. A host that is
    /// woken looks; one that looks at no time is only not told.
    pub fn poll(&self) -> Arrival {
        let mut state = self.lock();
        state.woken = false;
        std::mem::take(&mut state.arrived)
    }

    /// Whether a file is still being read and decoded, or an SVG drawn at a size it was asked at:
    /// the host will be woken for it. What has arrived is [`Images::poll`]'s once this says no.
    ///
    /// **Not for a frame**, which never waits for an image: for a host that wants the page whole
    /// before it looks (a test, a picture of the page) and waits for the wake while this says so.
    /// A file that waits for room (the view's pixels at their budget) is not on its way: it is
    /// tried again when it is asked for after [`ImageLimits::idle`], on the system's clock.
    pub fn busy(&self) -> bool {
        self.lock().entries.values().any(|entry| match entry {
            Entry::Pending
            | Entry::Released {
                reloading: true, ..
            } => true,
            Entry::Vector(drawing) => drawing
                .drawn
                .values()
                .any(|drawn| matches!(drawn, Drawn::Pending)),
            Entry::Raster(_) | Entry::Released { .. } | Entry::Refused | Entry::Waiting { .. } => {
                false
            }
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
            match state.entries.remove(src.as_str()) {
                Some(Entry::Raster(kept)) => freed += kept.bitmap.pixels.len(),
                Some(Entry::Vector(drawing)) => {
                    for drawn in drawing.drawn.into_values() {
                        if let Drawn::Ready(kept) = drawn {
                            freed += kept.bitmap.pixels.len();
                        }
                    }
                    drawings.push(src.clone());
                }
                _ => {}
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

/// The pixels that have gone unasked for the longest: when they were asked, the path they are of,
/// and the size of the drawing if they are an SVG's.
type Oldest = (Instant, String, Option<(u32, u32)>);

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

    /// Makes room for `bytes` more: lets go of the pixels of the images nothing has asked for in
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
        let mut consider = |asked: Instant, path: &String, at: Option<(u32, u32)>| {
            if now.saturating_duration_since(asked) >= idle
                && oldest.as_ref().is_none_or(|(before, ..)| asked < *before)
            {
                oldest = Some((asked, path.clone(), at));
            }
        };
        for (path, entry) in &state.entries {
            match entry {
                Entry::Raster(kept) => consider(kept.asked, path, None),
                Entry::Vector(drawing) => {
                    for (at, drawn) in &drawing.drawn {
                        if let Drawn::Ready(kept) = drawn {
                            consider(kept.asked, path, Some(*at));
                        }
                    }
                }
                _ => {}
            }
        }
        let Some((_, path, at)) = oldest else {
            return false;
        };
        let freed = match (state.entries.get_mut(&path), at) {
            (Some(entry @ Entry::Raster(_)), None) => {
                let Entry::Raster(kept) = std::mem::replace(entry, Entry::Pending) else {
                    return false;
                };
                *entry = Entry::Released {
                    size: Size::new(kept.bitmap.width as f32, kept.bitmap.height as f32),
                    reloading: false,
                };
                kept.bitmap.pixels.len()
            }
            (Some(Entry::Vector(drawing)), Some(at)) => match drawing.drawn.remove(&at) {
                Some(Drawn::Ready(kept)) => kept.bitmap.pixels.len(),
                _ => 0,
            },
            _ => 0,
        };
        state.held = state.held.saturating_sub(freed);
        true
    }

    /// The side and the memory approve an image of this size from its header: its pixels, held
    /// with the others, within the room (made if it can be), and what the decoder works in within
    /// what one image may be worked in.
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
        self.make_room(&mut self.lock(), bytes)
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
                    if matches!(
                        state.entries.get(&path),
                        Some(Entry::Pending | Entry::Released { .. })
                    ) {
                        let gave_up = Failure::Broken("the decoder gave up".into());
                        shared.refuse(&mut state, &path, &gave_up);
                    }
                }
                // An SVG that panicked when drawn stays the shapes it was, and is not drawn at
                // that size again; the page is told.
                Some((width, height)) => {
                    if let Some(Entry::Vector(drawing)) = state.entries.get_mut(&path)
                        && matches!(drawing.drawn.get(&(width, height)), Some(Drawn::Pending))
                    {
                        drawing.drawn.insert((width, height), Drawn::Refused);
                        state.arrived.diagnostics.push(Diagnostic {
                            path: path.clone(),
                            message: format!(
                                "could not be drawn: the renderer gave up (drawn at {width}\u{d7}{height})"
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

/// Reads and decodes the file at `path`.
fn decode_path(shared: &Arc<Shared>, vectors: &mut HashMap<String, Vector>, path: &str) {
    let outcome = read_and_decode(shared, path);
    let mut state = shared.lock();
    let now = Instant::now();
    match outcome {
        Ok(Decoded::Raster(raster)) => {
            let bitmap = shared.bitmap(&mut state, raster);
            state
                .entries
                .insert(path.to_owned(), Entry::Raster(Kept { bitmap, asked: now }));
            state.arrived.relayout = true;
            state.arrived.redraw = true;
        }
        Ok(Decoded::Vector(vector)) => {
            // The parsed drawing is held for as long as the view is, and is paid for.
            let kept = vector.weight().bytes;
            match shared.make_room(&mut state, kept) {
                Ok(()) => {
                    state.held = state.held.saturating_add(kept);
                    let (w, h) = vector.natural();
                    vectors.insert(path.to_owned(), *vector);
                    state.entries.insert(
                        path.to_owned(),
                        Entry::Vector(Drawing {
                            size: Size::new(w, h),
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

enum Decoded {
    Raster(Raster),
    Vector(Box<Vector>),
}

fn read_and_decode(shared: &Shared, path: &str) -> Result<Decoded, Failure> {
    let bytes = shared.assets.read(path).ok_or(Failure::Missing)?;
    if bytes.len() > shared.limits.file_bytes {
        return Err(Failure::Source {
            bytes: bytes.len(),
            most: shared.limits.file_bytes,
        });
    }
    match decode::sniff(&bytes).ok_or(Failure::Unknown)? {
        Format::Svg => Vector::parse(&bytes).map(|vector| Decoded::Vector(Box::new(vector))),
        format => decode::raster(&bytes, format, &mut |w, h, working| {
            shared.approves(w, h, working)
        })
        .map(Decoded::Raster),
    }
}

/// Draws the SVG at `path` at `at`, if that is still wanted of it.
fn draw(shared: &Arc<Shared>, vectors: &HashMap<String, Vector>, path: &str, at: (u32, u32)) {
    let (width, height) = at;
    {
        let state = shared.lock();
        let wanted = matches!(
            state.entries.get(path),
            Some(Entry::Vector(drawing)) if matches!(drawing.drawn.get(&at), Some(Drawn::Pending))
        );
        if !wanted {
            return;
        }
    }
    let Some(vector) = vectors.get(path) else {
        return;
    };
    // The pixels it makes, and the layers it is drawn on while it makes them, are approved before
    // the first is.
    let outcome = decode::pixel_bytes(width, height)
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
        .and_then(|()| vector.draw(width, height));
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
        // For good at this size: the shapes it was drawn as before stay what there is.
        Err(failure) => {
            told = Some(failure);
            Drawn::Refused
        }
    };
    if let Some(Entry::Vector(drawing)) = state.entries.get_mut(path) {
        drawing.drawn.insert(at, drawn);
    } else if let Drawn::Ready(kept) = drawn {
        state.held = state.held.saturating_sub(kept.bitmap.pixels.len());
    }
    if let Some(failure) = told {
        state.arrived.diagnostics.push(Diagnostic {
            path: path.to_owned(),
            message: format!("{failure} (drawn at {width}\u{d7}{height})"),
        });
    }
    let wake = shared.announce(&mut state);
    shared.finish(state, wake);
}
