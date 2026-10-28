//! A page of the kit — a module or a script in a sandbox ([`pencere_sandbox`]) — driven: what the
//! person does told to it, its frames laid out with its editors and its scrolling, and what it says
//! handed to the host.
//!
//! The module runs on a thread of its own and **the host never waits for it**: a frame takes the
//! newest page the module has handed out, lays it out, paints it, tells the module how it stands (so
//! the module can answer where the pointer is) and, if the page is for another size or is moving,
//! asks for the next one. What comes back calls the host's `woken`, from another thread; the host
//! then drains the page ([`Page::drain`]) and draws a frame. A page that does not change costs
//! nothing: nothing is asked, nothing is woken, no frame is drawn.
//!
//! **The page's images come the same way.** A file of the bundle is decoded on a thread of its own
//! ([`Images`]); the layout asks for its size, the paint for its pixels, and what is not there yet
//! is a box with nothing in it. The decoder calls the same `woken` — once for everything that
//! arrived since it last looked — and the drain reads what came ([`Images::poll`]): a size is a page
//! laid out again, pixels a page drawn again, a refusal a word for the page's author
//! ([`Drained::diagnostics`]).
//!
//! Until the module has handed out its first page — it is compiled first, which takes a moment —
//! the frame shows a line saying so, laid out like any page but not told to the module. A module
//! that goes wrong is gone: the host is told why ([`Drained::ended`]) and takes the page down.
//!
//! **A page's editors are the drive's** ([`crate::editing`]): the frame makes them for the rooms the
//! page laid out, does what its replies asked of them and draws them, and the host's keys, input
//! method, pasteboard and pointer reach them between frames ([`PageEditors`]), measured by the
//! host's measure (the object that measures is the object that draws).
//!
//! **A page can ask to be woken** ([`crate::waking`]): the sandbox keeps the time, the drain tells
//! the host when it changed ([`Drained::wake`]), and the first frame at or after it wakes the page
//! before it asks for the page; a host whose view is not seen wakes it itself
//! ([`Page::wake_unseen`]).
//!
//! **One clock.** The frames are given their time by the host; between frames, what the person does
//! (an edit, for what is undone together; a box let go, for where it springs back from) is put on
//! the same clock: the host's own reading where its program has one ([`Program::now`]), else the
//! last frame's time and what the system's clock says has passed since. A host whose clock is
//! driven by hand (a test) is the clock of everything the page does. **The program's readings and
//! the frames' times must be one clock** — the same origin, the same seconds: a reading behind the
//! last frame is taken as the frame's time, so a clock of another origin would stand still between
//! frames (what is typed undone as one, a caret that does not blink, a timer late). A reading more
//! than a second behind the last frame stops a debug build; a release build takes it for no reading.

use std::sync::Arc;
use std::time::{Duration, Instant};

use pencere_editor::{Buffer, Editors, Route};
use pencere_sandbox::{
    Answer, Assets, Fault, ImageLimits, Images, Module, ModuleLimits, Program, Runner, Script,
    Unbounded,
};
use pencere_view::edit::{EditCommand, EditorAsk, Range};
use pencere_view::{
    Button, Cursor, DrawList, EditorMetrics, Element, Event, Font, Id, Input, Key, Keyboard, Laid,
    Measure, Metrics, Mods, Motions, Phase, Place, Point, Rect, Reply, Role, ScrollTo, Size, div,
    text,
};

use crate::editing::{Editing, Told, in_a_room};
use crate::host::{Drained, Held, Host};
use crate::outline::{Field, Outline, Placement};
use crate::scrolling::{HostScrolls, Scrolling};
use crate::waking::Wakes;

/// What a host gives a page: what it is held to, who it speaks for and on what clock, where its
/// files and its images come from, where it stands in the host's view, how its carets blink and its
/// boxes scroll, and what measures its text.
pub struct Setup {
    /// What the module is held to.
    pub limits: ModuleLimits,
    /// The program the page speaks to, as far as the page's speed goes (what it can take of what the
    /// page says), and the host's clock where it keeps one that can be read from any thread
    /// ([`Program::now`]). [`Unbounded`] by default: a program that always has room, and no clock.
    /// Its clock is the one the frames are given their time on ([`FrameAt::now`]): the same origin
    /// and the same seconds (a reading more than a second behind the last frame stops a debug build,
    /// and a release build takes it for no reading).
    pub program: Arc<dyn Program>,
    /// Where the files of the page's bundle are read from, for its images; a page without them draws
    /// no image. A script's page takes its own bundle's when this is `None`.
    pub assets: Option<Arc<dyn Assets>>,
    /// What the page's images are held to.
    pub images: ImageLimits,
    /// Where the page stands in the host's view.
    pub placement: Placement,
    /// How long an editor's caret stays lit, then dark, seconds: the host's cursor's half period (0:
    /// a caret that does not blink).
    pub blink: f64,
    /// How the page's boxes scroll on the host's system.
    pub scrolling: Scrolling,
    /// What measures the page's text, in frames and between them: the host's, as it draws it.
    pub measure: Box<dyn Measure>,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            limits: ModuleLimits::default(),
            program: Arc::new(Unbounded),
            assets: None,
            images: ImageLimits::default(),
            placement: Placement::default(),
            blink: 0.5,
            scrolling: Scrolling::default(),
            measure: Box::new(NoText),
        }
    }
}

/// A measure that has no text to measure: every line takes no room. What a page is measured with
/// until the host says how its text is set.
pub struct NoText;

impl Measure for NoText {
    fn measure(&mut self, _font: &Font, _text: &str) -> Metrics {
        Metrics::default()
    }
}

/// What a frame is asked for: the host's view in pixels (its strip included, [`Placement::inset`]),
/// its pixels to a point, the frame's time on the host's clock, and whether the person asked for
/// less motion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameAt {
    pub size: (u32, u32),
    pub scale: f64,
    /// The frame's time, seconds, on the host's one clock: the clock of [`Setup::program`]'s
    /// readings where it has one ([`Program::now`]), never another.
    pub now: f64,
    pub reduce_motion: bool,
}

/// A frame of the page: what to draw — pixels, from the top-left corner of the page's area, below
/// the host's strip — with every editor drawn in its place, and the page's outline where it changed
/// since the last frame (the host moves its fields and tells its screen reader).
#[derive(Clone, Debug, PartialEq)]
pub struct Painted {
    pub list: DrawList,
    pub outline: Option<Outline>,
    /// What replies asked of fields that this frame lays out (their texts and the keyboard), held
    /// by [`Page::drain`] until the page had them: the host tells them to its fields once it has
    /// put them over the page by `outline`, in order ([`Painted::deliver_held`]). A name the frame
    /// does not have either is passed over, as the host passes over any name that is no field.
    pub held: Vec<Held>,
}

