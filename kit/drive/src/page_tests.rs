//! A page driven as a host drives it, frame by frame: what it is before it arrives, what its editors
//! do with what the person does and what the page asks, its wakes, its files and its images, and
//! that a page at rest asks for nothing and wakes nobody. Most of these run the interpreter or a
//! module of `make guest` and are heavy (`make check-heavy`).

use std::panic::{self, AssertUnwindSafe};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pencere_sandbox::{Assets, Program, Script};
use pencere_view::wire::Kind;
use pencere_view::{
    Button, DrawList, Font, Id, Input, Key, Measure, Metrics, Mods, Paint, Point, Rect,
};

use crate::{Drained, FrameAt, Outline, Page, Placement, Route, Setup};

/// Text measured with no renderer: each character half its font's size across, a line a quarter
/// again as high as the font.
struct Even;

impl Measure for Even {
    fn measure(&mut self, font: &Font, text: &str) -> Metrics {
        Metrics {
            width: text.chars().count() as f32 * font.size * 0.5,
            height: font.size * 1.25,
        }
    }
}

/// A host's view of a page: the page below a strip of 32 points, and what woke the host.
struct Rig {
    page: Page,
    woken: Receiver<()>,
}

/// What a test sets up a page with: the strip, the even measure, and the files.
fn setup(assets: Option<Arc<dyn Assets>>) -> Setup {
    Setup {
        placement: Placement {
            inset: 32.0,
            zoom: 1.0,
        },
        measure: Box::new(Even),
        assets,
        ..Setup::default()
    }
}

fn woken() -> (impl Fn() + Send + Sync + 'static, Receiver<()>) {
    let (told, woken) = channel();
    let told = Mutex::new(told);
    (
        move || {
            let _ = told.lock().unwrap().send(());
        },
        woken,
    )
}

fn script(source: &str, bundle: Option<Arc<dyn Assets>>) -> Script {
    Script {
        path: "page.lua".to_owned(),
        source: source.as_bytes().to_vec(),
        bundle,
    }
}

/// The page of the script `source`, started.
fn script_rig(source: &str) -> Rig {
    let (wake, woken) = woken();
    Rig {
        page: Page::script(script(source, None), setup(None), wake),
        woken,
    }
}

/// The page of the component `bytes`, started.
fn module_rig(bytes: Vec<u8>) -> Rig {
    let (wake, woken) = woken();
    Rig {
        page: Page::component(bytes, setup(None), wake),
        woken,
    }
}

/// A file `make guest` leaves in `kit/guest/dist`. **A test that needs one fails where it is not
/// there**: it is a heavy test, `make check-heavy` builds them first, and a test that went green by
/// returning early would be a guard that guards nothing.
fn guest_file(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../guest/dist")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|why| {
        panic!(
            "{} is not there ({why}): `make guest` builds the page modules",
            path.display()
        )
    })
}

/// A frame of 1000 × 600 points at two pixels to a point, at `now`: the size the container list is
/// written for.
fn wide_at(now: f64) -> FrameAt {
    FrameAt {
        size: (2000, 1200),
        scale: 2.0,
        now,
        reduce_motion: false,
    }
}

/// A frame of 800 × 600 pixels at one pixel to a point, at `now`.
fn frame_at(now: f64) -> FrameAt {
    FrameAt {
        size: (800, 600),
        scale: 1.0,
        now,
        reduce_motion: false,
    }
}

impl Rig {
    /// Waits for the module to hand out its first page (it is compiled first), as a host does when
    /// it is woken: what the drain said then.
    fn first(&mut self) -> Drained {
        assert!(
            self.page.module().flush(Duration::from_secs(60)),
            "the module never came to what it was told"
        );
        self.page.drain()
    }

    /// Frames and drains, as a host does, until the page has nothing more to say: the last frame's
    /// list. Each round waits for the module to have done what it was told (and the images to have
    /// come), so that nothing is guessed about how long that takes.
    fn settle_at(&mut self, at: FrameAt) -> DrawList {
        let mut list = self.page.frame(at).list;
        for _ in 0..100 {
            assert!(self.page.module().flush(Duration::from_secs(30)));
            while self
                .page
                .images()
                .is_some_and(pencere_sandbox::Images::busy)
            {
                let _ = self.woken.recv_timeout(Duration::from_secs(1));
            }
            let drained = self.page.drain();
            if !drained.redraw && !self.page.animating() {
                // What the host was woken for is taken.
                while self.woken.try_recv().is_ok() {}
                return list;
            }
            list = self.page.frame(at).list;
        }
        panic!("the page never came to rest");
    }

    fn settle(&mut self) -> DrawList {
        self.settle_at(frame_at(0.0))
    }

    fn press_key(&mut self, key: char) {
        self.page.input(Input::Key {
            key: Key::Character(key),
            mods: Mods::default(),
        });
    }

    /// A left click at `at`, the host's points: pressed and let go.
    fn click(&mut self, at: Point) {
        self.page.input(Input::Press {
            at,
            button: Button::Left,
            mods: Mods::default(),
            count: 1,
        });
        self.page.input(Input::Release {
            at,
            button: Button::Left,
        });
    }

    /// Drains, as a host does when it is woken, until `what` holds of a drain: that drain.
    fn drain_until(&mut self, mut what: impl FnMut(&Drained, &mut Page) -> bool) -> Drained {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let _ = self.woken.recv_timeout(Duration::from_millis(100));
            let drained = self.page.drain();
            if what(&drained, &mut self.page) {
                return drained;
            }
            assert!(
                Instant::now() < deadline,
                "what was waited for never came: {drained:?}"
            );
        }
    }

    /// Nothing wakes the host for a while.
    fn quiet(&self) -> bool {
        self.woken.recv_timeout(Duration::from_millis(300)).is_err()
    }

    fn editor_text(&self) -> String {
        self.page
            .editor(Id::named("e"))
            .map(pencere_editor::Buffer::text)
            .unwrap_or_default()
    }
}

