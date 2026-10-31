//! What a module is held to: how much memory it may have, how long a call of it may take,
//! how large a page it may hand out — and what is said when it goes past any of them.
//!
//! The module gets **nothing it was not given**: the linker holds the page's own interface and
//! no other (no clock, no file, no network, no environment, no way to print), so a module that
//! asks for anything else does not start. What it is given it is given on terms: a store that
//! refuses to grow past [`ModuleLimits::memory`], a call that is cut off after [`ModuleLimits::call`]
//! (the module's code is compiled to check for it at every loop head and function entry), and a
//! page that is read with its size, depth and strings counted.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use std::collections::HashMap;

use pencere_view::edit::{TextSnapshot, limits as editor_limits};
use pencere_view::{Drawing, Id, KeyConvention, Laid, ScrollTo, TerminalLook, WallTime};
use wasmtime::{Config, Engine, ResourceLimiter, Store, Trap};

use crate::luau::Script;
use crate::watchdog;

/// The program that opened the page, as far as the page's speed goes: what it can take of what
/// the page says, and how the page takes what it says; and the clocks and the keyboard of the
/// application's system, which the page reads. The application gives it; the sandbox only asks.
pub trait Program: Send + Sync + 'static {
    /// The module wants to say `bytes` to its program (a message): whether the program has the
    /// room for it. When not, the module is told *full* and keeps the message, and the
    /// application calls it again (`Module::room`) once the program has read enough. From the
    /// module's thread.
    ///
    /// `bytes` is the size of the message as the protocol counts it, with its escapes undone
    /// (`pencere_wire::json::unescaped_len`): what the application counts of the same message
    /// when it takes it from [`crate::Update::sent`] is to be counted so, or the two never meet.
    fn may_say(&self, bytes: usize) -> bool;

    /// The module has taken a message of `bytes` the program said: the program may say more.
    /// From the module's thread, after the page was told. `bytes` is counted as for
    /// [`Program::may_say`], from the text the application gave [`crate::Module::message`].
    fn taken(&self, bytes: usize);

    /// The application's clock now, in the seconds the module is given as `now` for a frame, if
    /// the application keeps one that can be read from any thread (`None` when it does not, as it
    /// need not: this is the default).
    ///
    /// A page that is made again because its program said something (and not because the
    /// application asked for a frame) is made for the time it is. A program speaks on its own
    /// clock, long after any frame, and a page that tells the time by `now` (how long ago a reading
    /// came) is not to be handed the time of a frame long gone. With no clock of the application's
    /// to read, the time is that of the last frame and what the system's clock says has passed
    /// since, which is right as long as the application's clock goes as the system's does; an
    /// application whose clock does not (one that stands still, one that is driven by hand) says
    /// what it reads here. From the module's thread; a time that is not a number, or is earlier
    /// than the last frame's, is not taken for one.
    fn now(&self) -> Option<f64> {
        None
    }

    /// The time of day now, as the application's system says it: what the page reads as
    /// `pencere.time.wall()` (the interface's `system.wall-clock`), as often as it asks. By default
    /// the system's clock, in UTC: the sandbox knows no time zone, and an application that knows
    /// the person's (every host with a screen) says its offset here. From the module's thread.
    fn wall_clock(&self) -> WallTime {
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0.0, |since| since.as_secs_f64());
        WallTime {
            seconds,
            utc_offset: 0,
        }
    }

    /// How the application's terminal draws its cursor and its selection, as the person set them
    /// (the interface's `system.terminal-look`; what the page reads as `pencere.terminal_look()`), in
    /// the words and units of an editor's options, a field only where the application has such a
    /// setting: by default none, which a page takes for an application with no such settings. Read
    /// as it is asked, so it is what the person has set now; an application that changes it tells
    /// the page (`Event::TerminalLook`). From the module's thread: not from the thread the
    /// application's settings are changed on.
    fn terminal_look(&self) -> TerminalLook {
        TerminalLook::default()
    }

    /// What the application draws that not every application does (the interface's `system.draws`):
    /// by default none, which a page takes for an application that draws only what every one
    /// does. The same every time it is asked. From the module's thread.
    fn draws(&self) -> Vec<Drawing> {
        Vec::new()
    }

    /// How the application's system makes and writes its keyboard shortcuts (the interface's
    /// `system.keyboard`): by default the system this is built for. From the module's thread.
    fn keyboard(&self) -> KeyConvention {
        if cfg!(target_os = "macos") || cfg!(target_os = "ios") {
            KeyConvention::Mac
        } else if cfg!(target_os = "windows") {
            KeyConvention::Windows
        } else {
            KeyConvention::Linux
        }
    }
}