impl Painted {
    /// Hands what was held for this frame to `host`, in the order the replies asked it: after the
    /// host has the fields `outline` names.
    pub fn deliver_held(&mut self, host: &mut dyn Host) {
        for held in std::mem::take(&mut self.held) {
            held.deliver(host);
        }
    }
}

/// How long a resized view waits, at most, for the page of its new size: two frames at 60 Hz. The
/// wait ends as soon as that page comes (a module answers in a millisecond or two), and the frame it
/// is drawn into is presented with the resize itself, so a longer wait costs a slow module's drag
/// some smoothness and never a picture of the wrong size. Bounded, so a module that does not answer
/// cannot hold the host.
const RESIZE_WAIT: Duration = Duration::from_millis(32);

/// How many times a frame lays the page out again for its editors (a caret an overlay stands by
/// moved, an editor's content grew): the overlay stands by the caret after one. What is left after
/// the last is the next frame's.
const RELAYOUTS: usize = 2;

/// A page of the kit, driven. See the module's documentation for how; every point it is told and
/// tells is the host's ([`Placement`]).
pub struct Page {
    /// What measures the page's text: the host's.
    measure: Box<dyn Measure>,
    /// The page's images, if it has a bundle to read them from.
    images: Option<Images>,
    state: State,
}

/// Everything of the page but what measures it: so that a measure made of the host's and the
/// images ([`Measured`]) can be held while the rest is worked on.
struct State {
    module: Module,
    clock: Clock,
    /// Refused images the host has not been told of.
    diagnostics: Vec<String>,
    /// The page as the module last handed it out.
    root: Option<Element>,
    /// The page as it was last laid out: what a key is asked against, for the command it does,
    /// and what the wheel moves a box on.
    laid: Option<Arc<Laid>>,
    /// The viewport (points) and scale `laid` is for.
    laid_for: Option<(Size, f32)>,
    /// The page has to be laid out again (a new page, an image's size): until it is, a frame for
    /// what only the scrolling changed moves the boxes of the page last laid out.
    fresh: bool,
    /// How far the host has the page's boxes scrolled.
    scrolls: HostScrolls,
    /// The scrolls the module asked for that the next frame does.
    asks: Vec<ScrollTo>,
    /// The page is moving: it is asked for again at every frame, until it says it is not.
    animating: bool,
    /// The time the page at rest wants to be asked for again, on the frames' clock.
    draw_at: Option<f64>,
    /// The time the page asked to be woken at, and the wakes it was given.
    wakes: Wakes,
    /// That time changed in a drain, which the host has not been told of ([`Drained::wake`]).
    rewake: bool,
    /// The viewport (points) and the scale the module was last asked for a page for, and the time.
    asked: Option<(Size, f32)>,
    asked_at: Option<f64>,
    /// Every event the page was told, where the host keeps them ([`Page::record`]).
    told: Option<Vec<Event>>,
    /// Where the page stands in the host's view.
    place: Placement,
    /// The fields and what a screen reader says, as last told, in the host's points.
    outline: Outline,
    /// A page arrived that the host has not been told of.
    redraw: bool,
    /// What the module said that the host has not taken.
    replies: Vec<Answer>,
    /// What replies asked of fields the page had not laid out when the host took them, in order: a
    /// frame that lays the page out gives it ([`Painted::held`]).
    held: Vec<Held>,
    /// Why the module is gone, until the host has been told.
    ended: Option<Fault>,
    told_end: bool,
    /// What the module said to its program that the host has not carried.
    sent: Vec<String>,
    /// The page's editors.
    editing: Editing,
    /// The shape the editor says the pointer has where it last was, over an editor's room.
    cursor_over: Option<Cursor>,
    /// The frame just drawn left something for the next (a scroll an editor asked, a layout): the
    /// host is asked for it.
    again: bool,
    /// When the caret of the editor with the keyboard next blinks, on the frames' clock.
    blink_at: Option<f64>,
    /// Where the elements of the page that move were: the host moves them between the module's
    /// frames, laying the same page out again at each frame while one moves.
    motions: Motions,
}

/// The drive's clock: the host's reading where its program has one, else the last frame's time and
/// what the system's clock says has passed since; never earlier than the last frame. The reading and
/// the frames' times are one clock; a debug build stops on a reading that is not ([`BEHIND`]).
struct Clock {
    program: Arc<dyn Program>,
    /// The time of the last frame, on the frames' clock, and when that was here.
    frame: f64,
    at: Instant,
}

/// How far the program's reading may be behind the last frame's time, seconds, before it is taken
/// for a host's mistake: a frame is stamped with when it will be shown, a little ahead of the clock.
const BEHIND: f64 = 1.0;

impl Clock {
    fn now(&self) -> f64 {
        let reading = self.program.now().filter(|now| now.is_finite());
        // A reading far behind the last frame is of another clock than the frames': taken as the
        // frame's time, it would stand still between frames.
        debug_assert!(
            reading.is_none_or(|now| now >= self.frame - BEHIND),
            "the program's clock reads {reading:?}, behind the last frame ({}) by more than {BEHIND} \
             s: `Setup::program` and `FrameAt::now` must be one clock",
            self.frame,
        );
        reading
            // Where a build does not stop on it, a reading of another clock is no reading.
            .filter(|now| *now >= self.frame - BEHIND)
            .unwrap_or_else(|| self.frame + self.at.elapsed().as_secs_f64())
            .max(self.frame)
    }

    /// A frame is being built at `now`.
    fn frame(&mut self, now: f64) {
        if now.is_finite() {
            self.frame = now;
            self.at = Instant::now();
        }
    }
}

/// The host's measure, with the page's images for the sizes of its pictures: the host measures
/// text, the drive knows the images.
struct Measured<'a> {
    text: &'a mut dyn Measure,
    images: Option<&'a Images>,
}

impl Measure for Measured<'_> {
    fn measure(&mut self, font: &Font, text: &str) -> Metrics {
        self.text.measure(font, text)
    }

    fn image(&mut self, src: &str) -> Option<Size> {
        match self.images {
            Some(images) => images.size(src),
            None => self.text.image(src),
        }
    }

    fn offsets(&mut self, font: &Font, text: &str, at: &[usize]) -> Vec<f32> {
        self.text.offsets(font, text, at)
    }

    fn editor(&mut self, id: Id) -> Option<EditorMetrics> {
        self.text.editor(id)
    }
}

