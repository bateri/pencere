//! `require` in a script: the other scripts of the bundle the script came in, and nothing else.
//! The tests that run the interpreter are `make check-heavy`'s; the ones that ask the sandbox's
//! side of the door what it will give (`script.has-module`, `script.module`) need no interpreter.

use std::collections::HashMap;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pencere_view::{Button, Event, Id, Input, Mods, Point};

use crate::guests_tests::{Open, Rough, SIZE};
use crate::luau_tests::{failed, until_seen, words};
use crate::sandbox::Host;
use crate::world::pencere::view::script::Host as ScriptHost;
use crate::{Assets, Fault, Module, ModuleLimits, Script, Unbounded, Update};

const LUA: &str = "text/x-lua";

/// A file of a bundle: its path, the media type the manifest gives it, and its text.
type File<'a> = (&'a str, &'a str, &'a str);

/// The files of a bundle with the types a manifest gave them, that remembers which it was asked
/// to read.
struct Files {
    files: HashMap<String, (String, Arc<[u8]>)>,
    reads: Mutex<Vec<String>>,
}

impl Files {
    fn new(files: &[File]) -> Arc<Self> {
        Arc::new(Self {
            files: files
                .iter()
                .map(|(path, media, text)| {
                    (
                        (*path).to_owned(),
                        ((*media).to_owned(), Arc::from(text.as_bytes())),
                    )
                })
                .collect(),
            reads: Mutex::new(Vec::new()),
        })
    }

    fn read_paths(&self) -> Vec<String> {
        self.reads.lock().unwrap().clone()
    }
}

impl Assets for Files {
    fn read(&self, path: &str) -> Option<Arc<[u8]>> {
        self.reads.lock().unwrap().push(path.to_owned());
        self.files.get(path).map(|(_, bytes)| Arc::clone(bytes))
    }

    fn media_type(&self, path: &str) -> Option<String> {
        self.files.get(path).map(|(media, _)| media.clone())
    }
}

/// A script `entry` of a bundle with `files` (the entry among them), opened.
fn open_bundle(entry: &str, files: &[File], limits: ModuleLimits) -> (Open, Arc<Files>) {
    let bundle = Files::new(files);
    let source = files
        .iter()
        .find(|(path, _, _)| *path == entry)
        .map(|(_, _, text)| text.as_bytes().to_vec())
        .expect("the entry is a file of the bundle");
    let (told, woken) = channel();
    let told = Mutex::new(told);
    let module = Module::from_script(
        Script {
            path: entry.to_owned(),
            source,
            bundle: Some(Arc::clone(&bundle) as Arc<dyn Assets>),
        },
        limits,
        Arc::new(Unbounded),
        move || {
            let _ = told.lock().unwrap().send(());
        },
    );
    (
        Open {
            module,
            woken,
            seen: Update::default(),
        },
        bundle,
    )
}

/// The first thing a bundle's page is: the page, or why it is closed.
fn first_of(entry: &str, files: &[File], limits: ModuleLimits) -> Update {
    let (mut page, _) = open_bundle(entry, files, limits);
    page.module.render(SIZE, 0.0, false);
    page.until(Duration::from_secs(20), |u| {
        u.frame.is_some() || u.closed.is_some()
    })
}

