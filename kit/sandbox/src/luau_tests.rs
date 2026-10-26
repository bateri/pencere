//! The interpreter (`luau.wasm`, built from `kit/luau`) through the sandbox, with scripts. It
//! is part of the repository, so these need no toolchain for WebAssembly, only the time to
//! compile it (`make check-heavy`), and they are what notices an interpreter built from another
//! interface than the one the sandbox speaks.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use pencere_view::wire::Kind;
use pencere_view::{
    Button, Color, Cursor, Ease, Element, Event, FieldKind, Fit, Id, Input, Key, Keyboard, Mix,
    Mods, Motion, Point, Rgba, Role, Shift, Source, Tip, image,
};
use wasmtime::component::Component;

use crate::guests_tests::{Open, Rough, SIZE};
use crate::{Fault, Module, ModuleLimits, Program, Script, Unbounded, Update};

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
fn first(script: &str, limits: ModuleLimits) -> Update {
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
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
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
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
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
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_page_that_asks_to_animate_is_asked_for_again() {
    let update = first(
        "return { render = function() return pencere.div(), true end }",
        ModuleLimits::default(),
    );
    assert!(update.frame.expect("a page").animating);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
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
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn an_error_while_the_script_starts_says_the_line() {
    let update = first("local t = nil\n\nreturn t.page", ModuleLimits::default());
    let why = failed(&update);
    assert!(why.starts_with("script.lua:3:"), "{why}");
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
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
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
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
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
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
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
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
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
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
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
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
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
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
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
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
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
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
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
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
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_script_that_never_returns_is_cut_off() {
    let limits = ModuleLimits {
        call: Duration::from_millis(200),
        ..ModuleLimits::default()
    };
    let began = Instant::now();
    let update = first(
        "return { render = function() while true do end end }",
        limits.clone(),
    );
    assert_eq!(update.closed, Some(Fault::Timeout));
    assert!(began.elapsed() < Duration::from_secs(10));
    // Also while it starts, and while it handles.
    let update = first("while true do end", limits.clone());
    assert_eq!(update.closed, Some(Fault::Timeout));
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
    assert_eq!(update.closed, Some(Fault::Timeout));
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
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
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_script_can_reach_nothing_of_the_system() {
    let update = first(
        r#"
        local names = { "io", "os", "debug", "require", "loadstring", "load", "dofile", "loadfile",
            "print", "getfenv", "setfenv", "newproxy", "collectgarbage", "gcinfo", "coroutine",
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
        end }
        "#,
        ModuleLimits::default(),
    );
    assert_eq!(update.closed, None, "{:?}", update.closed);
    let frame = update.frame.expect("a page");
    // Only the pure libraries are there, and none of their ways to reach out of the sandbox; and
    // `require`, which reaches the scripts of the bundle and nothing else (`luau_require_tests`).
    assert_eq!(
        words(&frame.root),
        ["require utf8 string table math", "math.randomseed"]
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_script_cannot_change_what_every_script_shares() {
    let update = first(
        r#"
        local ok1 = pcall(function() string.sub = nil end)
        local ok2 = pcall(function() pencere.div = nil end)
        local ok3 = pcall(function() _G.owned = 1 end)
        owned_by_me = 2
        return { render = function()
            return pencere.div():child(pencere.text(tostring(ok1) .. tostring(ok2) .. tostring(ok3) .. tostring(owned_by_me)))
        end }
        "#,
        ModuleLimits::default(),
    );
    assert_eq!(update.closed, None);
    // The libraries are read only; the script's own globals are its own.
    let text = words(&update.frame.expect("a page").root).join("");
    assert!(text.starts_with("falsefalse"), "{text}");
    assert!(text.ends_with('2'), "{text}");
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
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
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
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
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
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

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
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
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
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
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
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
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
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
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
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
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
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
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
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
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
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
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
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
                :child(pencere.div():press("restart"):label("Restart"):scroll_x(12))
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
    assert_eq!(nodes[9].style.scroll, (12.0, 0.0));
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
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
    assert_eq!(replies[4].fill, [(Id::named("name"), String::new())]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
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
                    :command("go", "Go", "ctrl+page_down")
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
            Command::new(Id::named("go"), "Go").keys(Shortcut::parse("ctrl+page_down").unwrap()),
        ]
    );
    assert!(nodes[9].meta.drop_target);
    assert!(nodes[9].meta.state.selected);
    assert_eq!(nodes[9].meta.state.expanded, Some(false));
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
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
                paths: vec!["/tmp/a.png".into(), "/tmp/b.png".into()]
            },
            &mut page
        ),
        "files 2 /tmp/a.png at 7"
    );
}
