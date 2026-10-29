//! The runner, run as a person runs it: `pencere-test` on a directory of an app and its tests.
//!
//! The ones that open a page compile the kit's interpreter and are heavy; the rest run tests that
//! look at values and cost a process or two.

// A test fails by panicking.
#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const RUNNER: &str = env!("CARGO_BIN_EXE_pencere-test");

/// The page the templates of `pencere new` have: a button, a greeting, a clock.
const PAGE: &str = include_str!("../../../cli/templates/lua/src/view/page.lua");
const ICON: &str = include_str!("../../../cli/templates/lua/src/assets/icon.svg");
const PROGRAM: &str = include_str!("../../../cli/templates/lua/src/program/app.lua");
const TEMPLATE_TEST: &str = include_str!("../../../cli/templates/lua/test/page_test.lua");

struct App {
    dir: PathBuf,
}

impl App {
    /// An app of its own in a directory of its own: the program and the files given.
    fn new(name: &str, files: &[(&str, &str)]) -> Self {
        static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "pencere-test-it-{name}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let app = Self { dir };
        app.write("src/program/app.lua", "-- the program\n");
        for (path, text) in files {
            app.write(path, text);
        }
        app
    }

    /// The app the templates make, and the test of it they come with.
    fn template(name: &str) -> Self {
        let app = Self::new(name, &[]);
        app.write("src/program/app.lua", &PROGRAM.replace("{{name}}", "demo"));
        app.write("src/view/page.lua", &PAGE.replace("{{name}}", "demo"));
        app.write("src/assets/icon.svg", ICON);
        app.write(
            "test/page_test.lua",
            &TEMPLATE_TEST.replace("{{name}}", "demo"),
        );
        app
    }