/// A program that can always be told, and that is never made to wait for the page.
#[derive(Clone, Copy, Debug, Default)]
pub struct Unbounded;

impl Program for Unbounded {
    fn may_say(&self, _bytes: usize) -> bool {
        true
    }

    fn taken(&self, _bytes: usize) {}
}

/// What a module is held to.
///
/// There are two sets of them. [`ModuleLimits::floor`] is what the protocol says every host gives
/// at least, and so what an author's page is written to and tested against (`pencere test`, the
/// headless host). [`ModuleLimits::default`] is what a terminal gives: the floor, with more time
/// to a call and to the start. Time is the wall clock's, and the wall clock of a busy machine is no
/// measure of a page — one that is compiling, or whose other windows are drawing, takes several
/// times as long to do the same — while the page runs on a thread of its own and the drawing never
/// waits for it, so a short time protects nobody's smoothness: it is there to stop a page that
/// does not come back at all.
#[derive(Clone, Debug)]
pub struct ModuleLimits {
    /// Bytes of memory and of tables the module may hold in all.
    pub memory: usize,
    /// How long one call (a frame, a settle, an event) may take: 250 ms at the floor.
    pub call: Duration,
    /// How long the module may take to start (its first code to run): 2 s at the floor.
    pub start: Duration,
    /// How long compiling the module may take. A component is code from a program, and what it
    /// takes to compile is the author's to make long; past this the module is refused (the
    /// compiling goes on, on a thread of its own, until it is done, and its result is dropped).
    pub compile: Duration,
    /// Elements in one frame.
    pub nodes: usize,
    /// Bytes in all that one frame (or one menu) says apart from its elements' fixed fields: the
    /// words of its lines of text and pictures' paths, four bytes a value of a sparkline, what a
    /// track takes. Counted each time an element names a part, so that parts that overlap are not
    /// free.
    pub frame_bytes: usize,
    /// Depth of the tree of one frame (the root is 1).
    pub depth: usize,
    /// Bytes of one line of text, of a title, of text for the pasteboard.
    pub text: usize,
    /// Tracks of a grid's columns or rows.
    pub tracks: usize,
    /// Values of a sparkline, steps of a path.
    pub values: usize,
    /// Parts of a text of many looks, paths of a canvas, commands of an element.
    pub parts: usize,
    /// Entries of a menu.
    pub menu: usize,
    /// Bytes of the component itself: a larger one is not compiled. For a script, bytes of the
    /// scripts together: the entry, and every script it requires, once each.
    pub component: usize,
    /// Messages to the program one call may send: a work limit, as the bytes of one are. How
    /// many the program can take is not this (see [`Program`]).
    pub messages: usize,
    /// Bytes of one message to the program: the protocol's own limit for one (1 MiB).
    pub message: usize,
    /// Bytes of text one reply may hand the page's editors (its loads, changes, names of layers
    /// and what it finds, together), and the most one query of an editor's text answers with: the
    /// floor of the specification, an editor's own (`pencere_view::edit::limits`).
    pub editor_text: usize,
    /// Decorations one reply may send the page's editors.
    pub editor_decorations: usize,
    /// What one reply may ask of the page's editors: asks, and changes, ranges or decorations of
    /// one of them besides.
    pub editor_asks: usize,
}

impl ModuleLimits {
    /// What every host gives a page at least (the protocol's floors, Section 9.8): the limits an
    /// author's page is held to wherever it is tried, and the ones a test of it should use.
    pub fn floor() -> Self {
        Self {
            memory: 64 << 20,
            call: Duration::from_millis(250),
            start: Duration::from_secs(2),
            compile: Duration::from_secs(20),
            nodes: 20_000,
            frame_bytes: 4 << 20,
            depth: 128,
            text: 64 << 10,
            tracks: 64,
            values: 4096,
            parts: 1024,
            menu: 256,
            component: 4 << 20,
            messages: 32,
            message: 1 << 20,
            editor_text: editor_limits::REPLY_TEXT_BYTES,
            editor_decorations: editor_limits::REPLY_DECORATIONS,
            editor_asks: 4096,
        }
    }

