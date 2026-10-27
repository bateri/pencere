//! A page module on a thread of its own, and the mailbox between it and the application.
//!
//! **Nothing of the module runs on the thread that draws.** The thread that owns the module
//! compiles it, starts it and calls it; the application only posts what it wants done (a frame
//! for this size, this event, this layout) and later takes what came out (the page, the replies)
//! — neither ever waits for the module. A module that does not come back is cut off where it is
//! ([`crate::ModuleLimits`]), its thread ends and the application is told why; the application never
//! hung with it.
//!
//! The posting is cheap and loses nothing that matters: a frame asked for while an older one is
//! still wanted replaces it (only the newest is made), a run of pointer moves is one move, a run of
//! wheel turns over one point is one turn by all of them, no other event is ever dropped. The
//! application is woken once per batch of what is waiting ([`Module::spawn`]'s `wake`), not once
//! per item.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use pencere_view::{Element, Event, Input, Laid, Reply, Size};
use pencere_wire::json::unescaped_len;

use crate::luau::Script;
use crate::sandbox::{Fault, ModuleLimits, Program};
use crate::world::World;

/// A page the module handed out: the tree, and whether the page is still moving.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub root: Element,
    /// The page is still moving: the application asks for the next frame itself until this
    /// says no.
    pub animating: bool,
    /// When the page wants to be asked for again if nothing happens before then, seconds on the
    /// clock `render` was given; always a finite time.
    pub wake_at: Option<f64>,
}

/// What a module can be asked, whatever it is made of. The sandbox is one; the tests have a
/// plain one.
pub trait Runner: Send + 'static {
    /// The page for a viewport of `size` points at `now` seconds.
    fn render(&mut self, size: Size, now: f64, reduce_motion: bool) -> Result<Frame, Fault>;
    /// Brings what depends on the layout back within it; whether the page changed.
    fn settle(&mut self) -> Result<bool, Fault>;
    /// The person did something.
    fn handle(&mut self, event: Event) -> Result<Reply, Fault>;
    /// The layout the module may ask of from now on.
    fn lay_out(&mut self, laid: Arc<Laid>);
    /// The program that opened the page said something (a JSON object, as written).
    fn message(&mut self, _text: String) -> Result<Reply, Fault> {
        Ok(Reply::none())
    }
    /// The program has read enough that what the module was told *full* to can be sent: called
    /// once for each time the module was told so.
    fn room(&mut self) -> Result<Reply, Fault> {
        Ok(Reply::none())
    }

    /// What the module has said to its program since the last taking.
    fn take_sent(&mut self) -> Vec<String> {
        Vec::new()
    }
}

enum Command {
    Render {
        size: Size,
        now: f64,
        reduce_motion: bool,
    },
    Settle,
    Laid(Arc<Laid>),
    Event(Event),
    Message(String),
    /// The program has read enough that the module may say what it kept.
    Room,
    Stop,
}

/// What the module said to an event, and whether the event was the pointer's (a move, a
/// press, the wheel): the pointer's shape after a key is nobody's to set.
#[derive(Clone, Debug, PartialEq)]
pub struct Answer {
    pub reply: Reply,
    pub pointer: bool,
}

/// What came out of the module and has not been taken.
#[derive(Default)]
struct Mailbox {
    waiting: Mutex<Waiting>,
    /// Told when a page or the module's end is put out ([`Module::wait_for_frame`]).
    arrived: Condvar,
    /// The application has been woken and has not taken what is waiting yet.
    woken: AtomicBool,
}

#[derive(Default)]
struct Waiting {
    frame: Option<Frame>,
    /// The size the waiting page was made for.
    frame_size: Option<Size>,
    replies: Vec<Answer>,
    sent: Vec<String>,
    closed: Option<Fault>,
}

/// What the application takes from the mailbox.
#[derive(Debug, Default, PartialEq)]
pub struct Update {
    /// The newest page since the last taking, if there is one.
    pub frame: Option<Frame>,
    /// What the module said to the events it was told, in order.
    pub replies: Vec<Answer>,
    /// What the module said to its program, in order: JSON objects, as written, not read.
    pub sent: Vec<String>,
    /// Why the module is gone, once it is. Told again on every taking after.
    pub closed: Option<Fault>,
}

/// A page module, on its thread.
pub struct Module {
    commands: Sender<Command>,
    mailbox: Arc<Mailbox>,
}

impl Module {
    /// Starts a module made by `make` on a thread of its own, and returns at once. `make` runs
    /// on that thread (compiling a component is slow); if it fails, the module is closed with
    /// the failure. `wake` is called, from the module's thread, when something is waiting to be
    /// taken ([`Module::poll`]): it hops to wherever the application listens and returns.
    pub fn spawn<R: Runner>(
        make: impl FnOnce() -> Result<R, Fault> + Send + 'static,
        program: Arc<dyn Program>,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        let (commands, inbox) = channel();
        let mailbox = Arc::new(Mailbox::default());
        let posting = Arc::clone(&mailbox);
        let spawned = std::thread::Builder::new()
            .name("view-module".into())
            .spawn(move || {
                let post = Post {
                    mailbox: &posting,
                    wake: &wake,
                };
                let outcome =
                    catch_unwind(AssertUnwindSafe(|| serve(make, &*program, &inbox, &post)));
                let ended = match outcome {
                    Ok(Ok(())) => return,
                    Ok(Err(fault)) => fault,
                    Err(_) => Fault::Trap("the host side of the module failed".into()),
                };
                post.close(ended);
            });
        if spawned.is_err() {
            mailbox.close_without_wake(Fault::Load("could not start a thread".into()));
        }
        Self { commands, mailbox }
    }

