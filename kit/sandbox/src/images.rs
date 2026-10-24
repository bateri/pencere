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
//! SVG is drawn once for each size it is asked at: a host at rest is asked for nothing, and so
//! wakes for nothing.

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use pencere_view::Size;

use crate::decode::{self, Failure, Format, Raster, Vector};

/// The files of the view's bundle, as the host has them: the one door the images have to them.
pub trait Assets: Send + Sync + 'static {
    /// The bytes of the file at `path`, as the manifest of the bundle names it, or `None` where the
    /// bundle has no such file. Called on the images' worker thread, never the one that draws; it
    /// may take the time reading a file does.
    fn read(&self, path: &str) -> Option<Arc<[u8]>>;
}

/// What the images of one view are held to.
#[derive(Clone, Debug)]
pub struct ImageLimits {
    /// The most pixels an image may be wide or tall (so the most a drawn SVG is).
    pub side: u32,
    /// Bytes of decoded pixels the view may hold in all, over its images (4 bytes a pixel).
    pub bytes: usize,
    /// Bytes a file of an image may be: the most any file of a bundle may be.
    pub source: usize,
    /// Different paths a view may name, as many as a bundle may have files; the ones after that
    /// are refused without being read.
    pub paths: usize,
}

impl Default for ImageLimits {
    fn default() -> Self {
        Self {
            side: 4096,
            bytes: 64 << 20,
            source: 8 << 20,
            paths: 1024,
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
pub struct Arrived {
    /// A size is known that was not: the page is laid out again, because the room an image takes
    /// is not what it was.
    pub relayout: bool,
    /// Pixels are there that were not: the page is drawn again.
    pub redraw: bool,
    /// Images that were refused, each once.
    pub diagnostics: Vec<Diagnostic>,
}

impl Arrived {
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
}

/// What is known of one path.
enum Entry {
    /// Being read and decoded.
    Pending,
    Raster(Bitmap),
    Vector(Drawing),
    /// Refused; the diagnostic was told.
    Refused,
}

/// An SVG: its size, and the pixels it was last drawn into.
struct Drawing {
    size: Size,
    bitmap: Option<Bitmap>,
    /// The size it is wanted at (drawn, or being drawn).
    wanted: Option<(u32, u32)>,
}

#[derive(Default)]
struct State {
    entries: HashMap<String, Entry>,
    /// Bytes of pixels held, over every bitmap in `entries`.
    held: usize,
    arrived: Arrived,
    /// The host has been woken and has not looked since.
    woken: bool,
    /// The next `Bitmap::id`.
    next: u64,
    /// Told that the view names more paths than it may.
    told_paths: bool,
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
        match state.entries.get(src) {
            Some(Entry::Raster(bitmap)) => {
                Some(Size::new(bitmap.width as f32, bitmap.height as f32))
            }
            Some(Entry::Vector(drawing)) => Some(drawing.size),
            Some(Entry::Pending | Entry::Refused) => None,
            None => {
                let wake = self.begin(&mut state, src);
                drop(state);
                self.wake_if(wake);
                None
            }
        }
    }

    /// The pixels of the image at `src`, to be drawn so that the whole image is `whole` pixels
    /// ([`pencere_view::Picture::full`]). A PNG or a JPEG has its pixels once, whatever the size;
    /// an SVG is drawn at the size, once for each size it is asked at, and until the drawing
    /// at a new size is done the one before it is what there is (none the first time). The host
    /// keeps the texture of a bitmap by its [`Bitmap::id`].
    pub fn bitmap(&self, src: &str, whole: Size) -> Option<Bitmap> {
        let mut state = self.lock();
        let wanted = drawn_size(whole, self.shared.limits.side);
        match state.entries.get_mut(src) {
            Some(Entry::Raster(bitmap)) => Some(bitmap.clone()),
            Some(Entry::Vector(drawing)) => {
                let held = drawing.bitmap.clone();
                if drawing.wanted != Some(wanted) {
                    drawing.wanted = Some(wanted);
                    let wake = self.post(&mut state, Job::Draw(src.to_owned(), wanted));
                    drop(state);
                    self.wake_if(wake);
                }
                held
            }
            Some(Entry::Pending | Entry::Refused) => None,
            None => {
                let wake = self.begin(&mut state, src);
                drop(state);
                self.wake_if(wake);
                None
            }
        }
    }

    /// What arrived since the last look, and the host is not woken again for it. A host that is
    /// woken looks; one that looks at no time is only not told.
    pub fn poll(&self) -> Arrived {
        let mut state = self.lock();
        state.woken = false;
        std::mem::take(&mut state.arrived)
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

    /// Whether `bytes` more of pixels, `freed` of the ones held being let go, stay within the limit.
    fn room(&self, state: &State, bytes: usize, freed: usize) -> Result<(), Failure> {
        if state.held.saturating_sub(freed).saturating_add(bytes) <= self.limits.bytes {
            Ok(())
        } else {
            Err(Failure::Budget {
                most: self.limits.bytes,
            })
        }
    }

    /// The side and the memory approve an image of this size from its header.
    fn approves(&self, width: u32, height: u32) -> Result<(), Failure> {
        let side = self.limits.side;
        if width > side || height > side {
            return Err(Failure::TooLarge {
                width,
                height,
                side,
            });
        }
        let bytes = decode::pixel_bytes(width, height).unwrap_or(usize::MAX);
        self.room(&self.lock(), bytes, 0)
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
        let path = match &job {
            Job::Decode(path) | Job::Draw(path, _) => path.clone(),
        };
        // A decoder that gives up by panicking costs the image and not the worker.
        let ran = catch_unwind(AssertUnwindSafe(|| match job {
            Job::Decode(path) => decode_path(shared, &mut vectors, &path),
            Job::Draw(path, at) => draw(shared, &vectors, &path, at),
        }));
        if ran.is_err() {
            // A file that panicked when decoded is refused; an SVG that panicked when drawn stays
            // the shape it was.
            let mut state = shared.lock();
            if matches!(state.entries.get(&path), Some(Entry::Pending)) {
                let gave_up = Failure::Broken("the decoder gave up".into());
                shared.refuse(&mut state, &path, &gave_up);
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
    match outcome {
        Ok(Decoded::Raster(raster)) => {
            let bitmap = shared.bitmap(&mut state, raster);
            state.entries.insert(path.to_owned(), Entry::Raster(bitmap));
            state.arrived.relayout = true;
            state.arrived.redraw = true;
        }
        Ok(Decoded::Vector(vector)) => {
            let (w, h) = vector.natural();
            vectors.insert(path.to_owned(), *vector);
            state.entries.insert(
                path.to_owned(),
                Entry::Vector(Drawing {
                    size: Size::new(w, h),
                    bitmap: None,
                    wanted: None,
                }),
            );
            state.arrived.relayout = true;
        }
        Err(failure) => shared.refuse(&mut state, path, &failure),
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
    if bytes.len() > shared.limits.source {
        return Err(Failure::Source {
            bytes: bytes.len(),
            most: shared.limits.source,
        });
    }
    match decode::sniff(&bytes).ok_or(Failure::Unknown)? {
        Format::Svg => Vector::parse(&bytes).map(|vector| Decoded::Vector(Box::new(vector))),
        format => {
            decode::raster(&bytes, format, &mut |w, h| shared.approves(w, h)).map(Decoded::Raster)
        }
    }
}

/// Draws the SVG at `path` at `at`, if that is still what is wanted of it.
fn draw(shared: &Arc<Shared>, vectors: &HashMap<String, Vector>, path: &str, at: (u32, u32)) {
    let (width, height) = at;
    let still_wanted = |state: &State| matches!(state.entries.get(path), Some(Entry::Vector(d)) if d.wanted == Some(at));
    // The sizes asked while this was waiting its turn made it stale: the last of them is the one
    // that is drawn.
    let freed = {
        let state = shared.lock();
        if !still_wanted(&state) {
            return;
        }
        match state.entries.get(path) {
            Some(Entry::Vector(drawing)) => drawing
                .bitmap
                .as_ref()
                .map_or(0, |bitmap| bitmap.pixels.len()),
            _ => 0,
        }
    };
    let Some(vector) = vectors.get(path) else {
        return;
    };
    let outcome = decode::pixel_bytes(width, height)
        .ok_or(Failure::Budget {
            most: shared.limits.bytes,
        })
        .and_then(|bytes| shared.room(&shared.lock(), bytes, freed))
        .and_then(|()| vector.draw(width, height));
    let mut state = shared.lock();
    if !still_wanted(&state) {
        return;
    }
    match outcome {
        Ok(raster) => {
            let bitmap = shared.bitmap(&mut state, raster);
            let mut released = 0;
            if let Some(Entry::Vector(drawing)) = state.entries.get_mut(path) {
                released = drawing
                    .bitmap
                    .replace(bitmap)
                    .map_or(0, |old| old.pixels.len());
            }
            state.held = state.held.saturating_sub(released);
            state.arrived.redraw = true;
        }
        Err(failure) => {
            // It stays the shape it was and what it was drawn as before is what there is; told once
            // for this size (it is not asked again at it).
            state.arrived.diagnostics.push(Diagnostic {
                path: path.to_owned(),
                message: format!("{failure} (drawn at {width}\u{d7}{height})"),
            });
        }
    }
    let wake = shared.announce(&mut state);
    shared.finish(state, wake);
}
