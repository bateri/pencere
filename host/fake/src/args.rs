//! The command line: which program to run in the pane, and what the stand-in
//! host does around it.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;

pub const USAGE: &str = "\
usage: pencere-fake-host [options] -- <program> [args...]

Runs <program> on a pty and plays the host of its views: it answers the
program's capability query, hands out the token, takes the bundle and starts a
view that is a script, not a web view. It opens the three kinds of view: a page
(`web` 1.0), and a component or a Lua script the terminal draws itself (`wasm`
and `lua`, 1.0); the script plays all three the same way, and nothing is drawn. The script is a page: it is told the
context first, then it sends the messages it was given, and logs what the program
says back. What happens is written to stderr, one line each, in the order it
happens (program:, host:, view:, screen:).

options:
  --call METHOD[=JSON] the page calls METHOD (with the params JSON, an object)
                       once it has the context; repeatable, one after the other,
                       each when the answer to the one before has come
  --subscribe TOPIC[=JSON]
                       the page subscribes to TOPIC (with the params JSON, an
                       object) right after the context; repeatable. The calls
                       wait until each topic has pushed once (or been cancelled),
                       and so does --then
  --link URL           the page asks the host to open URL; repeatable. The
                       stand-in person refuses
  --cancel-after N     the page cancels each call that reports progress after
                       its Nth report, and each subscription after its Nth push
                       (default: it never cancels); a cancelled call counts as
                       done, and an answer that comes after is ignored
  --then ACTION        what happens when the script is done (every call answered,
                       every subscription heard once or cancelled): wait (the
                       default; the person ends it), interrupt (^C is typed into
                       the pane), kill (the program is killed with SIGKILL),
                       hangup (the terminal goes away, as when an ssh connection
                       drops: the program is hung up and its tty fails), close
                       (the person closes the view), page-close (the page asks
                       to be closed), restart (the view is started again once,
                       as after a reconnection, and when its script is done
                       again ^C is typed into the pane)
  --on-view N          the action of --then waits for the Nth view the program
                       opens (default 1): a program that reopens its view shows
                       several, and the earlier ones only run their script
  --store DIR          keep verified bundle files in DIR, as a terminal that
                       remembers them between runs does (default: in memory)
  --no-host            act as a terminal that does not host pencere: answer
                       only the reply to CSI c
  --allow-network      grant the origins and permissions a page asks for
                       (default: refuse them)
  --passthrough        copy what the program writes to this program's stdout
  --size COLSxROWS     the pane's size (default: this terminal's, else 80x24)
  --limit SECONDS      kill the program and give up after this long (default 30)
  -h, --help           this text

environment:
  PENCERE_FAKE_GIST=N  how many characters of a result the record shows (default
                       100): a test that reads what the program answered asks for
                       all of it
";

/// What happens when the script is done.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Then {
    Wait,
    Interrupt,
    Kill,
    Hangup,
    Close,
    PageClose,
    Restart,
}

/// Something the page asks of the program: a call of a method, or a subscription to a
/// topic, with the params it was given (an object) or none.
#[derive(Debug, Clone, PartialEq)]
pub struct Ask {
    pub name: String,
    pub params: Option<Value>,
}

#[derive(Debug)]
pub struct Args {
    pub program: Vec<String>,
    pub calls: Vec<Ask>,
    pub subscriptions: Vec<Ask>,
    pub links: Vec<String>,
    /// The page cancels a call after this many reports of its progress, and a subscription
    /// after this many pushes.
    pub cancel_after: Option<u32>,
    pub then: Then,
    /// The view whose finished script the `then` action waits for, counting from 1.
    pub on_view: u32,
    pub store: Option<PathBuf>,
    pub answer: bool,
    pub allow_network: bool,
    pub passthrough: bool,
    pub size: Option<(u16, u16)>,
    pub limit: Duration,
}

/// What parsing asks of `main`.
#[derive(Debug)]
pub enum Outcome {
    Run(Box<Args>),
    Help,
    Usage(String),
}