impl Page {
    /// The page of the component in `bytes` (binary or text). `woken` is called from the module's
    /// thread, and from the images', when something is waiting to be taken: it hops to wherever the
    /// host listens and has the host drain the page, **and never lays out or draws itself** (the
    /// images may call it from inside a frame). Returns at once: the module is compiled and started
    /// on its thread.
    pub fn component(
        bytes: Vec<u8>,
        setup: Setup,
        woken: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        let limits = setup.limits.clone();
        Self::start(
            |program, wake| Module::from_component(bytes, limits, program, wake),
            setup,
            woken,
        )
    }

    /// The page of a script: the interpreter of the script's language as the module, and `script`
    /// as what it runs (its text, its path in its bundle — what its errors name and what a
    /// `require` of it is relative to — and the bundle). The other scripts of the bundle it can load
    /// are the ones that say they are of the type `text/x-lua`; the bundle is where its images come
    /// from too, unless the setup names other files. Otherwise as [`Page::component`].
    pub fn script(
        script: Script,
        mut setup: Setup,
        woken: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        if setup.assets.is_none() {
            setup.assets.clone_from(&script.bundle);
        }
        let limits = setup.limits.clone();
        Self::start(
            |program, wake| Module::from_script(script, limits, program, wake),
            setup,
            woken,
        )
    }

    /// The page of a module the host makes itself (`make` runs on the module's thread): a page
    /// written in Rust, run as a module is. Otherwise as [`Page::component`].
    pub fn spawn<R: Runner>(
        make: impl FnOnce() -> Result<R, Fault> + Send + 'static,
        setup: Setup,
        woken: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self::start(
            |program, wake| Module::spawn(make, program, wake),
            setup,
            woken,
        )
    }

    fn start(
        start: impl FnOnce(Arc<dyn Program>, Box<dyn Fn() + Send + Sync>) -> Module,
        setup: Setup,
        woken: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        // One way to the host for the module and for the images.
        let woken: Arc<dyn Fn() + Send + Sync> = Arc::new(woken);
        let images = setup.assets.map(|assets| {
            let woken = Arc::clone(&woken);
            Images::new(assets, setup.images, move || woken())
        });
        let module = start(Arc::clone(&setup.program), Box::new(move || woken()));
        let mut scrolls = HostScrolls::new();
        scrolls.set(setup.scrolling);
        Self {
            measure: setup.measure,
            images,
            state: State {
                module,
                clock: Clock {
                    program: setup.program,
                    frame: 0.0,
                    at: Instant::now(),
                },
                diagnostics: Vec::new(),
                root: None,
                laid: None,
                laid_for: None,
                fresh: true,
                scrolls,
                asks: Vec::new(),
                animating: false,
                draw_at: None,
                wakes: Wakes::default(),
                rewake: false,
                asked: None,
                asked_at: None,
                told: None,
                place: setup.placement,
                outline: Outline::default(),
                redraw: false,
                replies: Vec::new(),
                held: Vec::new(),
                ended: None,
                told_end: false,
                sent: Vec::new(),
                editing: Editing::new(setup.blink),
                cursor_over: None,
                again: false,
                blink_at: None,
                motions: Motions::default(),
            },
        }
    }

    // --- What the person does ---------------------------------------------------------------------

    /// The person did something with the pointer or a key, in the host's points. A pointer input
    /// over a box's scroll bar is the bar's, not the page's. A key that does one of the commands the
    /// page can be told to do now is that command, and the page is told the command and not the key
    /// (the kit's specification, §4). Over an editor, the pointer is the editor's as well (a left
    /// press puts its caret and gives it the keyboard; a left press outside every editor's room takes
    /// the keyboard back to the page), and the page is told the place under it. A press ends what an
    /// input method composes in an editor, which stays as typed: the host lets its own system forget
    /// it ([`PageEditors::marked`] says, before).
    ///
    /// A key while an editor has the keyboard is [`Page::key`]'s.
    pub fn input(&mut self, input: Input) {
        if matches!(input, Input::Press { .. })
            && self.state.editing.editors.marked_utf16().is_some()
        {
            self.editors().unmark();
        }
        let state = &mut self.state;
        let place = state.place;
        let now = state.clock.now();
        let (took, redraw) = state.scrolls.pointer(&into_page(input, place), now);
        state.redraw |= redraw;
        if took {
            state.tell_scrolled();
            return;
        }
        if let Input::Key { key, mods } = input
            && let Some(command) = state
                .laid
                .as_ref()
                .and_then(|laid| laid.command_for(key, mods))
        {
            state.event(Event::Command(command));
            return;
        }
        let input = into_page(input, place);
        // Over an editor, the pointer is the editor's as well, and the page is told the place under
        // it; after what the editor told (the keyboard it took, the caret it put).
        let (under, left) = self.pointer_in_editors(&input);
        let state = &mut self.state;
        state.cursor_over = match input {
            Input::Move(at) | Input::Press { at, .. } | Input::Release { at, .. } => state
                .laid
                .as_ref()
                .and_then(|laid| state.editing.editors.cursor_at(laid, at)),
            Input::Leave => None,
            _ => state.cursor_over,
        };
        if left {
            state.event(Event::Focus(None));
        }
        state.event(match under {
            Some(place) => Event::Pointer { input, place },
            None => Event::Input(input),
        });
    }

    /// A key pressed, and where it went: to the editor with the keyboard first — the page's command
    /// for the key, the editor's own, the text ([`PageEditors::key`]) — and where no editor has the
    /// keyboard, to the page ([`Page::input`]), which is [`Route::NotEditor`]. The rest of the route
    /// is the host's: `Type`, the key to its system's handling of text, which says what is typed
    /// ([`PageEditors::type_text`]); `Copy` and `Cut`, the text to its pasteboard; `Paste`, its
    /// pasteboard's text to [`PageEditors::paste`].
    pub fn key(&mut self, key: Key, mods: Mods) -> Route {
        let route = self.editors().key(key, mods);
        if route == Route::NotEditor {
            self.input(Input::Key { key, mods });
        }
        route
    }

    /// A key went up. An editor with the keyboard took the keys that went down, so the page heard
    /// none of them go down, and hears none go up.
    pub fn key_up(&mut self, key: Key, mods: Mods) {
        if self.state.editing.editors.focused().is_none() {
            self.input(Input::KeyUp { key, mods });
        }
    }