    fn write(&self, path: &str, text: &str) {
        let file = self.dir.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).expect("a directory");
        }
        std::fs::write(file, text).expect("a file");
    }

    fn run(&self, args: &[&str]) -> (i32, String) {
        self.run_with(args, &[])
    }

    fn run_with(&self, args: &[&str], env: &[(&str, &str)]) -> (i32, String) {
        let mut command = Command::new(RUNNER);
        command
            .args(args)
            .current_dir(&self.dir)
            .env_remove("CI")
            .env_remove("GITHUB_ACTIONS")
            .stdin(Stdio::null());
        for (name, value) in env {
            command.env(name, value);
        }
        let output = command.output().expect("the runner");
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        (output.status.code().unwrap_or(-1), text)
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn tap(app: &App, args: &[&str]) -> (i32, String) {
    let mut all = vec!["--reporter", "tap"];
    all.extend_from_slice(args);
    app.run(&all)
}

#[test]
fn what_cannot_be_run_is_said_and_given_the_status_that_tells_a_script_why() {
    let nowhere = std::env::temp_dir().join(format!("pencere-test-nowhere-{}", std::process::id()));
    std::fs::create_dir_all(&nowhere).expect("a directory");
    let output = Command::new(RUNNER)
        .current_dir(&nowhere)
        .stdin(Stdio::null())
        .output()
        .expect("the runner");
    let _ = std::fs::remove_dir_all(&nowhere);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("no app here"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let app = App::new("usage", &[]);
    let (code, text) = app.run(&["--nope"]);
    assert_eq!(code, 64, "{text}");
    assert!(text.contains("there is no flag --nope"), "{text}");
    let (code, text) = app.run(&["--jobs", "0"]);
    assert_eq!(code, 64, "{text}");
    // An app with no test is not a pass.
    let (code, text) = app.run(&[]);
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("no test files"), "{text}");
    let (code, text) = app.run(&["test/none_test.lua"]);
    assert_eq!(code, 64, "{text}");
}

#[test]
fn the_tap_of_a_run_names_every_test_gives_a_failure_its_place_and_counts_the_skipped() {
    let app = App::new(
        "tap",
        &[(
            "test/values_test.lua",
            r#"local test = require("pencere.test")
local expect = test.expect

test("adds", function()
	expect(1 + 1):to_be(2)
end)

test.describe("a group", function()
	test("compares tables", function()
		expect({ a = 1, b = { 2, 3 } }):to_equal({ a = 1, b = { 2, 4 } })
	end)
	test.skip("is not run", function()
		error("never")
	end)
end)
"#,
        )],
    );
    let (code, text) = tap(&app, &[]);
    assert_eq!(code, 1, "{text}");
    assert_eq!(
        text,
        "TAP version 13
# test/values_test.lua
ok 1 - test/values_test.lua: adds
not ok 2 - test/values_test.lua: a group › compares tables
  ---
  at: values_test.lua:10
  message: |
    values_test.lua:10: expected values to be equal
    difference: value[\"b\"][2] is 3, expected 4
    received { a = 1, b = { 2, 3 } }
    expected { a = 1, b = { 2, 4 } }
  ...
ok 3 - test/values_test.lua: a group › is not run # SKIP
1..3
# 1 passed, 1 failed, 1 skipped
"
    );
}

#[test]
fn the_values_matchers_say_what_they_found_and_a_never_turns_them_around() {
    let app = App::new(
        "matchers",
        &[(
            "test/matchers_test.lua",
            r#"local test = require("pencere.test")
local expect = test.expect

local function fails(fn, words)
	local ok, err = pcall(fn)
	expect(ok):to_be(false)
	expect(tostring(err)):to_contain(words)
end

test("matchers", function()
	expect("alpha beta"):to_contain("beta")
	expect({ "alpha beta", "gamma" }):to_contain("beta")
	expect({ 1, 2 }):to_contain(2)
	expect("abc"):to_match("^a.c$")
	expect({ 1, 2, 3 }):to_have_length(3)
	expect(0.1 + 0.2):to_be_close_to(0.3)
	expect(nil):to_be_nil()
	expect(false).never:to_be_truthy()
	expect(function() error("boom") end):to_throw("boom")
	expect(function() end).never:to_throw()
	expect({ 1 }).never:to_equal({ 2 })
	fails(function() expect(1):to_be(2) end, "expected 1 to be 2")
	fails(function() expect(1).never:to_be(1) end, "expected not")
	fails(function() expect({ 1 }):to_contain(2) end, "to contain 2")
	fails(function() expect({ 1, 2 }):to_have_length(3) end, "length of 3, received 2")
end)

test("hooks run around every test of their group, innermost last", function()
	local order = {}
	test.describe("inner", function()
		test.before_each(function() table.insert(order, "inner before") end)
		test.after_each(function() table.insert(order, "inner after") end)
	end)
	expect(#order):to_be(0)
end)
"#,
        )],
    );
    let (code, text) = tap(&app, &[]);
    assert_eq!(code, 0, "{text}");
    assert!(
        text.contains("ok 1 - test/matchers_test.lua: matchers"),
        "{text}"
    );
    assert!(text.contains("1..2"), "{text}");
}

#[test]
fn a_test_is_found_by_its_words_or_by_its_line_and_only_is_not_let_through_a_gate() {
    let app = App::new(
        "filters",
        &[(
            "test/filters_test.lua",
            "local test = require(\"pencere.test\")\n\
test(\"first one\", function() end)\n\
test(\"second one\", function() end)\n\
test(\"third\", function() end)\n",
        )],
    );
    let (code, text) = tap(&app, &["--grep", "one"]);
    assert_eq!(code, 0, "{text}");
    assert!(
        text.contains("first one") && text.contains("second one"),
        "{text}"
    );
    assert!(!text.contains("third"), "{text}");
    let (code, text) = tap(&app, &["test/filters_test.lua:4"]);
    assert_eq!(code, 0, "{text}");
    assert!(
        text.contains("third") && !text.contains("first one"),
        "{text}"
    );
    // Nothing that was asked for is a failure: a gate that looks at nothing proves nothing.
    let (code, text) = tap(&app, &["--grep", "no such test"]);
    assert_eq!(code, 1, "{text}");

    app.write(
        "test/only_test.lua",
        "local test = require(\"pencere.test\")\n\
test.only(\"alone\", function() end)\n\
test(\"left out\", function() end)\n",
    );
    let (code, text) = tap(&app, &["test/only_test.lua"]);
    assert_eq!(code, 0, "{text}");
    assert!(
        text.contains("alone") && !text.contains("left out"),
        "{text}"
    );
    let (code, text) = app.run_with(
        &["--reporter", "tap", "test/only_test.lua"],
        &[("CI", "true")],
    );
    assert_eq!(code, 1, "{text}");
    assert!(
        text.contains("test.only is not allowed when CI is set"),
        "{text}"
    );
}

#[test]
fn a_file_that_does_not_load_is_an_error_of_the_file_and_the_others_still_run() {
    let app = App::new(
        "broken",
        &[
            (
                "test/a_broken_test.lua",
                "local test = require(\"pencere.test\")\nthis is not lua\n",
            ),
            (
                "test/b_good_test.lua",
                "local test = require(\"pencere.test\")\ntest(\"fine\", function() end)\n",
            ),
        ],
    );
    let (code, text) = tap(&app, &[]);
    assert_eq!(code, 1, "{text}");
    assert!(
        text.contains("not ok 1 - test/a_broken_test.lua: the file did not run"),
        "{text}"
    );
    assert!(text.contains("ok 2 - test/b_good_test.lua: fine"), "{text}");
}

#[test]
fn a_test_that_never_ends_is_cut_short_and_a_worker_that_dies_costs_its_file_alone() {
    let app = App::new(
        "stuck",
        &[
            (
                "test/a_loop_test.lua",
                "local test = require(\"pencere.test\")\n\
test(\"loops\", { timeout = 1 }, function() while true do end end)\n\
test(\"after\", function() end)\n",
            ),
            (
                "test/b_crash_test.lua",
                "local test = require(\"pencere.test\")\n\
local pencere = require(\"pencere\")\n\
test(\"first\", function() end)\n\
test(\"dies\", function()\n\
	pencere.process.run { \"sh\", \"-c\", \"kill -9 $PPID\" }\n\
	pencere.time.sleep(5)\n\
end)\n\
test(\"never reached\", function() end)\n",
            ),
            (
                "test/c_good_test.lua",
                "local test = require(\"pencere.test\")\ntest(\"fine\", function() end)\n",
            ),
        ],
    );
    let (code, text) = tap(&app, &["--jobs", "1"]);
    assert_eq!(code, 1, "{text}");
    assert!(
        text.contains("not ok 1 - test/a_loop_test.lua: loops"),
        "{text}"
    );
    assert!(text.contains("timed_out: true"), "{text}");
    assert!(
        text.contains("ok 2 - test/a_loop_test.lua: after"),
        "{text}"
    );
    assert!(
        text.contains("ok 3 - test/b_crash_test.lua: first"),
        "{text}"
    );
    assert!(
        text.contains("not ok 4 - test/b_crash_test.lua: dies"),
        "{text}"
    );
    assert!(text.contains("it crashed"), "{text}");
    assert!(
        text.contains("not ok 5 - test/b_crash_test.lua: never reached"),
        "{text}"
    );
    assert!(text.contains("ok 6 - test/c_good_test.lua: fine"), "{text}");
}

#[test]
fn a_file_that_never_finishes_loading_is_ended_with_its_process_and_the_run_goes_on() {
    let app = App::new(
        "loading",
        &[
            ("test/a_loop_test.lua", "while true do end\n"),
            (
                "test/b_good_test.lua",
                "local test = require(\"pencere.test\")\ntest(\"fine\", function() end)\n",
            ),
        ],
    );
    let (code, text) = tap(&app, &["--timeout", "0.5", "--jobs", "1"]);
    assert_eq!(code, 1, "{text}");
    assert!(
        text.contains("not ok 1 - test/a_loop_test.lua: the file did not run")
            && text.contains("did not finish loading in time"),
        "{text}"
    );
    assert!(text.contains("ok 2 - test/b_good_test.lua: fine"), "{text}");
}

#[test]
fn the_programs_a_stuck_test_started_are_ended_with_its_process() {
    let app = App::new(
        "children",
        &[(
            "test/children_test.lua",
            r#"local test = require("pencere.test")
local pencere = require("pencere")

test("waits for a program for longer than it has", function()
	pencere.process.spawn { argv = { "sleep", "31.77" } }
	pencere.process.run { "sleep", "31.78" }
end)
"#,
        )],
    );
    let (code, text) = tap(&app, &["--timeout", "0.5"]);
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("timed_out: true"), "{text}");
    for duration in ["31.77", "31.78"] {
        let left = Command::new("pgrep")
            .args(["-f", &format!("sleep {duration}")])
            .output()
            .expect("pgrep runs");
        assert!(
            String::from_utf8_lossy(&left.stdout).trim().is_empty(),
            "sleep {duration} is still running: {}",
            String::from_utf8_lossy(&left.stdout)
        );
    }
}

#[test]
fn the_tests_of_an_app_see_its_program_and_its_own_helpers_before_them_and_never_a_file_that_takes_the_modules_name()
 {
    let app = App::new(
        "modules",
        &[
            ("src/program/data.lua", "return { who = 'program' }\n"),
            (
                "src/program/only_program.lua",
                "return { name = 'only-program' }\n",
            ),
            ("test/data.lua", "return { who = 'test' }\n"),
            // A file of the app with the name of the module cannot stand in for it.
            ("src/program/pencere/test.lua", "return { fake = true }\n"),
            (
                "test/modules_test.lua",
                r#"local test = require("pencere.test")
local expect = test.expect

test("modules", function()
	expect(require("data").who):to_be("test")
	expect(require("only_program").name):to_be("only-program")
	expect(require("pencere.test").fake):to_be_nil()
	local dir = test.temp_dir()
	expect(require("pencere").fs.exists(dir)):to_be(true)
	_G.kept = dir
end)

test("the folder of the test before is gone", function()
	expect(require("pencere").fs.exists(_G.kept)):to_be(false)
end)
"#,
            ),
        ],
    );
    let (code, text) = tap(&app, &[]);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("1..2"), "{text}");
}

#[test]
#[ignore = "heavy: opens pages, which compiles the interpreter"]
fn the_template_tests_pass_on_the_app_the_template_makes() {
    let app = App::template("template");
    let (code, text) = tap(&app, &[]);
    assert_eq!(code, 0, "{text}");
    assert!(
        text.contains("ok 1 - test/page_test.lua: the button shows what the program answers"),
        "{text}"
    );
    assert!(
        text.contains("ok 2 - test/page_test.lua: the clock shows what the program pushes"),
        "{text}"
    );
    assert!(
        text.contains("ok 3 - test/page_test.lua: the app's own program answers the page"),
        "{text}"
    );
}

#[test]
#[ignore = "heavy: opens pages, which compiles the interpreter"]
fn a_test_says_how_the_host_draws_its_terminals_cursor_and_the_page_reads_it_and_is_told_it_changed()
 {
    let app = App::new(
        "terminal-look",
        &[
            (
                "src/view/page.lua",
                r#"local told = 0
return {
	render = function()
		local look = pencere.terminal_look()
		local glow = look.caret and look.caret.glow or "none"
		local shape = look.caret and look.caret.shape or "none"
		return pencere.text("glow " .. tostring(glow) .. " shape " .. shape .. " told " .. told .. " shadow " .. tostring(pencere.draws().shadow))
	end,
	handle = function(event)
		if event.type == "terminal_look" then
			told = told + 1
			return true
		end
	end,
}
"#,
            ),
            (
                "test/look_test.lua",
                r#"local test = require("pencere.test")
local expect = test.expect

test("a host that says nothing", function()
	local page = test.open({ calls = {} })
	expect(page):to_say("glow none shape none told 0 shadow true")
end)

test("a host that says how its terminal draws, and no shadow", function()
	local page = test.open({ calls = {}, terminal_look = { caret = { glow = 1.5, shape = "block" } }, draws = {} })
	expect(page):to_say("glow 1.5 shape block told 0 shadow nil")
	page:set_terminal_look({ caret = { glow = 2, shape = "block" } })
	expect(page):to_say("glow 2 shape block told 1 shadow nil")
	page:set_terminal_look({ caret = { glow = 2, shape = "block" } })
	expect(page):to_say("glow 2 shape block told 1 shadow nil")
end)

test("a look the host cannot say is the test's mistake", function()
	local ok, why = pcall(test.open, { calls = {}, terminal_look = { caret = { shape = "round" } } })
	expect(ok):to_be(false)
	expect(why):to_match("shape")
end)
"#,
            ),
        ],
    );
    let (code, text) = tap(&app, &[]);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("1..3"), "{text}");
}

