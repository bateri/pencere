//! `containers-rust`: a made-up Docker host, and the smallest program that shows a view of its own.
//!
//! In a terminal that speaks pencere it opens the container list as a view of the terminal's
//! own (a WebAssembly module it carries, `containers.wasm`, or with `--lua` the same page as a
//! script in the Lua language, `containers.lua`) and, once a second, tells it how much CPU and
//! memory each running container uses — the view does not ask again. In a terminal that does not,
//! and when asked for text, it prints the list. ⌃C ends it and the shell is where it was. A view
//! that fails says why once the terminal is back: the error of a script, with its line.
//!
//! ```text
//! containers-rust            the view if the terminal shows one, the list as text if not
//! containers-rust --lua      the same, with the page as a Lua script
//! containers-rust --script F the view is the Lua script in the file F (to try a script of one's own)
//! containers-rust --flood    the view, with the readings pushed as fast as the terminal takes them
//! containers-rust --stall N  the view, with the readings stopping after N seconds (a Docker that hangs)
//! containers-rust --text     the list as text, whatever the terminal is
//! containers-rust --json     the list as JSON, a line for each container
//! ```
//!
//! In the view, Shift-U sends this program a large payload; the program counts what it gets and
//! says how much when it leaves.
//!
//! The list and the numbers are the page's own (`containers-page`): this program makes up nothing
//! the page does not know the names of. Everything the protocol asks of a program — the
//! handshake, the modes of the terminal, the signals, the way out — is the `pencere` library's;
//! what is left here is the program: a subscription and a call.

mod table;
#[cfg(test)]
mod view_tests;

use std::borrow::Cow;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use pencere::{Bundle, EndedBy, Kind, NoViewReason, Options, Outcome, Subscription, View};
use serde_json::{Value, json};

/// The view's module: built by `make guest`, which builds it before this program. Empty where the
/// program was built without it (`build.rs`).
static VIEW: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/containers.wasm"));

/// The same page as a script.
const SCRIPT: &[u8] = include_bytes!("../../../kit/luau/examples/containers.lua");

/// The version of the interface of the views the terminal draws itself that the page is written
/// against.
const VERSION: &str = "2.0";

/// How often the made-up host reports.
const EVERY: Duration = Duration::from_secs(1);

fn main() -> ExitCode {
    let mut flood = false;
    let mut stall = None;
    let mut lua = false;
    let mut script = None;
    let mut text = false;
    let mut json = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--flood" => flood = true,
            "--stall" => match args
                .next()
                .map(|s| s.parse::<f64>().map(Duration::try_from_secs_f64))
            {
                Some(Ok(Ok(after))) => stall = Some(after),
                _ => {
                    eprintln!("{USAGE}");
                    return ExitCode::from(64);
                }
            },
            "--lua" => lua = true,
            "--script" => match args.next().map(std::fs::read) {
                Some(Ok(bytes)) => script = Some(bytes),
                Some(Err(why)) => {
                    eprintln!("containers-rust: the script could not be read: {why}");
                    return ExitCode::from(66);
                }
                None => {
                    eprintln!("{USAGE}");
                    return ExitCode::from(64);
                }
            },
            "--text" => text = true,
            "--json" => json = true,
            "--help" | "-h" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(64);
            }
        }
    }
    match (text, json) {
        (true, true) => {
            eprintln!("{USAGE}");
            ExitCode::from(64)
        }
        (true, false) => {
            print!("{}", table::text());
            ExitCode::SUCCESS
        }
        (false, true) => {
            print!("{}", table::json());
            ExitCode::SUCCESS
        }
        (false, false) => match (script, lua) {
            (Some(bytes), _) => shown(Kind::Lua, "containers.lua", bytes.into(), flood, stall),
            (None, true) => shown(Kind::Lua, "containers.lua", SCRIPT.into(), flood, stall),
            (None, false) if VIEW.is_empty() => {
                eprintln!(
                    "containers-rust: this build carries no WebAssembly module (`make guest` builds it first); --lua shows the same page as a script"
                );
                ExitCode::from(69)
            }
            (None, false) => shown(Kind::Wasm, "containers.wasm", VIEW.into(), flood, stall),
        },
    }
}

/// The view if the terminal shows one, and the list as text if not.
fn shown(
    kind: Kind,
    name: &str,
    file: Cow<'static, [u8]>,
    flood: bool,
    stall: Option<Duration>,
) -> ExitCode {
    let bundle = match Bundle::single(name, file) {
        Ok(bundle) => bundle,
        Err(why) => {
            eprintln!("containers-rust: {why}");
            return ExitCode::from(66);
        }
    };
    let upload = Arc::new(Upload::default());
    let options = handlers(flood, stall, &upload)
        .view(View::new(kind, &[VERSION], bundle))
        .program("containers-rust")
        .title("Containers")
        // The terminal shows no view: the list as text. A refusal or a failure has its words.
        .fallback(|why| {
            if let Some(words) = &why.message {
                complain(&format!(
                    "containers-rust: the view failed: {}\n",
                    printable(words)
                ));
            } else if why.reason == NoViewReason::Refused {
                complain("containers-rust: the terminal refused the view\n");
            }
            say(&table::text());
        });
    let outcome = match pencere::serve(options) {
        Ok(outcome) => outcome,
        Err(why) => {
            complain(&format!("containers-rust: {why}\n"));
            return ExitCode::from(70);
        }
    };
    // The terminal is the shell's again: this is where the person can read what happened.
    let bytes = upload.total();
    if bytes > 0 {
        say(&format!("containers-rust: the view sent {bytes} bytes\n"));
    }
    if let Outcome::Closed(closed) = &outcome {
        say(&format!(
            "containers-rust: the view closed ({})\n",
            ended(closed.by)
        ));
        if closed.by == EndedBy::Error
            && let Some(words) = &closed.message
        {
            complain(&format!(
                "containers-rust: the view failed: {}\n",
                printable(words)
            ));
        }
    }
    ExitCode::from(outcome.shell_status())
}

