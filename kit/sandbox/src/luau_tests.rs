//! The interpreter (`luau.wasm`, built from `kit/luau`) through the sandbox, with scripts. It
//! is part of the repository, so these need no toolchain for WebAssembly, only the time to
//! compile it (`make check-heavy`), and they are what notices an interpreter built from another
//! interface than the one the sandbox speaks.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use pencere_view::wire::{Direction, Display, Kind, Overflow};
use pencere_view::{
    Built, Button, Color, Cursor, Ease, Element, Event, FieldKind, Fill, Fit, Id, Input, Key,
    Keyboard, Mix, Mods, Motion, Point, Rgba, Role, Rows, ScrollTo, Shift, Source, Tip, image,
};
use wasmtime::component::Component;

use crate::test_support::{Open, Rough, SIZE};
use crate::{Fault, Module, ModuleLimits, Program, Script, TimeLimit, Unbounded, Update};

/// Where the scripts of the tests that are given no bundle are, in their bundle.
pub(crate) const ENTRY: &str = "script.lua";

pub(crate) fn open_script(script: &str, limits: ModuleLimits) -> Open {
    open_script_for(script, limits, Arc::new(Unbounded))
}

pub(crate) fn open_script_for(
    script: &str,
    limits: ModuleLimits,
    program: Arc<dyn Program>,
) -> Open {
    let (told, woken) = channel();
    let told = std::sync::Mutex::new(told);
    let script = Script {
        path: ENTRY.to_owned(),
        source: script.as_bytes().to_vec(),
        bundle: None,
    };
    let module = Module::from_script(script, limits, program, move || {
        let _ = told.lock().unwrap().send(());
    });
    Open {
        module,
        woken,
        seen: Update::default(),
    }
}

/// The first thing a script's page is: the page, or why it is closed.
pub(crate) fn first(script: &str, limits: ModuleLimits) -> Update {
    let mut page = open_script(script, limits);
    page.module.render(SIZE, 0.0, false);
    page.until(Duration::from_secs(20), |u| {
        u.frame.is_some() || u.closed.is_some()
    })
}

/// The words of the text elements of a page, in order.
pub(crate) fn words(root: &Element) -> Vec<String> {
    root.fold()
        .into_iter()
        .filter_map(|node| match node.kind {
            pencere_view::wire::Kind::Text(text) => Some(text),
            _ => None,
        })
        .collect()
}

/// Takes until what has been seen so far (not only the last taking) says `done`, or `within`.
pub(crate) fn until_seen(page: &mut Open, within: Duration, done: impl Fn(&Update) -> bool) {
    let deadline = Instant::now() + within;
    while !done(&page.seen) && Instant::now() < deadline {
        page.take(Duration::from_millis(100));
    }
}