    /// How long a call held to `limit` may take.
    pub(crate) fn time(&self, limit: TimeLimit) -> Duration {
        match limit {
            TimeLimit::Call => self.call,
            TimeLimit::Start => self.start,
        }
    }
}

/// What a terminal gives a page: the floor, with the time a busy machine may need — 5 s to a call
/// and 10 s to the start. A page cut off at this is not slow, it has not come back.
impl Default for ModuleLimits {
    fn default() -> Self {
        Self {
            call: Duration::from_secs(5),
            start: Duration::from_secs(10),
            ..Self::floor()
        }
    }
}

/// Which of the times of [`ModuleLimits`] a call is held to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeLimit {
    /// One call: a frame, a settle, an event ([`ModuleLimits::call`]).
    Call,
    /// The start of the module ([`ModuleLimits::start`]).
    Start,
}

/// A time as a person reads it: `5 s`, `250 ms`.
fn said(time: Duration) -> String {
    if time.subsec_nanos() == 0 {
        format!("{} s", time.as_secs())
    } else if time.subsec_nanos() % 1_000_000 == 0 {
        format!("{} ms", time.as_millis())
    } else {
        format!("{time:?}")
    }
}

/// Why a module is gone. Each reads as a sentence on its own: it is what the program of the page is
/// told, and what the person who started the program may be shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fault {
    /// The bytes are not a component, or it needs more than the page's interface.
    Load(String),
    /// A call, or the start, did not come back in `within`: the time `limit` allows.
    Timeout { limit: TimeLimit, within: Duration },
    /// The module wanted more memory than it may have.
    Memory,
    /// The module trapped (an unreachable, a panic, a stack that ran out, a bad access).
    Trap(String),
    /// The module handed out something that is not a page, or one too large.
    Malformed(String),
    /// The module said more to its program in one call than a call may, or a message too long.
    Flooded(String),
    /// The module said it cannot go on, and why (`diagnostics.fail`): for a script, the error
    /// with the line it is on.
    Failed(String),
}

impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Load(why) => write!(f, "the page could not be loaded: {why}"),
            Self::Timeout {
                limit: TimeLimit::Call,
                within,
            } => write!(
                f,
                "the page did not come back within {} (a call: a frame, a settle, an event)",
                said(*within)
            ),
            Self::Timeout {
                limit: TimeLimit::Start,
                within,
            } => write!(
                f,
                "the page did not finish starting within {}",
                said(*within)
            ),
            Self::Memory => f.write_str("the page asked for more memory than it may have"),
            Self::Trap(why) => write!(f, "the page trapped: {why}"),
            Self::Malformed(why) => {
                write!(f, "the page handed out something that is not a page: {why}")
            }
            Self::Flooded(why) => write!(f, "the page said too much to its program: {why}"),
            Self::Failed(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for Fault {}

/// Refuses what the module may not have, and remembers that it did. What it may have is one
/// sum over every memory and table it holds (a module may hold several of each): each one under
/// the limit on its own would add up past it.
pub(crate) struct Limiter {
    memory: usize,
    /// Bytes of memory and of tables the module holds now.
    held: usize,
    pub(crate) exceeded: bool,
}

impl Limiter {
    pub(crate) fn new(memory: usize) -> Self {
        Self {
            memory,
            held: 0,
            exceeded: false,
        }
    }

    /// Whether growing a memory or a table from `current` to `desired` bytes keeps the sum
    /// within the limit; the sum counts it if so.
    fn grow(&mut self, current: usize, desired: usize) -> bool {
        let held = self.held.saturating_sub(current).saturating_add(desired);
        if held > self.memory {
            self.exceeded = true;
            return false;
        }
        self.held = held;
        true
    }
}

impl ResourceLimiter for Limiter {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        if !self.grow(current, desired) {
            // A trap and not a refusal: a module that is told "no" would go on asking.
            return Err(wasmtime::Error::msg("the module's memory limit"));
        }
        Ok(true)
    }

    fn table_growing(
        &mut self,
        current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        // A table's elements are pointers; their share of the memory is counted as eight bytes.
        if !self.grow(current.saturating_mul(8), desired.saturating_mul(8)) {
            return Err(wasmtime::Error::msg("the module's table limit"));
        }
        Ok(true)
    }

    fn instances(&self) -> usize {
        16
    }

    fn tables(&self) -> usize {
        8
    }

    fn memories(&self) -> usize {
        4
    }
}

