//! The runtime, run on scripts: what the loop does with timers, processes and tasks, and what the
//! module gives. The view itself is the end-to-end tests' (a stand-in terminal on a pty).

// A test fails by panicking, and the helpers here are the test's.
#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::collections::BTreeMap;
use std::time::Duration;

use super::{Config, Exit, Runner};
use crate::files::Files;

/// Runs `source` as `app.lua` (with the other files); what it printed, and how it ended.
fn run_files(files: &[(&str, &str)]) -> (Exit, String) {
    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let log = std::env::temp_dir().join(format!(
        "pencere-runtime-test-{}-{}.log",
        std::process::id(),
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    ));
    let _ = std::fs::remove_file(&log);
    let packed: BTreeMap<String, Vec<u8>> = files
        .iter()
        .map(|(path, text)| ((*path).to_owned(), text.as_bytes().to_vec()))
        .collect();
    let runner = Runner::new(Config {
        files: Files::packed(packed),
        name: "test".into(),
        args: vec!["one".into(), "two".into()],
        log: Some(log.clone()),
    })
    .expect("a runner");
    let exit = runner.run();
    let printed = std::fs::read_to_string(&log).unwrap_or_default();
    let _ = std::fs::remove_file(&log);
    (exit, printed)
}

fn run(source: &str) -> (Exit, String) {
    run_files(&[("app.lua", source)])
}