    /// Starts the component in `bytes` (binary or text) under `limits`, for `program`.
    pub fn from_component(
        bytes: Vec<u8>,
        limits: ModuleLimits,
        program: Arc<dyn Program>,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        let held = Arc::clone(&program);
        Self::spawn(move || World::load(&bytes, limits, held), program, wake)
    }

    /// Starts the interpreter on `script` (a page that is a script, not a component) under
    /// `limits`, for `program`: the same sandbox, the same terms.
    pub fn from_script(
        script: Script,
        limits: ModuleLimits,
        program: Arc<dyn Program>,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        let held = Arc::clone(&program);
        Self::spawn(
            move || World::load_script(script, limits, held),
            program,
            wake,
        )
    }

    /// Asks for the page for a viewport of `size` points at `now` seconds. Returns at once;
    /// the page comes with the next taking.
    pub fn render(&self, size: Size, now: f64, reduce_motion: bool) {
        let _ = self.commands.send(Command::Render {
            size,
            now,
            reduce_motion,
        });
    }

    /// Tells the module what the person did. Returns at once.
    pub fn event(&self, event: Event) {
        let _ = self.commands.send(Command::Event(event));
    }

    /// Tells the module what its program said (a JSON object, as written). Returns at once, and
    /// never waits: the caller is the application's thread. What the program says is held to
    /// what the module takes by the application, which stops reading the program's output
    /// while the module has not taken the last of it ([`Program::taken`]) — this queue is as
    /// long as that leaves it.
    pub fn message(&self, text: String) {
        let _ = self.commands.send(Command::Message(text));
    }

    /// Tells the module the program has read enough for it to say what it was told *full* to
    /// ([`Program::may_say`]). Returns at once.
    pub fn room(&self) {
        let _ = self.commands.send(Command::Room);
    }

    /// Tells the module the page was laid out as `laid`: it may ask of it from now on, and is
    /// asked to bring what depends on it back within it.
    pub fn lay_out(&self, laid: Arc<Laid>) {
        let _ = self.commands.send(Command::Laid(laid));
        let _ = self.commands.send(Command::Settle);
    }

    /// Waits until a page made for `size` that the application has not taken is put out, the
    /// module is gone, or `within` has passed; whether such a page is waiting then. For the one
    /// moment a host had better wait a little than draw a page made for another size: the pane
    /// was resized, and a page laid out for the size it was would be drawn for a frame before the
    /// one for its new size comes. A page made for the size it was (one the module made again for
    /// a hover just before) does not end the wait. The wait is held to `within`, so a page that is
    /// slow costs that and no more.
    pub fn wait_for_frame(&self, size: Size, within: Duration) -> bool {
        let waiting = self
            .mailbox
            .waiting
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let made_for =
            |waiting: &Waiting| waiting.frame.is_some() && waiting.frame_size == Some(size);
        let (waiting, _) = self
            .mailbox
            .arrived
            .wait_timeout_while(waiting, within, |waiting| {
                !made_for(waiting) && waiting.closed.is_none()
            })
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        made_for(&waiting)
    }

    /// Takes what the module has put out since the last taking. Never waits.
    pub fn poll(&self) -> Update {
        // Before the taking, so that what is posted while it happens wakes the application again.
        self.mailbox.woken.store(false, Ordering::SeqCst);
        let mut waiting = self
            .mailbox
            .waiting
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Update {
            frame: waiting.frame.take(),
            replies: std::mem::take(&mut waiting.replies),
            sent: std::mem::take(&mut waiting.sent),
            closed: waiting.closed.clone(),
        }
    }
}

impl Drop for Module {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Stop);
    }
}

impl Mailbox {
    fn close_without_wake(&self, fault: Fault) {
        let mut waiting = self
            .waiting
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        waiting.closed.get_or_insert(fault);
    }
}

/// The module's side of the mailbox: puts out, wakes.
struct Post<'a> {
    mailbox: &'a Mailbox,
    wake: &'a dyn Fn(),
}

impl Post<'_> {
    fn put(&self, frame: Option<(Frame, Size)>, answer: Option<Answer>) {
        {
            let mut waiting = self
                .mailbox
                .waiting
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some((frame, size)) = frame {
                waiting.frame = Some(frame);
                waiting.frame_size = Some(size);
                self.mailbox.arrived.notify_all();
            }
            if let Some(answer) = answer {
                waiting.replies.push(answer);
            }
        }
        self.wake();
    }

    /// What the module said to its program: kept in order, and the application woken.
    fn say(&self, sent: Vec<String>) {
        if sent.is_empty() {
            return;
        }
        self.mailbox
            .waiting
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .sent
            .extend(sent);
        self.wake();
    }

    fn close(&self, fault: Fault) {
        self.mailbox.close_without_wake(fault);
        self.mailbox.arrived.notify_all();
        self.wake();
    }

    /// Once per taking: a wake that is already on its way covers what is posted after it.
    fn wake(&self) {
        if !self.mailbox.woken.swap(true, Ordering::SeqCst) {
            (self.wake)();
        }
    }
}