/// What a store holds besides the module: the limits, the layout the module may ask of, and
/// what it has said to its program since the application last took it.
pub(crate) struct Host {
    pub(crate) limiter: Limiter,
    pub(crate) laid: Option<Arc<Laid>>,
    /// The text of each of the page's editors, as the application last posted it.
    pub(crate) texts: HashMap<Id, Arc<dyn TextSnapshot>>,
    /// The most bytes one query of an editor's text answers with.
    pub(crate) query_text: usize,
    /// What the module sent in the call that is running or ran last.
    pub(crate) sent: Vec<String>,
    /// The boxes the module asked to be scrolled since the application last took them.
    pub(crate) asks: Vec<ScrollTo>,
    /// What one call may send: messages, and bytes of one.
    pub(crate) allowed: (usize, usize),
    /// A call sent more than it may: the application closes the module when it comes back.
    pub(crate) overflowed: bool,
    /// The program the module is for: whether it has the room for what the module says.
    pub(crate) program: Arc<dyn Program>,
    /// What the module said when it could not go on (`diagnostics.fail`), the first time it did.
    pub(crate) failed: Option<String>,
    /// The script a module that is an interpreter runs (`script.source`), where it is in its
    /// bundle (`script.path`) and the bundle's other scripts (`script.module`).
    pub(crate) script: Option<Script>,
    /// Bytes of script given to the module so far, the entry's included.
    pub(crate) given: usize,
    /// The most bytes of script the module may be given in all.
    pub(crate) most: usize,
}

impl Host {
    pub(crate) fn new(limits: &ModuleLimits, program: Arc<dyn Program>) -> Self {
        Self {
            limiter: Limiter::new(limits.memory),
            laid: None,
            texts: HashMap::new(),
            query_text: limits.editor_text,
            sent: Vec::new(),
            asks: Vec::new(),
            allowed: (limits.messages, limits.message),
            overflowed: false,
            program,
            failed: None,
            script: None,
            given: 0,
            most: limits.component,
        }
    }
}

/// An engine for modules: compiled to be interrupted by the epoch, speaking the component
/// model and the exceptions of the interpreters, with nothing of the host's to offer. Every
/// engine is made the same way: a component compiled by one can be run by another
/// ([`crate::luau`]).
pub(crate) fn engine() -> Result<Engine, Fault> {
    let mut config = Config::new();
    config.epoch_interruption(true);
    config.wasm_component_model(true);
    config.wasm_exceptions(true);
    Engine::new(&config).map_err(|why| Fault::Load(format!("{why:#}")))
}

/// A store for one module under `limits`; `script` is what a module that runs one is given.
pub(crate) fn store(
    engine: &Engine,
    limits: &ModuleLimits,
    program: Arc<dyn Program>,
    script: Option<Script>,
) -> Store<Host> {
    let mut host = Host::new(limits, program);
    host.given = script.as_ref().map_or(0, |script| script.source.len());
    host.script = script;
    let mut store = Store::new(engine, host);
    store.limiter(|host| &mut host.limiter);
    store
}

/// Runs `body` on `store` with the time `limit` of `limits` to come back; what it returns is what
/// the module said, and a failure is told for what it was.
pub(crate) fn call<R>(
    store: &mut Store<Host>,
    limits: &ModuleLimits,
    limit: TimeLimit,
    body: impl FnOnce(&mut Store<Host>) -> wasmtime::Result<R>,
) -> Result<R, Fault> {
    let within = limits.time(limit);
    // The epoch moves once, `within` from now, if the call is still running.
    store.set_epoch_deadline(1);
    let guard = watchdog::arm(store.engine(), within);
    let outcome = body(store);
    drop(guard);
    match outcome {
        // A module that said it cannot go on is gone whatever it handed out after: what it
        // returned is not read.
        Ok(value) => match store.data_mut().failed.take() {
            Some(why) => Err(Fault::Failed(why)),
            None => Ok(value),
        },
        Err(why) => Err(fault_of(store, limit, within, &why)),
    }
}