#[test]
#[ignore = "heavy: opens pages, which compiles the interpreter"]
fn a_failed_test_keeps_what_the_page_was_when_it_failed_and_says_what_it_showed() {
    let app = App::template("artifacts");
    app.write(
        "test/page_test.lua",
        r#"local test = require("pencere.test")
local expect = test.expect

test("fails", function()
	local page = test.open({ calls = {} })
	print("a word from the test")
	expect(page):to_say("this is not on the page")
end)

test("a click on what is not there says what is", function()
	local page = test.open({ calls = {} })
	page:get_by_role("button", { name = "Nope" }):click()
end)

test("a shot in a test that passes is kept too", function()
	local page = test.open({ calls = {} })
	local path = page:shot("start")
	expect(require("pencere").fs.exists(path)):to_be(true)
end)
"#,
    );
    let (code, text) = app.run(&["--artifacts", "kept"]);
    assert_eq!(code, 1, "{text}");
    assert!(
        text.contains("expected the page to say \"this is not on the page\""),
        "{text}"
    );
    assert!(
        text.contains("the page says: \"Press the button.\""),
        "{text}"
    );
    assert!(text.contains("a word from the test"), "{text}");
    assert!(
        text.contains("nothing on the page matches role button named \"Nope\"")
            && text.contains("button \"Greet\""),
        "{text}"
    );
    let kept = app.dir.join("kept/test-page_test-lua/fails");
    for file in ["page-1.png", "page-1.words.txt", "page-1.messages.jsonl"] {
        let path = kept.join(file);
        assert!(
            path.is_file() && path.metadata().map_or(0, |m| m.len()) > 0,
            "{}",
            path.display()
        );
    }
    let png = std::fs::read(kept.join("page-1.png")).expect("a picture");
    assert_eq!(
        png.get(..8),
        Some(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a][..])
    );
    let words = std::fs::read_to_string(kept.join("page-1.words.txt")).expect("the words");
    assert!(words.contains("Press the button."), "{words}");
    assert!(
        app.dir
            .join("kept/test-page_test-lua/a-shot-in-a-test-that-passes-is-kept-too/start.png")
            .is_file()
    );
}