    /// The wheel or the trackpad over `at`, in the host's points, by `dx` and `dy` (positive: the
    /// content moves left and up), `phase` the part of the gesture the system says it is. The drive
    /// scrolls the box under the pointer itself, on the page as it was last laid out; a turn no box
    /// took is the page's, as [`Input::Scroll`].
    pub fn wheel(&mut self, at: Point, dx: f32, dy: f32, phase: Phase) {
        let state = &mut self.state;
        let place = state.place;
        let (page_at, zoom) = (place.to_page(at), place.zoom);
        let (dx, dy) = (dx / zoom, dy / zoom);
        let now = state.clock.now();
        let left = match state.laid.clone() {
            Some(laid) => {
                let wheel = state.scrolls.wheel(&laid, page_at, (dx, dy), phase, now);
                state.redraw |= wheel.took;
                state.tell_scrolled();
                wheel.left
            }
            None => Point::new(dx, dy),
        };
        // What no box took is the page's.
        if left.x != 0.0 || left.y != 0.0 {
            state.event(Event::Input(Input::Scroll {
                at: page_at,
                dx: left.x,
                dy: left.y,
            }));
        }
    }

    /// The text of the host's field named `field` changed: the person typed, pasted or deleted.
    pub fn text(&mut self, field: Id, text: &str) {
        self.state.event(Event::Text {
            field,
            text: text.to_owned(),
        });
    }

    /// The person clicked into the host's field named `field`, or the keyboard went back to the
    /// page of itself (`None`); not told when it went where the page's reply sent it.
    pub fn focus(&mut self, field: Option<Id>) {
        let state = &mut self.state;
        // The person clicked into a field: the keyboard is not an editor's any more.
        if field.is_some() {
            state.editing.editors.set_focus(None);
            state.editing.person_moved_keyboard();
            state.redraw = true;
        }
        state.event(Event::Focus(field));
    }

    /// The person picked the `index`th entry of the menu the last reply asked for.
    pub fn pick(&mut self, index: usize) {
        self.state.event(Event::MenuPick(index));
    }

    /// The person asked for the command named `id`, from the host's list of what the page can be
    /// told to do ([`Outline::commands`]). The keys of a command are not told here: the drive tells
    /// the page the command in place of a key it is told that does one.
    pub fn command(&mut self, id: Id) {
        self.state.event(Event::Command(id));
    }

    /// Files were dropped on the page at `at`, in the host's points: their paths.
    pub fn files(&mut self, at: Point, paths: Vec<String>) {
        let at = self.state.place.to_page(at);
        self.state.event(Event::Files { at, paths });
    }

    /// The program that opened the page said something: one JSON object, as it wrote it.
    pub fn message(&mut self, text: &str) {
        self.state.module.message(text.to_owned());
    }

    /// The program has read enough that the page may say what it was told the program had no room
    /// for.
    pub fn room(&mut self) {
        self.state.module.room();
    }

    // --- What the host is like ---------------------------------------------------------------------

    /// How the page's boxes scroll on the host's system.
    pub fn set_scrolling(&mut self, scrolling: Scrolling) {
        self.state.scrolls.set(scrolling);
        self.state.redraw = true;
    }

    /// The host's text is this much larger than the kit's own ([`Placement::zoom_for`]): the page is
    /// drawn that much larger, and laid out again. What is not a size (none, less, not a number)
    /// changes nothing.
    pub fn set_zoom(&mut self, zoom: f32) {
        if !(zoom.is_finite() && zoom > 0.0) {
            return;
        }
        let state = &mut self.state;
        if state.place.zoom != zoom {
            state.place.zoom = zoom;
            state.redraw = true;
        }
    }

    /// What measures the page's text from now on: the host's, as it now draws it (another font,
    /// another renderer). The page is laid out again.
    pub fn set_measure(&mut self, measure: Box<dyn Measure>) {
        self.measure = measure;
        self.state.fresh = true;
        self.state.redraw = true;
    }

    /// Whether the host's keyboard is in the page (not one of its fields, not another view) of the
    /// person's window: an editor's caret is drawn, and blinks, only then.
    pub fn set_active(&mut self, active: bool) {
        let state = &mut self.state;
        let was = state.editing.blink;
        state.editing.blink.set_active(active);
        if state.editing.blink != was && state.editing.editors.focused().is_some() {
            state.redraw = true;
        }
    }

    /// Where the page stands in the host's view.
    pub fn placement(&self) -> Placement {
        self.state.place
    }

    // --- Frames, wakes and what the page said ------------------------------------------------------

    /// The page for the frame `at`, laid out and painted, its editors in their places. The frame
    /// takes what the module has put out, wakes the page if the time it asked for has come (before
    /// it asks for the frame's page), and asks for a page for this size, for the next step of one
    /// that is moving, or for the time the page asked to be drawn at; a page that is none of these
    /// is not asked for.
    pub fn frame(&mut self, at: FrameAt) -> Painted {
        let mut measured = Measured {
            text: &mut *self.measure,
            images: self.images.as_ref(),
        };
        self.state.frame(at, &mut measured, self.images.as_ref())
    }

    /// Whether the page wants the next frame: it is moving, its boxes are, or the frame just drawn
    /// left something for it.
    pub fn animating(&self) -> bool {
        let state = &self.state;
        state.animating
            || state.motions.moving()
            || state.scrolls.moving(state.clock.frame)
            || state.again
    }

    /// Asks the page for itself again at the next frame, though nothing the drive knows of changed:
    /// what a host does when its view is shown anew, or a test that reads what the page shows of
    /// what changed in it with no reply that asked for a frame.
    pub fn refresh(&mut self) {
        self.state.asked = None;
        self.state.redraw = true;
    }

    /// Whether the frame just drawn left work for the next one that is not motion — a layout an
    /// editor asked for, a scroll to keep its caret in view — which a frame at the same time does. A
    /// host that draws frames by its clock need not ask: [`Page::animating`] says it too; a host
    /// that drives the page by hand and moves time itself draws the next frame at once for it, and
    /// draws none for what only moves.
    pub fn unsettled(&self) -> bool {
        self.state.again
    }

    /// The earliest of the times the page itself asked for, on the frames' clock: to be drawn at
    /// (its frame's `wake_at`) or woken at (a reply's `wake`). Not the caret's blink nor the scroll
    /// bars', which are the drive's ([`Page::wake_at`] has every one).
    pub fn asks_at(&self) -> Option<f64> {
        let state = &self.state;
        [state.draw_at, state.wakes.waiting()]
            .into_iter()
            .flatten()
            .reduce(f64::min)
    }

    /// Whether the drive keeps, from now on, every event it tells the page ([`Page::told`]) and
    /// every reply whole ([`Drained::replies`] with what they asked of the editors, which the drive
    /// otherwise takes out of them as it does it): what a host that tests a page reads. Off unless
    /// asked; turned off, what was kept goes.
    pub fn record(&mut self, record: bool) {
        match (record, self.state.told.is_some()) {
            (true, false) => self.state.told = Some(Vec::new()),
            (false, true) => self.state.told = None,
            _ => {}
        }
    }

    /// The events the drive told the page since it was asked to keep them ([`Page::record`]), in
    /// order: what the person did, the editors' events, the boxes scrolled, the commands.
    pub fn told(&self) -> &[Event] {
        self.state.told.as_deref().unwrap_or_default()
    }