/// `NAME` or `NAME=JSON` of an option `option`; the JSON is an object.
fn ask(option: &str, text: &str) -> Result<Ask, String> {
    let (name, params) = match text.split_once('=') {
        Some((name, json)) => {
            let params: Value = serde_json::from_str(json)
                .map_err(|error| format!("{option} {name}: the params are not JSON: {error}"))?;
            if !params.is_object() {
                return Err(format!("{option} {name}: the params must be a JSON object"));
            }
            (name, Some(params))
        }
        None => (text, None),
    };
    if name.is_empty() {
        return Err(format!("{option} needs a name"));
    }
    Ok(Ask {
        name: name.to_owned(),
        params,
    })
}

fn size(text: &str) -> Option<(u16, u16)> {
    let (cols, rows) = text.split_once('x')?;
    let (cols, rows) = (cols.parse().ok()?, rows.parse().ok()?);
    (cols > 0 && rows > 0).then_some((cols, rows))
}

pub fn parse<I: Iterator<Item = String>>(mut args: I) -> Outcome {
    match parse_with(&mut args) {
        Ok(Some(parsed)) => Outcome::Run(Box::new(parsed)),
        Ok(None) => Outcome::Help,
        Err(message) => Outcome::Usage(message),
    }
}

fn parse_with<I: Iterator<Item = String>>(args: &mut I) -> Result<Option<Args>, String> {
    let mut parsed = Args {
        program: Vec::new(),
        calls: Vec::new(),
        subscriptions: Vec::new(),
        links: Vec::new(),
        cancel_after: None,
        then: Then::Wait,
        on_view: 1,
        store: None,
        answer: true,
        allow_network: false,
        passthrough: false,
        size: None,
        limit: Duration::from_secs(30),
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--" => {
                parsed.program = args.collect();
                break;
            }
            "-h" | "--help" => return Ok(None),
            "--no-host" => parsed.answer = false,
            "--allow-network" => parsed.allow_network = true,
            "--passthrough" => parsed.passthrough = true,
            "--call" | "--subscribe" | "--link" | "--cancel-after" | "--then" | "--on-view"
            | "--store" | "--size" | "--limit" => {
                let value = args.next().ok_or_else(|| format!("{arg} needs a value"))?;
                option(&mut parsed, &arg, &value)?;
            }
            other => return Err(format!("unknown option {other}")),
        }
    }
    if parsed.program.is_empty() {
        return Err("no program: put it after --".to_owned());
    }
    Ok(Some(parsed))
}

/// A number from 1.
fn count(name: &str, value: &str) -> Result<u32, String> {
    value
        .parse()
        .ok()
        .filter(|&n| n > 0)
        .ok_or_else(|| format!("{name} {value}: want a number from 1"))
}