/// A page that shows what `require` of each path in `paths` came to, one line each: `true` and
/// what it returned, or `false` and why not.
fn page_requiring(paths: &[&str]) -> String {
    let list: Vec<String> = paths.iter().map(|path| format!("{path:?}")).collect();
    format!(
        r#"
        local lines = {{}}
        for _, path in ipairs({{ {} }}) do
            local ok, found = pcall(require, path)
            lines[#lines + 1] = tostring(ok) .. "|" .. tostring(found)
        end
        return {{ render = function()
            local column = pencere.div()
            for _, line in ipairs(lines) do column = column:child(pencere.text(line)) end
            return column
        end }}
        "#,
        list.join(", ")
    )
}

/// What a page that shows lines said, as lines.
fn lines_of(update: &Update) -> Vec<String> {
    assert_eq!(update.closed, None, "{:?}", update.closed);
    words(&update.frame.as_ref().expect("a page").root)
}

const COUNTER_PAGE: &str = r#"
local button = require("./parts/button")
local counter = require("./screens/counter")
local ui = pencere.Ui.new()
local state = { count = 0 }
return {
	render = function()
		return counter(state.count, button("add", "Add", ui.hover == "add"))
	end,
	handle = function(event)
		if event.type == "move" then
			return ui:point(pencere.layout, event)
		elseif event.type == "press" then
			ui:press(pencere.layout, event)
		elseif event.type == "release" then
			if ui:release(pencere.layout, event) == "add" then
				state.count += 1
				return true
			end
		end
	end,
}
"#;

const BUTTON: &str = r#"
return function(id, label, hovered)
	return pencere.div():press(id):px(14):py(8):bg(hovered and "raised3" or "raised2")
		:child(pencere.text(label))
end
"#;

const COUNTER: &str = r#"
return function(count, action)
	return pencere.div():size_full():child(pencere.text("Count " .. count)):child(action)
end
"#;

fn counter_files() -> Vec<File<'static>> {
    vec![
        ("page.lua", LUA, COUNTER_PAGE),
        ("parts/button.lua", LUA, BUTTON),
        ("screens/counter.lua", LUA, COUNTER),
    ]
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_page_split_into_three_files_loads_draws_and_answers_input() {
    let (mut page, bundle) = open_bundle("page.lua", &counter_files(), ModuleLimits::default());
    page.module.render(SIZE, 0.0, false);
    let update = page.until(Duration::from_secs(20), |u| {
        u.frame.is_some() || u.closed.is_some()
    });
    assert_eq!(update.closed, None);
    let frame = update.frame.expect("a page");
    assert_eq!(words(&frame.root), ["Count 0", "Add"]);
    // Laid out, the button the second file made is where the third put it: a click on it counts.
    let laid = Arc::new(pencere_view::layout(
        frame.root.clone(),
        SIZE,
        1.0,
        &mut Rough,
    ));
    page.module.lay_out(Arc::clone(&laid));
    let button = laid
        .bounds(Id::named("add"))
        .expect("the button is laid out");
    let at = Point::new(button.x + button.w / 2.0, button.y + button.h / 2.0);
    for input in [
        Input::Move(at),
        Input::Press {
            at,
            button: Button::Left,
            mods: Mods::default(),
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
    let frame = page.seen.frame.clone().expect("the page after the click");
    assert_eq!(words(&frame.root), ["Count 1", "Add"]);
    // Each file was read once, and no other.
    let mut read = bundle.read_paths();
    read.sort();
    assert_eq!(read, ["parts/button.lua", "screens/counter.lua"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn paths_are_relative_to_the_requiring_file_and_directories_have_an_init() {
    // `screens/logs.lua` reaches `parts/table.luau` by `../`; `parts` is a directory with an
    // `init.luau`, which is the module `parts`, whose own children are `@self/...`.
    let update = first_of(
        "page.lua",
        &[
            (
                "page.lua",
                LUA,
                "local logs = require('./screens/logs')\nlocal parts = require('./parts')\n\
                 return { render = function() return pencere.div():child(pencere.text(logs .. '|' .. parts)) end }",
            ),
            (
                "screens/logs.lua",
                LUA,
                "local table_part = require('../parts/table')\nreturn 'logs+' .. table_part",
            ),
            ("parts/table.luau", LUA, "return 'table'"),
            (
                "parts/init.luau",
                LUA,
                "local child = require('@self/child')\nreturn 'parts+' .. child",
            ),
            ("parts/child.lua", LUA, "return 'child'"),
        ],
        ModuleLimits::default(),
    );
    assert_eq!(lines_of(&update), ["logs+table|parts+child"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_script_runs_once_whoever_asks_for_it() {
    // `once` writes in the shared `log` when its first line runs; the entry and two other scripts
    // all ask for it, by different paths, and `require` gives all of them one value.
    let update = first_of(
        "page.lua",
        &[
            (
                "page.lua",
                LUA,
                "local a = require('./a')\nlocal b = require('./sub/b')\nlocal once = require('./once')\n\
                 local again = require('./sub/../once')\nlocal log = require('./log')\n\
                 local same = rawequal(once, again) and rawequal(a.once, b.once) and rawequal(a.once, once)\n\
                 return { render = function() return pencere.div():child(pencere.text(#log .. ' ' .. tostring(same))) end }",
            ),
            ("a.lua", LUA, "return { once = require('./once') }"),
            ("sub/b.lua", LUA, "return { once = require('../once') }"),
            (
                "once.lua",
                LUA,
                "local log = require('./log')\nlog[#log + 1] = 'ran'\nreturn { }",
            ),
            ("log.lua", LUA, "return { }"),
        ],
        ModuleLimits::default(),
    );
    assert_eq!(lines_of(&update), ["1 true"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_script_has_globals_of_its_own() {
    let update = first_of(
        "page.lua",
        &[
            (
                "page.lua",
                LUA,
                "require('./leaks')\nreturn { render = function() return pencere.div():child(pencere.text(tostring(leaked))) end }",
            ),
            ("leaks.lua", LUA, "leaked = 'yes'\nreturn { }"),
        ],
        ModuleLimits::default(),
    );
    assert_eq!(lines_of(&update), ["nil"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_cycle_is_an_error_that_names_the_chain() {
    let update = first_of(
        "page.lua",
        &[
            (
                "page.lua",
                LUA,
                "local a = require('./a')\nreturn { render = function() return pencere.div() end }",
            ),
            ("a.lua", LUA, "local b = require('./b')\nreturn { b = b }"),
            ("b.lua", LUA, "local a = require('./a')\nreturn { a = a }"),
        ],
        ModuleLimits::default(),
    );
    let why = failed(&update);
    assert!(
        why.contains("require cycle: a.lua -> b.lua -> a.lua"),
        "{why}"
    );
    // At the line of the `require` that closes it.
    assert!(why.starts_with("b.lua:1:"), "{why}");
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn the_entry_is_in_the_chain_of_a_cycle_and_is_no_module_afterwards() {
    let files = [
        (
            "page.lua",
            LUA,
            "local a = require('./a')\nreturn { render = function() return pencere.div() end }",
        ),
        ("a.lua", LUA, "return require('./page')"),
    ];
    let why = failed(&first_of("page.lua", &files, ModuleLimits::default()));
    assert!(
        why.contains("require cycle: page.lua -> a.lua -> page.lua"),
        "{why}"
    );
    // Once the entry has run it is the page, and not a script to run again.
    let update = first_of(
        "page.lua",
        &[(
            "page.lua",
            LUA,
            "return { render = function()\n  local ok, why = pcall(require, './page')\n  return pencere.div():child(pencere.text(tostring(ok) .. '|' .. why))\nend }",
        )],
        ModuleLimits::default(),
    );
    let lines = lines_of(&update);
    assert!(lines[0].starts_with("false|"), "{lines:?}");
    assert!(
        lines[0].contains("\"page.lua\" is the script of the page"),
        "{lines:?}"
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_script_that_is_not_in_the_bundle_is_an_error_that_names_the_path() {
    let update = first_of(
        "page.lua",
        &[(
            "page.lua",
            LUA,
            "local x = require('./screens/nope')\nreturn {}",
        )],
        ModuleLimits::default(),
    );
    let why = failed(&update);
    assert!(why.starts_with("page.lua:1:"), "{why}");
    assert!(why.contains("\"./screens/nope\""), "{why}");
    assert!(
        why.contains("the bundle has no script at \"screens/nope\""),
        "{why}"
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_script_reaches_nothing_but_the_scripts_of_its_bundle() {
    let paths = [
        // Out of the root of the bundle.
        "../outside",
        "./../../outside",
        // Absolute, and not from the requiring script.
        "/etc/passwd",
        "C:/windows/system32/x",
        // Aliases need a configuration file, and a bundle has none.
        "@lib/util",
        "@std/os",
        // A name with no `./`.
        "page",
        // Files of the bundle that are not scripts, by their own names and by a script's.
        "./data.json",
        "./data",
        "./image.png",
        "./text.lua",
        "./disguised",
        // The extension is not part of a module's name.
        "./helper.lua",
        // What is not a path at all.
        "./",
        ".",
        "..",
        "",
    ];
    let update = first_of(
        "page.lua",
        &[
            ("page.lua", LUA, &page_requiring(&paths)),
            ("helper.lua", LUA, "return 'helper'"),
            ("data.json", "application/json", "return 'data'"),
            ("image.png", "image/png", "return 'image'"),
            // A file called a script that the manifest does not type as one is not a script.
            ("text.lua", "text/plain", "return 'text'"),
            // A script that is not called one cannot be named by `require`.
            ("disguised.txt", LUA, "return 'disguised'"),
        ],
        ModuleLimits::default(),
    );
    let lines = lines_of(&update);
    assert_eq!(lines.len(), paths.len(), "{lines:?}");
    for (path, line) in paths.iter().zip(&lines) {
        assert!(line.starts_with("false|"), "{path:?} gave {line}");
        assert!(
            !line.contains("helper") || path.contains("helper"),
            "{line}"
        );
        for reached in ["'data'", "image", "'text'", "'disguised'"] {
            assert!(!line.contains(&format!("true|{reached}")), "{line}");
        }
    }
    // The causes are the ones that are told: the word of the library where the path itself is
    // wrong, ours where the bundle has nothing at it.
    assert!(lines[0].contains("could not get parent"), "{}", lines[0]);
    assert!(lines[1].contains("could not get parent"), "{}", lines[1]);
    assert!(
        lines[2].contains("must start with a valid prefix"),
        "{}",
        lines[2]
    );
    assert!(
        lines[4].contains("@lib is not a valid alias"),
        "{}",
        lines[4]
    );
    assert!(
        lines[6].contains("must start with a valid prefix"),
        "{}",
        lines[6]
    );
    assert!(
        lines[7].contains("the bundle has no script at \"data.json\""),
        "{}",
        lines[7]
    );
    assert!(
        lines[11].contains("the bundle has no script at \"disguised\""),
        "{}",
        lines[11]
    );
    // The file that is a script, by the name it is required by.
    let ok = first_of(
        "page.lua",
        &[
            ("page.lua", LUA, &page_requiring(&["./helper"])),
            ("helper.lua", LUA, "return 'helper'"),
        ],
        ModuleLimits::default(),
    );
    assert_eq!(lines_of(&ok), ["true|helper"]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_name_two_files_answer_to_is_ambiguous() {
    let update = first_of(
        "page.lua",
        &[
            ("page.lua", LUA, &page_requiring(&["./twice", "./dir"])),
            ("twice.lua", LUA, "return 1"),
            ("twice.luau", LUA, "return 2"),
            ("dir.lua", LUA, "return 3"),
            ("dir/init.lua", LUA, "return 4"),
        ],
        ModuleLimits::default(),
    );
    let lines = lines_of(&update);
    assert!(lines[0].contains("ambiguous"), "{lines:?}");
    assert!(lines[1].contains("ambiguous"), "{lines:?}");
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_page_with_no_bundle_finds_nothing_to_require() {
    let mut page = crate::luau_tests::open_script(
        "return { render = function() local ok, why = pcall(require, './x')\n return pencere.div():child(pencere.text(tostring(ok) .. '|' .. why)) end }",
        ModuleLimits::default(),
    );
    page.module.render(SIZE, 0.0, false);
    let update = page.until(Duration::from_secs(20), |u| {
        u.frame.is_some() || u.closed.is_some()
    });
    let lines = lines_of(&update);
    assert!(lines[0].starts_with("false|"), "{lines:?}");
    assert!(
        lines[0].contains("the bundle has no script at \"x\""),
        "{lines:?}"
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn an_error_in_a_required_script_says_its_file_and_its_line() {
    let page = "local logs = require('./screens/logs')\nreturn {}";
    // While the page starts.
    let why = failed(&first_of(
        "page.lua",
        &[
            ("page.lua", LUA, page),
            (
                "screens/logs.lua",
                LUA,
                "local t = nil\n\nlocal x = t.rows\nreturn {}",
            ),
        ],
        ModuleLimits::default(),
    ));
    assert!(why.starts_with("screens/logs.lua:3:"), "{why}");
    // A syntax error of its own.
    let why = failed(&first_of(
        "page.lua",
        &[
            ("page.lua", LUA, page),
            (
                "screens/logs.lua",
                LUA,
                "local ok = 1\nlocal oops = = 3\nreturn {}",
            ),
        ],
        ModuleLimits::default(),
    ));
    assert!(why.starts_with("screens/logs.lua:2:"), "{why}");
    // Not a value, or more than one.
    for body in ["local x = 1", "return 1, 2"] {
        let why = failed(&first_of(
            "page.lua",
            &[("page.lua", LUA, page), ("screens/logs.lua", LUA, body)],
            ModuleLimits::default(),
        ));
        // The line of the `require`, and the file that did not return what it should.
        assert!(why.starts_with("page.lua:1:"), "{why}");
        assert!(
            why.contains("screens/logs.lua: a script that is required returns exactly one value"),
            "{why}"
        );
    }
    // Later, when a call of the page asks for it: the page is closed at the line of its own.
    let why = failed(&first_of(
        "page.lua",
        &[
            (
                "page.lua",
                LUA,
                "return { render = function()\n  local late = require('./late')\n  return pencere.div()\nend }",
            ),
            ("late.lua", LUA, "-- late\nerror('too late')\nreturn {}"),
        ],
        ModuleLimits::default(),
    ));
    assert!(why.starts_with("late.lua:2: too late"), "{why}");
    // A helper the page API gives, called wrongly in a required script: the line is the script's.
    let why = failed(&first_of(
        "page.lua",
        &[
            (
                "page.lua",
                LUA,
                "local part = require('./part')\nreturn { render = function() return part() end }",
            ),
            (
                "part.lua",
                LUA,
                "return function()\n  local x = nil\n  return pencere.text(x)\nend",
            ),
        ],
        ModuleLimits::default(),
    ));
    assert!(why.starts_with("part.lua:3:"), "{why}");
    assert!(
        why.contains("pencere.text takes a string or a number"),
        "{why}"
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn an_error_the_require_is_given_says_the_line_of_the_page_that_asked() {
    let why = failed(&first_of(
        "page.lua",
        &[(
            "page.lua",
            LUA,
            "local a = 1\n\nlocal b = require('./missing')\nreturn {}",
        )],
        ModuleLimits::default(),
    ));
    assert!(why.starts_with("page.lua:3:"), "{why}");
    // And in a script that the page loaded, the line in that.
    let why = failed(&first_of(
        "page.lua",
        &[
            ("page.lua", LUA, "require('./deep/inner')\nreturn {}"),
            ("deep/inner.lua", LUA, "-- one\n-- two\nrequire('../../up')"),
        ],
        ModuleLimits::default(),
    ));
    assert!(why.starts_with("deep/inner.lua:3:"), "{why}");
    assert!(why.contains("\"../../up\""), "{why}");
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn the_scripts_a_page_loads_are_held_to_the_size_of_a_script_together() {
    // The entry is 100 bytes or so, and each of the others a little more than 300: with room for
    // 1000 the first two come, the third is refused in words.
    let filler = format!("return '{}'", "x".repeat(300));
    let page =
        "local a = require('./a')\nlocal b = require('./b')\nlocal c = require('./c')\nreturn {}";
    let limits = ModuleLimits {
        component: 1000,
        ..ModuleLimits::default()
    };
    let why = failed(&first_of(
        "page.lua",
        &[
            ("page.lua", LUA, page),
            ("a.lua", LUA, &filler),
            ("b.lua", LUA, &filler),
            ("c.lua", LUA, &filler),
        ],
        limits.clone(),
    ));
    assert!(why.starts_with("page.lua:3:"), "{why}");
    assert!(why.contains("held to 1000 bytes together"), "{why}");
    assert!(why.contains("\"c.lua\""), "{why}");
    // With the room for all of them the same page loads.
    let roomy = ModuleLimits {
        component: 1400,
        ..limits
    };
    let update = first_of(
        "page.lua",
        &[
            (
                "page.lua",
                LUA,
                &page.replace(
                    "return {}",
                    "return { render = function() return pencere.div() end }",
                ),
            ),
            ("a.lua", LUA, &filler),
            ("b.lua", LUA, &filler),
            ("c.lua", LUA, &filler),
        ],
        roomy,
    );
    assert_eq!(update.closed, None, "{:?}", update.closed);
    // A script the entry is itself over the limit is not started.
    let update = first_of(
        "page.lua",
        &[(
            "page.lua",
            LUA,
            &format!("-- {}\nreturn {{}}", "y".repeat(2000)),
        )],
        ModuleLimits {
            component: 1000,
            ..ModuleLimits::default()
        },
    );
    assert!(
        matches!(&update.closed, Some(Fault::Load(why)) if why.contains("the most that is run is 1000")),
        "{:?}",
        update.closed
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_chain_of_scripts_that_requires_deeper_than_the_interpreter_goes_is_an_error() {
    // Each script requires the next, and every one of them runs inside the one before it.
    let depth = 400;
    let texts: Vec<String> = (0..depth)
        .map(|n| {
            if n + 1 < depth {
                format!("return require('./m{}')", n + 1)
            } else {
                "return {}".to_owned()
            }
        })
        .collect();
    let names: Vec<String> = (0..depth).map(|n| format!("m{n}.lua")).collect();
    let mut files: Vec<File> = vec![(
        "page.lua",
        LUA,
        "require('./m0')\nreturn { render = function() return pencere.div() end }",
    )];
    for (name, text) in names.iter().zip(&texts) {
        files.push((name.as_str(), LUA, text.as_str()));
    }
    let update = first_of("page.lua", &files, ModuleLimits::default());
    // Not a page, and not a hang: the interpreter says so.
    assert!(update.frame.is_none(), "{:?}", update.frame);
    assert!(
        matches!(&update.closed, Some(Fault::Failed(why)) if why.contains("C stack overflow")),
        "{:?}",
        update.closed
    );
}

// --- The door: what the sandbox gives a script that asks, without an interpreter -----------------

fn host_with(files: &[File], entry: &str) -> (Host, Arc<Files>) {
    let bundle = Files::new(files);
    let mut host = Host::new(&ModuleLimits::default(), Arc::new(Unbounded));
    host.script = Some(Script {
        path: entry.to_owned(),
        source: b"return {}".to_vec(),
        bundle: Some(Arc::clone(&bundle) as Arc<dyn Assets>),
    });
    (host, bundle)
}

#[test]
fn a_host_gives_the_scripts_of_the_manifest_and_nothing_a_name_can_make_of_it() {
    let (mut host, bundle) = host_with(
        &[
            ("page.lua", LUA, "return {}"),
            ("screens/logs.luau", LUA, "return 'logs'"),
            ("data.json", "application/json", "{}"),
            ("text.lua", "text/plain", "return 'text'"),
        ],
        "page.lua",
    );
    assert_eq!(ScriptHost::path(&mut host), "page.lua");
    assert!(ScriptHost::has_module(
        &mut host,
        "screens/logs.luau".to_owned()
    ));
    assert_eq!(
        ScriptHost::module(&mut host, "screens/logs.luau".to_owned()),
        Ok(b"return 'logs'".to_vec())
    );
    for refused in [
        // Not in the bundle, and not a script.
        "screens/missing.lua",
        "data.json",
        "text.lua",
        // Not paths of a manifest: nothing is looked up for them.
        "../page.lua",
        "screens/../page.lua",
        "./page.lua",
        "/page.lua",
        "screens//logs.luau",
        "screens/logs.luau/",
        "screens\\logs.luau",
        "page.lua\0",
        "",
    ] {
        assert!(
            !ScriptHost::has_module(&mut host, refused.to_owned()),
            "{refused:?}"
        );
        assert!(
            ScriptHost::module(&mut host, refused.to_owned()).is_err(),
            "{refused:?}"
        );
    }
    // Nothing of what was refused was read from the bundle.
    assert_eq!(bundle.read_paths(), ["screens/logs.luau"]);
}

#[test]
fn a_host_with_no_bundle_gives_no_script() {
    let mut host = Host::new(&ModuleLimits::default(), Arc::new(Unbounded));
    assert_eq!(ScriptHost::path(&mut host), "");
    assert!(!ScriptHost::has_module(&mut host, "page.lua".to_owned()));
    assert!(ScriptHost::module(&mut host, "page.lua".to_owned()).is_err());
}

#[test]
fn the_scripts_a_host_gives_are_counted_with_the_entry() {
    let text = format!("return '{}'", "z".repeat(40));
    let (mut host, _) = host_with(
        &[
            ("page.lua", LUA, "return {}"),
            ("a.lua", LUA, &text),
            ("b.lua", LUA, &text),
        ],
        "page.lua",
    );
    host.given = 9;
    host.most = 9 + text.len() + 5;
    assert!(ScriptHost::module(&mut host, "a.lua".to_owned()).is_ok());
    let why = ScriptHost::module(&mut host, "b.lua".to_owned()).expect_err("over the limit");
    assert!(why.contains("\"b.lua\""), "{why}");
    assert!(why.contains("held to"), "{why}");
}