    /// The time a frame of the page at rest is next wanted at, on the frames' clock: the earliest of
    /// the time the page asked to be drawn at, the one it asked to be woken at (a wake is given from
    /// a frame), when the scroll bars go and when the caret next blinks.
    pub fn wake_at(&self) -> Option<f64> {
        let state = &self.state;
        [
            state.draw_at,
            state.wakes.waiting(),
            state.scrolls.wake_at(state.clock.frame),
            state.blink_at,
        ]
        .into_iter()
        .flatten()
        .reduce(f64::min)
    }

    /// The host's view is not seen and draws no frames, and it is `now` on the frames' clock: the
    /// page is woken if the time it asked to be woken at has come and a second has passed since it
    /// was last woken (the frames wake it while the view is seen). When to ask again, if the page
    /// waits for a wake.
    pub fn wake_unseen(&mut self, now: f64) -> Option<f64> {
        let state = &mut self.state;
        if state.wakes.unseen(now) {
            state.module.wake(now);
        }
        state.wakes.unseen_at()
    }

    /// Hands over what the page has said since the last call ([`Drained::deliver`] does it to a
    /// [`crate::Host`]). Never waits.
    pub fn drain(&mut self) -> Drained {
        let state = &mut self.state;
        state.sync(false, self.images.as_ref());
        let ended = if state.told_end {
            None
        } else {
            state.ended.clone().inspect(|_| state.told_end = true)
        };
        let mut replies = std::mem::take(&mut state.replies);
        hold(&mut replies, &state.outline.fields, &mut state.held);
        Drained {
            replies,
            redraw: std::mem::take(&mut state.redraw),
            diagnostics: std::mem::take(&mut state.diagnostics),
            ended,
            sent: std::mem::take(&mut state.sent),
            wake: std::mem::take(&mut state.rewake),
        }
    }

    // --- What the page is now ----------------------------------------------------------------------

    /// The fields, what a screen reader says, the tooltips and the commands of the page, as of the
    /// last frame, in the host's points.
    pub fn outline(&self) -> &Outline {
        &self.state.outline
    }

    /// The fields of the page, as of the last frame, in the host's points.
    pub fn fields(&self) -> &[Field] {
        &self.state.outline.fields
    }

    /// The page as the module last handed it out; `None` before its first.
    pub fn root(&self) -> Option<&Element> {
        self.state.root.as_ref()
    }

    /// The page as it was last laid out, in the page's own points; `None` before its first frame.
    pub fn laid(&self) -> Option<&Laid> {
        self.state.laid.as_deref()
    }

    /// The page's images, if it has a bundle: a host draws an image of the draw list with
    /// [`Images::bitmap`].
    pub fn images(&self) -> Option<&Images> {
        self.images.as_ref()
    }

    /// The module the page is: what a host that drives a page by hand waits for
    /// ([`Module::flush`]). Telling it anything past the drive is the host's own business.
    pub fn module(&self) -> &Module {
        &self.state.module
    }

    /// The page's editors, as the host's keys, input method, pasteboard and screen reader reach
    /// them.
    pub fn editors(&mut self) -> PageEditors<'_> {
        PageEditors { page: self }
    }

    /// The document of the page's editor named `id`, wherever the keyboard is.
    pub fn editor(&self, id: Id) -> Option<&Buffer> {
        self.state.editing.editors.buffer(id)
    }

    /// The cell of column `column` at line `line` of the page's editor named `id`, in the host's
    /// points, as of the last frame; none where it is not in view, or the editor has no such column.
    pub fn gutter_cell(&self, id: Id, column: &str, line: usize) -> Option<Rect> {
        let laid = self.state.laid.as_ref()?;
        let cell = self
            .state
            .editing
            .editors
            .cell_rect(laid, id, column, line)?;
        Some(self.state.place.to_host(cell))
    }

    /// The gaps the page's editor named `id` keeps open for the page, each by its name and its box in
    /// the host's points, as of the last frame.
    pub fn zones(&self, id: Id) -> Vec<(Id, Rect)> {
        let Some(laid) = self.state.laid.as_ref() else {
            return Vec::new();
        };
        let s = laid.scale();
        let Some(room) = laid.editors().into_iter().find(|room| room.id == id) else {
            return Vec::new();
        };
        let scroll = laid
            .scroll_extent(id)
            .map_or((0.0, 0.0), |extent| (extent.offset.x, extent.offset.y));
        self.state
            .editing
            .editors
            .zones(id)
            .into_iter()
            .map(|(zone, rect)| {
                let at = Rect::new(
                    room.pixels.x / s + rect.x / s - scroll.0,
                    room.pixels.y / s + rect.y / s - scroll.1,
                    rect.w / s,
                    rect.h / s,
                );
                (zone, self.state.place.to_host(at))
            })
            .collect()
    }

    /// The editor with the keyboard.
    pub fn focused_editor(&self) -> Option<Id> {
        self.state.editing.editors.focused()
    }

    // --- Within ------------------------------------------------------------------------------------

    /// Does `run` to the page's editors, on the page as it was last laid out, measuring as its
    /// frames do, at the drive's time; and tells the page and the host what came of it — a frame
    /// where anything did, or always where `drawn` (a key, the input method). `None` (and nothing
    /// done) before the page is laid out.
    fn edit<T>(
        &mut self,
        drawn: bool,
        run: impl FnOnce(&mut Editors, &Laid, &mut dyn Measure, f64) -> T,
    ) -> Option<T> {
        let state = &mut self.state;
        let laid = state.laid.clone()?;
        let now = state.clock.now();
        let mut measured = Measured {
            text: &mut *self.measure,
            images: self.images.as_ref(),
        };
        let out = run(&mut state.editing.editors, &laid, &mut measured, now);
        let told = state.editing.told();
        state.tell(told, false, drawn);
        Some(out)
    }

    /// A pointer input (the page's points) over the page's editors: a left press in one is the
    /// editor's (it puts the caret, and the editor has the keyboard), a move with it held drags,
    /// the release ends it; a left press outside every editor's room takes the keyboard from the
    /// one that had it, back to the page, as a press outside a field does. The place under the
    /// pointer, where it is over an editor, and whether the page is to be told the keyboard left.
    fn pointer_in_editors(&mut self, input: &Input) -> (Option<Place>, bool) {
        if matches!(input, Input::Leave) {
            self.edit(false, |editors, laid, _, _| editors.hover(laid, None));
            return (None, false);
        }
        if !matches!(
            input,
            Input::Press { .. } | Input::Move(_) | Input::Release { .. }
        ) {
            return (None, false);
        }
        let done = self.edit(false, |editors, laid, measure, _| {
            let mut left = false;
            let at = match *input {
                Input::Press {
                    at,
                    button: Button::Left,
                    mods,
                    count,
                } => {
                    let took = editors.press(laid, at, mods, count, measure);
                    if !took && editors.focused().is_some() && !in_a_room(laid, at) {
                        editors.set_focus(None);
                        left = true;
                    }
                    at
                }
                Input::Press {
                    at, button, mods, ..
                } => {
                    editors.press_other(laid, at, button, mods);
                    at
                }
                Input::Move(at) => {
                    editors.drag_to(laid, at, measure);
                    editors.hover(laid, Some(at));
                    at
                }
                Input::Release { at, button } => {
                    if button == Button::Left {
                        editors.release();
                    }
                    at
                }
                Input::Scroll { .. }
                | Input::Leave
                | Input::Key { .. }
                | Input::KeyUp { .. }
                | Input::Mods(_) => return (None, false),
            };
            let place = editors
                .place_at(laid, at, measure)
                .map(|(editor, at)| Place { editor, at });
            (place, left)
        });
        if matches!(input, Input::Press { .. }) {
            self.state.editing.person_moved_keyboard();
        }
        done.unwrap_or((None, false))
    }
}