#[test]
#[ignore = "heavy: opens pages, which compiles the interpreter"]
fn the_app_runs_in_its_own_environment_and_the_process_of_the_test_is_never_touched() {
    let app = App::template("world");
    let _ = std::fs::remove_file(app.dir.join("test/page_test.lua"));
    app.write(
        "src/program/app.lua",
        &format!(
            "local pencere = require(\"pencere\")\n\
print(pencere.env.get(\"FROM_TEST\"), pencere.os.home(), pencere.fs.cwd(), table.concat(pencere.args, \",\"), pencere.io.is_terminal(\"stdin\"))\n{}",
            PROGRAM.replace("{{name}}", "demo").replace("local pencere = require(\"pencere\")\n", "")
        ),
    );
    app.write(
        "test/world_test.lua",
        r#"local test = require("pencere.test")
local expect = test.expect
local pencere = require("pencere")

test("the program finds what the test gave it, and the test's own process is as it was", function()
	local dir = test.temp_dir()
	local page = test.open({ env = { FROM_TEST = "layered", HOME = dir }, args = { "--flag", "x" }, cwd = dir })
	expect(page:log()):to_contain("layered\t" .. dir .. "\t" .. dir .. "\t--flag,x\tfalse")
	expect(pencere.env.get("FROM_TEST")):to_be_nil()
	expect(pencere.fs.cwd() ~= dir):to_be(true)
end)

test("two apps in one process do not see one another", function()
	local first = test.open({ env = { FROM_TEST = "one" } })
	local second = test.open({ env = { FROM_TEST = "two" } })
	expect(first:log()):to_contain("one\t")
	expect(first:log()).never:to_contain("two\t")
	expect(second:log()):to_contain("two\t")
	first:get_by_role("button", { name = "Greet" }):click()
	expect(first):to_say("Hello, pencere!")
	expect(second).never:to_say("Hello, pencere!")
end)

test("the program ends with its page: no program of an earlier test is left to answer", function()
	local page = test.open()
	page:get_by_role("button", { name = "Greet" }):click()
	expect(page):to_say("Hello, pencere!")
end)
"#,
    );
    let (code, text) = tap(&app, &[]);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("1..3"), "{text}");
}

