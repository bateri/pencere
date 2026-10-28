//! A page and its program in one process: the program library's handlers (`pencere::Options`)
//! served by its `Conversation` to a page under the host with no screen. Heavy: the interpreter is
//! compiled and run (`make check-heavy`).

use std::ffi::OsStr;
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, ExitStatus};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, channel};
use std::thread;
use std::time::Duration;

use pencere::{CloseResult, HandOffResult, Options, ReopenResult, ViewControl};
use serde_json::{Value, json};

use crate::headless::{Error, Headless, InProcess};

/// A page that adds two numbers by its program, shows a count its program pushes, and asks it to
/// lend the terminal to an editor.
const PAGE: &str = r#"
local sum, count = "?", "?"
pencere.subscribe("count", nil, function(value)
    count = tostring(value.n)
    return true
end)
return {
    render = function()
        return pencere.div():command("add", "Add"):command("edit", "Edit")
            :child(pencere.text("sum " .. sum))
            :child(pencere.text("count " .. count))
    end,
    handle = function(event)
        if event.type == "command" and event.id == "add" then
            pencere.call("add", { a = 2, b = 3 }, function(result, err)
                sum = err and err.code or tostring(result)
                return true
            end)
        elseif event.type == "command" and event.id == "edit" then
            pencere.call("edit")
        end
    end,
}
"#;

/// How long a test waits for what a program says on its own.
const WITHIN: Duration = Duration::from_secs(10);

/// What the page shows after `head`.
fn shown(page: &Headless<InProcess>, head: &str) -> String {
    page.tree_words()
        .into_iter()
        .find_map(|line| line.strip_prefix(head).map(str::to_owned))
        .unwrap_or_default()
}

/// A program that adds, slowly, as a handler may.
fn adding() -> Options {
    Options::new().call("add", |params, _call| {
        thread::sleep(Duration::from_millis(50));
        let number = |name: &str| params.get(name).and_then(Value::as_i64).unwrap_or(0);
        Ok(json!(number("a") + number("b")))
    })
}

/// The view is over, and says why.
fn over(result: Result<(), Error>, why: &str) {
    assert!(
        matches!(&result, Err(Error::Closed(said)) if said.contains(why)),
        "{result:?}"
    );
}

/// The program's handlers answer the page's calls and push to its subscription, in this process,
/// on their own threads: a settling waits for the answers they owe, and what they push of
/// themselves is waited for.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_programs_handlers_in_this_process_answer_and_push_to_its_page() -> Result<(), Error> {
    let (tell, told) = channel::<u64>();
    let told = Mutex::new(Some(told));
    let options = adding().topic("count", move |_params, subscription| {
        let told: Option<Receiver<u64>> = told.lock().ok().and_then(|mut told| told.take());
        if let Some(told) = told {
            let feed = subscription.clone();
            thread::spawn(move || {
                while let Ok(n) = told.recv() {
                    if !feed.push(json!({ "n": n })) {
                        return;
                    }
                }
            });
        }
        Ok(())
    });
    let mut page = Headless::script(PAGE)
        .peer(InProcess::new(options))
        .open()?;
    page.command("add")?;
    assert_eq!(
        shown(&page, "sum "),
        "5",
        "the answer the settling waited for"
    );
    tell.send(7).expect("the topic listens");
    page.wait_for(WITHIN, |page| shown(page, "count ") == "7")?;
    assert!(
        page.received()
            .iter()
            .any(|said| said.contains(r#""push""#))
    );
    Ok(())
}

/// What the program does to its view ends the page's, as a terminal ends it: a terminal lent to a
/// command, a view shown anew, a view closed. The test opens the page anew with the same program
/// where a terminal shows it again, and the program speaks to that page.
#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_program_that_lends_the_terminal_or_shows_its_view_anew_speaks_to_the_page_opened_next()
-> Result<(), Error> {
    let control = ViewControl::new();
    let lends = control.clone();
    let (ended, how) = channel();
    let ended = Mutex::new(ended);
    let options = adding()
        .control(control.clone())
        .call("edit", move |_params, _call| {
            let status = match lends.hand_off(Command::new("vi")).wait() {
                HandOffResult::Returned(Ok(status)) => status.code(),
                _ => None,
            };
            if let Ok(ended) = ended.lock() {
                let _ = ended.send(status);
            }
            Ok(json!(null))
        });
    let mut page = Headless::script(PAGE)
        .peer(InProcess::new(options))
        .open()?;

    // The program lends the terminal to an editor: the page's view is over until it ends.
    over(page.command("edit"), "lent the terminal to `vi`");
    let mut program = page.into_peer();
    assert_eq!(
        program.lent().map(Command::get_program),
        Some(OsStr::new("vi"))
    );
    assert!(program.give_back(ExitStatus::from_raw(3 << 8)));
    assert_eq!(how.recv_timeout(WITHIN), Ok(Some(3)), "the program is told");
    assert!(!program.give_back(ExitStatus::from_raw(0)), "lent once");
    let mut page = Headless::script(PAGE).peer(program).open()?;
    page.command("add")?;
    assert_eq!(shown(&page, "sum "), "5", "the view shown again is heard");

    // The program shows its view anew: this one is over, and the next is the program's.
    let reopens = control.clone();
    let reopened = thread::spawn(move || reopens.reopen(None));
    over(page.wait_for(WITHIN, |_| false), "showed its view anew");
    assert!(matches!(reopened.join(), Ok(Ok(ReopenResult::Reopened))));
    let mut page = Headless::script(PAGE).peer(page.into_peer()).open()?;
    page.command("add")?;
    assert_eq!(shown(&page, "sum "), "5", "the new view is heard");

    // The program closes its view: the page's is over, and no page is heard again.
    let closed = thread::spawn(move || control.close());
    over(page.wait_for(WITHIN, |_| false), "closed its view");
    assert!(matches!(closed.join(), Ok(CloseResult::Closed)));
    let again = Headless::script(PAGE)
        .peer(page.into_peer())
        .open()
        .map(drop);
    over(again, "closed its view");
    Ok(())
}