impl State {
    /// Tells the module what the person did, or what came of it: the one way an event goes to it.
    fn event(&mut self, event: Event) {
        if let Some(told) = &mut self.told {
            told.push(event.clone());
        }
        self.module.event(event);
    }

    /// Takes what the module has put out into what the page keeps. `building`: a frame is being
    /// laid out and drawn from what is taken, so it is that frame that shows it — the host is not
    /// told to draw another for it (the wake that came with it finds nothing new to say). The drain,
    /// which is not a frame, is the one that says a page is to be drawn.
    fn sync(&mut self, building: bool, images: Option<&Images>) {
        let update = self.module.poll();
        if let Some(frame) = update.frame {
            self.root = Some(frame.root);
            self.animating = frame.animating;
            self.draw_at = frame.wake_at;
            self.fresh = true;
            self.redraw |= !building;
        }
        self.asks.extend(update.asks);
        // The time the page asked to be woken at, told at every taking while it stands: one a
        // frame takes is read with that frame's (`Page::wake_at`), one a drain takes is the host's
        // to arm its clock for, with no frame drawn for it.
        if self.wakes.told(update.wake) && !building {
            self.rewake = true;
        }
        if let Some(images) = images {
            let arrived = images.poll();
            // A size the layout was waiting for, pixels the paint was: either way a frame of the
            // page, the one that lays it out again and draws it.
            if (arrived.relayout || arrived.redraw) && !building {
                self.redraw = true;
            }
            self.fresh |= arrived.relayout || arrived.redraw;
            self.diagnostics
                .extend(arrived.diagnostics.iter().map(ToString::to_string));
        }
        let place = self.place;
        for mut answer in update.replies {
            // What it asks of the editors is done at the next frame, once they are made for the
            // page it came with: a frame is worth drawing.
            let keep = self.told.is_some();
            if self.editing.take_reply(&mut answer.reply, keep) && !building {
                self.redraw = true;
            }
            // The shape of the pointer over an editor's room is the editor's (the I-beam over its text,
            // the arrow in its gutter, the hand over what answers a press there), unless the page asks
            // for one of the shapes that mean something it does itself (a divider's, a drag's).
            if answer.pointer
                && matches!(
                    answer.reply.cursor,
                    Cursor::Arrow | Cursor::Text | Cursor::Pointer
                )
                && let Some(over) = self.cursor_over
            {
                answer.reply.cursor = over;
            }
            answer.reply = to_host(answer.reply, place);
            self.replies.push(answer);
        }
        // How much the module may say is not decided here: it was told *full* by the program when
        // the program had no room, and what it said is what the program has room for.
        if self.ended.is_none() {
            self.sent.extend(update.sent);
        }
        if let Some(fault) = update.closed
            && self.ended.is_none()
        {
            self.ended = Some(fault);
        }
    }

    /// Tells the module where the boxes went that moved since it was last told: at once, not at the
    /// next frame, so that the page starts on the rows that come into view while the frame that
    /// moves the boxes is drawn.
    fn tell_scrolled(&mut self) {
        let moved = self.scrolls.scrolls.take_moved();
        if !moved.is_empty() {
            self.event(Event::Scrolled(moved));
        }
    }

    /// What came of something done to the editors: told the module (the texts first, so that it
    /// reads the version the events tell of), the scrolls they ask for done by the next frame, the
    /// page laid out again there where they want it. `building`: a frame is being drawn, and a next
    /// one is asked for if there is anything for it; otherwise the host is to draw one
    /// ([`Drained::redraw`]) where anything came of it, or `drawn` says the editors changed.
    fn tell(&mut self, told: Told, building: bool, drawn: bool) {
        let more = !told.scrolls.is_empty() || told.relayout;
        let anything = !told.is_empty();
        if !told.events.is_empty() {
            self.module.texts(told.texts);
            for event in told.events {
                self.event(event);
            }
        }
        self.asks.extend(told.scrolls);
        self.fresh |= told.relayout;
        if building {
            self.again |= more;
        } else if anything || drawn {
            self.redraw = true;
        }
    }