/// What the program does for its view, wherever the view is: the readings, and the payload counted
/// into `upload`.
fn handlers(flood: bool, stall: Option<Duration>, upload: &Arc<Upload>) -> Options {
    let counted = Arc::clone(upload);
    let stalls_at = stall.map(|after| Instant::now() + after);
    Options::new()
        // The readings: the first is the view's at once, the next ones are a second apart.
        .topic("stats", move |_params, subscription| {
            let feed = subscription.clone();
            std::thread::spawn(move || report(&feed, flood, stalls_at));
            Ok(())
        })
        // The view sends a large payload in pieces, in a lane: they are taken in the order they
        // were sent, so the last one, which asks how much there was, comes after the others.
        .call_in("upload", "upload", move |params, _call| {
            Ok(counted.take(&params))
        })
}

/// The readings of the made-up host, pushed until the view stops listening. With `flood` as fast
/// as the terminal takes them; with a time to `stall_at`, until then.
fn report(feed: &Subscription, flood: bool, stall_at: Option<Instant>) {
    let mut tick = 0u32;
    while !feed.is_cancelled() {
        if stall_at.is_some_and(|at| Instant::now() >= at) {
            // A Docker that hangs: nothing more is said, and the subscription stays open.
            while !feed.sleep(Duration::from_millis(100)) {}
            return;
        }
        let readings: Vec<Value> = containers_page::readings(tick)
            .into_iter()
            .map(|r| json!({ "name": r.name, "cpu": r.cpu, "memory": r.memory, "share": r.share }))
            .collect();
        tick = tick.wrapping_add(1);
        if !feed.push(json!({ "readings": readings })) {
            return;
        }
        // `push` waits for the terminal when it is behind, which is the pace of a flood.
        if !flood && feed.sleep(EVERY) {
            return;
        }
    }
}

/// What the pieces of the payload the view sends have added up to. The pieces are calls of one
/// lane, taken one at a time in the order the view sent them: the last one, which says how much
/// there was, is taken when the others have been.
#[derive(Default)]
struct Upload {
    bytes: AtomicU64,
}

impl Upload {
    fn take(&self, params: &Value) -> Value {
        let data = params
            .get("data")
            .and_then(Value::as_str)
            .map_or(0, str::len) as u64;
        let bytes = self.bytes.fetch_add(data, Ordering::SeqCst) + data;
        if params.get("last").and_then(Value::as_bool) == Some(true) {
            json!({ "bytes": bytes })
        } else {
            json!({})
        }
    }

    fn total(&self) -> u64 {
        self.bytes.load(Ordering::SeqCst)
    }
}

/// Writes to the terminal; a terminal that has gone away is nobody to tell (`print!` would panic).
fn say(text: &str) {
    use std::io::Write;
    let _ = std::io::stdout().write_all(text.as_bytes());
}

/// The same for the error stream.
fn complain(text: &str) {
    use std::io::Write;
    let _ = std::io::stderr().write_all(text.as_bytes());
}

/// What ended the view, in a few words.
fn ended(by: EndedBy) -> String {
    match by {
        EndedBy::User => "by the person".into(),
        EndedBy::View => "by the view".into(),
        EndedBy::Program => "by the program".into(),
        EndedBy::Error => "it failed".into(),
        EndedBy::Signal(signal) => format!("by signal {signal}"),
        EndedBy::Terminal => "the terminal went away".into(),
        _ => "for another reason".into(),
    }
}

/// Words that were written by somebody else, fit for a terminal that is going to print them: the
/// terminal has cleaned them already, and a program does not rely on that.
fn printable(words: &str) -> String {
    words.chars().filter(|c| !c.is_control()).collect()
}

const USAGE: &str = "usage: containers-rust [--lua | --script FILE] [--flood] [--stall N] | --text | --json\n\n  \
    (no option)  a view in a terminal that shows one, the list as text in one that does not\n  \
    --lua        the view as a Lua script, not a WebAssembly component\n  \
    --script F   the view as the Lua script in the file F\n  \
    --flood      the view, with the readings pushed as fast as the terminal takes them\n  \
    --stall N    the view, with the readings stopping after N seconds\n  \
    --text       the list as text\n  \
    --json       the list as JSON, a line for each container";