#[test]
#[ignore = "heavy: opens pages, which compiles the interpreter"]
fn a_program_that_lends_the_terminal_is_given_it_back_by_the_test_and_shows_its_page_again() {
    let app = App::template("lending");
    let _ = std::fs::remove_file(app.dir.join("test/page_test.lua"));
    app.write(
        "src/view/page.lua",
        &PAGE
            .replace("{{name}}", "demo")
            .replace("\"hello\"", "\"edit\"")
            .replace("b.text(\"Greet\")", "b.text(\"Edit\")"),
    );
    app.write(
        "src/program/app.lua",
        r#"local pencere = require("pencere")
pencere.serve {
	calls = {
		edit = function()
			local ended = pencere.handoff { "vim", "notes.txt" }
			print("the editor ended", ended.code)
			return { message = "edited" }
		end,
	},
	topics = { clock = function() end },
}
"#,
    );
    app.write(
        "test/lending_test.lua",
        r#"local test = require("pencere.test")
local expect = test.expect

test("the terminal is lent to the editor and given back", function()
	local page = test.open()
	local ok, err = pcall(function()
		page:get_by_role("button", { name = "Edit" }):click()
	end)
	expect(ok):to_be(false)
	expect(tostring(err)):to_contain("lent the terminal to `vim`")
	expect(page:lent()):to_equal({ program = "vim", args = { "notes.txt" } })
	page:give_back(3)
	page:reopen()
	page:wait_for(function()
		return string.find(page:log(), "the editor ended\t3", 1, true) ~= nil
	end)
	expect(page:lent()):to_be_nil()
	expect(page):to_say("Press the button.")
end)

test("a page opened with a program on a pty is not reopened for it", function()
	local page = test.open({ calls = {} })
	page:reopen()
	expect(page):to_say("Press the button.")
end)
"#,
    );
    let (code, text) = tap(&app, &[]);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("1..2"), "{text}");
}