fn fault_of(
    store: &Store<Host>,
    limit: TimeLimit,
    within: Duration,
    why: &wasmtime::Error,
) -> Fault {
    if store.data().limiter.exceeded {
        return Fault::Memory;
    }
    if matches!(why.downcast_ref::<Trap>(), Some(Trap::Interrupt)) {
        return Fault::Timeout { limit, within };
    }
    // What it said before it stopped is the reason, the trap only how.
    if let Some(said) = store.data().failed.clone() {
        return Fault::Failed(said);
    }
    match why.downcast_ref::<Trap>() {
        Some(trap) => Fault::Trap(trap.to_string()),
        None => Fault::Trap(format!("{why:#}")),
    }
}

/// What a module said when it could not go on, fit to show a person: the control characters
/// (a terminal would act on them) are spaces, and it is cut to [`MAX_FAILURE`] characters.
pub(crate) fn failure_words(message: &str) -> String {
    message
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(MAX_FAILURE)
        .collect::<String>()
        .trim()
        .to_owned()
}

/// The most characters of the words a module says it failed with.
pub(crate) const MAX_FAILURE: usize = 1000;

#[cfg(test)]
mod tests {
    use super::*;
    use wasmtime::component::{Component, Linker};

    /// A component with one function, `run`, that does `body`.
    fn run(wat_body: &str, limits: &ModuleLimits) -> Result<(), Fault> {
        let engine = engine()?;
        let source = format!(
            r#"(component
                (core module $m
                    (memory (export "memory") 1)
                    (func (export "run") {wat_body}))
                (core instance $i (instantiate $m))
                (func (export "run") (canon lift (core func $i "run"))))"#
        );
        let component =
            Component::new(&engine, source).map_err(|why| Fault::Load(format!("{why:#}")))?;
        let mut store = store(&engine, limits, Arc::new(Unbounded), None);
        let linker = Linker::<Host>::new(&engine);
        let instance = call(&mut store, limits, TimeLimit::Start, |store| {
            linker.instantiate(store, &component)
        })?;
        let run = instance
            .get_typed_func::<(), ()>(&mut store, "run")
            .map_err(|why| Fault::Load(format!("{why:#}")))?;
        call(&mut store, limits, TimeLimit::Call, |store| {
            run.call(store, ())
        })
    }

    fn quick() -> ModuleLimits {
        ModuleLimits {
            call: Duration::from_millis(50),
            memory: 4 << 20,
            ..ModuleLimits::default()
        }
    }

    #[test]
    fn a_call_that_returns_is_not_touched() {
        assert_eq!(run("nop", &quick()), Ok(()));
    }

    #[test]
    fn a_module_that_never_returns_is_cut_off_after_its_time() {
        let began = std::time::Instant::now();
        assert_eq!(
            run("(loop $l (br $l))", &quick()),
            Err(Fault::Timeout {
                limit: TimeLimit::Call,
                within: Duration::from_millis(50)
            })
        );
        assert!(
            began.elapsed() < Duration::from_secs(5),
            "cut off in {:?}",
            began.elapsed()
        );
    }

    #[test]
    fn a_module_that_asks_for_more_memory_than_it_may_have_is_stopped() {
        // Sixty-four pages (4 MiB) at a time, forever.
        let grow = "(loop $l (drop (memory.grow (i32.const 64))) (br $l))";
        assert_eq!(run(grow, &quick()), Err(Fault::Memory));
    }

    #[test]
    fn a_module_that_traps_is_told_so_and_a_module_that_recurses_runs_out_of_stack() {
        assert!(matches!(run("unreachable", &quick()), Err(Fault::Trap(_))));
        let recursion = r#"(call 0)"#;
        // `run` is the component's function 0 of the core module: it calls itself.
        assert!(matches!(run(recursion, &quick()), Err(Fault::Trap(_))));
    }

    #[test]
    fn a_module_that_wants_something_the_page_does_not_offer_does_not_start() {
        let engine = engine().expect("engine");
        let component = Component::new(
            &engine,
            r#"(component
                (import "wasi:cli/environment@0.2.0" (instance (export "get-environment" (func))))
                (core module $m (func (export "run")))
                (core instance $i (instantiate $m))
                (func (export "run") (canon lift (core func $i "run"))))"#,
        )
        .expect("a component");
        let limits = quick();
        let mut store = store(&engine, &limits, Arc::new(Unbounded), None);
        let linker = Linker::<Host>::new(&engine);
        let why = linker
            .instantiate(&mut store, &component)
            .expect_err("an import nobody gave");
        assert!(
            format!("{why:#}").contains("wasi:cli/environment"),
            "{why:#}"
        );
    }

    #[test]
    fn a_module_that_never_starts_is_cut_off_after_the_time_to_start_and_is_told_so() {
        let engine = engine().expect("an engine");
        let component = Component::new(
            &engine,
            r#"(component
                (core module $m
                    (func $s (loop $l (br $l)))
                    (start $s))
                (core instance $i (instantiate $m)))"#,
        )
        .expect("a component");
        let limits = ModuleLimits {
            start: Duration::from_millis(50),
            call: Duration::from_secs(60),
            ..ModuleLimits::default()
        };
        let mut store = store(&engine, &limits, Arc::new(Unbounded), None);
        let linker = Linker::<Host>::new(&engine);
        let fault = call(&mut store, &limits, TimeLimit::Start, |store| {
            linker.instantiate(store, &component)
        })
        .err();
        assert_eq!(
            fault,
            Some(Fault::Timeout {
                limit: TimeLimit::Start,
                within: Duration::from_millis(50)
            })
        );
    }

    #[test]
    fn a_timeout_says_which_time_it_was_and_how_long() {
        let said = |limit, within| Fault::Timeout { limit, within }.to_string();
        assert_eq!(
            said(TimeLimit::Call, Duration::from_secs(5)),
            "the page did not come back within 5 s (a call: a frame, a settle, an event)"
        );
        assert_eq!(
            said(TimeLimit::Call, Duration::from_millis(250)),
            "the page did not come back within 250 ms (a call: a frame, a settle, an event)"
        );
        assert_eq!(
            said(TimeLimit::Start, Duration::from_secs(10)),
            "the page did not finish starting within 10 s"
        );
    }

    #[test]
    fn the_floor_is_the_protocols_and_what_a_terminal_gives_is_at_least_that() {
        let floor = ModuleLimits::floor();
        assert_eq!(floor.memory, 64 << 20);
        assert_eq!(floor.call, Duration::from_millis(250));
        assert_eq!(floor.start, Duration::from_secs(2));
        assert_eq!(floor.nodes, 20_000);
        assert_eq!(floor.depth, 128);
        assert_eq!(floor.text, 64 << 10);
        assert_eq!(floor.component, 4 << 20);
        assert_eq!(floor.messages, 32);
        assert_eq!(floor.message, 1 << 20);
        // A field added to the limits is a field to compare: the destructuring does not compile
        // without it.
        let ModuleLimits {
            memory,
            call,
            start,
            compile,
            nodes,
            frame_bytes,
            depth,
            text,
            tracks,
            values,
            parts,
            menu,
            component,
            messages,
            message,
            editor_text,
            editor_decorations,
            editor_asks,
        } = ModuleLimits::default();
        assert!(memory >= floor.memory);
        assert!(call >= floor.call);
        assert!(start >= floor.start);
        assert!(compile >= floor.compile);
        assert!(nodes >= floor.nodes);
        assert!(frame_bytes >= floor.frame_bytes);
        assert!(depth >= floor.depth);
        assert!(text >= floor.text);
        assert!(tracks >= floor.tracks);
        assert!(values >= floor.values);
        assert!(parts >= floor.parts);
        assert!(menu >= floor.menu);
        assert!(component >= floor.component);
        assert!(messages >= floor.messages);
        assert!(message >= floor.message);
        assert!(editor_text >= floor.editor_text);
        assert!(editor_decorations >= floor.editor_decorations);
        assert!(editor_asks >= floor.editor_asks);
        // What a terminal gives beyond the floor is time.
        assert_eq!(call, Duration::from_secs(5));
        assert_eq!(start, Duration::from_secs(10));
    }
}