fn option(parsed: &mut Args, name: &str, value: &str) -> Result<(), String> {
    match name {
        "--call" => parsed.calls.push(ask(name, value)?),
        "--subscribe" => parsed.subscriptions.push(ask(name, value)?),
        "--link" => {
            if value.is_empty() {
                return Err("--link needs a URL".to_owned());
            }
            parsed.links.push(value.to_owned());
        }
        "--cancel-after" => parsed.cancel_after = Some(count(name, value)?),
        "--then" => {
            parsed.then = match value {
                "wait" => Then::Wait,
                "interrupt" => Then::Interrupt,
                "kill" => Then::Kill,
                "hangup" => Then::Hangup,
                "close" => Then::Close,
                "page-close" => Then::PageClose,
                "restart" => Then::Restart,
                other => {
                    return Err(format!(
                        "--then {other}: not one of wait, interrupt, kill, hangup, close, page-close, restart"
                    ));
                }
            };
        }
        "--on-view" => parsed.on_view = count(name, value)?,
        "--store" => parsed.store = Some(PathBuf::from(value)),
        "--size" => {
            parsed.size =
                Some(size(value).ok_or_else(|| format!("--size {value}: want COLSxROWS"))?);
        }
        _ => {
            let seconds: u64 = value
                .parse()
                .map_err(|_| format!("--limit {value}: want a number of seconds"))?;
            parsed.limit = Duration::from_secs(seconds);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(args: &[&str]) -> Outcome {
        parse(args.iter().map(|arg| (*arg).to_owned()))
    }

    #[test]
    fn everything_after_the_double_dash_is_the_program() {
        let Outcome::Run(args) = parsed(&["--then", "kill", "--", "node", "--flag", "x"]) else {
            panic!("did not parse");
        };
        assert_eq!(args.program, ["node", "--flag", "x"]);
        assert_eq!(args.then, Then::Kill);
    }

    #[test]
    fn a_call_has_a_method_and_optional_params() {
        let Outcome::Run(args) = parsed(&[
            "--call",
            "stop={\"id\":\"web\"}",
            "--call",
            "list",
            "--",
            "p",
        ]) else {
            panic!("did not parse");
        };
        assert_eq!(args.calls.len(), 2);
        assert_eq!(args.calls[0].name, "stop");
        assert_eq!(
            args.calls[0].params.as_ref().map(|params| &params["id"]),
            Some(&Value::from("web"))
        );
        assert_eq!(args.calls[1].params, None);
    }

    #[test]
    fn a_subscription_a_link_and_the_cancel_are_kept() {
        let Outcome::Run(args) = parsed(&[
            "--subscribe",
            "ticks={\"every\":5}",
            "--subscribe",
            "stats",
            "--link",
            "https://example.com/docs",
            "--cancel-after",
            "2",
            "--",
            "p",
        ]) else {
            panic!("did not parse");
        };
        assert_eq!(args.subscriptions.len(), 2);
        assert_eq!(args.subscriptions[0].name, "ticks");
        assert_eq!(args.subscriptions[1].params, None);
        assert_eq!(args.links, ["https://example.com/docs"]);
        assert_eq!(args.cancel_after, Some(2));
        let Outcome::Run(args) = parsed(&["--", "p"]) else {
            panic!("did not parse");
        };
        assert_eq!(args.cancel_after, None);
    }

    #[test]
    fn hangup_is_one_of_the_actions() {
        let Outcome::Run(args) = parsed(&["--then", "hangup", "--", "p"]) else {
            panic!("did not parse");
        };
        assert_eq!(args.then, Then::Hangup);
    }

    #[test]
    fn restart_and_page_close_are_among_the_actions() {
        let Outcome::Run(args) = parsed(&["--then", "restart", "--", "p"]) else {
            panic!("did not parse");
        };
        assert_eq!(args.then, Then::Restart);
        let Outcome::Run(args) = parsed(&["--then", "page-close", "--", "p"]) else {
            panic!("did not parse");
        };
        assert_eq!(args.then, Then::PageClose);
    }

    #[test]
    fn the_action_can_wait_for_a_later_view() {
        let Outcome::Run(args) = parsed(&["--then", "interrupt", "--on-view", "2", "--", "p"])
        else {
            panic!("did not parse");
        };
        assert_eq!(args.on_view, 2);
        let Outcome::Run(args) = parsed(&["--", "p"]) else {
            panic!("did not parse");
        };
        assert_eq!(args.on_view, 1);
    }

    #[test]
    fn mistakes_say_what_they_are() {
        for bad in [
            &["--"][..],
            &["--then", "later", "--", "p"],
            &["--call", "x=[1]", "--", "p"],
            &["--call", "={}", "--", "p"],
            &["--subscribe", "x=3", "--", "p"],
            &["--link", "", "--", "p"],
            &["--cancel-after", "0", "--", "p"],
            &["--size", "80", "--", "p"],
            &["--on-view", "0", "--", "p"],
            &["--on-view", "two", "--", "p"],
            &["--wat", "--", "p"],
            &["--limit"],
        ] {
            assert!(matches!(parsed(bad), Outcome::Usage(_)), "{bad:?}");
        }
        assert!(matches!(parsed(&["--help"]), Outcome::Help));
    }
}