/// What the page was last made for, and when. A page the module makes again by itself is made
/// for the same viewport. What the person did is answered at the time the page was last made for,
/// the one the person saw; what the program said, at the time that has passed since — a program
/// speaks on its own clock, long after any frame, and a page that tells the time by `now` (how
/// long ago a reading came) is not handed the time of a frame long gone. The page that was made
/// for what the program said is made for a time, and what follows it is made for that time or a
/// later one.
#[derive(Clone, Copy)]
struct Asked {
    size: Size,
    now: f64,
    reduce_motion: bool,
    at: Instant,
}

impl Asked {
    /// The time on the application's clock now: its own reading if it has a clock to read
    /// ([`Program::now`]), otherwise the time it gave for the frame and what the system's clock
    /// says has passed since. Never earlier than the frame.
    fn since(&self, program: &dyn Program) -> f64 {
        program
            .now()
            .filter(|now| now.is_finite())
            .unwrap_or_else(|| self.now + self.at.elapsed().as_secs_f64())
            .max(self.now)
    }
}

/// The module's life: made, then asked until it is stopped or gone.
fn serve<R: Runner>(
    make: impl FnOnce() -> Result<R, Fault>,
    program: &dyn Program,
    inbox: &Receiver<Command>,
    post: &Post<'_>,
) -> Result<(), Fault> {
    let mut runner = make()?;
    // What the page was last made for: what an event that changed it makes it again for.
    let mut last: Option<Asked> = None;
    // How many times in a row a settle has asked for the page again without anyone else asking
    // for anything: a module that always wants another page does not make an endless chain of them.
    let mut settled = 0_u8;
    // The module is up: the application hears of it with the first page.
    loop {
        let Ok(first) = inbox.recv() else {
            return Ok(());
        };
        let mut batch = vec![first];
        while let Ok(more) = inbox.try_recv() {
            batch.push(more);
        }
        // The program said something that changed the page: one page for all it said in this
        // batch, made after the last of it (a program that pushes fast does not make the
        // module build a page for every push).
        let mut owed = false;
        // What the wheel turns that were run into the next one came to, points across and down.
        let mut wheel = (0.0_f32, 0.0_f32);
        for (at, command) in batch.iter().enumerate() {
            let later = batch.get(at + 1..).unwrap_or_default();
            match command {
                Command::Stop => return Ok(()),
                Command::Laid(laid) => runner.lay_out(Arc::clone(laid)),
                // Only the newest of the pages asked for is made.
                Command::Render { .. } if later.iter().any(is_render) => {}
                Command::Render {
                    size,
                    now,
                    reduce_motion,
                } => {
                    // What the page is made for again, by an event or by what its program said,
                    // is a time that only rises: a page that tells how long ago something was
                    // (by `now`) is not handed an earlier time than one it has already seen,
                    // whichever of the application's frame and its own reading of the clock was
                    // behind the other. The frame itself is made for the time it was asked for.
                    let kept = last.map_or(*now, |before| before.now);
                    last = Some(Asked {
                        size: *size,
                        now: if now.is_finite() { now.max(kept) } else { kept },
                        reduce_motion: *reduce_motion,
                        at: Instant::now(),
                    });
                    settled = 0;
                    let frame = runner.render(*size, *now, *reduce_motion)?;
                    post.say(runner.take_sent());
                    post.put(Some((frame, *size)), None);
                }
                Command::Settle if later.iter().any(|c| matches!(c, Command::Settle)) => {}
                Command::Settle => {
                    if runner.settle()?
                        && settled < SETTLES
                        && let Some(asked) = last
                    {
                        settled += 1;
                        let frame = runner.render(asked.size, asked.now, asked.reduce_motion)?;
                        post.put(Some((frame, asked.size)), None);
                    }
                    post.say(runner.take_sent());
                }
                // A run of pointer moves is the last of them.
                Command::Event(Event::Input(Input::Move(_)))
                    if matches!(
                        later.first(),
                        Some(Command::Event(Event::Input(Input::Move(_))))
                    ) => {}
                // A run of wheel turns over one point is one turn by all of them: a page slower
                // to make than the wheel turns does not fall behind it, a page for each turn.
                Command::Event(Event::Input(Input::Scroll { at: here, dx, dy }))
                    if matches!(
                        later.first(),
                        Some(Command::Event(Event::Input(Input::Scroll { at, .. }))) if at == here
                    ) =>
                {
                    wheel = (wheel.0 + dx, wheel.1 + dy);
                }
                Command::Event(event) => {
                    settled = 0;
                    let event = match event {
                        Event::Input(Input::Scroll { at, dx, dy }) => {
                            let (before_x, before_y) = std::mem::take(&mut wheel);
                            Event::Input(Input::Scroll {
                                at: *at,
                                dx: before_x + dx,
                                dy: before_y + dy,
                            })
                        }
                        event => event.clone(),
                    };
                    let reply = runner.handle(event.clone())?;
                    post.say(runner.take_sent());
                    let frame = match (reply.redraw, last) {
                        (true, Some(asked)) => {
                            let frame =
                                runner.render(asked.size, asked.now, asked.reduce_motion)?;
                            post.say(runner.take_sent());
                            Some((frame, asked.size))
                        }
                        _ => None,
                    };
                    let pointer = matches!(
                        event,
                        Event::Input(
                            Input::Move(_)
                                | Input::Leave
                                | Input::Press { .. }
                                | Input::Release { .. }
                                | Input::Scroll { .. }
                        )
                    );
                    post.put(frame, Some(Answer { reply, pointer }));
                }
                Command::Message(text) => {
                    settled = 0;
                    let reply = runner.message(text.clone())?;
                    // Taken: the program may say more. After the page was told, and not for a
                    // page that failed on it (the view is over, and the application lets go
                    // of what it counted).
                    program.taken(unescaped_len(text));
                    post.say(runner.take_sent());
                    owed |= answered(reply, post);
                }
                Command::Room => {
                    settled = 0;
                    let reply = runner.room()?;
                    post.say(runner.take_sent());
                    owed |= answered(reply, post);
                }
            }
        }
        if owed && let Some(asked) = last {
            let now = asked.since(program);
            let frame = runner.render(asked.size, now, asked.reduce_motion)?;
            // The page was made for `now`: what it is made again for next (a hover, a key) is no
            // earlier, and the time that passes from here is counted from here.
            last = Some(Asked {
                now,
                at: Instant::now(),
                ..asked
            });
            post.say(runner.take_sent());
            post.put(Some((frame, asked.size)), None);
        }
    }
}