#[test]
fn an_app_prints_and_ends_with_the_status_it_says() {
    let (exit, printed) = run(r#"
        local pencere = require("pencere")
        print("hello", 1, true, nil)
        pencere.exit(3)
        print("never")
    "#);
    assert_eq!(exit, Exit::Done(3));
    assert_eq!(printed, "hello\t1\ttrue\tnil\n");
}

#[test]
fn an_app_that_returns_is_done_and_one_that_raises_has_failed() {
    assert_eq!(run("local x = 1").0, Exit::Done(0));
    let (exit, printed) = run("local x = nil; print(x.y)");
    let Exit::Failed(why) = exit else {
        panic!("failed: {exit:?}");
    };
    assert!(why.contains("app.lua:1"), "{why}");
    assert!(
        printed.contains("app.lua:1"),
        "the log has the traceback: {printed}"
    );
    // A script that does not compile fails the same way.
    let Exit::Failed(why) = run("local = ").0 else {
        panic!("failed");
    };
    assert!(why.contains("app.lua"), "{why}");
}

#[test]
fn the_arguments_and_the_name_are_the_apps() {
    let (_, printed) = run(r#"
        local pencere = require("pencere")
        print(#pencere.args, pencere.args[1], pencere.args[2], pencere.name)
    "#);
    assert_eq!(printed, "2\tone\ttwo\ttest\n");
}

#[test]
fn timers_run_in_order_and_a_sleeping_task_does_not_hold_the_others() {
    let (exit, printed) = run(r#"
        local pencere = require("pencere")
        local time = pencere.time
        local ticks = 0
        local every
        every = time.every(0.02, function()
            ticks += 1
            if ticks == 3 then every:cancel() end
        end)
        time.after(0.05, function() print("after") end)
        print("before")
        time.sleep(0.15)
        print("slept", ticks)
    "#);
    assert_eq!(exit, Exit::Done(0));
    assert_eq!(printed, "before\nafter\nslept\t3\n");
}

#[test]
fn a_process_gives_its_lines_and_its_status() {
    let (exit, printed) = run(r#"
        local pencere = require("pencere")
        local process = pencere.process
        local lines = {}
        process.spawn {
            argv = { "sh", "-c", "echo one; echo two >&2; printf 'three'; exit 4" },
            on_stdout = function(line) lines[#lines + 1] = "out:" .. line end,
            on_stderr = function(line) lines[#lines + 1] = "err:" .. line end,
            on_exit = function(code, signal) print(table.concat(lines, ","), code, signal) end,
        }
        pencere.time.sleep(0.5)
    "#);
    assert_eq!(exit, Exit::Done(0));
    // The two streams are separate; each keeps its own order.
    assert!(printed.starts_with("out:one,"), "{printed}");
    assert!(printed.contains("out:three"), "{printed}");
    assert!(printed.contains("err:two"), "{printed}");
    assert!(printed.trim_end().ends_with("4\tnil"), "{printed}");
}

#[test]
fn run_waits_for_a_process_and_hands_back_what_it_said() {
    let (exit, printed) = run(r#"
        local pencere = require("pencere")
        local result = pencere.process.run { "sh", "-c", "echo hi; echo oops >&2; exit 2" }
        print(result.code, result.ok, result.stdout, result.stderr)
        local missing = pcall(pencere.process.run, { "no-such-program-here" })
        print(missing)
        local piped = pencere.process.run { argv = { "cat" }, input = "from the app" }
        print(piped.stdout)
    "#);
    assert_eq!(exit, Exit::Done(0));
    assert_eq!(printed, "2\tfalse\thi\toops\nfalse\nfrom the app\n");
}

#[test]
fn a_process_that_is_killed_says_which_signal_and_one_still_running_at_the_end_is_ended() {
    let (exit, printed) = run(r#"
        local pencere = require("pencere")
        local child = pencere.process.spawn {
            argv = { "sleep", "30" },
            on_exit = function(code, signal) print("exit", code, signal) end,
        }
        print(child:alive(), child.pid > 0)
        pencere.time.after(0.05, function() child:kill("TERM") end)
        pencere.time.sleep(0.4)
        print(child:alive())
        -- Left running when the app ends.
        pencere.process.spawn { argv = { "sleep", "30" } }
    "#);
    assert_eq!(exit, Exit::Done(0));
    assert_eq!(printed, "true\ttrue\nexit\tnil\t15\nfalse\n");
}

#[test]
fn json_files_and_paths_do_what_they_say() {
    let dir = std::env::temp_dir().join(format!("pencere-modules-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let source = format!(
        r#"
        local pencere = require("pencere")
        local json, fs, path = pencere.json, pencere.fs, pencere.path
        local dir = {dir:?}
        fs.mkdir(path.join(dir, "a", "b"))
        local file = path.join(dir, "a", "data.json")
        fs.write(file, json.encode({{ name = "x", list = {{ 1, 2, 3 }}, none = json.object(), empty = {{}} }}))
        print(fs.read(file))
        local back = json.decode(fs.read(file))
        print(back.name, #back.list, next(back.none))
        fs.append(file, "\n")
        print(fs.stat(file).kind, fs.stat(file).size, fs.stat(path.join(dir, "nope")))
        local names = {{}}
        for _, entry in ipairs(fs.list(path.join(dir, "a"))) do names[#names + 1] = entry.name .. ":" .. entry.kind end
        print(table.concat(names, " "))
        print(path.basename(file), path.dirname(file) == path.join(dir, "a"), path.extension(file), path.join("a", "/b"))
        print(pcall(fs.read, path.join(dir, "missing")))
        print(pcall(fs.remove, path.join(dir, "a")))
        fs.remove(path.join(dir, "a"), true)
        print(fs.exists(path.join(dir, "a")))
        print(pcall(json.decode, "{{nope"))
        print(pencere.env.get("PENCERE_SURELY_UNSET"), type(pencere.env.all()))
    "#
    );
    let (exit, printed) = run(&source);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(exit, Exit::Done(0), "{printed}");
    let lines: Vec<&str> = printed.lines().collect();
    assert_eq!(
        lines[0],
        r#"{"empty":[],"list":[1,2,3],"name":"x","none":{}}"#
    );
    assert_eq!(lines[1], "x\t3\tnil");
    assert!(lines[2].starts_with("file\t"), "{}", lines[2]);
    assert_eq!(lines[3], "b:dir data.json:file");
    assert_eq!(lines[4], "data.json\ttrue\tjson\ta/b".replace("a/b", "/b"));
    assert!(lines[5].starts_with("false\tfs.read: "), "{}", lines[5]);
    assert!(lines[6].starts_with("false\tfs.remove: "), "{}", lines[6]);
    assert_eq!(lines[7], "false");
    assert!(lines[8].starts_with("false\tjson.decode: "), "{}", lines[8]);
    assert_eq!(lines[9], "nil\ttable");
}

#[test]
fn a_module_of_the_app_is_required_by_its_name_once() {
    let (exit, printed) = run_files(&[
        (
            "app.lua",
            r#"
            local a = require("lib.util")
            local b = require("lib/util")
            local c = require("things")
            print(a.twice(4), a == b, c, pcall(require, "nothing"))
            "#,
        ),
        (
            "lib/util.lua",
            "print('loading', ...) return { twice = function(n) return n * 2 end }",
        ),
        ("things/init.lua", "return 'init'"),
    ]);
    assert_eq!(exit, Exit::Done(0));
    assert!(printed.starts_with("loading\tlib.util\n8\t"), "{printed}");
    assert!(printed.contains("\tinit\tfalse\t"), "{printed}");
    assert!(printed.contains("no module nothing"), "{printed}");
}

#[test]
fn an_error_in_a_callback_is_logged_and_the_app_goes_on() {
    let (exit, printed) = run(r#"
        local pencere = require("pencere")
        pencere.time.after(0.01, function() error("in a timer") end)
        pencere.time.sleep(0.1)
        print("still here")
    "#);
    assert_eq!(exit, Exit::Done(0));
    assert!(printed.contains("in a timer"), "{printed}");
    assert!(printed.ends_with("still here\n"), "{printed}");
}

#[test]
fn serve_without_a_terminal_is_the_fallback_not_an_error() {
    // The tests' standard input is not a terminal: there is nowhere to ask for a view.
    let (exit, printed) = run_files(&[
        (
            "app.lua",
            r#"
            local pencere = require("pencere")
            local outcome = pencere.serve {
                page = "page.lua",
                calls = { hello = function() return { ok = true } end },
                fallback = function(why) print("fallback", why.reason) end,
            }
            print(outcome.shown, outcome.reason, outcome.status)
            "#,
        ),
        (
            "page.lua",
            "return { render = function() return pencere.div() end }",
        ),
    ]);
    assert_eq!(exit, Exit::Done(0));
    assert_eq!(printed, "fallback\tno-tty\nfalse\tno-tty\t0\n");
}

#[test]
fn serve_with_a_page_that_is_not_there_raises() {
    let (exit, printed) = run(r#"
        local pencere = require("pencere")
        print(pcall(pencere.serve, { page = "missing.lua" }))
        print(pcall(pencere.serve, 3))
    "#);
    assert_eq!(exit, Exit::Done(0));
    assert!(printed.contains("missing.lua"), "{printed}");
}

#[test]
fn the_check_of_a_build_finds_a_script_that_does_not_compile() {
    let mut files = BTreeMap::new();
    files.insert("app.lua".to_owned(), b"print('fine')".to_vec());
    assert!(super::check(&files).is_ok());
    files.insert("lib/broken.lua".to_owned(), b"local = 3".to_vec());
    let why = super::check(&files).expect_err("a script that does not compile");
    assert!(why.contains("lib/broken.lua"), "{why}");
    let mut files = BTreeMap::new();
    files.insert("app.lua".to_owned(), b"".to_vec());
    files.insert("page.lua".to_owned(), vec![b'x'; 9 * 1024 * 1024]);
    let why = super::check(&files).expect_err("a page over the limit");
    assert!(why.contains("page.lua"), "{why}");
}

#[test]
fn a_loop_with_nothing_to_wait_for_ends_instead_of_hanging() {
    // `suspend` with no one to resume: nothing can wake the app.
    let started = std::time::Instant::now();
    let (exit, _) = run(r#"
        local pencere = require("pencere")
        pencere.process.run { "true" }
        print("ran")
        pencere.exit()
    "#);
    assert_eq!(exit, Exit::Done(0));
    assert!(started.elapsed() < Duration::from_secs(10));
}

fn specs_from(source: &str) -> mlua::Result<Vec<super::core::ViewSpec>> {
    let lua = mlua::Lua::new();
    let options: mlua::Table = lua.load(source).eval().expect("a table");
    super::serve::specs_of(&options)
}

#[test]
fn serve_names_its_views_in_the_order_the_app_prefers_them() {
    use super::core::ViewSpec;
    let lua = |page: &str| ViewSpec::Lua {
        page: page.into(),
        assets: "assets".into(),
    };
    let web = |dir: &str| ViewSpec::Web {
        dir: dir.into(),
        access: None,
    };
    // Nothing named: the page of the usual name.
    assert_eq!(specs_from("return {}").expect("specs"), [lua("page.lua")]);
    assert_eq!(
        specs_from(r#"return { page = "ui.lua" }"#).expect("specs"),
        [lua("ui.lua")]
    );
    assert_eq!(
        specs_from(r#"return { web = "site" }"#).expect("specs"),
        [web("site")]
    );
    // Both: the terminal's own kind of view first, the web page for the terminals that have none.
    assert_eq!(
        specs_from(r#"return { page = "page.lua", web = "web" }"#).expect("specs"),
        [lua("page.lua"), web("web")]
    );
    assert_eq!(
        specs_from(r#"return { views = { { kind = "web", dir = "web" }, { kind = "lua" } } }"#)
            .expect("specs"),
        [web("web"), lua("page.lua")]
    );
    for (source, wanted) in [
        (r#"return { views = {} }"#, "empty"),
        (r#"return { views = { { kind = "wasm" } } }"#, "lua"),
        (
            r#"return { views = { { kind = "web" } }, web = "web" }"#,
            "goes in its entries",
        ),
        (
            r#"return { web = "web", access = { permissions = { "nose" } } }"#,
            "nose",
        ),
    ] {
        let why = specs_from(source).expect_err(source).to_string();
        assert!(why.contains(wanted), "{source}: {why}");
    }
}

#[test]
fn a_web_views_access_is_what_the_page_may_reach() {
    use super::core::ViewSpec;
    let specs = specs_from(
        r#"return { web = "web", access = { connect = { "https://api.example.com" }, permissions = { "camera", "clipboardWrite" } } }"#,
    )
    .expect("specs");
    let [
        ViewSpec::Web {
            access: Some(access),
            ..
        },
    ] = specs.as_slice()
    else {
        panic!("a web view with an access: {specs:?}");
    };
    assert_eq!(access.connect, ["https://api.example.com"]);
    assert_eq!(
        access.permissions,
        [
            pencere::Permission::Camera,
            pencere::Permission::ClipboardWrite
        ]
    );
}

#[test]
fn a_web_view_is_the_files_of_its_directory_with_index_html_as_its_entry() {
    use super::core::ViewSpec;
    let files = Files::packed(
        [
            ("app.lua", &b"-"[..]),
            ("web/index.html", b"<p>hi</p>"),
            ("web/app.js", b"let a"),
            ("web/css/site.css", b"p{}"),
            ("elsewhere/other.txt", b"no"),
        ]
        .into_iter()
        .map(|(path, bytes)| (path.to_owned(), bytes.to_vec()))
        .collect(),
    );
    let spec = |dir: &str| ViewSpec::Web {
        dir: dir.into(),
        access: None,
    };
    let views = super::serve::views_of(&files, &[spec("web")]).expect("a view");
    assert_eq!(views.len(), 1);
    let why = super::serve::views_of(&files, &[spec("elsewhere")]).expect_err("no entry");
    assert!(why.contains("index.html"), "{why}");
    let why = super::serve::views_of(&files, &[spec("nothing")]).expect_err("no files");
    assert!(why.contains("no files"), "{why}");
}

#[test]
fn a_hash_a_system_and_the_terminals_text_are_in_the_module() {
    let (exit, printed) = run(r#"
        local pencere = require("pencere")
        print(pencere.hash.sha256("abc"))
        print(pencere.hash.sha256(""))
        print(pencere.os.platform == "macos" or pencere.os.platform == "linux", type(pencere.os.arch))
        print(type(pencere.os.home()), pencere.os.tmpdir() ~= "")
        print(type(pencere.io.read_line), type(pencere.io.write))
    "#);
    assert_eq!(exit, Exit::Done(0));
    let lines: Vec<&str> = printed.lines().collect();
    assert_eq!(
        lines[0],
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        lines[1],
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(lines[2], "true\tstring");
    assert_eq!(lines[3], "string\ttrue");
    // (Reading a line waits for the person; the end-to-end tests do not type.)
    assert_eq!(lines[4], "function\tfunction");
}

#[test]
fn a_process_that_takes_too_long_is_ended_when_run_is_given_a_timeout() {
    let started = std::time::Instant::now();
    let (exit, printed) = run(r#"
        local pencere = require("pencere")
        local result = pencere.process.run { argv = { "sleep", "30" }, timeout = 0.2 }
        print(result.timed_out, result.ok, result.signal)
        local quick = pencere.process.run { argv = { "true" }, timeout = 5 }
        print(quick.timed_out, quick.ok)
    "#);
    assert_eq!(exit, Exit::Done(0));
    assert_eq!(printed, "true\tfalse\t9\nfalse\ttrue\n");
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn files_that_change_under_a_directory_are_told_to_the_app_and_nothing_is_told_while_none_do() {
    let dir = std::env::temp_dir().join(format!("pencere-fswatch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a directory");
    let source = format!(
        r#"
        local pencere = require("pencere")
        local fs, time, path = pencere.fs, pencere.time, pencere.path
        local dir = {dir:?}
        local told = 0
        local seen = {{}}
        local watch = fs.watch(dir, function(changes)
            told += 1
            for _, change in ipairs(changes) do
                seen[#seen + 1] = path.basename(change.path) .. ":" .. change.kind
            end
        end)
        -- Nothing changes: nothing is told.
        time.sleep(0.4)
        print("quiet", told)
        fs.write(path.join(dir, "a.txt"), "one")
        time.sleep(0.6)
        print("told", told > 0, table.concat(seen, ","):find("a.txt", 1, true) ~= nil)
        print(watch:stop(), watch:stop())
        local before = told
        fs.write(path.join(dir, "b.txt"), "two")
        time.sleep(0.4)
        print("stopped", told == before)
        print(pcall(fs.watch, path.join(dir, "nope"), function() end))
    "#
    );
    let (exit, printed) = run(&source);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(exit, Exit::Done(0), "{printed}");
    let lines: Vec<&str> = printed.lines().collect();
    assert_eq!(lines[0], "quiet\t0", "{printed}");
    assert_eq!(lines[1], "told\ttrue\ttrue", "{printed}");
    assert_eq!(lines[2], "true\tfalse");
    assert_eq!(lines[3], "stopped\ttrue");
    assert!(lines[4].starts_with("false\tfs.watch: "), "{printed}");
}

#[test]
fn a_program_given_the_terminal_runs_and_the_app_goes_on_when_it_ends() {
    let (exit, printed) = run(r#"
        local pencere = require("pencere")
        local result = pencere.handoff { "sh", "-c", "exit 7" }
        print(result.code, result.signal)
        local killed = pencere.handoff { argv = { "sh", "-c", "kill -9 $$" } }
        print(killed.code, killed.signal)
        print(pcall(pencere.handoff, { "no-such-program-here" }))
        print(pcall(pencere.handoff, {}))
    "#);
    assert_eq!(exit, Exit::Done(0), "{printed}");
    let lines: Vec<&str> = printed.lines().collect();
    assert_eq!(lines[0], "7\tnil");
    assert_eq!(lines[1], "nil\t9");
    assert!(
        lines[2].starts_with("false\thandoff: no-such-program-here"),
        "{printed}"
    );
    assert!(lines[3].starts_with("false\thandoff needs"), "{printed}");
}

// ---- what a program that edits files needs ------------------------------------------------------

#[test]
fn a_file_is_created_only_where_nothing_is_and_a_link_is_never_followed() {
    let dir = std::env::temp_dir().join(format!("pencere-create-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a directory");
    let target = dir.join("target-of-a-dangling-link");
    std::os::unix::fs::symlink(&target, dir.join("dangling")).expect("a link");
    std::fs::write(dir.join("kept"), "keep me").expect("a file");
    let source = format!(
        r#"
        local pencere = require("pencere")
        local fs, path = pencere.fs, pencere.path
        local dir = {dir:?}
        fs.create(path.join(dir, "new"))
        print(fs.stat(path.join(dir, "new")).size)
        print((pcall(fs.create, path.join(dir, "kept"))))
        print((pcall(fs.create, path.join(dir, "dangling"))))
        print(fs.exists(path.join(dir, "target-of-a-dangling-link")))
        print(fs.read(path.join(dir, "kept")))
        fs.rename(path.join(dir, "new"), path.join(dir, "renamed"))
        print(fs.exists(path.join(dir, "new")), fs.exists(path.join(dir, "renamed")))
        print(fs.writable(path.join(dir, "kept")), fs.writable(path.join(dir, "nowhere")))
        local links = {{}}
        for _, entry in ipairs(fs.list(dir)) do links[#links + 1] = entry.name .. ":" .. entry.kind .. ":" .. tostring(entry.link) end
        print(table.concat(links, " "))
    "#
    );
    let (exit, printed) = run(&source);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(exit, Exit::Done(0), "{printed}");
    assert_eq!(
        printed,
        "0\nfalse\nfalse\nfalse\nkeep me\nfalse\ttrue\ntrue\tfalse\ndangling:other:true kept:file:false renamed:file:false\n"
    );
}

#[test]
fn output_that_is_not_text_comes_as_it_was_written_and_too_much_of_it_ends_the_program() {
    let (exit, printed) = run(r#"
        local pencere = require("pencere")
        local result = pencere.process.run { argv = { "printf", "a\\0b\\nc\\r\\n\\0" }, raw = true }
        print(#result.stdout, (string.gsub(result.stdout, "%z", "0")))
        local lines = pencere.process.run { argv = { "printf", "a\\nb\\r\\n" } }
        print(lines.stdout == "a\nb")
        -- Past a megabyte the lines of the program are cut; raw is not.
        local big = pencere.process.run { argv = { "sh", "-c", "head -c 3000000 /dev/zero | tr '\\0' 'x'" }, raw = true }
        print(#big.stdout, big.overflowed)
        local capped = pencere.process.run { argv = { "sh", "-c", "yes" }, raw = true, max_output = 100000 }
        print(capped.overflowed, capped.ok, #capped.stdout < 1000000)
    "#);
    assert_eq!(exit, Exit::Done(0), "{printed}");
    assert_eq!(
        printed,
        "8\ta0b\nc\r\n0\ntrue\n3000000\tfalse\ntrue\tfalse\ttrue\n"
    );
}

#[test]
fn null_is_sent_as_null_and_the_machine_has_a_name_and_a_user() {
    let (exit, printed) = run(r#"
        local pencere = require("pencere")
        local json = pencere.json
        print(json.encode({ git = json.null, list = {}, none = json.object() }))
        print(json.decode("null"), json.decode('{"a":null}').a)
        print(#pencere.os.hostname() > 0, #pencere.os.username() > 0)
    "#);
    assert_eq!(exit, Exit::Done(0), "{printed}");
    assert_eq!(
        printed,
        "{\"git\":null,\"list\":[],\"none\":{}}\nnil\tnil\ntrue\ttrue\n"
    );
}

#[test]
fn text_is_counted_in_utf16_units_and_names_are_ordered_as_people_read_them() {
    let (exit, printed) = run(r#"
        local pencere = require("pencere")
        local text = pencere.text
        local s = "aş😀z"
        print(text.units(s), text.units(s, 1, 1), text.units(s, 2, 3), text.units(s, 4, 7))
        print(text.index(s, 0), text.index(s, 1), text.index(s, 2), text.index(s, 3), text.index(s, 4), text.index(s, 5), text.index(s, 6))
        print(text.index(s, 1, 2), text.index(s, 2, 2), #s + 1)
        print(text.valid("abc"), text.valid("a\255b"), text.lossy("a\255b") == "a\239\191\189b")
        print(text.wire_size('a"b\n'), text.wire_size("ş"), text.wire_size("😀"), text.wire_size("\1"))
        print(text.wire_fit("abcdef", 1, 3), text.wire_fit("abcdef", 3, 100), text.wire_fit("ş😀ş", 1, 12), text.wire_fit("ş😀ş", 1, 5))
        local names = { "file10", "b.txt", "A.txt", "file9", "zdir" }
        table.sort(names, function(a, b) return text.compare(a, b, true) < 0 end)
        print(table.concat(names, " "), text.compare("a", "A"), text.compare("a", "A", true))
    "#);
    assert_eq!(exit, Exit::Done(0), "{printed}");
    let lines: Vec<&str> = printed.lines().collect();
    assert_eq!(lines[0], "5\t1\t1\t2");
    assert_eq!(lines[1], "1\t2\t4\tnil\t8\t9\tnil");
    assert_eq!(lines[2], "4\tnil\t9");
    assert_eq!(lines[3], "true\tfalse\ttrue");
    assert_eq!(lines[4], "6\t6\t12\t6");
    assert_eq!(lines[5], "3\t6\t2\t2\t1");
    assert_eq!(lines[6], "A.txt b.txt file9 file10 zdir\t0\t1");
}

#[test]
fn a_pattern_found_by_a_person_is_a_regex_with_words_for_what_is_wrong() {
    let (exit, printed) = run(r#"
        local pencere = require("pencere")
        local regex = pencere.regex
        local re = regex.new("orders?\\b")
        print(re:find("no order here", 1))
        print(re:find("no order here", 8))
        print(re:find("x orders", 1))
        local word = regex.new("order", { whole_word = true, ignore_case = true })
        print(word:find("reorder ORDER", 1))
        print(word:find("orders", 1))
        print(regex.new("é+", {}):find("aééb", 1))
        print(regex.new("\\p{L}+É", { ignore_case = true }):find("1 abcé", 1))
        print(regex.new("^"):find("abc", 1))
        print(regex.new("(a)\\1"):find("xaa", 1))
        print(regex.escape("a.b*c"), regex.new(regex.escape("a.b")):find("a.b", 1))
        local ok, why = pcall(regex.new, "(")
        print(ok, string.sub(why, 1, 11))
        print(re:find("é order", 2))
    "#);
    assert_eq!(exit, Exit::Done(0), "{printed}");
    assert_eq!(
        printed,
        "4\t8\nnil\tnil\n3\t8\n9\t13\nnil\tnil\n2\t5\n3\t7\n1\t0\n2\t3\na\\.b\\*c\t1\t3\nfalse\tregex.new: \n4\t8\n"
    );
}

#[test]
fn an_app_has_an_icon_for_each_background_and_the_size_a_png_says_it_is() {
    // The start of a PNG: its signature and the header with a width and a height.
    let png = |side: u32| {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13];
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&side.to_be_bytes());
        bytes.extend_from_slice(&side.to_be_bytes());
        bytes
    };
    let files = Files::packed(BTreeMap::from([
        ("assets/dark-32.png".to_owned(), png(32)),
        ("assets/light-64.png".to_owned(), png(64)),
    ]));
    let lua = mlua::Lua::new();
    let list: mlua::Table = lua
        .load(
            r#"return {
                { file = "assets/dark-32.png", theme = "dark" },
                { file = "assets/light-64.png", theme = "light" },
                { file = "assets/dark-32.png" },
            }"#,
        )
        .eval()
        .expect("a table");
    let icons = super::serve::icons_of(&files, &list).expect("icons");
    assert_eq!(icons.len(), 3);
    assert_eq!(icons[0].sizes, ["32x32"]);
    assert_eq!(icons[0].theme, Some(pencere::IconTheme::Dark));
    assert_eq!(icons[1].sizes, ["64x64"]);
    assert_eq!(icons[1].theme, Some(pencere::IconTheme::Light));
    assert_eq!(icons[2].theme, None);
    assert!(icons[0].src.starts_with("data:image/png;base64,"));
    let bad: mlua::Table = lua
        .load(r#"return { { file = "assets/dark-32.png", theme = "blue" } }"#)
        .eval()
        .expect("a table");
    assert!(super::serve::icons_of(&files, &bad).is_err());
}