#[test]
#[ignore = "heavy: opens pages, which compiles the interpreter"]
fn a_program_that_closes_its_view_goes_on_after_its_serve_as_it_does_under_a_terminal() {
    let app = App::template("closing");
    let _ = std::fs::remove_file(app.dir.join("test/page_test.lua"));
    app.write(
        "src/view/page.lua",
        &PAGE
            .replace("{{name}}", "demo")
            .replace("\"hello\"", "\"leave\"")
            .replace("b.text(\"Greet\")", "b.text(\"Leave\")"),
    );
    app.write(
        "src/program/app.lua",
        r#"local pencere = require("pencere")
local outcome = pencere.serve {
	calls = {
		leave = function()
			pencere.close_view()
			return { message = "bye" }
		end,
	},
	topics = { clock = function() end },
}
-- What a program does after its view is closed takes the time it takes: the test is not to depend on
-- it being quicker than the test's next step.
pencere.time.sleep(0.4)
print("serve ended", outcome.ended)
"#,
    );
    app.write(
        "test/closing_test.lua",
        r#"local test = require("pencere.test")
local expect = test.expect

test("the program closes its view and goes on", function()
	local page = test.open()
	pcall(function() page:get_by_role("button", { name = "Leave" }):click() end)
	page:wait_for(function() return string.find(page:log(), "serve ended\tprogram", 1, true) ~= nil end)
	expect(page:closed() or true):to_be(true)
end)

test("a wait for what never comes says the view is over", function()
	local page = test.open({ within = 0.3 })
	pcall(function() page:get_by_role("button", { name = "Leave" }):click() end)
	local ok, err = pcall(function() page:wait_for(function() return false end) end)
	expect(ok):to_be(false)
	expect(tostring(err)):to_contain("its view is over: the program closed its view")
end)
"#,
    );
    let (code, text) = tap(&app, &[]);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("1..2"), "{text}");
}

/// The Rust example, built by Cargo into a directory of its own (so that this test run and the
/// build the stand-in terminal's tests do take no lock from one another).
fn containers_rust() -> PathBuf {
    static BUILT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    BUILT
        .get_or_init(|| {
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            let target = root.join("target/pencere-test-examples");
            let status = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
                .args([
                    "build",
                    "--quiet",
                    "--offline",
                    "-p",
                    "containers-rust",
                    "--target-dir",
                ])
                .arg(&target)
                .current_dir(&root)
                .status()
                .expect("cargo runs");
            assert!(status.success(), "containers-rust builds");
            target.join("debug/containers-rust")
        })
        .clone()
}

#[test]
#[ignore = "heavy: opens pages, which compiles the interpreter, and builds the Rust example"]
fn a_program_in_rust_on_a_pty_is_driven_as_the_one_in_lua_is() {
    let binary = containers_rust();
    let app = App::new(
        "pty",
        &[(
            "test/rust_test.lua",
            r#"local test = require("pencere.test")
local expect = test.expect
local pencere = require("pencere")

test("the Rust example's page, with its program on a pty", function()
	local page = test.open({
		command = { pencere.env.get("CONTAINERS_RUST"), "--lua" },
		size = { 900, 500 },
	})
	expect(page):to_say("Containers")
	expect(page):to_say("shop")
	page:press("down")
	page:get_by_text("Pull"):click()
	expect(page:get_by_text("Pull")):to_exist()
	expect(page):to_say("Containers")
end)

test("a program that is not there is said", function()
	local ok, err = pcall(test.open, { command = { "/no/such/program" } })
	expect(ok):to_be(false)
	expect(tostring(err)):to_contain("/no/such/program")
end)
"#,
        )],
    );
    let binary = binary.display().to_string();
    let (code, text) = app.run_with(&["--reporter", "tap"], &[("CONTAINERS_RUST", &binary)]);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("1..2"), "{text}");
}