/// What the page said back to something the program did: whether it wants to be drawn again
/// (the page that follows the batch), and the rest of the reply (a menu, the keyboard, the
/// pasteboard, the end), which is told now.
fn answered(reply: Reply, post: &Post<'_>) -> bool {
    let redraw = reply.redraw;
    if reply.close || reply.menu.is_some() || reply.keyboard.is_some() || reply.copy.is_some() {
        post.put(
            None,
            Some(Answer {
                reply,
                pointer: false,
            }),
        );
    }
    redraw
}

/// The most pages in a row a settle may ask for: the first settles what depends on the layout,
/// the second what the first changed — the way a page is laid out up to three times for a frame.
const SETTLES: u8 = 3;

fn is_render(command: &Command) -> bool {
    matches!(command, Command::Render { .. })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::Unbounded;
    use pencere_view::{Button, Mods, Point, div, text};
    use std::sync::mpsc;
    use std::time::Duration;

    /// A module that is a few lines of Rust: it logs what it was asked and answers as it was told.
    struct Plain {
        log: Arc<Mutex<Vec<String>>>,
        /// Holds the first render until the test has queued what it wants queued.
        gate: Option<Receiver<()>>,
        fail_on: Option<&'static str>,
        settles: bool,
        said: bool,
    }

    impl Plain {
        fn note(&self, what: String) -> Result<(), Fault> {
            self.log.lock().unwrap().push(what.clone());
            if self.fail_on.is_some_and(|name| what.starts_with(name)) {
                return Err(Fault::Trap(format!("planned: {what}")));
            }
            Ok(())
        }
    }

    impl Runner for Plain {
        fn render(&mut self, size: Size, now: f64, _reduce: bool) -> Result<Frame, Fault> {
            if let Some(gate) = self.gate.take() {
                // Said before waiting, for a test that has to know the module is held.
                self.log.lock().unwrap().push("held".into());
                let _ = gate.recv();
            }
            self.note(format!("render {} {now}", size.w))?;
            Ok(Frame {
                root: div().child(text(format!("{now}"))),
                animating: false,
                wake_at: None,
            })
        }

        fn settle(&mut self) -> Result<bool, Fault> {
            self.note("settle".into())?;
            Ok(self.settles)
        }

        fn handle(&mut self, event: Event) -> Result<Reply, Fault> {
            self.note(format!("handle {event:?}"))?;
            Ok(match event {
                Event::Text { .. } => Reply::redraw(),
                _ => Reply::none(),
            })
        }

        fn lay_out(&mut self, _laid: Arc<Laid>) {
            let _ = self.note("laid".into());
        }

        fn message(&mut self, text: String) -> Result<Reply, Fault> {
            self.note(format!("message {text}"))?;
            Ok(if text.contains("redraw") {
                Reply::redraw()
            } else {
                Reply::none()
            })
        }

        fn take_sent(&mut self) -> Vec<String> {
            let log = self.log.lock().unwrap();
            // A module that answers a message `ping` with a message of its own, once.
            if log.last().is_some_and(|last| last == "message ping") && !self.said {
                drop(log);
                self.said = true;
                return vec![r#"{"cancel":{"id":1}}"#.to_owned()];
            }
            Vec::new()
        }
    }

    struct Rig {
        module: Module,
        log: Arc<Mutex<Vec<String>>>,
        woken: Receiver<()>,
    }

    fn rig(gate: Option<Receiver<()>>, fail_on: Option<&'static str>, settles: bool) -> Rig {
        rig_for(Arc::new(Unbounded), gate, fail_on, settles)
    }

    fn rig_for(
        program: Arc<dyn Program>,
        gate: Option<Receiver<()>>,
        fail_on: Option<&'static str>,
        settles: bool,
    ) -> Rig {
        let log = Arc::new(Mutex::new(Vec::new()));
        let (told, woken) = mpsc::channel();
        let told = Mutex::new(told);
        let plain = Plain {
            log: Arc::clone(&log),
            gate,
            fail_on,
            settles,
            said: false,
        };
        let module = Module::spawn(
            move || Ok(plain),
            program,
            move || {
                let _ = told.lock().unwrap().send(());
            },
        );
        Rig { module, log, woken }
    }

    impl Rig {
        fn woken(&self) {
            self.woken
                .recv_timeout(Duration::from_secs(5))
                .expect("the application was woken");
        }

        fn log(&self) -> Vec<String> {
            self.log.lock().unwrap().clone()
        }
    }

    /// A program whose application keeps a clock that stands where the test puts it.
    struct Clocked(Mutex<Option<f64>>);

    impl Program for Clocked {
        fn may_say(&self, _bytes: usize) -> bool {
            true
        }

        fn taken(&self, _bytes: usize) {}

        fn now(&self) -> Option<f64> {
            *self.0.lock().unwrap()
        }
    }

    /// The time of the render `n` (counting from 1) in the log, once there is one.
    fn render_time(rig: &Rig, n: usize) -> f64 {
        let began = std::time::Instant::now();
        loop {
            let log = rig.log();
            let renders: Vec<&String> = log.iter().filter(|l| l.starts_with("render")).collect();
            if let Some(line) = renders.get(n - 1) {
                return line.rsplit(' ').next().unwrap().parse().unwrap();
            }
            assert!(
                began.elapsed() < Duration::from_secs(5),
                "no render {n}: {log:?}"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_page_made_again_for_what_its_program_said_is_made_for_the_time_on_the_application_s_clock()
    {
        // An application with a clock it can read says what it reads, whatever the system's
        // clock has done since the frame: the page's time is the application's.
        let clock = Arc::new(Clocked(Mutex::new(Some(40.0))));
        let rig = rig_for(clock.clone(), None, None, false);
        rig.module.render(Size::new(10.0, 10.0), 1.0, false);
        assert_eq!(render_time(&rig, 1), 1.0);
        rig.module.message("redraw".into());
        assert_eq!(render_time(&rig, 2), 40.0);

        // A clock that is behind the page's time, or is not a number, is not taken for one: the
        // page is never made for a time before one it was made for already, and a clock that says
        // nothing sensible is as good as none.
        *clock.0.lock().unwrap() = Some(0.5);
        rig.module.message("redraw".into());
        assert_eq!(render_time(&rig, 3), 40.0);
        *clock.0.lock().unwrap() = Some(f64::NAN);
        rig.module.message("redraw".into());
        let by_the_system = render_time(&rig, 4);
        assert!((40.0..70.0).contains(&by_the_system), "{by_the_system}");
    }

    #[test]
    fn the_page_is_not_made_for_an_earlier_time_by_what_the_person_does_after_it_was_made_for_the_program_s()
     {
        let clock = Arc::new(Clocked(Mutex::new(Some(40.0))));
        let rig = rig_for(clock, None, None, false);
        rig.module.render(Size::new(10.0, 10.0), 1.0, false);
        assert_eq!(render_time(&rig, 1), 1.0);
        rig.module.message("redraw".into());
        assert_eq!(render_time(&rig, 2), 40.0);
        // A hover that changes the page is answered at the time the page now stands at, not at
        // the time of the frame the person saw before the program spoke.
        rig.module.event(Event::Text {
            field: pencere_view::Id::named("field"),
            text: "a".into(),
        });
        assert_eq!(render_time(&rig, 3), 40.0);
        // A frame the application asks for is made for the time it says, and what follows it is
        // no earlier than the page has been.
        rig.module.render(Size::new(10.0, 10.0), 2.0, false);
        assert_eq!(render_time(&rig, 4), 2.0);
        rig.module.event(Event::Text {
            field: pencere_view::Id::named("field"),
            text: "b".into(),
        });
        assert_eq!(render_time(&rig, 5), 40.0);
    }

    #[test]
    fn what_the_program_said_is_given_back_as_the_protocol_counts_it() {
        struct Taken(Mutex<Vec<usize>>);
        impl Program for Taken {
            fn may_say(&self, _bytes: usize) -> bool {
                true
            }
            fn taken(&self, bytes: usize) {
                self.0.lock().unwrap().push(bytes);
            }
        }
        let program = Arc::new(Taken(Mutex::new(Vec::new())));
        let rig = rig_for(program.clone(), None, None, false);
        // Fourteen bytes of text, ten once the escape is undone.
        rig.module.message(r#"{"a":"\u00e9"}"#.into());
        let began = std::time::Instant::now();
        while program.0.lock().unwrap().is_empty() {
            assert!(began.elapsed() < Duration::from_secs(5), "never taken");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(*program.0.lock().unwrap(), [10]);
    }

    #[test]
    fn without_a_clock_of_the_application_s_the_page_is_made_for_the_frame_s_time_and_what_passed()
    {
        let rig = rig(None, None, false);
        rig.module.render(Size::new(10.0, 10.0), 100.0, false);
        // The time passes from the frame the module made, not from the one it was asked for.
        assert_eq!(render_time(&rig, 1), 100.0);
        std::thread::sleep(Duration::from_millis(60));
        rig.module.message("redraw".into());
        let again = render_time(&rig, 2);
        assert!(
            (100.06..130.0).contains(&again),
            "the frame's time and the time that passed: {again}"
        );
    }

    fn move_to(x: f32) -> Event {
        Event::Input(Input::Move(Point::new(x, 1.0)))
    }

    #[test]
    fn a_host_can_wait_a_little_for_the_page_of_the_size_it_asked_for_and_no_longer() {
        let small = Size::new(10.0, 10.0);
        let large = Size::new(20.0, 10.0);
        let (hold, gate) = mpsc::channel();
        let rig = rig(Some(gate), None, false);
        rig.module.render(small, 0.0, false);
        // Held: the page does not come, and the wait ends when it said.
        let began = std::time::Instant::now();
        assert!(!rig.module.wait_for_frame(small, Duration::from_millis(20)));
        assert!(began.elapsed() >= Duration::from_millis(20));
        // Let go: the wait ends with the page, and a page taken is not waiting any more.
        hold.send(()).unwrap();
        assert!(rig.module.wait_for_frame(small, Duration::from_secs(5)));
        assert!(rig.module.poll().frame.is_some());
        assert!(!rig.module.wait_for_frame(small, Duration::from_millis(5)));
        // A page made again for the size it was is not the one a resized pane waits for.
        rig.module.message("redraw".into());
        let began = std::time::Instant::now();
        while rig.log().iter().filter(|l| l.starts_with("render")).count() < 2 {
            assert!(began.elapsed() < Duration::from_secs(5), "never made again");
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(!rig.module.wait_for_frame(large, Duration::from_millis(20)));
        rig.module.render(large, 1.0, false);
        assert!(rig.module.wait_for_frame(large, Duration::from_secs(5)));
    }

    #[test]
    fn what_the_program_says_is_heard_in_order_one_page_follows_a_run_of_it_and_what_the_module_says_back_is_kept()
     {
        let (hold, gate) = mpsc::channel();
        let rig = rig(Some(gate), None, false);
        rig.module.render(Size::new(10.0, 10.0), 0.0, false);
        // Queued while the first render is held: all of it is one batch. (Held: the module has
        // taken the render, and with it whatever was waiting then, and is not looking at the
        // mailbox until it is let go. Messages sent before that could be in the first batch, with
        // the render, and a page would follow it too.)
        let began = std::time::Instant::now();
        while !rig.log().contains(&"held".to_string()) {
            assert!(began.elapsed() < Duration::from_secs(5), "never held");
            std::thread::sleep(Duration::from_millis(2));
        }
        rig.module.message("ping".into());
        rig.module.message("redraw 1".into());
        rig.module.message("quiet".into());
        rig.module.message("redraw 2".into());
        hold.send(()).unwrap();
        rig.woken();
        let mut update = rig.module.poll();
        // The page the module made last is the one that comes: wait for the second render, the
        // one after the run of messages.
        let began = std::time::Instant::now();
        let renders = |rig: &Rig| rig.log().iter().filter(|l| l.starts_with("render")).count();
        while (update.sent.is_empty() || renders(&rig) < 2)
            && began.elapsed() < Duration::from_secs(5)
        {
            std::thread::sleep(Duration::from_millis(10));
            let more = rig.module.poll();
            update.sent.extend(more.sent);
            update.frame = more.frame.or(update.frame);
        }
        assert!(update.frame.is_some());
        assert_eq!(update.sent, [r#"{"cancel":{"id":1}}"#.to_owned()]);
        let log = rig.log();
        let heard: Vec<&String> = log.iter().filter(|l| l.starts_with("message")).collect();
        assert_eq!(
            heard,
            [
                "message ping",
                "message redraw 1",
                "message quiet",
                "message redraw 2"
            ]
        );
        // One page for the whole run: the first render, and one more after the last message.
        assert_eq!(renders(&rig), 2, "{log:?}");
        assert_eq!(rig.module.poll().closed, None);
    }

    #[test]
    fn a_module_that_fails_on_a_message_is_closed_like_one_that_fails_on_anything() {
        let rig = rig(None, Some("message"), false);
        rig.module.render(Size::new(10.0, 10.0), 0.0, false);
        rig.woken();
        assert!(rig.module.poll().frame.is_some());
        rig.module.message("hello".into());
        rig.woken();
        let update = rig.module.poll();
        assert!(matches!(update.closed, Some(Fault::Trap(_))), "{update:?}");
    }

    #[test]
    fn a_page_asked_for_comes_with_one_wake_and_never_makes_the_asker_wait() {
        let rig = rig(None, None, false);
        rig.module.render(Size::new(100.0, 50.0), 1.5, false);
        rig.woken();
        let update = rig.module.poll();
        assert_eq!(update.closed, None);
        let frame = update.frame.expect("a page");
        assert_eq!(frame.root, div().child(text("1.5")));
        assert!(update.replies.is_empty());
        // Nothing more is waiting.
        assert_eq!(rig.module.poll(), Update::default());
    }

    #[test]
    fn of_the_pages_asked_for_while_the_module_is_busy_only_the_newest_is_made() {
        let (open, gate) = mpsc::channel();
        let rig = rig(Some(gate), None, false);
        let size = Size::new(10.0, 10.0);
        // The module may be inside the first (held) or not have looked yet: either way the
        // three after it are behind it when it is let go.
        for now in [1.0, 2.0, 3.0, 4.0] {
            rig.module.render(size, now, false);
        }
        open.send(()).unwrap();
        let began = std::time::Instant::now();
        while !rig.log().contains(&"render 10 4".to_string())
            && began.elapsed() < Duration::from_secs(5)
        {
            std::thread::sleep(Duration::from_millis(5));
        }
        let log = rig.log();
        assert!(log.contains(&"render 10 4".to_string()), "{log:?}");
        assert!(!log.contains(&"render 10 2".to_string()), "{log:?}");
        assert!(!log.contains(&"render 10 3".to_string()), "{log:?}");
    }

    #[test]
    fn an_event_that_changed_the_page_comes_with_the_page_and_its_reply_together() {
        let rig = rig(None, None, false);
        rig.module.render(Size::new(10.0, 10.0), 1.0, false);
        rig.woken();
        let _ = rig.module.poll();
        rig.module.event(Event::Text {
            field: pencere_view::Id::named("field"),
            text: "api".into(),
        });
        rig.woken();
        let update = rig.module.poll();
        assert_eq!(
            update.replies,
            vec![Answer {
                reply: Reply::redraw(),
                pointer: false
            }]
        );
        assert!(update.frame.is_some());
    }

    #[test]
    fn a_run_of_pointer_moves_is_the_last_of_them_and_other_events_are_all_kept() {
        let (open, gate) = mpsc::channel();
        let rig = rig(Some(gate), None, false);
        rig.module.render(Size::new(10.0, 10.0), 1.0, false);
        for x in [1.0, 2.0, 3.0] {
            rig.module.event(move_to(x));
        }
        rig.module.event(Event::Input(Input::Press {
            at: Point::new(3.0, 1.0),
            button: Button::Left,
            mods: Mods::default(),
            count: 1,
        }));
        rig.module.event(move_to(4.0));
        open.send(()).unwrap();
        let began = std::time::Instant::now();
        while rig.log().iter().filter(|l| l.starts_with("handle")).count() < 3
            && began.elapsed() < Duration::from_secs(5)
        {
            std::thread::sleep(Duration::from_millis(5));
        }
        let handled: Vec<String> = rig
            .log()
            .into_iter()
            .filter(|l| l.starts_with("handle"))
            .collect();
        assert_eq!(handled.len(), 3, "{handled:?}");
        assert!(handled[0].contains("x: 3.0"), "{handled:?}");
        assert!(handled[1].contains("Press"), "{handled:?}");
        assert!(handled[2].contains("x: 4.0"), "{handled:?}");
    }

    fn wheel(x: f32, dy: f32) -> Event {
        Event::Input(Input::Scroll {
            at: Point::new(x, 1.0),
            dx: 0.0,
            dy,
        })
    }

    #[test]
    fn a_run_of_wheel_turns_over_one_point_is_one_turn_by_all_of_them() {
        let (open, gate) = mpsc::channel();
        let rig = rig(Some(gate), None, false);
        rig.module.render(Size::new(10.0, 10.0), 1.0, false);
        for dy in [10.0, 20.0, 30.0] {
            rig.module.event(wheel(1.0, dy));
        }
        // Over another point the wheel may be over another box: a turn of its own.
        rig.module.event(wheel(5.0, 4.0));
        open.send(()).unwrap();
        let began = std::time::Instant::now();
        while rig.log().iter().filter(|l| l.starts_with("handle")).count() < 2
            && began.elapsed() < Duration::from_secs(5)
        {
            std::thread::sleep(Duration::from_millis(5));
        }
        let handled: Vec<String> = rig
            .log()
            .into_iter()
            .filter(|l| l.starts_with("handle"))
            .collect();
        assert_eq!(handled.len(), 2, "{handled:?}");
        assert!(
            handled[0].contains("x: 1.0") && handled[0].contains("dy: 60.0"),
            "{handled:?}"
        );
        assert!(
            handled[1].contains("x: 5.0") && handled[1].contains("dy: 4.0"),
            "{handled:?}"
        );
    }

    #[test]
    fn a_layout_is_followed_by_a_settle_and_a_settle_that_changed_the_page_makes_it_again() {
        let rig = rig(None, None, true);
        rig.module.render(Size::new(10.0, 10.0), 1.0, false);
        rig.woken();
        let _ = rig.module.poll();
        let laid = Arc::new(pencere_view::layout(
            div(),
            Size::new(10.0, 10.0),
            1.0,
            &mut Nothing,
        ));
        rig.module.lay_out(laid);
        rig.woken();
        let update = rig.module.poll();
        assert!(update.frame.is_some(), "{:?}", rig.log());
        let log = rig.log();
        let at = |what: &str| log.iter().position(|l| l == what).unwrap();
        assert!(at("laid") < at("settle"));
        assert_eq!(log.iter().filter(|l| l.starts_with("render")).count(), 2);
    }

    #[test]
    fn a_settle_that_always_wants_another_page_is_heard_a_few_times_and_then_not() {
        let rig = rig(None, None, true);
        rig.module.render(Size::new(10.0, 10.0), 1.0, false);
        rig.woken();
        let _ = rig.module.poll();
        let laid = Arc::new(pencere_view::layout(
            div(),
            Size::new(10.0, 10.0),
            1.0,
            &mut Nothing,
        ));
        for done in 1..=6 {
            rig.module.lay_out(Arc::clone(&laid));
            // One at a time: the module has settled before it is told the next layout.
            let began = std::time::Instant::now();
            while rig.log().iter().filter(|l| *l == "settle").count() < done
                && began.elapsed() < Duration::from_secs(5)
            {
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        let renders = rig.log().iter().filter(|l| l.starts_with("render")).count();
        // The one asked for, and three from the settles.
        assert_eq!(renders, 1 + usize::from(SETTLES), "{:?}", rig.log());
    }

    struct Nothing;

    impl pencere_view::Measure for Nothing {
        fn measure(&mut self, _font: &pencere_view::Font, _text: &str) -> pencere_view::Metrics {
            pencere_view::Metrics::default()
        }
    }

    #[test]
    fn a_module_that_fails_closes_with_the_reason_and_goes_on_being_safe_to_talk_to() {
        let rig = rig(None, Some("handle"), false);
        rig.module.event(Event::Text {
            field: pencere_view::Id::named("field"),
            text: "x".into(),
        });
        rig.woken();
        let update = rig.module.poll();
        assert!(matches!(update.closed, Some(Fault::Trap(_))), "{update:?}");
        // Told again, and nothing the application does afterwards can fail.
        rig.module.render(Size::new(1.0, 1.0), 1.0, false);
        rig.module.event(move_to(1.0));
        assert!(matches!(rig.module.poll().closed, Some(Fault::Trap(_))));
    }

    #[test]
    fn a_module_that_cannot_be_made_is_closed_with_why() {
        struct Never;
        impl Runner for Never {
            fn render(&mut self, _: Size, _: f64, _: bool) -> Result<Frame, Fault> {
                unreachable!()
            }
            fn settle(&mut self) -> Result<bool, Fault> {
                unreachable!()
            }
            fn handle(&mut self, _: Event) -> Result<Reply, Fault> {
                unreachable!()
            }
            fn lay_out(&mut self, _: Arc<Laid>) {}
        }
        let (told, woken) = mpsc::channel();
        let told = Mutex::new(told);
        let module = Module::spawn(
            || Err::<Never, _>(Fault::Load("not a component".into())),
            Arc::new(Unbounded),
            move || {
                let _ = told.lock().unwrap().send(());
            },
        );
        woken.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            module.poll().closed,
            Some(Fault::Load("not a component".into()))
        );
    }

    #[test]
    fn a_panic_in_the_host_side_of_a_module_is_a_closed_module_and_not_a_dead_application() {
        struct Panics;
        impl Runner for Panics {
            fn render(&mut self, _: Size, _: f64, _: bool) -> Result<Frame, Fault> {
                panic!("planned")
            }
            fn settle(&mut self) -> Result<bool, Fault> {
                Ok(false)
            }
            fn handle(&mut self, _: Event) -> Result<Reply, Fault> {
                Ok(Reply::none())
            }
            fn lay_out(&mut self, _: Arc<Laid>) {}
        }
        let (told, woken) = mpsc::channel();
        let told = Mutex::new(told);
        let module = Module::spawn(
            || Ok(Panics),
            Arc::new(Unbounded),
            move || {
                let _ = told.lock().unwrap().send(());
            },
        );
        module.render(Size::new(1.0, 1.0), 0.0, false);
        woken.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(module.poll().closed, Some(Fault::Trap(_))));
    }

    #[test]
    fn dropping_the_module_ends_its_thread() {
        struct Marker(mpsc::Sender<()>);
        impl Drop for Marker {
            fn drop(&mut self) {
                let _ = self.0.send(());
            }
        }
        impl Runner for Marker {
            fn render(&mut self, _: Size, _: f64, _: bool) -> Result<Frame, Fault> {
                Ok(Frame {
                    root: div(),
                    animating: false,
                    wake_at: None,
                })
            }
            fn settle(&mut self) -> Result<bool, Fault> {
                Ok(false)
            }
            fn handle(&mut self, _: Event) -> Result<Reply, Fault> {
                Ok(Reply::none())
            }
            fn lay_out(&mut self, _: Arc<Laid>) {}
        }
        let (gone, gone_rx) = mpsc::channel();
        let module = Module::spawn(|| Ok(Marker(gone)), Arc::new(Unbounded), || {});
        drop(module);
        gone_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the module was dropped with its thread");
    }
}