    /// The page for the frame `at`, laid out and painted measuring with `measure`, its editors in
    /// their places: the draw list, and the outline where it changed.
    fn frame(
        &mut self,
        at: FrameAt,
        measure: &mut dyn Measure,
        images: Option<&Images>,
    ) -> Painted {
        #[allow(clippy::cast_possible_truncation)]
        let pixels = at.scale as f32;
        let inset = (self.place.inset * pixels).round();
        let scale = pixels * self.place.zoom;
        #[allow(clippy::cast_precision_loss)]
        let (width, height) = (at.size.0 as f32, at.size.1 as f32);
        let height = (height - inset).max(0.0);
        let size = Size::new(width / scale, height / scale);
        let viewport = Size::new(width, height);
        self.again = false;
        self.clock.frame(at.now);
        self.sync(true, images);
        // The time the page asked to be woken at has come: it is woken at this frame's time, before
        // its page is asked for, and once a frame however often it asks (the kit's specification,
        // §4). Its answer comes as any reply does, and a frame with it where it changed the page:
        // none is asked for the wake itself.
        if self.wakes.frame(at.now) {
            self.module.wake(at.now);
        }
        // A page for this size, the next step of one that is moving and the page whose time has
        // come; a page that is none of these is not asked for.
        // A page that moves is asked for its next step at a later time: a frame built again at the
        // time it was asked for (a host that draws twice in a tick, one driven by hand) would be
        // given the page it has, an event that changed it having made it again already.
        let due = self.draw_at.is_some_and(|draw| at.now >= draw);
        let moving = self.animating && self.asked_at.is_none_or(|asked| at.now > asked);
        let resized = self.asked.is_some_and(|asked| asked != (size, scale));
        if self.asked != Some((size, scale)) || moving || due {
            self.asked = Some((size, scale));
            self.asked_at = Some(at.now);
            self.draw_at = None;
            self.module.render(size, at.now, at.reduce_motion);
        }
        // A resized view waits a little for the page made for its size rather than draw, for a
        // frame, the one laid out for the size it was: that frame is what flickers. A page that
        // comes meanwhile for another size (one asked before) does not end it.
        if resized && self.module.wait_for_frame(size, RESIZE_WAIT) {
            self.sync(true, images);
        }
        let asks = std::mem::take(&mut self.asks);
        self.scrolls.ask(asks);
        // Only the scrolling changed: the boxes move on the page as it was laid out.
        let still = !self.fresh && self.laid_for == Some((size, scale)) && !self.motions.moving();
        let cached = self.laid.clone().filter(|_| still);
        let root = self.root.clone();
        let starting = root.is_none();
        let mut laid = match cached {
            Some(laid) if !starting => {
                let mut laid = (*laid).clone();
                self.scrolls
                    .scrolls
                    .place(&mut laid, at.now, at.reduce_motion);
                laid
            }
            _ => self.lay(root.clone(), viewport, scale, at, measure),
        };
        let mut caret = false;
        let mut told = Told::default();
        if !starting {
            // The editors made for the rooms there, and what the page asked of them done; where a
            // caret an overlay stands by moved (an edit, the scrolling), the same tree is laid out
            // again, the page asked nothing.
            for pass in 0..=RELAYOUTS {
                self.editing.settle(&laid, measure, at.now);
                if !self.editing.editors.wants_layout() && !laid.wants_layout() {
                    break;
                }
                if pass == RELAYOUTS {
                    // What is left is the next frame's, which lays the page out again.
                    self.again = true;
                    break;
                }
                laid = self.lay(root.clone(), viewport, scale, at, measure);
            }
            told = self.editing.told();
            let shown = self.editing.editors.focused().is_some();
            let (lit, next) = self.editing.blink.at(at.now, at.reduce_motion, shown);
            caret = lit;
            self.blink_at = next;
        }
        let laid = Arc::new(laid);
        let draws = laid.paint(measure);
        let list = self.editing.editors.paint(draws, measure, caret);
        // The module may ask of this layout from now on, and brings what depends on it back within
        // it; a page that was already within it answers nothing. The line that stands in for a page
        // is not the module's.
        if !starting {
            self.module.lay_out(Arc::clone(&laid));
            self.laid = Some(Arc::clone(&laid));
            self.laid_for = Some((size, scale));
            self.fresh = false;
            // Where the boxes went (a spring, a glide, an ask, a list that got shorter): the page
            // builds what came into view.
            self.tell_scrolled();
            // And what the editors have for it, read against this layout.
            self.tell(told, true, false);
        }
        let mut outline = Outline::of(&laid, self.place);
        outline.add_markers(self.editing.editors.labels(&laid), self.place);
        let changed = (outline != self.outline).then(|| outline.clone());
        self.outline = outline;
        // The host has the fields this frame names once it has the outline: what replies asked of
        // them goes with the frame, a name it does not have either passed over there. A line that
        // stands in for a page names none, so what waits goes on waiting.
        let held = if starting {
            Vec::new()
        } else {
            std::mem::take(&mut self.held)
        };
        Painted {
            list,
            outline: changed,
            held,
        }
    }

    /// `root` (the line that stands in for a page, where there is none yet) laid out where the host
    /// has its boxes scrolled, measuring with the page's editors, and the boxes placed.
    fn lay(
        &mut self,
        root: Option<Element>,
        viewport: Size,
        scale: f32,
        at: FrameAt,
        measure: &mut dyn Measure,
    ) -> Laid {
        let starting = root.is_none();
        let mut root = root.unwrap_or_else(starting_line);
        self.scrolls.scrolls.apply(&mut root);
        let mut measured = self.editing.editors.measure(measure);
        let mut laid = self.motions.layout(
            root,
            viewport,
            scale,
            &mut measured,
            at.now,
            at.reduce_motion,
        );
        if !starting {
            self.scrolls
                .scrolls
                .place(&mut laid, at.now, at.reduce_motion);
        }
        laid
    }
}

/// The editors of a page, as the host's keys, input method, pasteboard and screen reader reach them
/// ([`Page::editors`]). What is done is done to the editor with the keyboard, on the page as it was
/// last laid out, and what comes of it is told the page at once; the host drains the page after it
/// ([`Page::drain`]) and draws the frame that shows it. The ranges an input method speaks of are in
/// units of UTF-16, as macOS counts them; the points are the host's.
pub struct PageEditors<'a> {
    page: &'a mut Page,
}