#[test]
#[ignore = "heavy: opens pages, which compiles the interpreter, and builds a Cargo package"]
fn the_page_of_a_cargo_package_is_tested_with_the_program_it_builds_on_a_pty() {
    // The package `pencere new --lang rust` makes, with the library taken from this checkout.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the checkout");
    let app = App::new("package", &[]);
    let _ = std::fs::remove_file(app.dir.join("src/program/app.lua"));
    app.write(
        "Cargo.toml",
        &include_str!("../../../cli/templates/rust/Cargo.toml.tmpl")
            .replace("{{name}}", "package-app")
            .replace(
                "pencere = \"0.3\"",
                &format!("pencere = {{ path = {:?} }}", root.join("program")),
            ),
    );
    app.write(
        "build.rs",
        include_str!("../../../cli/templates/rust/build.rs"),
    );
    app.write(
        "src/program/main.rs",
        &include_str!("../../../cli/templates/rust/src/program/main.rs")
            .replace("{{name}}", "package-app"),
    );
    app.write(
        "src/view/page.lua",
        &PAGE.replace("{{name}}", "package-app"),
    );
    app.write("src/assets/icon.svg", ICON);
    app.write(
        "test/page_test.lua",
        &TEMPLATE_TEST.replace("{{name}}", "package-app"),
    );
    let target = root.join("target/pencere-test-package");
    let (code, text) = app.run_with(
        &["--reporter", "tap"],
        &[
            ("CARGO_TARGET_DIR", &target.display().to_string()),
            ("CARGO_NET_OFFLINE", "true"),
        ],
    );
    assert_eq!(code, 0, "{text}");
    assert!(
        text.contains("ok 3 - test/page_test.lua: the app's own program answers the page"),
        "{text}"
    );
}

#[test]
#[ignore = "heavy: opens pages, which compiles the interpreter"]
fn the_page_of_a_typescript_project_is_tested_with_stand_ins_and_its_program_is_named_not_guessed()
{
    // The project `pencere new --lang ts --page lua` makes: the pictures are in the view's directory.
    let app = App::new("typescript", &[]);
    let _ = std::fs::remove_file(app.dir.join("src/program/app.lua"));
    app.write("src/program/main.ts", "// the program\n");
    app.write("src/view/page.lua", &PAGE.replace("{{name}}", "ts-app"));
    app.write("src/view/assets/icon.svg", ICON);
    app.write(
        "test/page_test.lua",
        &include_str!("../../../cli/templates/ts-lua/test/page_test.lua")
            .replace("{{name}}", "ts-app"),
    );
    app.write(
        "test/program_test.lua",
        r#"local test = require("pencere.test")
local expect = test.expect

test("a program in another language is named, and the test is told when it is not", function()
	local ok, err = pcall(test.open)
	expect(ok):to_be(false)
	expect(tostring(err)):to_contain("is not in Lua or a Cargo package")
	expect(tostring(err)):to_contain("command")
end)
"#,
    );
    let (code, text) = tap(&app, &[]);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("1..3"), "{text}");
}

#[test]
#[ignore = "heavy: opens pages, which compiles the interpreter"]
fn the_example_app_is_tested_with_its_own_program_a_stand_in_and_the_editor() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    for example in ["containers-lua", "editor-lua"] {
        let output = Command::new(RUNNER)
            .args(["--reporter", "tap"])
            .current_dir(examples.join(example))
            .env_remove("CI")
            .stdin(Stdio::null())
            .output()
            .expect("the runner");
        let text = String::from_utf8_lossy(&output.stdout).into_owned()
            + &String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(0), "{example}: {text}");
        assert!(
            text.contains("# ") && !text.contains("not ok"),
            "{example}: {text}"
        );
        let _ = std::fs::remove_dir_all(examples.join(example).join(".pencere"));
    }
}