pub(crate) fn failed(update: &Update) -> String {
    match &update.closed {
        Some(Fault::Failed(why)) => why.clone(),
        other => panic!("expected a script's failure, got {other:?}"),
    }
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn the_interpreter_asks_for_nothing_the_page_interface_does_not_give() {
    let engine = crate::sandbox::engine().expect("an engine");
    let component = Component::new(&engine, crate::LUAU_COMPONENT).expect("a component");
    let imports: Vec<String> = component
        .component_type()
        .imports(&engine)
        .map(|(name, _)| name.to_owned())
        .collect();
    assert!(!imports.is_empty());
    assert!(
        imports.iter().all(|name| name.starts_with("pencere:view/")),
        "an interpreter that imports more than the interface: {imports:?}"
    );
    // And it is the interface of this sandbox: it starts, and runs a page.
    let update = first(
        "return { render = function() return pencere.div() end }",
        ModuleLimits::default(),
    );
    assert_eq!(update.closed, None);
    assert!(update.frame.is_some());
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_script_that_returns_a_page_is_drawn_with_what_it_made_of_the_viewport() {
    let update = first(
        r#"
        return {
            render = function(size, now, reduce)
                return pencere.div():gap(4):child(pencere.text("w " .. size.w .. " h " .. size.h)):child(
                    pencere.text(string.format("now %.1f %s", now, tostring(reduce)))), false
            end,
        }
        "#,
        ModuleLimits::default(),
    );
    assert_eq!(update.closed, None);
    let frame = update.frame.expect("a page");
    assert!(!frame.animating);
    assert_eq!(words(&frame.root), ["w 1000 h 700", "now 0.0 false"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_page_that_asks_to_animate_is_asked_for_again() {
    let update = first(
        "return { render = function() return pencere.div(), true end }",
        ModuleLimits::default(),
    );
    assert!(update.frame.expect("a page").animating);
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_syntax_error_closes_the_page_and_says_the_line() {
    let update = first(
        "local ok = 1\nlocal oops = = 3\nreturn {}",
        ModuleLimits::default(),
    );
    let why = failed(&update);
    assert!(why.starts_with("script.lua:2:"), "{why}");
    assert!(update.frame.is_none());
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn an_error_while_the_script_starts_says_the_line() {
    let update = first("local t = nil\n\nreturn t.page", ModuleLimits::default());
    let why = failed(&update);
    assert!(why.starts_with("script.lua:3:"), "{why}");
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_helper_given_the_wrong_argument_says_the_line_of_the_script_that_called_it() {
    // The error is the helper's, raised in the prelude; the line is the script's.
    for (call, says) in [
        (
            "pencere.text(x)",
            "pencere.text takes a string or a number, not nil",
        ),
        (
            "pencere.spark(x)",
            "pencere.spark takes a list of numbers, not nil",
        ),
        (
            "pencere.div():children(x)",
            "children takes a list of elements, not nil",
        ),
        (
            "pencere.image(x)",
            "pencere.image takes the path of a file of the bundle, not nil",
        ),
        (
            "pencere.image('a.png'):fit('stretch')",
            "fit is \"contain\" or \"cover\", not stretch",
        ),
        (
            "pencere.div():opacity('half')",
            "opacity is a number from 0 to 1, not string",
        ),
        (
            "pencere.div():tint('error')",
            "tint is for an image, and this is a box",
        ),
    ] {
        let update = first(
            &format!("return {{ render = function()\n  local x = nil\n  return {call}\nend }}"),
            ModuleLimits::default(),
        );
        let why = failed(&update);
        assert!(why.starts_with("script.lua:3:"), "{why}");
        assert!(why.contains(says), "{why}");
    }
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn an_error_in_render_says_the_line_and_the_frame_is_not_read() {
    let update = first(
        "return { render = function()\n  local x = nil\n  return pencere.div():child(x.y)\nend }",
        ModuleLimits::default(),
    );
    let why = failed(&update);
    assert!(why.starts_with("script.lua:3:"), "{why}");
    assert!(
        update.frame.is_none(),
        "a page from a call that failed is not read"
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn an_error_in_a_handler_closes_the_page_with_its_line() {
    let mut page = open_script(
        "return {\n render = function() return pencere.div() end,\n handle = function(e)\n  error('no ' .. e.type)\n end,\n}",
        ModuleLimits::default(),
    );
    page.module.render(SIZE, 0.0, false);
    page.until(Duration::from_secs(20), |u| u.frame.is_some());
    page.module
        .event(Event::Input(Input::Move(Point::new(1.0, 1.0))));
    let update = page.until(Duration::from_secs(5), |u| u.closed.is_some());
    let why = failed(&update);
    assert!(why.starts_with("script.lua:4: no move"), "{why}");
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn an_error_the_script_catches_is_not_an_error() {
    let update = first(
        r#"
        local ok, why = pcall(function() error("inner", 0) end)
        local ok2, why2 = pcall(function() local t = nil; return t.x end)
        return { render = function()
            return pencere.div():child(pencere.text(tostring(ok) .. " " .. why .. " | " .. why2))
        end }
        "#,
        ModuleLimits::default(),
    );
    assert_eq!(update.closed, None);
    let text = words(&update.frame.expect("a page").root).join("");
    assert!(text.starts_with("false inner | script.lua:3:"), "{text}");
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_script_that_does_not_return_a_page_is_closed_and_told_so() {
    for script in ["return 3", "return {}", "return { render = 4 }", ""] {
        let update = first(script, ModuleLimits::default());
        let why = failed(&update);
        assert!(why.contains("must return a page"), "{script:?}: {why}");
    }
    let update = first(
        "return { render = function() return 5 end }",
        ModuleLimits::default(),
    );
    assert!(failed(&update).contains("render must return an element"));
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_tree_that_is_not_a_page_is_closed_with_what_is_wrong_in_it() {
    let update = first(
        "return { render = function() return pencere.div():bg('puce') end }",
        ModuleLimits::default(),
    );
    assert!(failed(&update).contains("\"puce\" is not the name of a role"));
    let update = first(
        "return { render = function() local d = pencere.div(); d:child(d); return d end }",
        ModuleLimits::default(),
    );
    assert!(failed(&update).contains("nested more than"));
    let update = first(
        "return { render = function() return { kind = 'box', style = { width = 'wide' } } end }",
        ModuleLimits::default(),
    );
    assert!(failed(&update).contains("width"));
}

/// The text colour of each line a script's page has, in order.
fn text_colours(script: &str) -> Vec<Option<Color>> {
    let update = first(script, ModuleLimits::default());
    assert_eq!(update.closed, None);
    update
        .frame
        .expect("a page")
        .root
        .fold()
        .into_iter()
        .filter(|node| matches!(node.kind, pencere_view::wire::Kind::Text(_)))
        .map(|node| node.style.text.color)
        .collect()
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_script_names_every_role_of_the_theme_and_the_roles_are_the_engines() {
    // Every name the page API takes, and the role it is.
    let names: [(&str, Role); 17] = [
        ("selection", Role::Selection),
        ("black", Role::Black),
        ("red", Role::Red),
        ("green", Role::Green),
        ("yellow", Role::Yellow),
        ("blue", Role::Blue),
        ("magenta", Role::Magenta),
        ("cyan", Role::Cyan),
        ("white", Role::White),
        ("bright-black", Role::BrightBlack),
        ("bright-red", Role::BrightRed),
        ("bright-green", Role::BrightGreen),
        ("bright-yellow", Role::BrightYellow),
        ("bright-blue", Role::BrightBlue),
        ("bright-magenta", Role::BrightMagenta),
        ("bright-cyan", Role::BrightCyan),
        ("bright-white", Role::BrightWhite),
    ];
    let list = names
        .iter()
        .map(|(name, _)| format!("{name:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    let script = format!(
        "local names = {{ {list} }}
         return {{ render = function()
             local root = pencere.div()
             for _, name in ipairs(names) do root:child(pencere.text(name):color(name)) end
             return root
         end }}"
    );
    let colours = text_colours(&script);
    assert_eq!(colours.len(), names.len());
    for ((name, role), colour) in names.iter().zip(colours) {
        assert_eq!(colour, Some(role.solid()), "{name}");
    }
    // The palette's names, in the order of its numbers.
    for (at, (_, role)) in names[1..].iter().enumerate() {
        assert_eq!(role.ansi_index(), Some(at as u8));
    }
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_script_gives_a_value_with_a_form_for_a_dark_theme_and_an_opacity_on_any_colour() {
    let colours = text_colours(
        r##"return { render = function()
            return pencere.div()
                :child(pencere.text("a"):color("#1A2b3c"))
                :child(pencere.text("b"):color("#1a2b3c80"))
                :child(pencere.text("c"):color({ light = "#203040", dark = "#E0D0C0" }))
                :child(pencere.text("d"):color({ light = "#203040" }))
                :child(pencere.text("e"):color(pencere.alpha("bright-red", 0.5)))
                :child(pencere.text("f"):color(pencere.alpha("#ff000080", 0.25)))
                :child(pencere.text("g"):color(pencere.alpha({ light = "#010203", dark = "#040506" }, 0.75)))
                :child(pencere.text("h"):color({ "selection", 0.5 }))
        end }"##,
    );
    let value = |light: Rgba, dark: Option<Rgba>, alpha: f32| Color {
        source: Source::Rgba { light, dark },
        mix: None,
        alpha,
    };
    assert_eq!(
        colours,
        vec![
            Some(Rgba::rgb(0x1a, 0x2b, 0x3c).solid()),
            Some(Rgba::new(0x1a, 0x2b, 0x3c, 0x80).solid()),
            Some(Rgba::rgb(0x20, 0x30, 0x40).or_dark(Rgba::rgb(0xe0, 0xd0, 0xc0))),
            Some(Rgba::rgb(0x20, 0x30, 0x40).solid()),
            Some(Role::BrightRed.alpha(0.5)),
            Some(value(Rgba::new(0xff, 0, 0, 0x80), None, 0.25)),
            Some(value(Rgba::rgb(1, 2, 3), Some(Rgba::rgb(4, 5, 6)), 0.75)),
            Some(Role::Selection.alpha(0.5)),
        ]
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_script_says_an_image_and_the_engine_reads_what_it_said() {
    let update = first(
        r##"return { render = function()
            return pencere.div()
                :child(pencere.image("icons/stop.svg"))
                :child(pencere.image("logo.png"):fit("cover"):opacity(0.5):size(48, 24):rounded(4))
                :child(pencere.image("icons/run.svg"):tint("error"):w(16))
                :child(pencere.image("icons/run.svg"):tint({ light = "#203040", dark = "#e0d0c0" }))
                :child(pencere.image("icons/run.svg"):tint(pencere.alpha("accent", 0.5)))
        end }"##,
        ModuleLimits::default(),
    );
    assert_eq!(update.closed, None);
    let root = update.frame.expect("a page").root;
    let nodes = root.fold();
    let images: Vec<&pencere_view::wire::Node> = nodes
        .iter()
        .filter(|node| matches!(node.kind, Kind::Image(_)))
        .collect();
    let said = |node: &pencere_view::wire::Node| match &node.kind {
        Kind::Image(image) => image.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(images.len(), 5);
    // The same elements, as Rust says them.
    let rust = |element: Element| match element.fold().remove(0).kind {
        Kind::Image(image) => image,
        other => panic!("{other:?}"),
    };
    assert_eq!(said(images[0]), rust(image("icons/stop.svg", Fit::Contain)));
    assert_eq!(said(images[1]), rust(image("logo.png", Fit::Cover)));
    assert_eq!(images[1].style.opacity, Some(0.5), "the element's opacity");
    assert_eq!(
        (images[1].style.width, images[1].style.height),
        (
            pencere_view::wire::Length::Px(48.0),
            pencere_view::wire::Length::Px(24.0)
        )
    );
    assert_eq!(images[1].style.radii, [4.0; 4]);
    assert_eq!(
        said(images[2]),
        rust(image("icons/run.svg", Fit::Contain).tint(Role::Error))
    );
    assert_eq!(
        said(images[3]),
        rust(
            image("icons/run.svg", Fit::Contain)
                .tint(Rgba::rgb(0x20, 0x30, 0x40).or_dark(Rgba::rgb(0xe0, 0xd0, 0xc0)))
        )
    );
    assert_eq!(
        said(images[4]),
        rust(image("icons/run.svg", Fit::Contain).tint(Role::Accent.alpha(0.5)))
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_colour_that_is_not_one_is_refused_in_words_that_say_what_one_is() {
    let refused = |colour: &str| {
        let update = first(
            &format!("return {{ render = function() return pencere.div():bg({colour}) end }}"),
            ModuleLimits::default(),
        );
        failed(&update)
    };
    for bad in [
        r##""#12345""##,
        r##""#1234567""##,
        r##""#12345g""##,
        r##""# 1a2b3c""##,
        r##""#1a2b3c8""##,
        r##""#""##,
    ] {
        assert!(
            refused(bad).contains("is not a colour value"),
            "{bad}: {}",
            refused(bad)
        );
    }
    assert!(refused("'chartreuse'").contains("\"chartreuse\" is not the name of a role"));
    assert!(refused("{ dark = '#000000' }").contains("a colour is a role's name"));
    assert!(refused("{ light = 5 }").contains("light must be a colour value"));
    assert!(refused("{ light = '#000000', dark = 5 }").contains("dark must be a colour value"));
    assert!(refused("5").contains("a colour is a role's name"));
    assert!(refused("{ light = 'red' }").contains("is not a colour value"));
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_script_that_never_returns_is_cut_off() {
    let limits = ModuleLimits {
        call: Duration::from_millis(200),
        ..ModuleLimits::floor()
    };
    let call_cut_off = Some(Fault::Timeout {
        limit: TimeLimit::Call,
        within: Duration::from_millis(200),
    });
    let began = Instant::now();
    let update = first(
        "return { render = function() while true do end end }",
        limits.clone(),
    );
    assert_eq!(update.closed, call_cut_off);
    assert!(began.elapsed() < Duration::from_secs(10));
    // Also while it starts (under the time to start, which is the floor's 2 s), and while it
    // handles.
    let update = first("while true do end", limits.clone());
    assert_eq!(
        update.closed,
        Some(Fault::Timeout {
            limit: TimeLimit::Start,
            within: Duration::from_secs(2),
        })
    );
    let mut page = open_script(
        "return { render = function() return pencere.div() end, handle = function() local n = 0 repeat n += 1 until false end }",
        limits,
    );
    page.module.render(SIZE, 0.0, false);
    page.until(Duration::from_secs(20), |u| u.frame.is_some());
    page.module.event(Event::Input(Input::Key {
        key: Key::Character('x'),
        mods: Mods::default(),
    }));
    let update = page.until(Duration::from_secs(10), |u| u.closed.is_some());
    assert_eq!(update.closed, call_cut_off);
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_script_that_takes_memory_without_end_is_refused_it() {
    // The time of a call is long enough here that only the memory can stop it: how long 24 MiB take
    // to fill is the machine's, and on a busy one a short time would end the script first.
    let limits = ModuleLimits {
        memory: 24 << 20,
        call: Duration::from_secs(60),
        ..ModuleLimits::default()
    };
    let update = first(
        "local held = {}\nreturn { render = function() while true do held[#held + 1] = string.rep('y', 100000) .. #held end end }",
        limits,
    );
    assert_eq!(update.closed, Some(Fault::Memory));
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_script_that_recurses_without_end_is_closed_with_the_error_of_its_stack() {
    // The stack of the script runs out before the runtime's does: the page is closed with the
    // script's failure, whose words say so, and neither hangs nor takes the sandbox with it.
    let began = Instant::now();
    let update = first(
        "local function down(n) return 1 + down(n + 1) end\n\
         return { render = function() return pencere.div { pencere.text(tostring(down(1))) } end }",
        ModuleLimits::default(),
    );
    let why = failed(&update);
    assert!(why.contains("stack overflow"), "{why}");
    assert!(began.elapsed() < Duration::from_secs(20));
    // Also while the script starts.
    let update = first(
        "local function down(n) return 1 + down(n + 1) end\nlocal _ = down(1)",
        ModuleLimits::default(),
    );
    let why = failed(&update);
    assert!(why.contains("stack overflow"), "{why}");
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_script_can_reach_nothing_of_the_system() {
    let update = first(
        r#"
        local names = { "io", "os", "debug", "require", "loadstring", "load", "dofile", "loadfile",
            "getfenv", "setfenv", "newproxy", "collectgarbage", "gcinfo", "coroutine",
            "package", "bit32", "buffer", "vector", "utf8", "string", "table", "math" }
        local found = {}
        for _, name in ipairs(names) do
            if rawget(_G, name) ~= nil or _G[name] ~= nil then found[#found + 1] = name end
        end
        local more = {}
        for _, name in ipairs({ "string.dump", "math.randomseed" }) do
            local lib, fn = name:match("(%w+)%.(%w+)")
            if _G[lib][fn] ~= nil then more[#more + 1] = name end
        end
        return { render = function()
            return pencere.div():child(pencere.text(table.concat(found, " "))):child(pencere.text(table.concat(more, " ")))
                :child(pencere.text(type(print) .. " " .. type(warn)))
        end }
        "#,
        ModuleLimits::default(),
    );
    assert_eq!(update.closed, None, "{:?}", update.closed);
    let frame = update.frame.expect("a page");
    // Only the pure libraries are there, and none of their ways to reach out of the sandbox; and
    // `require`, which reaches the scripts of the bundle and nothing else (`luau_require_tests`).
    // `print` is not the base library's (the page has no standard output): `warn` and it are the
    // prelude's, which say to the program (`luau_print_tests`).
    assert_eq!(
        words(&frame.root),
        [
            "require utf8 string table math",
            "math.randomseed",
            "function function"
        ]
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_script_cannot_change_what_every_script_shares() {
    let update = first(
        r#"
        local ok1 = pcall(function() string.sub = nil end)
        local ok2 = pcall(function() pencere.div = nil end)
        local ok3 = pcall(function() _G.owned = 1 end)
        owned_by_me = 2

        -- What the prelude keeps to itself is not there to read.
        local hidden = {}
        for _, name in { "host", "_keymaps", "_lists", "_time", "start", "render", "settle", "handle", "message", "room" } do
            hidden[#hidden + 1] = name .. "=" .. tostring(pencere[name])
        end
        for _, name in { "_heard", "_asked" } do
            hidden[#hidden + 1] = "layout." .. name .. "=" .. tostring(pencere.layout[name])
        end
        hidden[#hidden + 1] = "f32=" .. tostring(pencere.f32)
        -- A timer has nothing a script can read but what it does.
        local timer = pencere.time.after(1, function() end)
        for _, name in { "fn", "waiting", "due", "delay", "every", "order" } do
            hidden[#hidden + 1] = "timer." .. name .. "=" .. tostring(timer[name])
        end
        local waited = timer:active()
        local stopped = timer:cancel()
        local timer_says = string.format("%s %s %s", tostring(waited), tostring(stopped), tostring(timer:active()))

        -- The tables it makes public, and the classes of what a script is handed, cannot be changed.
        local changed = {}
        local function held(name, change)
            if pcall(change) then changed[#changed + 1] = name end
        end
        held("json.decode", function() pencere.json.decode = nil end)
        held("json.encode", function() pencere.json.encode = function() return "" end end)
        held("json.new", function() pencere.json.extra = 1 end)
        held("json.null", function() pencere.json.null.extra = 1 end)
        held("json.null metatable", function() getmetatable(pencere.json.null).__tostring = nil end)
        held("json.array", function() pencere.json.array.extra = 1 end)
        held("layout.hit", function() pencere.layout.hit = nil end)
        held("layout.new", function() pencere.layout.extra = 1 end)
        held("time.after", function() pencere.time.after = nil end)
        held("Element", function() pencere.Element.child = nil end)
        held("an element's class", function() getmetatable(pencere.div()).child = nil end)
        held("a span's class", function() getmetatable(pencere.span("x")).look = nil end)
        held("a path's class", function() getmetatable(pencere.path()).fill = nil end)
        held("a timer's class", function() getmetatable(pencere.time.after(1, function() end)).cancel = nil end)
        -- What several pages are given is one table: the command of a default key, what the host draws.
        held("a default key's command", function() pencere.layout.keymap("e")[1].does.movement = "up" end)
        held("what the host draws", function() pencere.draws().shadow = true end)

        -- What a page makes with them is its own.
        local mine = pencere.div()
        local ok4 = pcall(function()
            mine.style.gap_x = 4
            mine.extra = true
        end)
        return { render = function()
            return pencere.div()
                :child(pencere.text(tostring(ok1) .. tostring(ok2) .. tostring(ok3) .. tostring(owned_by_me)))
                :child(pencere.text(table.concat(hidden, " ")))
                :child(pencere.text(#changed == 0 and "all held" or table.concat(changed, ", ")))
                :child(pencere.text(tostring(ok4)))
                :child(pencere.text(timer_says))
        end }
        "#,
        ModuleLimits::default(),
    );
    assert_eq!(update.closed, None, "{:?}", update.closed);
    // The libraries are read only; the script's own globals are its own.
    let said = words(&update.frame.expect("a page").root);
    assert!(said[0].starts_with("falsefalse"), "{}", said[0]);
    assert!(said[0].ends_with('2'), "{}", said[0]);
    assert_eq!(
        said[1],
        "host=nil _keymaps=nil _lists=nil _time=nil start=nil render=nil settle=nil handle=nil \
         message=nil room=nil layout._heard=nil layout._asked=nil f32=nil timer.fn=nil \
         timer.waiting=nil timer.due=nil timer.delay=nil timer.every=nil timer.order=nil"
    );
    assert_eq!(said[2], "all held");
    assert_eq!(said[3], "true");
    assert_eq!(
        said[4], "true true false",
        "a timer waits, is stopped, and is not waiting"
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn json_is_written_and_read_the_same_way_in_both_directions() {
    let update = first(
        r#"
        local json = pencere.json
        local text = json.encode({ b = { 1, 2.5, "x\n\"y\"" }, a = true, c = setmetatable({}, json.array), d = { z = 1 } })
        local back = json.decode(text)
        local bad, why = json.decode('{"a": [1, 2,')
        local uni = json.decode('"\\u00e9\\ud83d\\ude00"')
        return { render = function()
            return pencere.div():child(pencere.text(text)):child(pencere.text(tostring(back.b[3])))
                :child(pencere.text(tostring(bad) .. " " .. tostring(why))):child(pencere.text(uni))
        end }
        "#,
        ModuleLimits::default(),
    );
    assert_eq!(update.closed, None, "{:?}", update.closed);
    let said = words(&update.frame.expect("a page").root);
    assert_eq!(
        said[0],
        r#"{"a":true,"b":[1,2.5,"x\n\"y\""],"c":[],"d":{"z":1}}"#
    );
    assert_eq!(said[1], "x\n\"y\"");
    assert!(said[2].starts_with("nil "), "{}", said[2]);
    assert_eq!(said[3], "é😀");
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn json_nested_128_levels_deep_is_neither_read_nor_written() {
    // The protocol's rule (§4.5): 127 levels, the outermost counting as the first, and no more.
    let update = first(
        r#"
        local json = pencere.json
        local function nested(levels)
            return string.rep("[", levels) .. string.rep("]", levels)
        end
        local function value(levels)
            local inner = {}
            for _ = 2, levels do inner = { inner } end
            return inner
        end
        local ok127 = json.decode(nested(127)) ~= nil
        local bad128, why = json.decode(nested(128))
        local wrote127 = pcall(json.encode, value(127))
        local wrote128 = pcall(json.encode, value(128))
        return { render = function()
            return pencere.div():child(pencere.text(tostring(ok127))):child(pencere.text(tostring(bad128)))
                :child(pencere.text(tostring(why))):child(pencere.text(tostring(wrote127)))
                :child(pencere.text(tostring(wrote128)))
        end }
        "#,
        ModuleLimits::default(),
    );
    assert_eq!(update.closed, None, "{:?}", update.closed);
    let said = words(&update.frame.expect("a page").root);
    assert_eq!(said[0], "true");
    assert_eq!(said[1], "nil");
    assert!(said[2].contains("nested too deep"), "{}", said[2]);
    assert_eq!(said[3], "true");
    assert_eq!(said[4], "false");
}

/// A program that has room for so many of what the page says.
struct Room {
    left: AtomicUsize,
    nothing_left: AtomicBool,
}

impl Program for Room {
    fn may_say(&self, _bytes: usize) -> bool {
        let had = self.left.load(Ordering::SeqCst);
        if had == 0 {
            self.nothing_left.store(true, Ordering::SeqCst);
            return false;
        }
        self.left.store(had - 1, Ordering::SeqCst);
        true
    }

    fn taken(&self, _bytes: usize) {}
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_page_that_asks_its_program_things_is_answered_and_pushed_to() {
    let mut page = open_script(
        r#"
        local seen = "nothing yet"
        local answered = "no answer"
        pencere.subscribe("stats", nil, function(value) seen = "push " .. value.n; return true end)
        pencere.call("sum", { a = 1, b = 2 }, function(result, err)
            answered = err and ("error " .. err.code) or ("sum " .. result.total)
            return true
        end)
        return { render = function()
            return pencere.div():child(pencere.text(seen)):child(pencere.text(answered))
        end }
        "#,
        ModuleLimits::default(),
    );
    page.module.render(SIZE, 0.0, false);
    page.until(Duration::from_secs(20), |u| {
        u.frame.is_some() || u.closed.is_some()
    });
    // What the script asked at its start was said, in order, as the six messages are written.
    assert_eq!(
        page.seen.sent,
        [
            r#"{"subscribe":{"id":1,"topic":"stats"}}"#,
            r#"{"call":{"id":2,"method":"sum","params":{"a":1,"b":2}}}"#,
        ]
    );
    page.module
        .message(r#"{"push":{"id":1,"value":{"n":7}}}"#.into());
    page.module
        .message(r#"{"response":{"id":2,"result":{"total":3}}}"#.into());
    // A message that changed the page makes the page again, by itself.
    until_seen(&mut page, Duration::from_secs(5), |seen| {
        seen.frame
            .as_ref()
            .is_some_and(|frame| words(&frame.root) == ["push 7", "sum 3"])
    });
    let said = words(&page.seen.frame.clone().expect("a page").root);
    assert_eq!(said, ["push 7", "sum 3"]);
    // What nobody asked for, and what is not understood, is let go.
    page.module
        .message(r#"{"push":{"id":9,"value":{"n":1}}}"#.into());
    page.module.message("not json at all".into());
    let update = page.until(Duration::from_millis(300), |u| u.closed.is_some());
    assert_eq!(update.closed, None);
}

/// What the program says after the page gave a request up, or after a subscription ended, is not the
/// page's `message`; a subscription that fails tells the page once; what has a number the kit did
/// not give reaches `message`.
#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn what_the_program_says_of_a_request_that_is_over_is_not_told_to_the_page() {
    let mut page = open_script(
        r#"
        local seen = {}
        local function note(text) seen[#seen + 1] = text; return true end
        pencere.call("kept", nil, function() return note("kept answered") end)
        local gone = pencere.call("gone", nil, function() return note("gone answered") end)
        pencere.subscribe("refused", nil, function() return note("refused pushed") end,
            function(err) return note("refused " .. err.code) end)
        pencere.subscribe("silent", nil, function() return note("silent pushed") end)
        pencere.subscribe("live", nil, function() return note("live pushed") end)
        local ended = pencere.subscribe("ended", nil, function() return note("ended pushed") end)
        pencere.cancel(gone)
        pencere.cancel(ended)
        return {
            render = function() return pencere.div():child(pencere.text(table.concat(seen, ","))) end,
            message = function(message)
                local body = message.response or message.push or message.progress
                return note("message " .. tostring(body and body.id))
            end,
        }
        "#,
        ModuleLimits::default(),
    );
    page.module.render(SIZE, 0.0, false);
    page.until(Duration::from_secs(20), |u| {
        u.frame.is_some() || u.closed.is_some()
    });
    assert_eq!(page.seen.sent.len(), 8, "{:?}", page.seen.sent);
    for message in [
        // The call the page gave up: its answer, its progress, and a push that is not of a call.
        r#"{"response":{"id":2,"result":{}}}"#,
        r#"{"progress":{"id":2,"done":1}}"#,
        r#"{"push":{"id":2,"value":{}}}"#,
        // The subscription that ended: what the program still says of it.
        r#"{"push":{"id":6,"value":{}}}"#,
        r#"{"response":{"id":6,"error":{"code":"internal","message":"no"}}}"#,
        // A subscription the program refuses: told once, then over.
        r#"{"response":{"id":3,"error":{"code":"not-found","message":"no such topic"}}}"#,
        r#"{"response":{"id":3,"error":{"code":"not-found","message":"no such topic"}}}"#,
        r#"{"push":{"id":3,"value":{}}}"#,
        // One without an `on_error` ends without a word.
        r#"{"response":{"id":4,"error":{"code":"internal","message":"no"}}}"#,
        r#"{"push":{"id":4,"value":{}}}"#,
        // A subscription is answered by an error and by nothing else: it goes on.
        r#"{"response":{"id":5,"result":{}}}"#,
        r#"{"push":{"id":5,"value":{}}}"#,
        // A number the kit did not give, and what is not of a request: the page's.
        r#"{"response":{"id":50,"result":{}}}"#,
        r#"{"hello":1}"#,
    ] {
        page.module.message(message.into());
    }
    let expected = "refused not-found,live pushed,message 50,message nil";
    until_seen(&mut page, Duration::from_secs(5), |seen| {
        seen.frame
            .as_ref()
            .is_some_and(|frame| words(&frame.root) == [expected])
    });
    let said = words(&page.seen.frame.clone().expect("a page").root);
    assert_eq!(said, [expected], "{:?}", page.seen.closed);
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_page_told_full_keeps_what_it_meant_to_say_until_it_is_called_with_room() {
    let program = Arc::new(Room {
        left: AtomicUsize::new(1),
        nothing_left: AtomicBool::new(false),
    });
    let mut page = open_script_for(
        r#"
        for n = 1, 3 do pencere.call("part", { n = n }) end
        return { render = function() return pencere.div() end,
            room = function() return true end }
        "#,
        ModuleLimits::default(),
        program.clone(),
    );
    page.module.render(SIZE, 0.0, false);
    page.until(Duration::from_secs(20), |u| u.frame.is_some());
    // The program took one; the page was told full for the second and keeps it and the third.
    assert_eq!(
        page.seen.sent,
        [r#"{"call":{"id":1,"method":"part","params":{"n":1}}}"#]
    );
    assert!(program.nothing_left.load(Ordering::SeqCst));
    program.left.store(10, Ordering::SeqCst);
    page.module.room();
    let update = page.until(Duration::from_secs(5), |u| !u.sent.is_empty());
    assert_eq!(update.closed, None);
    assert_eq!(
        page.seen.sent[1..],
        [
            r#"{"call":{"id":2,"method":"part","params":{"n":2}}}"#,
            r#"{"call":{"id":3,"method":"part","params":{"n":3}}}"#,
        ]
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_page_that_says_a_great_deal_is_not_closed_for_it() {
    // More than a call may say is said over several calls; the page asks to be drawn again
    // until it is all said.
    let mut page = open_script(
        "for n = 1, 100 do pencere.call('part', { n = n }) end\nreturn { render = function() return pencere.div() end }",
        ModuleLimits::default(),
    );
    let mut rounds = 0;
    while page.seen.sent.len() < 100 && rounds < 50 {
        rounds += 1;
        page.module.render(SIZE, 0.0, false);
        let update = page.until(Duration::from_secs(10), |u| {
            u.frame.is_some() || u.closed.is_some()
        });
        assert_eq!(update.closed, None);
    }
    assert_eq!(page.seen.sent.len(), 100);
    assert!(
        page.seen.frame.is_some_and(|f| !f.animating),
        "all said, so at rest"
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn what_a_script_hands_out_that_is_not_text_is_made_into_text() {
    // A string in Lua is any bytes; the application is only handed text.
    let update = first(
        "return { render = function() return pencere.div():child(pencere.text('a\\xffb\\xc3')) end }",
        ModuleLimits::default(),
    );
    assert_eq!(update.closed, None);
    assert_eq!(
        words(&update.frame.expect("a page").root),
        ["a\u{fffd}b\u{fffd}"]
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn the_compiled_interpreter_is_kept_and_the_next_page_starts_from_it() {
    // The first page of a process compiles the interpreter; the next reads what was left.
    let began = Instant::now();
    let _ = first(
        "return { render = function() return pencere.div() end }",
        ModuleLimits::default(),
    );
    let compiled = began.elapsed();
    let began = Instant::now();
    let update = first(
        "return { render = function() return pencere.div() end }",
        ModuleLimits::default(),
    );
    let kept = began.elapsed();
    eprintln!(
        "a page after the interpreter was compiled: {kept:?} (a page that may have compiled it: {compiled:?})"
    );
    assert_eq!(update.closed, None);
    assert!(kept < Duration::from_secs(5), "{kept:?}");
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn errors_a_script_catches_are_not_kept() {
    // An error is an exception in the interpreter, an object on the runtime's collected heap:
    // a script that catches them by the hundred thousand does not run the heap out.
    // (What is under test is the heap and not the speed: a call may take as long as a loaded
    // machine makes it.)
    let limits = ModuleLimits {
        call: Duration::from_secs(10),
        ..ModuleLimits::default()
    };
    let mut page = open_script(
        "return { render = function() for n = 1, 5000 do pcall(error, 'again') end return pencere.div() end }",
        limits,
    );
    for step in 0..60 {
        page.module.render(SIZE, f64::from(step), false);
        let update = page.until(Duration::from_secs(20), |u| {
            u.frame.is_some() || u.closed.is_some()
        });
        assert_eq!(update.closed, None, "after {} errors", step * 5000);
    }
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn the_example_page_counts_clicks() {
    let mut page = open_script(
        include_str!("../../luau/examples/hello.lua"),
        ModuleLimits::default(),
    );
    page.module.render(SIZE, 0.0, false);
    let update = page.until(Duration::from_secs(20), |u| {
        u.frame.is_some() || u.closed.is_some()
    });
    assert_eq!(update.closed, None);
    let frame = update.frame.expect("a page");
    assert_eq!(words(&frame.root), ["Clicked 0 times", "Click me"]);
    // Laid out, the button is where the page put it: a press and a release on it is a click.
    let laid = Arc::new(pencere_view::layout(
        frame.root.clone(),
        SIZE,
        1.0,
        &mut Rough,
    ));
    page.module.lay_out(Arc::clone(&laid));
    let button = laid
        .bounds(Id::named("counter button"))
        .expect("the button is laid out");
    let at = Point::new(button.x + button.w / 2.0, button.y + button.h / 2.0);
    for input in [
        Input::Move(at),
        Input::Press {
            at,
            button: Button::Left,
            mods: Mods::default(),
            count: 1,
        },
        Input::Release {
            at,
            button: Button::Left,
        },
    ] {
        page.module.event(Event::Input(input));
    }
    until_seen(&mut page, Duration::from_secs(10), |seen| {
        seen.replies.len() >= 3
    });
    assert_eq!(page.seen.replies.len(), 3);
    // The pointer moving onto the button, and the click, change the page; the press does not.
    let redraws: Vec<bool> = page.seen.replies.iter().map(|a| a.reply.redraw).collect();
    assert_eq!(redraws, [true, false, true]);
    let frame = page.seen.frame.clone().expect("the page after the click");
    assert_eq!(words(&frame.root), ["Clicked 1 times", "Click me"]);
}

/// What an event asks of the program past what one call may say goes out in the frames after it,
/// with nothing else asking than the host's own frames: twenty calls, eight a call, and all of
/// them said, in order.
#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn what_an_event_asks_past_a_calls_share_goes_out_in_the_frames_after_it() {
    let mut page = open_script(
        "return {\n\
           render = function() return pencere.div() end,\n\
           handle = function() for i = 1, 20 do pencere.call('ping', { n = i }) end end,\n\
         }",
        ModuleLimits::default(),
    );
    page.module.render(SIZE, 0.0, false);
    until_seen(&mut page, Duration::from_secs(20), |seen| {
        seen.frame.is_some()
    });
    page.module.event(Event::Input(Input::Key {
        key: Key::Enter,
        mods: Mods::default(),
    }));
    // The host's side: a page that says it is moving is rendered again, frame after frame.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut now = 0.0;
    while page.seen.sent.len() < 20 && Instant::now() < deadline {
        page.take(Duration::from_millis(50));
        if page
            .seen
            .frame
            .as_ref()
            .is_some_and(|frame| frame.animating)
        {
            now += 0.016;
            page.seen.frame = None;
            page.module.render(SIZE, now, false);
        }
    }
    let numbers: Vec<_> = page
        .seen
        .sent
        .iter()
        .filter_map(|message| {
            let at = message.find("\"n\":")? + 4;
            message[at..]
                .split(|c: char| !c.is_ascii_digit())
                .next()?
                .parse::<u32>()
                .ok()
        })
        .collect();
    assert_eq!(
        numbers,
        (1..=20).collect::<Vec<_>>(),
        "{:?}",
        page.seen.sent
    );
    assert!(page.seen.closed.is_none());
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_script_says_how_an_element_moves_and_what_its_tip_says() {
    let update = first(
        r##"return { render = function()
            return pencere.div()
                :child(pencere.div():id("row"):motion({
                    seconds = 0.35, ease = "spring", color = true, place = true,
                    enter = { opacity = 0, dy = 6, seconds = 0.28 }, exit = { dy = -3 },
                }))
                :child(pencere.div():press("sec:containers"):tip("Containers", "1"))
                :child(pencere.div():id("plain"):motion({ size = true }):tip("Only words"))
        end }"##,
        ModuleLimits::default(),
    );
    assert_eq!(update.closed, None);
    let nodes = update.frame.expect("a page").root.fold();
    assert_eq!(
        nodes[1].meta.motion,
        Some(
            Motion::new(0.35)
                .ease(Ease::Spring)
                .color()
                .place()
                .enter(Shift::fade(0.28).by(0.0, 6.0))
                .exit(Shift::fade(0.35).by(0.0, -3.0))
        )
    );
    assert_eq!(
        nodes[2].meta.tip,
        Some(Tip {
            words: "Containers".into(),
            keys: Some("1".into())
        })
    );
    assert_eq!(nodes[3].meta.motion, Some(Motion::new(0.2).size()));
    assert_eq!(
        nodes[3].meta.tip.as_ref().map(|tip| tip.keys.clone()),
        Some(None)
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_script_mixes_colours_fades_elements_names_fields_and_sets_its_words_apart() {
    let update = first(
        r##"return { render = function()
            return pencere.div()
                :child(pencere.text("brighter"):color(pencere.mix("dim", "foreground", 0.25)))
                :child(pencere.div():bg(pencere.alpha(pencere.mix("raised1", { light = "#102030" }, 0.5), 0.5)))
                :child(pencere.div():opacity(0.4):italic():underline():strike():child(pencere.text("set apart")))
                :child(pencere.div():text_field("name"):child(pencere.text("Your name")))
                :child(pencere.div():text_field("password", "secret"):label("Password"))
                :child(pencere.div():id("notes"):text_field(nil, "lines"))
                :child(pencere.div():press("restart"):label("Restart"):scroll())
        end }"##,
        ModuleLimits::default(),
    );
    assert_eq!(update.closed, None);
    let nodes = update.frame.expect("a page").root.fold();
    assert_eq!(
        nodes[1].style.text.color,
        Some(Color {
            source: Source::Role(Role::Dim),
            mix: Some(Mix {
                toward: Source::Role(Role::Foreground),
                amount: 0.25
            }),
            alpha: 1.0
        })
    );
    assert_eq!(
        nodes[2].style.fill,
        Some(Color {
            source: Source::Role(Role::Raised(1)),
            mix: Some(Mix {
                toward: Source::Rgba {
                    light: Rgba::rgb(0x10, 0x20, 0x30),
                    dark: None
                },
                amount: 0.5
            }),
            alpha: 0.5
        }),
        "an opacity on a mix keeps the mix"
    );
    let apart = &nodes[3].style;
    assert_eq!(apart.opacity, Some(0.4));
    assert_eq!(
        (apart.text.italic, apart.text.underline, apart.text.strike),
        (Some(true), Some(true), Some(true))
    );
    let field = |at: usize| {
        (
            nodes[at].meta.id,
            nodes[at].meta.field,
            nodes[at].meta.label.clone(),
        )
    };
    assert_eq!(
        field(5),
        (Some(Id::named("name")), Some(FieldKind::Line), None)
    );
    assert_eq!(
        field(7),
        (
            Some(Id::named("password")),
            Some(FieldKind::Secret),
            Some("Password".to_owned())
        )
    );
    assert_eq!(
        field(8),
        (Some(Id::named("notes")), Some(FieldKind::Lines), None)
    );
    assert_eq!(nodes[9].meta.label.as_deref(), Some("Restart"));
    assert_eq!(nodes[9].style.overflow, Overflow::Scroll);
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_script_is_told_the_new_events_and_answers_with_fields_and_cursors() {
    let mut page = open_script(
        r##"
        local said = "nothing"
        return {
            render = function() return pencere.div():child(pencere.text(said)) end,
            handle = function(e)
                if e.type == "press" then
                    said = "press " .. e.count
                    return { redraw = true, cursor = "resize_horizontal", keyboard = { field = "name" } }
                elseif e.type == "scroll" then
                    said = "scroll " .. e.dx .. " " .. e.dy
                    return { redraw = true, cursor = "not_allowed", keyboard = "page" }
                elseif e.type == "key" then
                    said = "key " .. e.key
                    return { redraw = true, cursor = "grab" }
                elseif e.type == "text" then
                    said = "text " .. e.field .. " " .. e.text
                    return { redraw = true, fill = { name = "" } }
                elseif e.type == "focus" then
                    said = "focus " .. tostring(e.field)
                    if e.field then
                        return {
                            redraw = true,
                            fill = {
                                name = { text = "draft.md", select = { from = 1, to = 6 } },
                                notes = { select = true },
                            },
                        }
                    end
                    return true
                end
            end,
        }
        "##,
        ModuleLimits::default(),
    );
    page.module.render(SIZE, 0.0, false);
    page.until(Duration::from_secs(20), |u| {
        u.frame.is_some() || u.closed.is_some()
    });
    let at = Point::new(1.0, 1.0);
    let events = [
        Event::Input(Input::Press {
            at,
            button: Button::Left,
            mods: Mods::default(),
            count: 2,
        }),
        Event::Input(Input::Scroll {
            at,
            dx: 3.0,
            dy: -4.0,
        }),
        Event::Input(Input::Key {
            key: Key::Backspace,
            mods: Mods::default(),
        }),
        Event::Input(Input::Key {
            key: Key::Function(5),
            mods: Mods::default(),
        }),
        Event::Text {
            field: Id::named("name"),
            text: "ada".into(),
        },
        Event::Focus(Some(Id::named("name"))),
        Event::Focus(None),
    ];
    let mut heard = Vec::new();
    for event in events {
        page.module.event(event);
        page.module.render(SIZE, 0.0, false);
        let before = page.seen.replies.len();
        until_seen(&mut page, Duration::from_secs(10), |seen| {
            seen.replies.len() > before
        });
        page.take(Duration::from_millis(200));
        heard.push(words(&page.seen.frame.clone().expect("a page").root).join(" "));
    }
    assert_eq!(
        heard,
        [
            "press 2",
            "scroll 3 -4",
            "key backspace",
            "key f5",
            "text name ada",
            "focus name",
            "focus nil"
        ]
    );
    let replies: Vec<_> = page.seen.replies.iter().map(|a| a.reply.clone()).collect();
    assert_eq!(replies[0].cursor, Cursor::ResizeHorizontal);
    assert_eq!(
        replies[0].keyboard,
        Some(Keyboard::Field(Id::named("name")))
    );
    assert_eq!(replies[1].cursor, Cursor::NotAllowed);
    assert_eq!(replies[1].keyboard, Some(Keyboard::Page));
    assert_eq!(replies[2].cursor, Cursor::Grab);
    assert_eq!(replies[4].fill, [(Id::named("name"), Fill::text(""))]);
    let mut filled = replies[5].fill.clone();
    filled.sort_by_key(|(field, _)| *field != Id::named("name"));
    assert_eq!(
        filled,
        [
            (Id::named("name"), Fill::text("draft.md").select(0..5)),
            (Id::named("notes"), Fill::selection(0..u32::MAX as usize)),
        ]
    );
}

/// A menu's entry has keys written as a command's (`keys`); one a page wrote as `key` before is
/// read as keys where it is some, and as its first character where it is not, as it was.
#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_script_gives_a_menus_entries_their_keys_and_reads_an_old_key_as_it_was() {
    use pencere_view::{MenuEntry, Shortcut};
    let mut page = open_script(
        r##"
        return {
            render = function() return pencere.div() end,
            handle = function(e)
                if e.type == "press" then
                    return { menu = { at = { x = 1, y = 1 }, entries = {
                        { title = "Close", keys = "ctrl+w" },
                        { title = "Rename", key = "f2" },
                        { title = "Old", key = "⌘W" },
                        "-",
                    } } }
                end
            end,
        }
        "##,
        ModuleLimits::default(),
    );
    page.module.render(SIZE, 0.0, false);
    page.until(Duration::from_secs(20), |u| {
        u.frame.is_some() || u.closed.is_some()
    });
    page.module.event(Event::Input(Input::Press {
        at: Point::new(1.0, 1.0),
        button: Button::Left,
        mods: Mods::default(),
        count: 1,
    }));
    until_seen(&mut page, Duration::from_secs(10), |seen| {
        seen.replies
            .iter()
            .any(|answer| answer.reply.menu.is_some())
    });
    let menu = page
        .seen
        .replies
        .iter()
        .find_map(|answer| answer.reply.menu.clone())
        .expect("a menu");
    let keys: Vec<Option<Shortcut>> = menu
        .entries
        .iter()
        .map(|entry| match entry {
            MenuEntry::Item { keys, .. } | MenuEntry::Paste { keys, .. } => *keys,
            MenuEntry::Separator => None,
        })
        .collect();
    assert_eq!(
        keys,
        [
            Shortcut::parse("ctrl+w"),
            Shortcut::parse("f2"),
            Some(Shortcut::new(Key::Character('⌘'), Mods::default())),
            None,
        ]
    );
}

/// A pick is told the script with the entry of the menu it was a pick of, once: one of a menu the
/// script did not ask for, of one picked from already, or of a line that is no entry is not told,
/// and the script never sees the number the menu was given.
#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_pick_is_told_the_script_with_the_entry_of_its_menu_and_only_once() {
    let mut page = open_script(
        r##"
        return {
            render = function() return pencere.div() end,
            handle = function(e)
                if e.type == "press" then
                    return { menu = { at = { x = 1, y = 1 }, entries = {
                        { title = "Copy", says = "copy" }, "-", { title = "Cut", says = "cut" },
                    } } }
                elseif e.type == "menu" then
                    return { copy = e.entry.says .. " " .. e.index .. " " .. tostring(e.serial) }
                end
            end,
        }
        "##,
        ModuleLimits::default(),
    );
    page.module.render(SIZE, 0.0, false);
    page.until(Duration::from_secs(20), |u| {
        u.frame.is_some() || u.closed.is_some()
    });
    let told = |page: &mut Open, event: Event| {
        let before = page.seen.replies.len();
        page.module.event(event);
        until_seen(page, Duration::from_secs(10), |seen| {
            seen.replies.len() > before
        });
        page.seen.replies[before..]
            .iter()
            .filter_map(|answer| answer.reply.copy.clone())
            .collect::<Vec<_>>()
    };
    let asked = told(
        &mut page,
        Event::Input(Input::Press {
            at: Point::new(1.0, 1.0),
            button: Button::Left,
            mods: Mods::default(),
            count: 1,
        }),
    );
    assert!(asked.is_empty());
    let menu = page
        .seen
        .replies
        .iter()
        .find_map(|answer| answer.reply.menu.clone())
        .expect("a menu");
    let pick = |serial, index| Event::MenuPick { serial, index };
    // A menu the script did not ask for, a line that is no entry, and an entry past the end.
    assert!(told(&mut page, pick(menu.serial + 1, 0)).is_empty());
    assert!(told(&mut page, pick(0, 0)).is_empty());
    assert!(told(&mut page, pick(menu.serial, 1)).is_empty());
    assert!(told(&mut page, pick(menu.serial, 3)).is_empty());
    // The entry of the menu, with its place from 1 and no number.
    assert_eq!(
        told(&mut page, pick(menu.serial, 2)),
        ["cut 3 nil".to_owned()]
    );
    // A menu is picked from once.
    assert!(told(&mut page, pick(menu.serial, 0)).is_empty());
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_script_asks_to_open_a_link_and_only_one_the_host_may_open_reaches_it() {
    let mut page = open_script(
        r##"
        return {
            render = function()
                return pencere.rich({ "see ", pencere.span("the docs"):link("docs") })
            end,
            handle = function(e)
                if e.type == "key" and e.key == "d" then
                    return { link = "https://example.com/docs" }
                elseif e.type == "key" and e.key == "f" then
                    return { redraw = true, cursor = "pointer", link = "file:///etc/passwd" }
                elseif e.type == "key" and e.key == "n" then
                    return { link = 5 }
                end
            end,
            message = function(message) return { link = message.open } end,
        }
        "##,
        ModuleLimits::default(),
    );
    page.module.render(SIZE, 0.0, false);
    page.until(Duration::from_secs(20), |u| {
        u.frame.is_some() || u.closed.is_some()
    });
    let key = |page: &mut Open, character: char| {
        let before = page.seen.replies.len();
        page.module.event(Event::Input(Input::Key {
            key: Key::Character(character),
            mods: Mods::default(),
        }));
        until_seen(page, Duration::from_secs(10), |seen| {
            seen.replies.len() > before
        });
        page.seen.replies.last().expect("an answer").reply.clone()
    };
    // An address the host may open reaches it as the reply's link, and nothing is said to the
    // program: a link is the host's, not one of the six messages.
    let opened = key(&mut page, 'd');
    assert_eq!(opened.link.as_deref(), Some("https://example.com/docs"));
    // One it may not open is dropped, as a web page's is; the rest of the reply stands, and the
    // page is not closed for it.
    let dropped = key(&mut page, 'f');
    assert_eq!(dropped.link, None);
    assert!(dropped.redraw);
    assert_eq!(dropped.cursor, Cursor::Pointer);
    // A reply to what the program said is the same: the address it sent is opened.
    let before = page.seen.replies.len();
    page.module
        .message(r#"{"open":"https://example.com/from-the-program"}"#.into());
    until_seen(&mut page, Duration::from_secs(10), |seen| {
        seen.replies.len() > before
    });
    assert_eq!(
        page.seen
            .replies
            .last()
            .expect("an answer")
            .reply
            .link
            .as_deref(),
        Some("https://example.com/from-the-program")
    );
    assert!(page.seen.sent.is_empty(), "{:?}", page.seen.sent);
    // What is not an address at all is the script's mistake.
    page.module.event(Event::Input(Input::Key {
        key: Key::Character('n'),
        mods: Mods::default(),
    }));
    let update = page.until(Duration::from_secs(5), |u| u.closed.is_some());
    let why = failed(&update);
    assert!(why.contains("link is a string"), "{why}");
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_script_says_texts_of_many_looks_canvases_overlays_commands_and_roles_as_the_engine_does() {
    use pencere_view::{
        AccessibleRole, Align, Command, Family, Shortcut, Side, rich, span, wire::Length,
    };
    let update = first(
        r##"return { render = function()
            return pencere.div()
                :command("quit", "Quit", "cmd+q")
                :child(pencere.rich({
                    "run ",
                    pencere.span("make"):family("terminal"):bg("raised2"):bold(),
                    pencere.span(" docs"):link("docs"):color("accent"),
                }):wrap())
                :child(pencere.canvas({
                    pencere.path():rect(0, 0, 4, 8):fill("raised3"),
                    pencere.path():arc(10, 10, 8, 0, 180):stroke(2, "accent"),
                }):size(20, 20))
                :child(pencere.div():id("menu"):anchor("button", "above", { gap = 4, align = "end" }))
                :child(pencere.div():overlay():inset(0, 0, 0, 0):bg_gradient(90, "accent", "background"))
                :child(pencere.div():press("group"):role("switch"):checked(true):max_h(30))
                :child(pencere.text("Title"):role("heading", 9))
                :child(pencere.div():role("progress"):value(40):disabled())
                :child(pencere.div()
                    :focused()
                    :command("find", "Find", "cmd+shift+f", false)
                    :command("go", "Go", { "ctrl+pagedown", "ctrl+alt+right", "cmd++" })
                    :draggable("card"))
                :child(pencere.div():drop_target("done"):selected(true):expanded(false))
        end }"##,
        ModuleLimits::default(),
    );
    assert_eq!(update.closed, None, "{:?}", update.closed);
    let nodes = update.frame.expect("a page").root.fold();
    let root = &nodes[0].meta;
    assert_eq!(
        root.commands,
        vec![Command::new(Id::named("quit"), "Quit").keys(Shortcut::parse("cmd+q").unwrap())]
    );
    let expected = rich([
        span("run "),
        span("make")
            .family(Family::Terminal)
            .bg(Role::Raised(2))
            .bold(),
        span(" docs").link(Id::named("docs")).color(Role::Accent),
    ]);
    let Kind::Rich(said) = &nodes[1].kind else {
        panic!("a text of many looks: {:?}", nodes[1].kind);
    };
    let Kind::Rich(rust) = expected.fold().remove(0).kind else {
        panic!("rich");
    };
    assert_eq!(said, &rust);
    assert!(nodes[1].style.wrap);
    let Kind::Canvas(paths) = &nodes[2].kind else {
        panic!("a canvas: {:?}", nodes[2].kind);
    };
    assert_eq!(paths.len(), 2);
    assert_eq!(paths[0].fill, Some(Role::Raised(3).solid()));
    assert_eq!(paths[0].segments.len(), 5);
    // Half a turn: a move and two quarter curves, ending at six o'clock.
    assert_eq!(paths[1].segments.len(), 3);
    let pencere_view::Segment::Cubic { to, .. } = paths[1].segments[2] else {
        panic!("a curve");
    };
    assert!(
        (to.x - 10.0).abs() < 1e-3 && (to.y - 18.0).abs() < 1e-3,
        "{to:?}"
    );
    assert_eq!(paths[1].stroke.map(|stroke| stroke.width), Some(2.0));
    let anchor = nodes[3].style.anchor.expect("an anchor");
    assert!(nodes[3].style.overlay);
    assert_eq!(
        (anchor.to, anchor.side, anchor.align, anchor.gap),
        (Id::named("button"), Side::Above, Align::End, 4.0)
    );
    assert!(nodes[4].style.overlay);
    assert_eq!(nodes[4].style.inset, [Some(0.0); 4]);
    let gradient = nodes[4].style.gradient.expect("a gradient");
    assert_eq!(
        (gradient.angle, gradient.from, gradient.to),
        (90.0, Role::Accent.solid(), Role::Background.solid())
    );
    assert_eq!(nodes[5].meta.role, Some(AccessibleRole::Switch));
    assert_eq!(nodes[5].meta.state.checked, Some(true));
    assert_eq!(nodes[5].style.max_height, Length::Px(30.0));
    assert_eq!(nodes[6].meta.role, Some(AccessibleRole::Heading(6)));
    assert_eq!(nodes[7].meta.state.value.as_deref(), Some("40"));
    assert!(nodes[7].meta.state.disabled);
    let focused = &nodes[8].meta;
    assert!(focused.focused && focused.draggable);
    assert_eq!(focused.id, Some(Id::named("card")));
    assert_eq!(
        focused.commands,
        vec![
            Command::new(Id::named("find"), "Find")
                .keys(Shortcut::parse("cmd+shift+f").unwrap())
                .enabled(false),
            Command::new(Id::named("go"), "Go")
                .keys(Shortcut::parse("ctrl+page_down").unwrap())
                .keys(Shortcut::parse("ctrl+alt+right").unwrap())
                .keys(Shortcut::parse("cmd++").unwrap()),
        ]
    );
    assert!(nodes[9].meta.drop_target);
    assert!(nodes[9].meta.state.selected);
    assert_eq!(nodes[9].meta.state.expanded, Some(false));
}

/// A flag is on when it is called with nothing and what it is told otherwise, so that a field with
/// nothing in it (`:selected(row.chosen)`) is off and not on.
#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_flag_is_on_when_called_with_nothing_and_what_it_is_told_otherwise() {
    let update = first(
        r##"
        local FLAGS = {
            "selected", "disabled", "focused", "focusable", "hidden", "truncate", "wrap",
            "absolute", "overlay", "checked", "expanded", "italic", "underline", "strike",
        }
        return { render = function()
            local root = pencere.div()
            for _, flag in FLAGS do
                local function made(form) return pencere.div():id(flag .. form) end
                root:child(made(1)[flag](made(1)))
                root:child(made(2)[flag](made(2), true))
                root:child(made(3)[flag](made(3), false))
                root:child(made(4)[flag](made(4), nil))
            end
            return root
        end }"##,
        ModuleLimits::default(),
    );
    assert_eq!(update.closed, None, "{:?}", update.closed);
    let nodes = update.frame.expect("a page").root.fold();
    let forms = |flag: usize| [0, 1, 2, 3].map(|form| &nodes[1 + flag * 4 + form]);
    let on = [true, true, false, false];
    assert_eq!(forms(0).map(|n| n.meta.state.selected), on, "selected");
    assert_eq!(forms(1).map(|n| n.meta.state.disabled), on, "disabled");
    assert_eq!(forms(2).map(|n| n.meta.focused), on, "focused");
    assert_eq!(forms(3).map(|n| n.meta.focusable), on, "focusable");
    assert_eq!(
        forms(4).map(|n| n.style.display == Display::None),
        on,
        "hidden"
    );
    assert_eq!(forms(5).map(|n| n.style.truncate), on, "truncate");
    assert_eq!(forms(6).map(|n| n.style.wrap), on, "wrap");
    assert_eq!(forms(7).map(|n| n.style.absolute), on, "absolute");
    assert_eq!(forms(8).map(|n| n.style.overlay), on, "overlay");
    let said = on.map(Some);
    assert_eq!(forms(9).map(|n| n.meta.state.checked), said, "checked");
    assert_eq!(forms(10).map(|n| n.meta.state.expanded), said, "expanded");
    assert_eq!(forms(11).map(|n| n.style.text.italic), said, "italic");
    assert_eq!(forms(12).map(|n| n.style.text.underline), said, "underline");
    assert_eq!(forms(13).map(|n| n.style.text.strike), said, "strike");
}

/// A flag that is off takes back only what it said itself, whatever else was said of the element
/// and in whichever order; a part of a text says what it is told too.
#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_flag_that_is_off_takes_back_only_what_it_said() {
    let update = first(
        r##"return { render = function()
            local span = pencere.span
            return pencere.div()
                :child(pencere.div():row():hidden())
                :child(pencere.div():hidden():row())
                :child(pencere.div():row():hidden(false))
                :child(pencere.div():grid({ 10 }):hidden(false))
                :child(pencere.div():id("scrolled"):scroll():clip(false))
                :child(pencere.div():id("back"):scroll():scroll(false))
                :child(pencere.div():clip():clip(false))
                :child(pencere.div():clip():scroll(false))
                :child(pencere.div():no_shrink():no_shrink(false))
                :child(pencere.div():no_shrink():no_shrink(true))
                :child(pencere.rich({
                    span("off"):italic(false):underline(false):strike(false),
                    span("before"):underline(false):link("before"),
                    span("after"):link("after"):underline(false),
                    span("plain"):link("plain"),
                    span("light"):bold():bold(false),
                    span("heavy"):bold(true),
                }))
        end }"##,
        ModuleLimits::default(),
    );
    assert_eq!(update.closed, None, "{:?}", update.closed);
    let nodes = update.frame.expect("a page").root.fold();
    assert_eq!(nodes[1].style.display, Display::None);
    assert_eq!(
        nodes[2].style.display,
        Display::None,
        "whichever came first"
    );
    assert_eq!(nodes[3].style.display, Display::Flex);
    assert_eq!(nodes[3].style.direction, Direction::Row, "its row is kept");
    assert_eq!(nodes[4].style.display, Display::Grid, "its grid is kept");
    assert_eq!(nodes[5].style.overflow, Overflow::Scroll);
    assert_eq!(nodes[6].style.overflow, Overflow::Visible);
    assert_eq!(nodes[7].style.overflow, Overflow::Visible);
    assert_eq!(nodes[8].style.overflow, Overflow::Hidden);
    assert_eq!(nodes[9].style.shrink, None);
    assert_eq!(nodes[10].style.shrink, Some(0.0));
    let Kind::Rich(spans) = &nodes[11].kind else {
        panic!("a text of many looks: {:?}", nodes[11].kind);
    };
    assert_eq!(
        (spans[0].italic, spans[0].underline, spans[0].strike),
        (Some(false), Some(false), Some(false))
    );
    assert_eq!(spans[1].underline, Some(false), "a link that says not");
    assert_eq!(spans[2].underline, Some(false), "whichever came first");
    assert_eq!(spans[3].underline, Some(true), "a link is underlined");
    assert_eq!(spans[4].weight, None);
    assert_eq!(spans[5].weight, Some(700));
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_script_drags_and_is_told_commands_keys_that_go_up_modifiers_and_dropped_files() {
    let mut page = open_script(
        r##"
        local ui = pencere.Ui.new()
        local said = "nothing"
        return {
            render = function()
                return pencere.div():row()
                    :command("find", "Find", "cmd+f")
                    :child(pencere.div():drop_target("todo"):size(50, 100)
                        :child(pencere.div():draggable("card"):size(40, 20)))
                    :child(pencere.div():drop_target("done"):size(50, 100))
                    :child(pencere.text(said))
            end,
            handle = function(event)
                local laid = pencere.layout
                local at = event.x and { x = event.x, y = event.y }
                if event.type == "move" then
                    ui:point(laid, at)
                    local drag = ui:dragging()
                    if drag then
                        said = "dragging " .. drag.source .. " over " .. tostring(drag.target)
                    end
                elseif event.type == "press" then
                    ui:press(laid, at)
                elseif event.type == "release" then
                    ui:release(laid, at)
                    local dropped = ui:take_drop()
                    if dropped then
                        said = "dropped " .. dropped.source .. " on " .. tostring(dropped.target)
                    else
                        said = "released " .. event.button
                    end
                elseif event.type == "command" then
                    said = "command " .. event.id
                elseif event.type == "key_up" then
                    said = "up " .. event.key
                elseif event.type == "mods" then
                    said = "mods alt " .. tostring(event.mods.alt)
                elseif event.type == "files" then
                    said = "files " .. #event.paths .. " " .. event.paths[1] .. " at " .. event.x
                end
                return true
            end,
        }
        "##,
        ModuleLimits::default(),
    );
    page.module.render(SIZE, 0.0, false);
    let update = page.until(Duration::from_secs(20), |u| {
        u.frame.is_some() || u.closed.is_some()
    });
    assert_eq!(update.closed, None, "{:?}", update.closed);
    let frame = update.frame.expect("a page");
    let laid = Arc::new(pencere_view::layout(
        frame.root.clone(),
        SIZE,
        1.0,
        &mut Rough,
    ));
    page.module.lay_out(Arc::clone(&laid));
    let told = |event: Event, page: &mut Open| -> String {
        page.module.event(event);
        page.module.render(SIZE, 0.0, false);
        let replies = page.seen.replies.len();
        until_seen(page, Duration::from_secs(10), |seen| {
            seen.replies.len() > replies
        });
        page.take(Duration::from_millis(200));
        let frame = page.seen.frame.clone().expect("a page");
        words(&frame.root).last().cloned().unwrap_or_default()
    };
    let press = |x: f32, y: f32| {
        Event::Input(Input::Press {
            at: Point::new(x, y),
            button: Button::Left,
            mods: Mods::default(),
            count: 1,
        })
    };
    told(press(5.0, 5.0), &mut page);
    assert_eq!(
        told(Event::Input(Input::Move(Point::new(60.0, 10.0))), &mut page),
        "dragging card over done"
    );
    assert_eq!(
        told(
            Event::Input(Input::Release {
                at: Point::new(60.0, 10.0),
                button: Button::Left
            }),
            &mut page
        ),
        "dropped card on done"
    );
    assert_eq!(
        told(
            Event::Input(Input::Release {
                at: Point::new(1.0, 1.0),
                button: Button::Middle
            }),
            &mut page
        ),
        "released middle"
    );
    assert_eq!(
        told(Event::Command(Id::named("find")), &mut page),
        "command find"
    );
    assert_eq!(
        told(
            Event::Input(Input::KeyUp {
                key: Key::Character('a'),
                mods: Mods::default()
            }),
            &mut page
        ),
        "up a"
    );
    assert_eq!(
        told(
            Event::Input(Input::Mods(Mods {
                alt: true,
                ..Mods::default()
            })),
            &mut page
        ),
        "mods alt true"
    );
    assert_eq!(
        told(
            Event::Files {
                at: Point::new(7.0, 3.0),
                source: pencere_view::FileSource::Drop,
                entries: Vec::new(),
                paths: vec!["/tmp/a.png".into(), "/tmp/b.png".into()]
            },
            &mut page
        ),
        "files 2 /tmp/a.png at 7"
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_script_asks_the_application_to_scroll_and_is_told_where_its_boxes_went() {
    let mut page = open_script(
        r##"
        local said = "nothing"
        return {
            render = function()
                local y = pencere.layout.scroll_offset("list")
                return pencere.div()
                    :child(pencere.text(said .. " at " .. y))
                    :child(pencere.div():id("list"):size(100, 40):scroll():child(pencere.div():size(100, 400):no_shrink()))
            end,
            handle = function(e)
                if e.type == "key" then
                    pencere.layout.scroll_to("list", { y = 60, animate = true })
                    said = "asked"
                    return true
                elseif e.type == "scrolled" then
                    local box = e.boxes[1]
                    said = "scrolled " .. box.id .. " " .. box.x .. " " .. box.y
                    return true
                end
            end,
        }
        "##,
        ModuleLimits::default(),
    );
    page.module.render(SIZE, 0.0, false);
    page.until(Duration::from_secs(20), |u| {
        u.frame.is_some() || u.closed.is_some()
    });
    let text = |page: &Open| words(&page.seen.frame.clone().expect("a page").root).join(" ");
    // The application lays the page out: the box can go 360 down.
    let root = page.seen.frame.clone().expect("a page").root;
    page.module
        .lay_out(Arc::new(pencere_view::layout(root, SIZE, 1.0, &mut Rough)));
    let tell = |page: &mut Open, event: Event| {
        page.module.event(event);
        page.module.render(SIZE, 0.0, false);
        let before = page.seen.replies.len();
        until_seen(page, Duration::from_secs(10), |seen| {
            seen.replies.len() > before
        });
        page.take(Duration::from_millis(200));
    };
    tell(
        &mut page,
        Event::Input(Input::Key {
            key: Key::Down,
            mods: Mods::default(),
        }),
    );
    assert_eq!(
        page.seen.asks,
        [ScrollTo {
            id: Id::named("list"),
            x: None,
            y: Some(60.0),
            animate: true,
        }]
    );
    assert_eq!(
        text(&page),
        "asked at 60",
        "the page builds where it asked to be"
    );
    tell(
        &mut page,
        Event::Scrolled(vec![(Id::named("list"), Point::new(0.0, 25.0))]),
    );
    assert_eq!(text(&page), "scrolled list 0 25 at 25");
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_script_builds_a_long_list_where_the_engine_would() {
    // The same lists, the same way down and back up, in the script and in the engine: where the
    // rows stand, and which the page builds and when.
    let update = first(
        r##"
        local heights = {}
        for i = 1, 3000 do
            heights[i] = (i - 1) % 9 == 0 and 64 or 26
        end
        local lists = {
            pencere.Rows.even(5000, 32):padded(6, 8),
            pencere.Rows.new(heights),
            pencere.Rows.from_tops(pencere.Rows.new(heights).tops):padded(0, 10),
        }
        return {
            render = function()
                local page = pencere.div()
                local function say(...)
                    page:child(pencere.text(table.concat({ ... }, " ")))
                end
                for _, rows in ipairs(lists) do
                    say("len", rows:len(), "total", rows:total(), "at", rows:at(5), rows:at(40), rows:at(1e9))
                    for _, offset in ipairs({ 0, 13, 4000, 1e9 }) do
                        local first, last = rows:window(offset, 500, 32)
                        say("window", offset, first, last, "reveal", rows:reveal(rows:len(), offset, 500))
                    end
                    local built = pencere.Built.new()
                    local offsets = {}
                    for offset = 0, 20000, 450 do
                        offsets[#offsets + 1] = offset
                    end
                    for offset = 20000, 0, -300 do
                        offsets[#offsets + 1] = offset
                    end
                    for _, offset in ipairs(offsets) do
                        local stale = built:stale(rows, offset, 600)
                        local first, last = built:span(rows, offset, 600)
                        say("at", offset, tostring(stale), first, last)
                    end
                end
                return page
            end,
        }
        "##,
        ModuleLimits::default(),
    );
    assert_eq!(update.closed, None, "{}", failed(&update));
    let said = words(&update.frame.expect("a page").root);

    let heights: Vec<f32> = (0..3000)
        .map(|i| if i % 9 == 0 { 64.0 } else { 26.0 })
        .collect();
    let lists = [
        Rows::even(5000, 32.0).padded(6.0, 8.0),
        Rows::new(heights.iter().copied()),
        Rows::new(heights.iter().copied()).padded(0.0, 10.0),
    ];
    let mut expected = Vec::new();
    for rows in &lists {
        // A row of the script is counted from 1, and a window is its first and last.
        let row = |index: Option<usize>| index.map_or("nil".to_owned(), |i| (i + 1).to_string());
        expected.push(format!(
            "len {} total {} at {} {} {}",
            rows.len(),
            rows.total(),
            row(rows.at(5.0)),
            row(rows.at(40.0)),
            row(rows.at(1.0e9)),
        ));
        for offset in [0.0, 13.0, 4000.0, 1.0e9] {
            let window = rows.window(offset, 500.0, 32.0);
            expected.push(format!(
                "window {} {} {} reveal {}",
                offset,
                window.start + 1,
                window.end,
                rows.reveal(rows.len() - 1, offset, 500.0),
            ));
        }
        let mut built = Built::new();
        let offsets = (0..=20_000)
            .step_by(450)
            .chain((0..=20_000).rev().step_by(300))
            .map(|offset| offset as f32);
        for offset in offsets {
            let stale = built.stale(rows, offset, 600.0);
            let span = built.span(rows, offset, 600.0);
            expected.push(format!(
                "at {offset} {stale} {} {}",
                span.start + 1,
                span.end
            ));
        }
    }
    assert_eq!(said, expected);
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_long_list_is_built_near_what_is_in_view_and_again_only_when_it_is_scrolled_near_its_end() {
    let mut page = open_script(
        r##"
        local rows = pencere.Rows.even(1000, 20)
        local shown, tall = true, false
        return {
            render = function()
                local first, last, view = pencere.list_window("list")
                local said = "before " .. tostring(first) .. " " .. tostring(last) .. " " .. tostring(view)
                local page = pencere.div():child(pencere.text(said))
                if shown then
                    page:child(pencere.div():id("list"):h(tall and 5000 or 200):no_shrink():scroll()
                        :list(rows, function(i)
                            return pencere.div():h(20):no_shrink():child(pencere.text("row " .. i))
                        end))
                end
                return page
            end,
            handle = function(e)
                if e.type == "key" then
                    if e.key == "t" then
                        tall = true
                    elseif e.key == "x" then
                        shown = false
                    end
                    return true
                end
                -- Nothing of the page's own changes for a scroll: the kit asks for what the list needs.
                return false
            end,
        }
        "##,
        ModuleLimits::default(),
    );
    let list = Id::named("list");
    let said = |page: &Open| words(&page.seen.frame.clone().expect("a page").root);
    let frame = |page: &mut Open| {
        page.seen.frame = None;
        let update = page.until(Duration::from_secs(20), |u| {
            u.frame.is_some() || u.closed.is_some()
        });
        assert_eq!(update.closed, None, "{}", failed(&update));
    };
    let laid = |page: &Open| {
        let root = page.seen.frame.clone().expect("a page").root;
        Arc::new(pencere_view::layout(root, SIZE, 1.0, &mut Rough))
    };
    let tell = |page: &mut Open, event: Event| -> bool {
        let before = page.seen.replies.len();
        page.module.event(event);
        until_seen(page, Duration::from_secs(10), |seen| {
            seen.replies.len() > before
        });
        page.seen.replies.last().expect("an answer").reply.redraw
    };

    // Not laid out yet: the window is the page's, and as much is made below as could be above.
    page.module.render(SIZE, 0.0, false);
    frame(&mut page);
    let first = said(&page);
    assert!(first.contains(&"before nil nil nil".to_owned()));
    assert!(first.contains(&"row 175".to_owned()) && !first.contains(&"row 176".to_owned()));
    // The spacers and the rows made are all of the list's length.
    let at_first = laid(&page);
    let extent = at_first.scroll_extent(list).expect("the list scrolls");
    assert_eq!(extent.content.h, 20.0 * 1000.0);
    // Laid out in a window the rows made fill, nothing is built again.
    page.module.lay_out(Arc::clone(&at_first));
    page.seen.frame = None;
    page.take(Duration::from_millis(300));
    assert!(page.seen.frame.is_none(), "the rows made fill the window");

    // A scroll within the rows made is the host's alone; one near their end builds the list again,
    // though the page itself said nothing changed.
    assert!(!tell(
        &mut page,
        Event::Scrolled(vec![(list, Point::new(0.0, 1000.0))])
    ));
    assert!(tell(
        &mut page,
        Event::Scrolled(vec![(list, Point::new(0.0, 3400.0))])
    ));
    page.module.render(SIZE, 0.1, false);
    frame(&mut page);
    // Built again going down: half a view behind what is in view, two ahead.
    let far = said(&page);
    assert!(
        far.contains(&"row 166".to_owned()) && far.contains(&"row 200".to_owned()),
        "{far:?}"
    );
    assert!(!far.contains(&"row 165".to_owned()) && !far.contains(&"row 201".to_owned()));

    // The box laid out taller than the rows made fill: the page is built again of itself.
    assert!(tell(
        &mut page,
        Event::Input(Input::Key {
            key: Key::Character('t'),
            mods: Mods::default()
        })
    ));
    page.module.render(SIZE, 0.2, false);
    frame(&mut page);
    assert!(
        !said(&page).contains(&"row 400".to_owned()),
        "built for the window it had"
    );
    page.module.lay_out(laid(&page));
    frame(&mut page);
    assert!(
        said(&page).contains(&"row 400".to_owned()),
        "and again for the one it has"
    );

    // A list the page no longer builds is forgotten.
    assert!(tell(
        &mut page,
        Event::Input(Input::Key {
            key: Key::Character('x'),
            mods: Mods::default()
        })
    ));
    page.module.render(SIZE, 0.3, false);
    frame(&mut page);
    page.module.render(SIZE, 0.4, false);
    frame(&mut page);
    assert!(said(&page).contains(&"before nil nil nil".to_owned()));
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_long_list_in_a_box_that_cannot_hold_one_says_what_is_wrong() {
    let make = r#"function(i) return pencere.div():h(20) end"#;
    for (script, says) in [
        (
            format!("pencere.div():scroll():list(pencere.Rows.even(10, 20), {make})"),
            "give it one (:id)",
        ),
        (
            format!("pencere.div():id('l'):pt(4):scroll():list(pencere.Rows.even(10, 20), {make})"),
            "padding above its rows",
        ),
        (
            format!("pencere.div():id('l'):list(pencere.Rows.even(10, 20), {make})"),
            "does not scroll",
        ),
        (
            format!("pencere.div():id('l'):row():scroll():list(pencere.Rows.even(10, 20), {make})"),
            "is not a column",
        ),
        (
            format!("pencere.div():id('l'):scroll():list({{}}, {make})"),
            "a pencere.Rows",
        ),
    ] {
        let update = first(
            &format!("return {{ render = function() return {script} end }}"),
            ModuleLimits::default(),
        );
        assert!(
            failed(&update).contains(says),
            "{script}: {}",
            failed(&update)
        );
    }
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_long_list_asked_to_its_end_is_built_at_the_end_it_has_now() {
    // A followed log: asked to its end every frame, and grown by more than the rows made ahead.
    let mut page = open_script(
        r##"
        local count = 100
        return {
            render = function()
                pencere.layout.scroll_to("log", { y = 1e9 })
                return pencere.div():child(pencere.div():id("log"):h(200):no_shrink():scroll()
                    :list(pencere.Rows.even(count, 20), function(i)
                        return pencere.div():h(20):no_shrink():child(pencere.text("line " .. i))
                    end))
            end,
            handle = function(e)
                if e.type == "key" then
                    count = 1000
                    return true
                end
            end,
        }
        "##,
        ModuleLimits::default(),
    );
    let frame = |page: &mut Open| {
        page.seen.frame = None;
        let update = page.until(Duration::from_secs(20), |u| {
            u.frame.is_some() || u.closed.is_some()
        });
        assert_eq!(update.closed, None, "{}", failed(&update));
        words(&page.seen.frame.clone().expect("a page").root)
    };
    page.module.render(SIZE, 0.0, false);
    assert!(frame(&mut page).contains(&"line 100".to_owned()));
    // Laid out at the end of a hundred lines, then nine hundred more come at once.
    let root = page.seen.frame.clone().expect("a page").root;
    page.module
        .lay_out(Arc::new(pencere_view::layout(root, SIZE, 1.0, &mut Rough)));
    page.module.event(Event::Input(Input::Key {
        key: Key::Down,
        mods: Mods::default(),
    }));
    page.module.render(SIZE, 0.1, false);
    let grown = frame(&mut page);
    assert!(
        grown.contains(&"line 1000".to_owned()),
        "the end it has now is built, not the end it had: {:?}",
        grown.last()
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn replies_put_together_ask_all_they_ask_in_order_and_the_earliest_wake() {
    let update = first(
        r#"
        local merge = pencere.merge
        local one = { editor = { e = { decorate = "a" } }, fill = { name = "one", note = "1" },
            keyboard = "page", wake = 2 }
        local two = { editor = { e = { { decorate = "b" }, { decorate = "c" } }, f = { decorate = "d" } },
            fill = { name = "two" }, wake = 0.5, copy = "text" }
        local three = { keyboard = { field = "name" }, wake = 1, redraw = false }
        local out = merge(one, nil, two, true, three)
        local said = {}
        for _, ask in ipairs(out.editor.e) do
            said[#said + 1] = ask.decorate
        end
        said[#said + 1] = out.editor.f.decorate
        said[#said + 1] = out.fill.name .. "/" .. out.fill.note
        said[#said + 1] = out.keyboard.field
        said[#said + 1] = out.copy
        said[#said + 1] = tostring(out.wake)
        said[#said + 1] = tostring(out.redraw)
        -- None of them is changed.
        said[#said + 1] = one.fill.name .. " " .. tostring(one.redraw) .. " " .. #two.editor.e
        -- Replies that are no tables.
        said[#said + 1] = tostring(merge()) .. " " .. tostring(merge(nil, nil)) .. " "
            .. tostring(merge(false, nil)) .. " " .. tostring(merge(nil, false, true))
        return { render = function()
            return pencere.text(table.concat(said, ","))
        end }
        "#,
        ModuleLimits::default(),
    );
    assert_eq!(update.closed, None, "{}", failed(&update));
    assert_eq!(
        words(&update.frame.expect("a page").root),
        ["a,b,c,d,two/1,name,text,0.5,true,one nil 2,nil nil false true"]
    );
}

/// A Lua string that says exactly `bytes`, whatever they are.
fn lua_literal(bytes: &[u8]) -> String {
    let mut out = String::from("\"");
    for &byte in bytes {
        if (b' '..=b'~').contains(&byte) && byte != b'"' && byte != b'\\' {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("\\{byte:03}"));
        }
    }
    out.push('"');
    out
}

/// Texts of JSON, good and bad, as `json.decode` (and with it every message of a page's program)
/// reads them. What each says is written out in `JSON_READ`, in the same order: the values, the
/// `null`s, the reasons and the places they name. The reader is the bridge's, in C++; the cases are
/// those the reader in Lua that it replaced gave the same answers to.
const JSON_TEXTS: &[&[u8]] = &[
    b"{}",
    b"[]",
    b"[ ]",
    b" [1 , 2 ] ",
    b"{\"a\":1,\"b\":[true,false,null],\"c\":{\"d\":null}}",
    b"{\"a\":null}",
    b"{\"a\":1,\"a\":null}",
    b"{\"a\":null,\"a\":1}",
    b"null",
    b" null ",
    b"true",
    b"false",
    b"\"x\"",
    b"12",
    b"-0",
    b"01",
    b"1.",
    b"1.e2",
    b"1e",
    b"1e+",
    b"1+2",
    b"-",
    b"--1",
    b"1.5.5",
    b"0x10",
    b"1e999",
    b"1E5",
    b"-12.5e-3",
    b"123456789012345678901234567890",
    b"\"\\u00e9\"",
    b"\"\\ud83d\\ude00\"",
    b"\"\\ud83d\"",
    b"\"\\ud83dx\"",
    b"\"\\ude00\"",
    b"\"\\ud83d\\u0041\"",
    b"\"\\q\"",
    b"\"\\u12\"",
    b"\"\\u12G4\"",
    b"\"\\u0000\"",
    b"\"a\\/b\\b\\f\\n\\r\\t\\\"\\\\\"",
    b"\"a\nb\"",
    b"\"a\x7fb\"",
    b"\"a\x00b\"",
    b"\"caf\xc3\xa9\"",
    b"\"a\xc8b\"",
    b"\"abc",
    b"\"abc\\",
    b"\"abc\\\"",
    b"{\"a\" 1}",
    b"{\"a\":1 \"b\":2}",
    b"{\"a\":1,}",
    b"{,}",
    b"[1,]",
    b"[,1]",
    b"[1 2]",
    b"[1,2",
    b"{\"a\"",
    b"{\"a\":",
    b"{",
    b"[",
    b"",
    b"   ",
    b"tru",
    b"nul",
    b"nullx",
    b"truex",
    b"1 2",
    b"[1] x",
    b"[]]",
    b"{\"a\":1}}",
    b"\"x\" \"y\"",
    b"{1:2}",
    b"{'a':1}",
    b"\t\n [1]\r\n",
    b"\x00",
    b"[1,\x00]",
    b"{\"a\":\x00}",
    b"{\"a\":{\"b\":{\"c\":[1,{\"d\":null}]}}}",
    b"[null,null]",
    b"{\"a\":[],\"b\":{}}",
    b"{\"\":1}",
    b"{\"1\":2,\"01\":3}",
];

const JSON_READ: &[&str] = &[
    "T{} | nil | n=1",
    "A{} | nil | n=1",
    "A{} | nil | n=1",
    "T{1=1,2=2} | nil | n=1",
    "T{a=1,b=T{1=true,2=false,3=NULL},c=T{}~d} | nil | n=1",
    "T{}~a | nil | n=1",
    "T{a=1} | nil | n=1",
    "T{a=1} | nil | n=1",
    "nil | nil | n=1",
    "nil | nil | n=1",
    "true | nil | n=1",
    "false | nil | n=1",
    "\"x\" | nil | n=1",
    "12 | nil | n=1",
    "-0 | nil | n=1",
    "1 | nil | n=1",
    "1 | nil | n=1",
    "100 | nil | n=1",
    "nil | a bad number at 1 | n=2",
    "nil | a bad number at 1 | n=2",
    "nil | a bad number at 1 | n=2",
    "nil | a value was expected at 1 | n=2",
    "nil | a value was expected at 1 | n=2",
    "nil | more after the value at 4 | n=2",
    "nil | more after the value at 2 | n=2",
    "inf | nil | n=1",
    "100000 | nil | n=1",
    "-0.012500000000000001 | nil | n=1",
    "1.2345678901234568e+29 | nil | n=1",
    "\"\\xc3\\xa9\" | nil | n=1",
    "\"\\xf0\\x9f\\x98\\x80\" | nil | n=1",
    "\"\\xef\\xbf\\xbd\" | nil | n=1",
    "\"\\xef\\xbf\\xbdx\" | nil | n=1",
    "\"\\xef\\xbf\\xbd\" | nil | n=1",
    "\"\\xef\\xbf\\xbdA\" | nil | n=1",
    "nil | a bad escape at 2 | n=2",
    "nil | a bad \\u escape at 2 | n=2",
    "nil | a bad \\u escape at 2 | n=2",
    "\"\\x00\" | nil | n=1",
    "\"a/b\\x08\\x0c\\x0a\\x0d\\x09\\x22\\x5c\" | nil | n=1",
    "nil | a control character in a string at 3 | n=2",
    "nil | a control character in a string at 3 | n=2",
    "nil | a control character in a string at 3 | n=2",
    "\"caf\\xc3\\xa9\" | nil | n=1",
    "\"a\\xc8b\" | nil | n=1",
    "nil | a string that does not end at 1 | n=2",
    "nil | a bad escape at 5 | n=2",
    "nil | a string that does not end at 1 | n=2",
    "nil | a colon was expected at 6 | n=2",
    "nil | a comma or a brace was expected at 8 | n=2",
    "nil | a key was expected at 8 | n=2",
    "nil | a key was expected at 2 | n=2",
    "nil | a value was expected at 4 | n=2",
    "nil | a value was expected at 2 | n=2",
    "nil | a comma or a bracket was expected at 4 | n=2",
    "nil | a comma or a bracket was expected at 5 | n=2",
    "nil | a colon was expected at 5 | n=2",
    "nil | a value was expected at 6 | n=2",
    "nil | a key was expected at 2 | n=2",
    "nil | a value was expected at 2 | n=2",
    "nil | a value was expected at 1 | n=2",
    "nil | a value was expected at 4 | n=2",
    "nil | a value was expected at 1 | n=2",
    "nil | a value was expected at 1 | n=2",
    "nil | more after the value at 5 | n=2",
    "nil | more after the value at 5 | n=2",
    "nil | more after the value at 2 | n=2",
    "nil | more after the value at 4 | n=2",
    "nil | more after the value at 3 | n=2",
    "nil | more after the value at 8 | n=2",
    "nil | more after the value at 4 | n=2",
    "nil | a key was expected at 2 | n=2",
    "nil | a key was expected at 2 | n=2",
    "T{1=1} | nil | n=1",
    "nil | a value was expected at 1 | n=2",
    "nil | a value was expected at 4 | n=2",
    "nil | a value was expected at 6 | n=2",
    "T{a=T{b=T{c=T{1=1,2=T{}~d}}}} | nil | n=1",
    "T{1=NULL,2=NULL} | nil | n=1",
    "T{a=A{},b=T{}} | nil | n=1",
    "T{=1} | nil | n=1",
    "T{01=3,1=2} | nil | n=1",
];

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn json_is_read_as_it_always_was() {
    let texts: Vec<String> = JSON_TEXTS.iter().map(|text| lua_literal(text)).collect();
    let script = format!(
        r####"
        local json = pencere.json
        local function quote(text)
            return '"' .. string.gsub(text, '[%c"\\\128-\255]', function(c)
                return string.format("\\x%02x", string.byte(c))
            end) .. '"'
        end
        local function show(v)
            local kind = type(v)
            if v == json.null then return "NULL" end
            if kind == "string" then return quote(v) end
            if kind == "number" then return string.format("%.17g", v) end
            if kind ~= "table" then return tostring(v) end
            local keys = {{}}
            for k in pairs(v) do keys[#keys + 1] = k end
            table.sort(keys, function(a, b) return tostring(a) < tostring(b) end)
            local out = {{}}
            for _, k in ipairs(keys) do
                out[#out + 1] = tostring(k) .. "=" .. show(v[k])
            end
            local nulls = {{}}
            for _, k in ipairs({{ "a", "b", "c", "d" }}) do
                if json.is_null(v, k) then nulls[#nulls + 1] = k end
            end
            return (getmetatable(v) == json.array and "A" or "T") .. "{{" .. table.concat(out, ",") .. "}}"
                .. (#nulls > 0 and ("~" .. table.concat(nulls)) or "")
        end
        local texts = {{ {} }}
        local lines = {{}}
        for _, text in ipairs(texts) do
            local value, why = json.decode(text)
            lines[#lines + 1] = show(value) .. " | " .. tostring(why) .. " | n=" .. select("#", json.decode(text))
        end
        return {{ render = function()
            local div = pencere.div()
            for _, line in ipairs(lines) do div:child(pencere.text(line)) end
            return div
        end }}
        "####,
        texts.join(", ")
    );
    let update = first(&script, ModuleLimits::default());
    assert_eq!(update.closed, None, "{:?}", update.closed);
    let said = words(&update.frame.expect("a page").root);
    assert_eq!(said.len(), JSON_TEXTS.len());
    for (at, said) in said.iter().enumerate() {
        let text = String::from_utf8_lossy(JSON_TEXTS[at]);
        assert_eq!(
            Some(said.as_str()),
            JSON_READ.get(at).copied(),
            "reading {text:?} (case {at})\n{said}"
        );
    }
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn json_objects_are_held_to_the_same_depth_as_lists() {
    let update = first(
        r#"
        local json = pencere.json
        local function nested(levels)
            return string.rep('{"a":', levels) .. "1" .. string.rep("}", levels)
        end
        local ok127 = json.decode(nested(127)) ~= nil
        local bad128, why = json.decode(nested(128))
        -- Lists and objects count together.
        local mixed, mixed_why = json.decode(string.rep('[{"a":', 63) .. "1" .. string.rep("}]", 63))
        local mixed_over, over_why = json.decode(string.rep('[{"a":', 64) .. "1" .. string.rep("}]", 64))
        return { render = function()
            return pencere.div():child(pencere.text(tostring(ok127))):child(pencere.text(tostring(bad128)))
                :child(pencere.text(tostring(why))):child(pencere.text(tostring(mixed ~= nil)))
                :child(pencere.text(tostring(mixed_over) .. " " .. tostring(over_why)))
        end }
        "#,
        ModuleLimits::default(),
    );
    assert_eq!(update.closed, None, "{:?}", update.closed);
    let said = words(&update.frame.expect("a page").root);
    assert_eq!(said[0], "true");
    assert_eq!(said[1], "nil");
    assert!(said[2].starts_with("nested too deep at "), "{}", said[2]);
    assert_eq!(said[3], "true");
    assert!(
        said[4].starts_with("nil nested too deep at "),
        "{}",
        said[4]
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_message_of_the_program_hundreds_of_kib_long_is_read_as_a_short_one_is() {
    // A message of 300 KiB and more, which the reader in Lua was not asked for: a long string, a
    // long list and a long object, as the program's own messages and as `json.decode` reads them.
    let mut page = open_script(
        r#"
        local seen = "nothing yet"
        pencere.subscribe("stats", nil, function(value)
            local text = value.text
            seen = #text .. " " .. #value.list .. " " .. tostring(value.list[#value.list])
                .. " " .. tostring(value.last)
            return true
        end)
        local big = string.rep("[1,", 100000) .. "2" .. string.rep("]", 100000)
        local list = pencere.json.decode("[" .. string.rep("7,", 150000) .. "8]")
        local deep, why = pencere.json.decode(big)
        return { render = function()
            return pencere.div():child(pencere.text(seen))
                :child(pencere.text(#list .. " " .. list[#list]))
                :child(pencere.text(tostring(deep) .. " " .. tostring(why)))
        end }
        "#,
        ModuleLimits::default(),
    );
    page.module.render(SIZE, 0.0, false);
    page.until(Duration::from_secs(20), |u| {
        u.frame.is_some() || u.closed.is_some()
    });
    let list = format!("[{}8]", "7,".repeat(150_000));
    let pushed = format!(
        r#"{{"push":{{"id":1,"value":{{"text":"{}","list":{list},"last":true}}}}}}"#,
        "x".repeat(300 * 1024)
    );
    assert!(pushed.len() > 512 * 1024, "{}", pushed.len());
    page.module.message(pushed);
    until_seen(&mut page, Duration::from_secs(20), |seen| {
        seen.frame
            .as_ref()
            .is_some_and(|frame| words(&frame.root)[0] != "nothing yet")
    });
    assert_eq!(page.seen.closed, None);
    let said = words(&page.seen.frame.clone().expect("a page").root);
    assert_eq!(said[0], format!("{} 150001 8 true", 300 * 1024));
    assert_eq!(said[1], "150001 8");
    assert!(
        said[2].starts_with("nil nested too deep at "),
        "{}",
        said[2]
    );
}

/// Each line of a script that gives the page API something it refuses names the script's own line
/// in the error (`script.lua:3:`), however many helpers of the prelude lie between that line and
/// the refusal; the families are those of the prelude's parts.
#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn what_the_page_api_refuses_it_refuses_at_the_line_of_the_script_that_gave_it() {
    let families: &[(&str, &[&str])] = &[
        (
            "elements",
            &[
                "pencere.text({})",
                r#"pencere.spark("x")"#,
                "pencere.image(5)",
                "pencere.span({})",
                "pencere.rich(5)",
                "pencere.rich({ 5 })",
                "pencere.canvas(5)",
                "pencere.canvas({ 5 })",
                "pencere.div():children(5)",
            ],
        ),
        (
            "an image",
            &[
                r#"pencere.div():fit("cover")"#,
                r#"pencere.image("a.png"):fit("zoom")"#,
                r#"pencere.image("a.png"):opacity("x")"#,
            ],
        ),
        (
            "fields and editors",
            &[
                "pencere.div():text_field()",
                r#"pencere.div():text_field("a", "zig")"#,
                "pencere.div():editor()",
                r#"pencere.div():editor("e", 5)"#,
                r#"pencere.div():editor("e", { nope = 1 })"#,
                r#"pencere.div():editor("e", { caret = { nope = 1 } })"#,
            ],
        ),
        (
            "what an element tells",
            &[
                "pencere.div():zone()",
                "pencere.div():label(5)",
                "pencere.div():motion(5)",
                r#"pencere.div():motion({ ease = "zig" })"#,
                "pencere.div():motion({ spin = 0 })",
                "pencere.div():tip(5)",
                r#"pencere.div():tip("a", 5)"#,
                r#"pencere.div():role("zig")"#,
                "pencere.div():value({})",
                "pencere.div():command()",
                r#"pencere.div():command("a", 5)"#,
                r#"pencere.div():command("a", "A", 5)"#,
                r#"pencere.div():command("a", "A", { 5 })"#,
                r#"pencere.div():command("a", "A", "hyper+k")"#,
                r#"pencere.div():anchor(pencere.caret_of("e"), "zig")"#,
                r#"pencere.div():anchor(pencere.caret_of("e"), "below", { align = "zig" })"#,
                r#"pencere.div():bg_gradient("x")"#,
                r#"pencere.div():grid("x")"#,
                "pencere.div():grid_rows(5)",
            ],
        ),
        (
            "keys and colours",
            &[
                "pencere.keys(5)",
                r#"pencere.keys("hyper+k")"#,
                r#"pencere.mix("a", "b", "c")"#,
                r#"pencere.range_of("e", 5)"#,
                r#"pencere.ease(0.5, "zig")"#,
            ],
        ),
        (
            "timers",
            &[
                r#"pencere.time.after("x", 1)"#,
                "pencere.time.every(0, function() end)",
                "pencere.time.after(1, 5)",
            ],
        ),
        (
            "the program",
            &[
                "pencere.call(1)",
                r#"pencere.call("x", 5)"#,
                r#"pencere.call("x", nil, "y")"#,
                r#"pencere.call("x", nil, nil, 1)"#,
                "pencere.subscribe({})",
                r#"pencere.subscribe("x", "y")"#,
                r#"pencere.subscribe("x", nil, 1)"#,
                r#"pencere.subscribe("x", nil, nil, "y")"#,
                r#"pencere.cancel("x")"#,
                "pencere.cancel()",
            ],
        ),
        (
            "json",
            &[
                "pencere.json.encode(function() end)",
                "local deep = {}; for _ = 1, 200 do deep = { deep } end; pencere.json.encode(deep)",
                "pencere.json.decode(nil)",
            ],
        ),
        (
            "the layout",
            &[
                "pencere.layout.hit({})",
                "pencere.layout.hit(nil)",
                "pencere.layout.ids_at(5)",
                "pencere.layout.drag_source()",
                "pencere.layout.drop_target({ x = 1 })",
                r#"pencere.layout.scroll_to("box", nil)"#,
                r#"pencere.layout.line_start("e", -1)"#,
                r#"pencere.layout.editor_text("e", 0, 1.5)"#,
            ],
        ),
        ("a long list's arguments", &["pencere.div():list(1, 2)"]),
    ];
    for (family, calls) in families {
        for call in *calls {
            let script = format!(
                "-- the first line\n-- the second\n{call}\nreturn {{ render = function() return pencere.div() end }}"
            );
            let update = first(&script, ModuleLimits::default());
            let Some(Fault::Failed(why)) = &update.closed else {
                panic!("{family}: {call}\nwas not refused: {:?}", update.closed);
            };
            assert!(
                why.starts_with("script.lua:3: ") && !why.contains("prelude/"),
                "{family}: {call}\n{why}"
            );
        }
    }
}

/// A long list in a box that cannot hold it is found once the tree is made, and says the line of
/// the `:list` that made it, not the render's end.
#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_long_list_in_a_box_that_cannot_hold_it_names_the_line_that_made_the_list() {
    let boxes = [
        (r#"pencere.div():id("log")"#, "does not scroll"),
        (r#"pencere.div():id("log"):scroll():row()"#, "not a column"),
        (
            r#"pencere.div():id("log"):scroll():p(4)"#,
            "padding above its rows",
        ),
        (
            r#"pencere.div():id("log"):scroll():gap(3)"#,
            "gap between its rows",
        ),
    ];
    for (shape, why) in boxes {
        let script = format!(
            "local rows = pencere.Rows.even(3, 10)\nreturn {{ render = function()\nreturn {shape}:list(rows, function() return pencere.text('x') end)\nend }}"
        );
        let said = failed(&first(&script, ModuleLimits::default()));
        assert!(
            said.starts_with("script.lua:3: ") && said.contains(why),
            "{shape}\n{said}"
        );
    }
}

/// A point that is not two numbers, or a place that is not a whole number of bytes from 1, is the script's
/// mistake where the host's own function is called: the line is the script's, not the prelude's.
#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_call_the_page_api_passes_to_the_host_that_is_refused_names_the_script_too() {
    let why = failed(&first(
        "local a = 1\npencere.layout.hit({ x = 1 })\nreturn {}",
        ModuleLimits::default(),
    ));
    assert_eq!(why, "script.lua:2: a point is { x =, y = }, two numbers");
    let why = failed(&first(
        "local a = 1\npencere.layout.line_of('e', 1.5)\nreturn {}",
        ModuleLimits::default(),
    ));
    assert_eq!(
        why,
        "script.lua:2: a place is a whole number of bytes from 1"
    );
    // Places count from 1: the first byte is 1, and 0 is no place.
    let why = failed(&first(
        "local a = 1\npencere.layout.editor_text('e', 0, 1)\nreturn {}",
        ModuleLimits::default(),
    ));
    assert_eq!(why, "script.lua:2: from is a whole number of bytes from 1");
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives it for seconds; make check-heavy"]
fn a_page_past_eight_thousand_elements_takes_the_room_of_its_own() {
    // The bridge counts a frame's elements and takes the list of its nodes once, at that size.
    // Grown by doubling, the list of a page past 8 192 elements held the room of 24 576 nodes while
    // it moved (a node is large: its style and its options are in it), and the module's memory,
    // which does not shrink, kept it: this page took near 60 MiB and is drawn in 48 now.
    let script = "return { render = function(size, now)\n\
        local root = pencere.div():col()\n\
        for i = 1, 3000 do\n\
            root:child(pencere.div():row():h(26):px(8):items_center():child(\n\
                pencere.div():grow(1):min_w(0):child(pencere.text('row ' .. i .. ' ' .. now):color('dim'):truncate())))\n\
        end\n\
        return root\n\
    end }";
    let mut page = open_script(
        script,
        ModuleLimits {
            memory: 48 << 20,
            call: Duration::from_secs(60),
            ..ModuleLimits::default()
        },
    );
    // Twenty frames: what one frame leaves for the collector is there while the next is drawn.
    for frame in 0..20 {
        page.module.render(SIZE, f64::from(frame), false);
        let update = page.until(Duration::from_secs(120), |u| {
            u.frame.is_some() || u.closed.is_some()
        });
        assert_eq!(update.closed, None, "frame {frame}");
        assert_eq!(update.frame.expect("a frame").root.fold().len(), 9_001);
    }
}