/// The words of the page as the module last made it.
fn page_words(page: &Page) -> Vec<String> {
    page.root()
        .map(|root| {
            root.fold()
                .into_iter()
                .filter_map(|node| match node.kind {
                    Kind::Text(words) => Some(words),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The words drawn in `list`.
fn drawn(list: &DrawList) -> Vec<String> {
    list.items
        .iter()
        .filter_map(|item| match item {
            Paint::Text(run) => Some(run.text.clone()),
            _ => None,
        })
        .collect()
}

fn boxes(list: &DrawList) -> usize {
    list.items
        .iter()
        .filter(|item| matches!(item, Paint::Quad(_)))
        .count()
}

// --- A page that goes wrong ---------------------------------------------------------------------

/// A module that is not a page is closed with why, and the host is told once.
#[test]
fn a_module_that_is_not_a_page_ends_with_a_reason_told_once() {
    let mut rig = module_rig(b"not a component".to_vec());
    let why = rig.first().ended.expect("a reason").to_string();
    assert!(why.contains("loaded"), "{why}");
    assert_eq!(rig.page.drain().ended, None);
}

/// The program's clock and the frames' are one: a host whose program reads another clock (one from
/// nought, while its frames are stamped with the system's) would have time stand still between
/// frames, and a debug build stops on it. A reading a little behind a frame (a frame stamped with
/// when it will be shown) is no mistake.
#[cfg(debug_assertions)]
#[test]
fn a_program_clock_behind_the_frames_is_a_mistake_a_debug_build_stops_on() {
    struct FromNought;
    impl Program for FromNought {
        fn may_say(&self, _bytes: usize) -> bool {
            true
        }
        fn taken(&self, _bytes: usize) {}
        fn now(&self) -> Option<f64> {
            Some(0.5)
        }
    }
    let (wake, _woken) = woken();
    let program = Arc::new(FromNought);
    let mut page = Page::component(
        b"not a component".to_vec(),
        Setup {
            program,
            ..setup(None)
        },
        wake,
    );
    let key = Input::Key {
        key: Key::Character('a'),
        mods: Mods::default(),
    };
    page.frame(frame_at(1.0));
    page.input(key);
    page.frame(frame_at(1000.0));
    let stopped = panic::catch_unwind(AssertUnwindSafe(|| page.input(key)));
    let why = stopped.expect_err("a debug build stops on another clock");
    let why = why
        .downcast_ref::<String>()
        .map(String::as_str)
        .unwrap_or_default();
    assert!(why.contains("must be one clock"), "{why}");
}

/// Specification §5.4: a component written against another version of the interface than the one
/// the host opens cannot run, and its page ends in words that name both versions — those words are
/// what the host hands to its session for the program to read.
#[test]
fn a_component_of_another_interface_ends_in_words_that_name_both_versions() {
    // `(component (instance $page) (export "pencere:view/page@0.5.0" (instance $page)))`, in the
    // binary form: an empty instance, exported under the name the interface had.
    let name = b"pencere:view/page@0.5.0";
    let mut old = vec![0x00, 0x61, 0x73, 0x6d, 0x0d, 0x00, 0x01, 0x00];
    old.extend_from_slice(&[0x05, 0x03, 0x01, 0x01, 0x00]);
    old.extend_from_slice(&[0x0b, 6 + u8::try_from(name.len()).unwrap(), 0x01, 0x00]);
    old.push(u8::try_from(name.len()).unwrap());
    old.extend_from_slice(name);
    old.extend_from_slice(&[0x05, 0x00, 0x00]);
    let mut rig = module_rig(old);
    let why = rig.first().ended.expect("a reason").to_string();
    assert!(
        why.contains("written against `pencere:view@0.5.0`"),
        "{why}"
    );
    assert!(why.contains("opens `pencere:view@2.0.0`"), "{why}");
}

/// A script that makes an error ends its page with the error and the line it is on, which is what
/// the program is told, once.
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it; make check-heavy"]
fn a_script_that_makes_an_error_ends_with_its_line_told_once() {
    let mut rig = script_rig("local ok = 1\nerror('no page for you', 0)");
    assert_eq!(
        rig.first().ended.map(|why| why.to_string()).as_deref(),
        Some("no page for you")
    );
    assert_eq!(rig.page.drain().ended, None);

    let mut rig = script_rig("local x = 1\nlocal y = = 2");
    let why = rig.first().ended.expect("a reason").to_string();
    assert!(why.starts_with("page.lua:2:"), "{why}");
}

// --- The page arrives, is drawn, and rests ------------------------------------------------------

/// A script's page: a line in its place until the interpreter has made it, then the page.
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it; make check-heavy"]
fn a_scripts_page_is_a_line_until_it_comes_and_then_drawn() {
    let mut rig = script_rig(
        r#"return { render = function()
        return pencere.div():bg("raised1"):p(8):child(pencere.text("hello")):child(
            pencere.div():size(20, 20):bg("accent"))
    end }"#,
    );
    let starting = rig.page.frame(frame_at(0.0));
    assert_eq!(boxes(&starting.list), 0);
    assert_eq!(drawn(&starting.list), ["Starting the page\u{2026}"]);
    let drained = rig.first();
    assert!(drained.redraw && drained.ended.is_none(), "{drained:?}");
    let list = rig.page.frame(frame_at(0.1)).list;
    assert!(boxes(&list) >= 2, "the two boxes of the page");
    assert_eq!(drawn(&list), ["hello"]);
}

/// The container list's module, as the host shows it: the line that stands for the page until it
/// arrives (told to a screen reader, with no field), then the list, its field below the strip; a
/// page at rest asks nothing of the module and wakes nobody; the pointer over a row changes it.
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it; make check-heavy"]
fn a_modules_page_is_empty_until_it_arrives_then_drawn_and_then_at_rest() {
    let mut rig = module_rig(guest_file("containers.wasm"));
    let mut told: Vec<Outline> = Vec::new();
    // A line instead of a page yet, and the first frame does not wait for the module.
    let began = Instant::now();
    let first = rig.page.frame(wide_at(0.0));
    assert_eq!(boxes(&first.list), 0);
    assert!(began.elapsed() < Duration::from_secs(1));
    told.extend(first.outline);
    let drained = rig.first();
    assert!(drained.redraw, "{drained:?}");
    assert_eq!(drained.ended, None);
    let shown = rig.page.frame(wide_at(0.1));
    assert!(boxes(&shown.list) > 100, "the rows and dots of the list");
    told.extend(shown.outline);
    let field = rig
        .page
        .fields()
        .first()
        .expect("room for the filter's field")
        .rect;
    assert!(field.y >= 32.0, "below the strip: {field:?}");
    rig.settle_at(wide_at(0.2));
    assert!(!rig.page.animating());
    for step in 0..3 {
        told.extend(rig.page.frame(wide_at(1.0 + f64::from(step))).outline);
    }
    assert!(rig.quiet(), "a page at rest woke the host");
    // The host is told where the fields are as they change: none before the page, the filter's
    // once it comes. In between, the line that stands for the page is read out, and has none.
    let mut places: Vec<Vec<Rect>> = told
        .iter()
        .map(|outline| outline.fields.iter().map(|f| f.rect).collect())
        .collect();
    places.dedup();
    assert_eq!(places, [Vec::new(), vec![field]]);
    assert!(
        told.iter().any(|outline| outline.fields.is_empty()
            && outline
                .accessible
                .iter()
                .any(|a| a.words == "Starting the page\u{2026}")),
        "the line that stands for the page is told to a screen reader: {told:?}"
    );

    // The pointer over a row: the module answers, the page changes, the host is told.
    rig.page.input(Input::Move(Point::new(300.0, 32.0 + 150.0)));
    assert!(rig.page.module().flush(Duration::from_secs(30)));
    let drained = rig.page.drain();
    assert!(drained.redraw);
    assert_eq!(drained.replies.len(), 1);
    assert!(
        drained.replies[0].pointer,
        "a pointer move is the pointer's"
    );
    assert!(drained.replies[0].reply.redraw);
}

/// The container list as a script: the same as the module's.
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it; make check-heavy"]
fn a_scripts_container_list_is_drawn_then_at_rest_and_answers_the_pointer() {
    let mut rig = script_rig(include_str!("../../luau/examples/containers.lua"));
    assert_eq!(boxes(&rig.page.frame(wide_at(0.0)).list), 0);
    let drained = rig.first();
    assert!(drained.redraw, "{drained:?}");
    assert_eq!(drained.ended, None);
    let list = rig.page.frame(wide_at(0.1)).list;
    assert!(boxes(&list) > 100, "the rows and dots of the list");
    let field = rig
        .page
        .fields()
        .first()
        .expect("room for the filter's field")
        .rect;
    assert!(field.y >= 32.0, "below the strip: {field:?}");
    rig.settle_at(wide_at(0.2));
    assert!(!rig.page.animating());
    for step in 0..3 {
        rig.page.frame(wide_at(1.0 + f64::from(step)));
    }
    assert!(rig.quiet(), "a page at rest woke the host");
    rig.page.input(Input::Move(Point::new(300.0, 32.0 + 150.0)));
    assert!(rig.page.module().flush(Duration::from_secs(30)));
    let drained = rig.page.drain();
    assert!(drained.redraw);
    assert_eq!(drained.replies.len(), 1);
    assert!(drained.replies[0].pointer && drained.replies[0].reply.redraw);
}

/// The page of a module a program opened: it asks the program for what it shows, takes what the
/// program pushes, redraws once for it, and a page at rest after that wakes nobody; what is not one
/// of the program's messages changes nothing.
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it; make check-heavy"]
fn a_page_asks_its_program_for_readings_takes_them_and_is_at_rest_after() {
    let mut rig = module_rig(guest_file("containers.wasm"));
    rig.page.frame(wide_at(0.0));
    let drained = rig.first();
    // Said at its start, to a program that may be there: one message, a subscription.
    assert_eq!(
        drained.sent,
        [r#"{"subscribe":{"id":1,"topic":"stats"}}"#.to_owned()]
    );
    rig.settle_at(wide_at(0.1));

    // The program pushes a reading: the page changes, once, and nothing is said back.
    rig.page.message(
        r#"{"push":{"id":1,"value":{"readings":[{"name":"api","cpu":42.5,"memory":"130MiB","share":0.3}]}}}"#,
    );
    assert!(rig.page.module().flush(Duration::from_secs(30)));
    let drained = rig.page.drain();
    assert!(drained.redraw, "{drained:?}");
    assert!(drained.sent.is_empty());
    rig.settle_at(wide_at(0.3));
    for step in 0..3 {
        rig.page.frame(wide_at(1.0 + f64::from(step)));
    }
    assert!(rig.quiet(), "a page at rest woke the host");

    // A message that is not one of the program's, or is for nobody: nothing changes.
    for noise in [
        "not json",
        r#"{"push":{"id":9,"value":{}}}"#,
        r#"{"response":{"id":1,"result":{}}}"#,
    ] {
        rig.page.message(noise);
    }
    assert!(rig.page.module().flush(Duration::from_secs(30)));
    assert!(!rig.page.drain().redraw, "noise made the page change");
}

/// A key that does a command of the page is told to it as the command, and so is a command picked
/// from the host's list; the page's commands are in its outline, and a figure it draws is in the
/// list as one.
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it; make check-heavy"]
fn a_key_that_does_a_command_is_the_command_and_a_canvas_is_a_figure() {
    let mut rig = script_rig(
        r#"local said = "nothing"
        return {
            render = function()
                return pencere.div()
                    :command("find", "Find", "cmd+f")
                    :child(pencere.text(said))
                    :child(pencere.canvas({ pencere.path():circle(10, 10, 8):fill("accent") }):size(20, 20))
            end,
            handle = function(event)
                said = event.type .. " " .. tostring(event.id or event.key)
                return true
            end,
        }"#,
    );
    rig.page.frame(frame_at(0.0));
    rig.first();
    let list = rig.settle();
    assert_eq!(
        list.items
            .iter()
            .filter(|item| matches!(item, Paint::Figure(_)))
            .count(),
        1,
        "the canvas is a figure"
    );
    let commands = rig.page.outline().commands.clone();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].title, "Find");

    let command = Mods {
        command: true,
        ..Mods::default()
    };
    rig.page.input(Input::Key {
        key: Key::Character('f'),
        mods: command,
    });
    rig.settle();
    assert_eq!(page_words(&rig.page), ["command find"]);
    rig.press_key('g');
    rig.settle();
    assert_eq!(page_words(&rig.page), ["key g"]);
    rig.page.command(Id::named("find"));
    rig.settle();
    assert_eq!(page_words(&rig.page), ["command find"]);
}

// --- Editors ------------------------------------------------------------------------------------

/// A page with an editor 200 points high below 40 of its own, and a line of what it was told: `l`
/// loads the editor and gives it the keyboard; an edit is read back from the text the host posted;
/// ⌘⏎ is a command of the page's.
const EDITOR_PAGE: &str = r#"local said = {}
local function note(words) said[#said + 1] = words end
return {
    render = function()
        return pencere.div():size_full()
            :command("run", "Run", "cmd+enter")
            :child(pencere.div():h(40))
            :child(pencere.editor("e", { gutter = false }):h(200):w(400))
            :child(pencere.text(table.concat(said, "|")))
    end,
    handle = function(event)
        if event.type == "key" and event.key == "l" then
            note("load")
            return { redraw = true, keyboard = { field = "e" }, editor = { e = { load = "hello\nworld" } } }
        elseif event.type == "edit" then
            note("edit " .. event.origin .. " " .. pencere.layout.editor_text("e", 0, pencere.layout.editor_length("e")))
        elseif event.type == "focus" then
            note("focus " .. tostring(event.field))
        elseif event.type == "press" then
            note("press " .. (event.place and (event.place.editor .. "@" .. event.place.at) or "page"))
        elseif event.type == "command" then
            note("command " .. event.id)
        elseif event.type == "key" then
            note("key " .. event.key)
        end
        return true
    end,
}"#;

/// The editor's page, up and settled, its editor loaded and given the keyboard by `l`.
fn loaded(source: &str) -> Rig {
    let mut rig = script_rig(source);
    rig.page.frame(frame_at(0.0));
    rig.first();
    rig.settle();
    rig.press_key('l');
    rig.settle();
    rig
}

/// The page's editor: a reply loads it and gives it the keyboard, the frame draws its lines in its
/// room; the host's keys and text reach it, the page reads the edit from the text posted before it
/// is told; the page's command comes first.
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it; make check-heavy"]
fn a_pages_editor_is_loaded_by_its_reply_drawn_and_typed_into() {
    let mut rig = script_rig(EDITOR_PAGE);
    rig.page.frame(frame_at(0.0));
    rig.first();
    rig.settle();
    assert_eq!(rig.page.editors().focused(), None);
    // `l` is the page's while no editor has the keyboard.
    assert_eq!(
        rig.page.key(Key::Character('l'), Mods::default()),
        Route::NotEditor
    );
    let list = rig.settle();
    assert_eq!(rig.editor_text(), "hello\nworld");
    assert_eq!(rig.page.editors().focused(), Some(Id::named("e")));
    let words = drawn(&list);
    assert!(words.iter().any(|w| w == "hello"), "{words:?}");
    assert!(words.iter().any(|w| w == "world"), "{words:?}");
    assert!(
        list.items
            .iter()
            .all(|item| !matches!(item, Paint::Editor(_))),
        "every editor's place is filled"
    );
    // A character types (the host's system says what), and the page reads it back.
    assert_eq!(
        rig.page.key(Key::Character('x'), Mods::default()),
        Route::Type
    );
    rig.page.editors().type_text("x");
    rig.settle();
    assert_eq!(rig.editor_text(), "xhello\nworld");
    let said = page_words(&rig.page).join(" ");
    assert!(said.contains("edit person xhello\nworld"), "{said}");
    // ⌥ types what it makes (⌥Q on a Turkish keyboard).
    let alt = Mods {
        alt: true,
        ..Mods::default()
    };
    assert_eq!(rig.page.key(Key::Character('@'), alt), Route::Type);
    // The editor's own keys; the pasteboard is the host's.
    let command = Mods {
        command: true,
        ..Mods::default()
    };
    assert_eq!(rig.page.key(Key::Character('a'), command), Route::Done);
    assert_eq!(
        rig.page.key(Key::Character('c'), command),
        Route::Copy("xhello\nworld".into())
    );
    assert_eq!(rig.page.key(Key::Character('v'), command), Route::Paste);
    rig.page.editors().paste("pasted");
    rig.settle();
    assert_eq!(rig.editor_text(), "pasted");
    assert_eq!(rig.page.key(Key::Character('z'), command), Route::Done);
    rig.settle();
    assert_eq!(rig.editor_text(), "xhello\nworld");
    // The page's command for a key comes before the editor's.
    assert_eq!(
        rig.page.key(Key::Enter, command),
        Route::Command(Id::named("run"))
    );
    rig.settle();
    assert!(page_words(&rig.page).join(" ").contains("command run"));
    // Esc with one caret is the page's, told as a key.
    assert_eq!(rig.page.key(Key::Escape, Mods::default()), Route::Page);
    rig.settle();
    assert!(page_words(&rig.page).join(" ").contains("key escape"));
}

/// What an input method composes is in the document until it is done, and it asks where the caret
/// is in the host's points and what is around it, in units of UTF-16; a press ends what it composes.
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it; make check-heavy"]
fn an_input_method_composes_in_the_editor_and_is_answered_in_units_of_utf16() {
    let mut rig = loaded(EDITOR_PAGE);
    let mut editors = rig.page.editors();
    assert_eq!(editors.selected(), Some((0, 0)));
    editors.compose("k", (1, 0), None);
    assert_eq!(editors.marked(), Some((0, 1)));
    assert_eq!(editors.key(Key::Down, Mods::default()), Route::Type);
    editors.compose("か", (1, 0), None);
    editors.type_text("か");
    rig.settle();
    let mut editors = rig.page.editors();
    assert_eq!(editors.marked(), None);
    assert_eq!(editors.selected(), Some((1, 0)));
    assert_eq!(editors.text(0, 3).as_deref(), Some("かhe"));
    assert_eq!(editors.length(), Some(12));
    // The caret's box: in the editor's room, below the strip and the page's own 40 points.
    let caret = editors.rect(1).expect("a caret");
    assert!(
        caret.y >= 32.0 + 40.0 && caret.y < 32.0 + 240.0,
        "{caret:?}"
    );
    assert!(caret.h > 0.0, "{caret:?}");
    // What is under a point of the host's, there.
    let under = Point::new(caret.x + 0.5, caret.y + caret.h / 2.0);
    assert_eq!(editors.index_at(under), Some(1));
    let (text, selected) = editors.contents(Id::named("e")).expect("contents");
    assert_eq!((text.as_str(), selected), ("かhello\nworld", (1, 0)));
    assert_eq!(rig.editor_text(), "かhello\nworld");
    // A press ends what is composed: it stays, as typed.
    rig.page.editors().compose("x", (1, 0), None);
    assert!(rig.page.editors().marked().is_some());
    rig.click(under);
    assert_eq!(rig.page.editors().marked(), None);
    rig.settle();
    assert_eq!(rig.editor_text(), "かxhello\nworld");
}

/// The height of a line of the editor, points (one pixel to a point in these tests).
fn editors_line_height(page: &Page) -> f32 {
    let laid = page.laid().expect("laid out");
    let room = laid.editors().into_iter().next().expect("the room");
    room.font.size * 1.25
}

/// A press in the editor is the editor's — it takes the keyboard and puts the caret — and the page
/// hears it with the place under it; a press elsewhere on the page gives the keyboard back to the
/// page.
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it; make check-heavy"]
fn a_press_in_an_editor_puts_its_caret_and_one_elsewhere_takes_the_keyboard_back() {
    let mut rig = loaded(EDITOR_PAGE);
    rig.press_key('l');
    rig.settle();
    // The keyboard back to the page by a press below the editor, then into it by one on the
    // second line (the host's points: the strip is 32 high, the room starts 40 below it).
    rig.click(Point::new(500.0, 32.0 + 300.0));
    rig.settle();
    assert_eq!(rig.page.editors().focused(), None);
    let said = page_words(&rig.page).join(" ");
    assert!(said.contains("focus nil|press page"), "{said}");
    let line = editors_line_height(&rig.page);
    rig.click(Point::new(390.0, 32.0 + 40.0 + line * 1.5 + 4.0));
    rig.settle();
    assert_eq!(rig.page.editors().focused(), Some(Id::named("e")));
    // At the end of the second line: "hello\n" is 6, "world" 5.
    assert_eq!(rig.page.editors().selected(), Some((11, 0)));
    let said = page_words(&rig.page).join(" ");
    assert!(said.contains("focus e|press e@11"), "{said}");
}

/// A caret moved out of the room is brought into view by the host's scrolling of the room, and the
/// page is told the lines in view; the caret blinks on the frames' clock, which is all a page at
/// rest with an editor that has the keyboard wants a frame for.
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it; make check-heavy"]
fn the_caret_is_kept_in_view_by_the_rooms_scrolling_and_blinks_at_rest() {
    let long: String = (0..60).map(|n| format!("line {n}\n")).collect();
    let source = EDITOR_PAGE
        .replace(r#"load = "hello\nworld""#, &format!("load = {long:?}"))
        .replace(
            r#"elseif event.type == "command" then"#,
            r#"elseif event.type == "viewport" then
            note("viewport " .. event.first .. "-" .. event.last)
        elseif event.type == "command" then"#,
        );
    let mut rig = loaded(&source);
    let command = Mods {
        command: true,
        ..Mods::default()
    };
    assert_eq!(rig.page.key(Key::Down, command), Route::Done);
    rig.settle();
    let offset = rig
        .page
        .laid()
        .and_then(|laid| laid.scroll_extent(Id::named("e")))
        .map(|extent| extent.offset.y)
        .expect("the room scrolls");
    assert!(offset > 0.0, "the room is scrolled to the caret: {offset}");
    let said = page_words(&rig.page).join(" ");
    let told = said
        .rsplit("viewport ")
        .next()
        .and_then(|lines| lines.split(['-', '|']).nth(1))
        .and_then(|last| last.trim().parse::<usize>().ok());
    assert!(told.is_some_and(|last| last >= 59), "{said}");
    // At rest: no frame is asked for but the caret's next blink.
    assert!(!rig.page.animating());
    assert_eq!(rig.page.wake_at(), Some(0.5));
}

/// On a screen of two pixels to a point, the page drawn half as large again as its own size: the
/// caret's box an input method is told and the place under a point are each other's, a press there
/// puts the caret there, and a press below the room gives the keyboard back.
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it; make check-heavy"]
fn an_editor_answers_in_the_hosts_points_at_any_scale_and_zoom() {
    let mut rig = script_rig(EDITOR_PAGE);
    rig.page.set_zoom(1.5);
    let at = FrameAt {
        size: (1600, 1200),
        scale: 2.0,
        now: 0.0,
        reduce_motion: false,
    };
    rig.page.frame(at);
    rig.first();
    rig.settle_at(at);
    rig.press_key('l');
    rig.settle_at(at);
    assert_eq!(rig.page.editors().focused(), Some(Id::named("e")));
    let middle = |rect: Rect| Point::new(rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    for k in [0, 3, 6, 8, 11] {
        let mut editors = rig.page.editors();
        let caret = editors.rect(k).expect("a caret");
        assert!(
            caret.y >= 32.0 + 40.0 * 1.5,
            "below the strip and 40 points: {caret:?}"
        );
        assert_eq!(editors.index_at(middle(caret)), Some(k), "{caret:?}");
    }
    let eighth = rig.page.editors().rect(8).expect("a caret");
    rig.click(middle(eighth));
    rig.settle_at(at);
    assert_eq!(rig.page.editors().selected(), Some((8, 0)));
    let room = {
        let laid = rig.page.laid().expect("laid out");
        let room = laid.editors().into_iter().next().expect("the room").rect;
        rig.page.placement().to_host(room)
    };
    rig.click(Point::new(room.x + 10.0, room.y + room.h + 5.0));
    rig.settle_at(at);
    assert_eq!(rig.page.editors().focused(), None, "{room:?}");
}

/// An editor whose long lines wrap comes to rest like any page: its room's width is what its rows
/// are, and nothing is laid out again for ever.
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it; make check-heavy"]
fn an_editor_of_wrapped_lines_comes_to_rest() {
    let long: String = (0..40)
        .map(|n| format!("{n} {}\n", "word ".repeat(60)))
        .collect();
    let source = EDITOR_PAGE
        .replace("{ gutter = false }", "{ gutter = false, wrap = true }")
        .replace(r#"load = "hello\nworld""#, &format!("load = {long:?}"));
    let mut rig = loaded(&source);
    assert!(rig.editor_text().starts_with("0 word"));
    let extent = rig
        .page
        .laid()
        .and_then(|laid| laid.scroll_extent(Id::named("e")))
        .expect("the room scrolls");
    assert!(extent.content.w <= extent.view.w + 0.5, "{extent:?}");
    assert!(extent.content.h > extent.view.h, "{extent:?}");
    for step in 0..3 {
        rig.page.frame(frame_at(0.1 * f64::from(step)));
        assert!(!rig.page.animating(), "frame {step} asked for another");
    }
}

// --- Wakes --------------------------------------------------------------------------------------

/// A page that notes each wake it is given (`woke` and the time), drawn as one line of what it
/// noted: `w` asks to be woken in 2 seconds and `f` in 20, `a` asks for 0 and every wake after asks
/// for 0 again; `d` and `e` have the page ask to be drawn again at 500 and at 250 (`render`'s own
/// time, not a wake).
const WAKE_PAGE: &str = r#"local said = {}
local again = nil
local draw_at = nil
return {
    render = function()
        return pencere.text(table.concat(said, "|")), false, draw_at
    end,
    handle = function(event)
        if event.type == "key" then
            if event.key == "w" then return { wake = 2 } end
            if event.key == "f" then return { wake = 20 } end
            if event.key == "a" then again = 0; return { wake = 0 } end
            if event.key == "d" then draw_at = 500; return true end
            if event.key == "e" then draw_at = 250; return true end
        elseif event.type == "wake" then
            said[#said + 1] = "woke " .. event.now
            return { redraw = true, wake = again }
        end
        return false
    end,
}"#;

/// The wakes the page noted, in order.
fn wakes_noted(page: &Page) -> Vec<String> {
    page_words(page)
        .join("|")
        .split('|')
        .filter(|said| !said.is_empty())
        .map(str::to_owned)
        .collect()
}

fn wake_rig() -> Rig {
    let mut rig = script_rig(WAKE_PAGE);
    rig.page.frame(frame_at(0.0));
    rig.first();
    rig.settle();
    rig
}

/// A page at rest that asks nothing has no time to be drawn or woken at, and its drain has none to
/// tell. A reply that asks to be woken is told by the drain though nothing is to be drawn; the
/// frames before the time do not wake the page, and the first at or after it does, once. The module
/// counts the seconds from its own clock (the frame's time and what passed since), so the times the
/// frames are given are well before and well after the one asked.
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it; make check-heavy"]
fn a_wake_a_reply_asks_is_given_at_the_first_frame_at_its_time_and_a_page_at_rest_asks_none() {
    let mut rig = wake_rig();
    assert_eq!(rig.page.wake_at(), None, "a page at rest asks for no frame");
    assert!(!rig.page.animating());
    let drained = rig.page.drain();
    assert!(!drained.wake && !drained.redraw, "{drained:?}");

    rig.press_key('w');
    let drained = rig.drain_until(|drained, _| drained.wake);
    assert!(!drained.redraw, "a wake alone is not a frame: {drained:?}");
    assert!(rig.page.wake_at().is_some(), "the time to wake the page at");
    // The kit's timers count a wake asked outside a wake from the next one, which it asks for at
    // once: the frame at 50 wakes the kit, and it asks for the page's time from there.
    rig.settle_at(frame_at(50.0));
    assert!(
        wakes_noted(&rig.page).is_empty(),
        "{:?}",
        wakes_noted(&rig.page)
    );
    let due = rig.page.wake_at().expect("the page's own time");
    assert!((51.9..52.1).contains(&due), "{due}");
    rig.settle_at(frame_at(51.0));
    assert!(wakes_noted(&rig.page).is_empty(), "not before its time");
    rig.settle_at(frame_at(100.0));
    assert_eq!(wakes_noted(&rig.page), ["woke 100"]);
    // The wake that came is gone, and the page asked no other.
    assert_eq!(rig.page.wake_at(), None);
    let drained = rig.page.drain();
    assert!(!drained.wake, "{drained:?}");
}

/// Of the time the page asks to be drawn at and the one it asks to be woken at, the earliest is the
/// one its frame is asked at, whichever of the two it is.
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it; make check-heavy"]
fn of_the_time_a_page_is_drawn_at_and_the_one_it_is_woken_at_the_earliest_is_asked() {
    let mut rig = wake_rig();
    rig.press_key('d');
    rig.settle();
    assert_eq!(rig.page.wake_at(), Some(500.0));
    // A wake in 2 seconds is before it (counted from the wake the kit asks for at once, at 100).
    rig.press_key('w');
    rig.drain_until(|drained, _| drained.wake);
    rig.settle_at(frame_at(100.0));
    let due = rig.page.wake_at().expect("a time");
    assert!((101.9..102.1).contains(&due), "{due}");
    rig.settle_at(frame_at(200.0));
    assert_eq!(wakes_noted(&rig.page), ["woke 200"]);
    assert_eq!(
        rig.page.wake_at(),
        Some(500.0),
        "the wake is gone, the frame's time stays"
    );
    // A frame's time at 250 is before a wake in 20 seconds from 240.
    rig.press_key('e');
    rig.settle_at(frame_at(200.0));
    assert_eq!(rig.page.wake_at(), Some(250.0));
    rig.press_key('f');
    rig.drain_until(|drained, _| drained.wake);
    rig.settle_at(frame_at(240.0));
    assert_eq!(rig.page.wake_at(), Some(250.0));
    assert_eq!(wakes_noted(&rig.page), ["woke 200"]);
}

/// A page that asks to be woken again at once each time it is woken is woken once a frame: its
/// answer asks for the time of the wake itself, which the next frame is after; the same frame built
/// again does not wake it again.
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it; make check-heavy"]
fn a_page_that_asks_again_at_once_is_woken_once_a_frame() {
    let mut rig = wake_rig();
    rig.press_key('a');
    rig.drain_until(|drained, _| drained.wake);
    rig.page.frame(frame_at(100.0));
    rig.drain_until(|_, page| page.wake_at() == Some(100.0));
    rig.page.frame(frame_at(100.0));
    rig.settle_at(frame_at(100.0));
    assert_eq!(wakes_noted(&rig.page), ["woke 100"]);
    rig.settle_at(frame_at(100.5));
    assert_eq!(wakes_noted(&rig.page), ["woke 100", "woke 100.5"]);
    assert_eq!(
        rig.page.wake_at(),
        Some(100.5),
        "it waits for the next frame again"
    );
}

/// While its view is not seen, a page is woken by the host's clock, at its time and a second after
/// its last wake at the soonest, and the wake held back is given, not lost; seen again, the frames
/// wake it. A page that waits for no wake gives the host's clock nothing to do.
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it; make check-heavy"]
fn a_page_whose_view_is_not_seen_is_woken_a_second_apart_and_loses_no_wake() {
    let mut rig = wake_rig();
    assert_eq!(
        rig.page.wake_unseen(50.0),
        None,
        "nothing asked, nothing armed"
    );
    rig.press_key('a');
    rig.drain_until(|drained, _| drained.wake);
    assert_eq!(
        rig.page.wake_unseen(50.0),
        None,
        "woken now, and nothing asked after"
    );
    // The wake's answer asks again at once: held back to a second after it. (The time it asks is
    // told before the page it drew for the wake comes.)
    rig.drain_until(|_, page| {
        page.wake_unseen(50.25) == Some(51.0) && !wakes_noted(page).is_empty()
    });
    assert_eq!(wakes_noted(&rig.page), ["woke 50"]);
    assert_eq!(rig.page.wake_unseen(51.0), None);
    rig.drain_until(|_, page| wakes_noted(page).len() == 2);
    assert_eq!(wakes_noted(&rig.page), ["woke 50", "woke 51"]);
    // Seen again: the next frame wakes it, with no second to wait.
    rig.drain_until(|_, page| page.wake_at() == Some(51.0));
    rig.settle_at(frame_at(51.25));
    assert_eq!(wakes_noted(&rig.page), ["woke 50", "woke 51", "woke 51.25"]);
}

// --- Several files ------------------------------------------------------------------------------

/// The files of a bundle as a manifest types them: a path, a media type and the text.
struct Typed(Vec<(&'static str, &'static str, &'static str)>);

impl Assets for Typed {
    fn read(&self, path: &str) -> Option<Arc<[u8]>> {
        self.0
            .iter()
            .find(|(name, _, _)| *name == path)
            .map(|(_, _, text)| Arc::from(text.as_bytes()))
    }

    fn media_type(&self, path: &str) -> Option<String> {
        self.0
            .iter()
            .find(|(name, _, _)| *name == path)
            .map(|(_, media, _)| (*media).to_owned())
    }
}

const LUA: &str = "text/x-lua";

/// The page of the script `page.lua` of a bundle of `files`, started.
fn several_files(files: Vec<(&'static str, &'static str, &'static str)>) -> Rig {
    let page = files
        .iter()
        .find(|(path, _, _)| *path == "page.lua")
        .map(|(_, _, text)| *text)
        .expect("a page among the files");
    let bundle: Arc<dyn Assets> = Arc::new(Typed(files.clone()));
    let (wake, woken) = woken();
    Rig {
        page: Page::script(script(page, Some(bundle)), setup(None), wake),
        woken,
    }
}

/// A script of several files, like one of a single file: the script it requires is read from the
/// bundle, and what it makes is drawn.
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it; make check-heavy"]
fn a_scripts_page_of_several_files_is_drawn() {
    let mut rig = several_files(vec![
        (
            "page.lua",
            LUA,
            "local box = require('./parts/box')\nreturn { render = function()\n  return pencere.div():bg('raised1'):p(8):child(pencere.text('two files')):child(box())\nend }",
        ),
        (
            "parts/box.lua",
            LUA,
            "return function() return pencere.div():size(20, 20):bg('accent') end",
        ),
    ]);
    assert_eq!(boxes(&rig.page.frame(frame_at(0.0)).list), 0);
    let drained = rig.first();
    assert!(drained.redraw && drained.ended.is_none(), "{drained:?}");
    // The page's own box, and the small one the other file made.
    assert!(boxes(&rig.page.frame(frame_at(0.1)).list) >= 2);
}

/// What the page requires that the bundle does not have ends the page, with the path the page
/// wrote and the line it wrote it on.
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it; make check-heavy"]
fn a_script_that_requires_what_the_bundle_lacks_ends_with_the_path_and_the_line() {
    let mut rig = several_files(vec![(
        "page.lua",
        LUA,
        "local ok = 1\nrequire('./parts/missing')\nreturn {}",
    )]);
    let why = rig.first().ended.expect("a reason").to_string();
    assert!(why.starts_with("page.lua:2:"), "{why}");
    assert!(why.contains("\"./parts/missing\""), "{why}");
}

/// An error in the script that is required says that script's file and line, and a file the bundle
/// holds that its manifest does not call a script is not one to require.
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it; make check-heavy"]
fn a_required_scripts_error_says_its_file_and_a_file_that_is_not_a_script_is_not_one() {
    let mut rig = several_files(vec![
        ("page.lua", LUA, "require('./parts/box')\nreturn {}"),
        ("parts/box.lua", LUA, "local x = nil\n\nreturn x.y"),
    ]);
    let why = rig.first().ended.expect("a reason").to_string();
    assert!(why.starts_with("parts/box.lua:3:"), "{why}");

    let mut rig = several_files(vec![
        ("page.lua", LUA, "require('./parts/box')\nreturn {}"),
        ("parts/box.lua", "text/plain", "return {}"),
    ]);
    let why = rig.first().ended.expect("a reason").to_string();
    assert!(
        why.contains("the bundle has no script at \"parts/box\""),
        "{why}"
    );
}

// --- Images -------------------------------------------------------------------------------------

/// A 2 × 2 PNG: red, green, blue, cream.
const PNG: [u8; 84] = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x08, 0x06, 0x00, 0x00, 0x00, 0x72, 0xb6, 0x0d,
    0x24, 0x00, 0x00, 0x00, 0x1b, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x38, 0xe1, 0xe0, 0xf0,
    0xdf, 0x61, 0x41, 0xc0, 0x7f, 0x06, 0x87, 0x84, 0x0b, 0xff, 0x1f, 0x5c, 0x98, 0xf0, 0x1f, 0x00,
    0x51, 0xbc, 0x0a, 0x25, 0x0f, 0x57, 0x03, 0xad, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44,
    0xae, 0x42, 0x60, 0x82,
];

const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12"><rect width="12" height="12" fill="#c84040"/></svg>"##;

/// The files of a bundle that are read only when the test lets them be: the images' worker waits at
/// the gate, so the test knows what the page was before a file came and what it is after, and counts
/// what the host was woken for in between.
struct Gated {
    files: Vec<(&'static str, Vec<u8>)>,
    gate: Mutex<Receiver<()>>,
    /// Told, from the worker, that a file is wanted and the worker is at the gate.
    asked: Mutex<std::sync::mpsc::Sender<()>>,
    reads: std::sync::atomic::AtomicUsize,
}

impl Assets for Gated {
    fn read(&self, path: &str) -> Option<Arc<[u8]>> {
        self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let _ = self.asked.lock().unwrap().send(());
        let _ = self
            .gate
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(30));
        self.files
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| Arc::from(bytes.as_slice()))
    }

    fn media_type(&self, path: &str) -> Option<String> {
        self.files
            .iter()
            .any(|(name, _)| *name == path)
            .then(|| "application/octet-stream".to_owned())
    }
}

/// The pictures of `list` that have pixels to draw, as a host draws them.
fn pictures(page: &Page, list: &DrawList) -> usize {
    list.items
        .iter()
        .filter(|item| match item {
            Paint::Image(picture) => page
                .images()
                .and_then(|images| images.bitmap(&picture.src, picture.full()))
                .is_some(),
            _ => false,
        })
        .count()
}

/// A page with a bundle, held where its decoder is at the gate.
struct ImageRig {
    rig: Rig,
    /// Lets the decoder have the file.
    open: std::sync::mpsc::Sender<()>,
    assets: Arc<Gated>,
}

/// A script that shows one file of its bundle, up, **with the decoder held at its gate**: the page
/// has been laid out, the file asked for, and nothing more is to come until the test opens the gate.
/// Returns what the page is until then.
fn image_rig(file: &'static str, bytes: Vec<u8>) -> (ImageRig, DrawList) {
    let (open, gate) = channel();
    let (asked, was_asked) = channel();
    let assets = Arc::new(Gated {
        files: vec![(file, bytes)],
        gate: Mutex::new(gate),
        asked: Mutex::new(asked),
        reads: 0.into(),
    });
    let source = format!(
        r#"return {{ render = function()
            return pencere.div():p(8):child(pencere.image("{file}"))
        end }}"#
    );
    let bundle: Arc<dyn Assets> = Arc::clone(&assets) as Arc<dyn Assets>;
    let (wake, woken) = woken();
    let mut rig = Rig {
        page: Page::script(script(&source, Some(bundle)), setup(None), wake),
        woken,
    };
    // The script's page comes, is laid out, and its image is asked for.
    rig.page.frame(frame_at(0.0));
    rig.first();
    rig.page.frame(frame_at(0.1));
    assert!(
        was_asked.recv_timeout(Duration::from_secs(30)).is_ok(),
        "the page did not ask for its image"
    );
    // And what the script says after its page, if anything, is said before the test goes on.
    let mut list = rig.page.frame(frame_at(0.2)).list;
    assert!(rig.page.module().flush(Duration::from_secs(30)));
    while rig.page.drain().redraw {
        list = rig.page.frame(frame_at(0.3)).list;
        assert!(rig.page.module().flush(Duration::from_secs(30)));
    }
    while rig.woken.try_recv().is_ok() {}
    (ImageRig { rig, open, assets }, list)
}

fn wait(woken: &Receiver<()>) -> bool {
    woken.recv_timeout(Duration::from_secs(30)).is_ok()
}

/// A decoded PNG wakes the host once, the drain says the page is to be drawn again, that frame has
/// the picture, and after it the page at rest asks for and wakes nothing.
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it; make check-heavy"]
fn a_decoded_image_wakes_the_page_once_and_a_page_at_rest_not_at_all() {
    // The page is up and the file is asked for (the decoder is at its gate): a box with nothing in
    // it, and nothing more to come until the file does.
    let (
        ImageRig {
            mut rig,
            open,
            assets,
        },
        list,
    ) = image_rig("dot.png", PNG.to_vec());
    assert_eq!(
        pictures(&rig.page, &list),
        0,
        "nothing before the file is read"
    );
    assert_eq!(assets.reads.load(std::sync::atomic::Ordering::SeqCst), 1);

    // The file arrives: one wake for it, one drain that says the page is to be laid out and drawn
    // again, and that frame has the picture.
    open.send(()).unwrap();
    assert!(wait(&rig.woken), "the decoded image woke nobody");
    let drained = rig.page.drain();
    assert!(drained.redraw, "{drained:?}");
    assert!(drained.diagnostics.is_empty(), "{drained:?}");
    let list = rig.page.frame(frame_at(0.4)).list;
    assert_eq!(pictures(&rig.page, &list), 1, "the image is in the list");
    assert!(rig.quiet(), "one image woke the host more than once");

    // At rest: the frames that follow ask nothing, and the drain has nothing to say.
    for step in 0..3 {
        let list = rig.page.frame(frame_at(1.0 + f64::from(step))).list;
        assert_eq!(pictures(&rig.page, &list), 1);
    }
    let drained = rig.page.drain();
    assert!(!drained.redraw, "{drained:?}");
    assert!(rig.quiet(), "a page at rest woke the host");
    assert_eq!(assets.reads.load(std::sync::atomic::Ordering::SeqCst), 1);
}

/// A frame that is built between the file's arrival and the drain of the wake that came with it is
/// the frame that shows the picture; the drain after it has no other frame to ask for.
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it; make check-heavy"]
fn a_frame_built_before_the_drain_shows_the_arrived_image_and_no_other_is_asked_for() {
    let (ImageRig { mut rig, open, .. }, _) = image_rig("dot.png", PNG.to_vec());
    open.send(()).unwrap();
    assert!(wait(&rig.woken), "the decoded image woke nobody");
    // The frame comes first, for a reason of its own (a pointer move, a tick).
    let list = rig.page.frame(frame_at(0.4)).list;
    assert_eq!(pictures(&rig.page, &list), 1, "the frame has the picture");
    let drained = rig.page.drain();
    assert!(
        !drained.redraw,
        "the wake of an image a frame already drew asked for one more: {drained:?}"
    );
    assert!(rig.quiet());
}

/// An SVG is two arrivals — its size, which lays the page out again, then its pixels at that size —
/// and a page at rest after them wakes nobody.
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it; make check-heavy"]
fn a_vector_image_wakes_the_page_for_its_size_then_for_its_pixels() {
    let (ImageRig { mut rig, open, .. }, list) = image_rig("dot.svg", SVG.as_bytes().to_vec());
    assert_eq!(pictures(&rig.page, &list), 0);
    open.send(()).unwrap();
    // Its size: the page is laid out again, and the frame asks for the pixels at the size the page
    // gave it.
    assert!(wait(&rig.woken), "the size of the image woke nobody");
    assert!(rig.page.drain().redraw);
    let list = rig.page.frame(frame_at(0.4)).list;
    assert_eq!(pictures(&rig.page, &list), 0, "no pixels at this size yet");
    // Its pixels.
    assert!(wait(&rig.woken), "the pixels of the image woke nobody");
    assert!(rig.page.drain().redraw);
    let list = rig.page.frame(frame_at(0.5)).list;
    assert_eq!(pictures(&rig.page, &list), 1, "the image is in the list");
    assert!(rig.quiet(), "the image woke the host a third time");
    assert!(!rig.page.drain().redraw);
}

/// A file the decoder refuses is a box with nothing in it and a word for the page's author, told
/// once; the page goes on.
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it; make check-heavy"]
fn an_image_that_is_not_one_is_left_out_with_a_word_for_the_author() {
    let (ImageRig { mut rig, open, .. }, list) = image_rig("dot.png", b"not an image".to_vec());
    assert_eq!(pictures(&rig.page, &list), 0);
    open.send(()).unwrap();
    assert!(wait(&rig.woken), "a refused image told nobody");
    let drained = rig.page.drain();
    assert_eq!(drained.diagnostics.len(), 1, "{drained:?}");
    assert!(drained.diagnostics[0].contains("dot.png"), "{drained:?}");
    assert_eq!(drained.ended, None);
    let list = rig.page.frame(frame_at(0.4)).list;
    assert_eq!(pictures(&rig.page, &list), 0);
    assert!(rig.page.drain().diagnostics.is_empty());
}

// --- Scrolling ----------------------------------------------------------------------------------

/// A list longer than its box: the host scrolls the box under the wheel itself, on the page as it
/// was laid out, tells the page where it went, and a turn no box takes is the page's.
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it; make check-heavy"]
fn the_host_scrolls_the_box_under_the_wheel_and_a_turn_no_box_takes_is_the_pages() {
    let mut rig = script_rig(
        r#"local said = {}
        return {
            render = function()
                local list = pencere.div():id("list"):h(100):w(200):scroll()
                for n = 1, 50 do list:child(pencere.div():h(20):child(pencere.text("row " .. n))) end
                return pencere.div():size_full()
                    :child(list)
                    :child(pencere.text(table.concat(said, "|")))
            end,
            handle = function(event)
                if event.type == "scrolled" then
                    said[#said + 1] = "scrolled " .. event.boxes[1].id .. " " .. event.boxes[1].y
                elseif event.type == "scroll" then
                    said[#said + 1] = "scroll " .. event.dy
                end
                return true
            end,
        }"#,
    );
    rig.page.frame(frame_at(0.0));
    rig.first();
    rig.settle();
    // Over the list (the host's points: below the strip).
    rig.page.wheel(
        Point::new(50.0, 32.0 + 50.0),
        0.0,
        60.0,
        pencere_view::Phase::Wheel,
    );
    rig.settle();
    let offset = rig
        .page
        .laid()
        .and_then(|laid| laid.scroll_extent(Id::named("list")))
        .map(|extent| extent.offset.y)
        .expect("the list scrolls");
    assert_eq!(offset, 60.0);
    let said = page_words(&rig.page).join(" ");
    assert!(said.contains("scrolled list 60"), "{said}");
    // Below the list nothing scrolls: the turn is the page's.
    rig.page.wheel(
        Point::new(50.0, 32.0 + 400.0),
        0.0,
        30.0,
        pencere_view::Phase::Wheel,
    );
    rig.settle();
    let said = page_words(&rig.page).join(" ");
    assert!(said.contains("scroll 30"), "{said}");
}