impl PageEditors<'_> {
    /// The editor with the keyboard.
    pub fn focused(&self) -> Option<Id> {
        self.page.state.editing.editors.focused()
    }

    /// A key pressed while an editor has the keyboard, and where it went ([`Route`]): the page's
    /// command for it first, then the editor's own, then the text. A command, or a key that is the
    /// page's, is told the page here, and what the editor does is done. The rest is the host's:
    /// `Type`, the key to its system's handling of text (an input method, which says what is
    /// typed: [`PageEditors::type_text`], [`PageEditors::compose`]); `Copy` and `Cut`, the text to
    /// its pasteboard; `Paste`, its pasteboard's text to [`PageEditors::paste`]. `NotEditor`: no
    /// editor has the keyboard, or the page is not laid out yet.
    pub fn key(&mut self, key: Key, mods: Mods) -> Route {
        let route = self
            .page
            .edit(true, |editors, laid, measure, now| {
                editors.key(laid, key, mods, measure, now)
            })
            .unwrap_or(Route::NotEditor);
        let state = &mut self.page.state;
        match route {
            Route::Command(id) => state.event(Event::Command(id)),
            Route::Page => state.event(Event::Input(Input::Key { key, mods })),
            _ => {}
        }
        route
    }

    /// The person put the keyboard in the editor named `id` some way other than a press in it (the
    /// tab key, a screen reader), or took it back to the page (`None`): the caret stays where it was.
    pub fn focus(&mut self, id: Option<Id>) {
        self.page.state.editing.person_moved_keyboard();
        self.page
            .edit(true, |editors, _, _, _| editors.set_focus(id));
    }

    /// The person selected `ranges` (each where it starts and where it ends, in characters of the
    /// document as it is) of the editor named `id`, the first the main one, some way other than the
    /// pointer and the keys — a screen reader, the host's own command — and the editor's is the
    /// keyboard.
    pub fn select(&mut self, id: Id, ranges: Vec<Range>) {
        self.page.state.editing.person_moved_keyboard();
        self.page.edit(true, |editors, laid, measure, now| {
            editors.set_focus(Some(id));
            let Some(version) = editors.buffer(id).map(Buffer::version) else {
                return;
            };
            let ask = EditorAsk::Select {
                version,
                ranges,
                main: 0,
            };
            editors.ask(laid, id, ask, measure, now);
        });
    }

    /// The person had the editor with the keyboard do `command`, as a key of its own or an entry of
    /// the host's menus does (fold the region the caret is in, undo, move by a word).
    pub fn command(&mut self, command: EditCommand) {
        self.page.edit(true, |editors, laid, measure, now| {
            if let Some(id) = editors.focused() {
                editors.ask(laid, id, EditorAsk::Command(command), measure, now);
            }
        });
    }

    /// Text typed, from the system's handling of text: in place of what an input method composed,
    /// where it did.
    pub fn type_text(&mut self, text: &str) {
        self.page.edit(true, |editors, laid, measure, now| {
            editors.type_text(laid, text, measure, now);
        });
    }

    /// The pasteboard's text, pasted.
    pub fn paste(&mut self, text: &str) {
        self.page.edit(true, |editors, laid, measure, now| {
            editors.paste(laid, text, measure, now);
        });
    }

    /// An input method composes `text`: the part of it selected (where it starts, how long), and
    /// the range of the document it replaces (`None`: what it composed before, else the selection).
    pub fn compose(
        &mut self,
        text: &str,
        selected: (usize, usize),
        replace: Option<(usize, usize)>,
    ) {
        self.page.edit(true, |editors, laid, measure, now| {
            editors.set_marked_utf16(laid, text, selected, replace, measure, now);
        });
    }

    /// The input method is done: what it composed stays, as typed.
    pub fn unmark(&mut self) {
        self.page.edit(true, |editors, laid, measure, _| {
            editors.unmark(laid, measure);
        });
    }

    /// The main selection of the editor with the keyboard: where it starts and how long it is.
    pub fn selected(&self) -> Option<(usize, usize)> {
        self.page.state.editing.editors.selected_utf16()
    }

    /// What the input method composes there, likewise.
    pub fn marked(&self) -> Option<(usize, usize)> {
        self.page.state.editing.editors.marked_utf16()
    }

    /// The text of a range of it (cut to the document).
    pub fn text(&self, start: usize, len: usize) -> Option<String> {
        self.page.state.editing.editors.text_utf16(start, len)
    }

    /// How long its text is.
    pub fn length(&self) -> Option<usize> {
        let editors = &self.page.state.editing.editors;
        let buffer = editors.buffer(editors.focused()?)?;
        buffer.utf16_of(buffer.len_chars())
    }

    /// The box, in the host's points, of the place `units` into its text: where an input method
    /// opens its window.
    pub fn rect(&mut self, units: usize) -> Option<Rect> {
        let rect = self
            .page
            .edit(false, |editors, laid, measure, _| {
                editors.rect_utf16(laid, units, measure)
            })
            .flatten()?;
        Some(self.page.state.place.to_host(rect))
    }

    /// The place of its text under `at` (the host's points), where `at` is over it.
    pub fn index_at(&mut self, at: Point) -> Option<usize> {
        let at = self.page.state.place.to_page(at);
        let (id, place) = self
            .page
            .edit(false, |editors, laid, measure, _| {
                editors.place_at(laid, at, measure)
            })
            .flatten()?;
        let editors = &self.page.state.editing.editors;
        if editors.focused() != Some(id) {
            return None;
        }
        editors.buffer(id)?.utf16_of(place)
    }

    /// What a screen reader is told of the editor named `id`, whether it has the keyboard or not:
    /// its text, and its main selection (where it starts, how long).
    pub fn contents(&self, id: Id) -> Option<(String, (usize, usize))> {
        let buffer = self.page.state.editing.editors.buffer(id)?;
        let main = buffer.main();
        let start = buffer.utf16_of(main.start())?;
        let end = buffer.utf16_of(main.end())?;
        Some((buffer.text(), (start, end.saturating_sub(start))))
    }
}

/// What stands where the page will be until the module has handed out its first.
fn starting_line() -> Element {
    div()
        .size_full()
        .items_center()
        .justify_center()
        .child(text("Starting the page\u{2026}").color(Role::Dim))
}

/// An input in the host's points as the page's: the strip's room is not the page's, and a page
/// drawn larger has fewer points.
fn into_page(input: Input, place: Placement) -> Input {
    let at = |at: Point| place.to_page(at);
    match input {
        Input::Move(here) => Input::Move(at(here)),
        Input::Press {
            at: here,
            button,
            mods,
            count,
        } => Input::Press {
            at: at(here),
            button,
            mods,
            count,
        },
        Input::Release { at: here, button } => Input::Release {
            at: at(here),
            button,
        },
        Input::Scroll { at: here, dx, dy } => Input::Scroll {
            at: at(here),
            dx: dx / place.zoom,
            dy: dy / place.zoom,
        },
        other @ (Input::Leave | Input::Key { .. } | Input::KeyUp { .. } | Input::Mods(_)) => other,
    }
}

/// A reply of the page with its points in the host's.
/// Takes out of `replies` what they ask of a field of the host's that `fields` (the outline the host
/// has) does not name yet — the texts and the keyboard — and puts it, in order, in `held`. Once
/// anything is held, what comes after it is held too, so that what a reply asked is done in the
/// order it asked it.
pub(crate) fn hold(replies: &mut [Answer], fields: &[Field], held: &mut Vec<Held>) {
    let has = |id: Id| fields.iter().any(|field| field.id == id);
    for Answer { reply, .. } in replies {
        for (field, fill) in std::mem::take(&mut reply.fill) {
            if held.is_empty() && has(field) {
                reply.fill.push((field, fill));
            } else {
                held.push(Held::Fill(field, fill));
            }
        }
        if let Some(to) = reply.keyboard {
            let waits = match to {
                Keyboard::Field(field) => !has(field),
                Keyboard::Page => false,
            };
            if waits || !held.is_empty() {
                reply.keyboard = None;
                held.push(Held::Keyboard(to));
            }
        }
    }
}

fn to_host(mut reply: Reply, place: Placement) -> Reply {
    if let Some(menu) = &mut reply.menu {
        let at = place.to_host(Rect::new(menu.at.x, menu.at.y, 0.0, 0.0));
        menu.at = Point::new(at.x, at.y);
    }
    reply
}
