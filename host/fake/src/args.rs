//! The command line: which program to run in the pane, and what the stand-in
//! host does around it.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;

use crate::stage::Flaws;

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
  --link URL           the page asks the host to open URL (a web page by its
                       bridge, a view the terminal draws itself in a reply);
                       repeatable. The stand-in person refuses
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
  --no-dismiss         act as a terminal that cannot close a view at the
                       program's request: `dismiss` is not in the welcome's
                       features, and one the program sends is ignored
  --refuse-open N      refuse the Nth open of a view the program asks for, as a
                       terminal's own policy refuses one (`policy`), counting
                       from 1 (a view shown again after a hand-off or a
                       reopening is an open too); repeatable
  --drop PATH          as each view starts, the person drops the file at PATH on
                       it; repeatable. The handles the person's files and
                       places are granted are `$1`, `$2`, … in that order, and a
                       string $N in the params of a --call is the Nth
  --paste PATH         as --drop, the person pastes the file
  --pick PATH          as each view starts, the view asks for the Open panel and
                       the person chooses PATH in it; repeatable
  --pick-nothing       the view asks for the Open panel and the person dismisses it
  --save NAME          as each view starts, the view asks for the Save panel,
                       proposing NAME; the person dismisses it unless --save-in
  --save-in DIR        the person saves in DIR, as the name proposed
  --interrupt-at-read N
                       ^C is typed into the pane as the session asks for the Nth
                       piece of a file the person gave, counting from 1
  --stage DIR          keep the host's folder (specification section 8.11) at
                       DIR, made if it is not there, and let the bytes of a file
                       cross through pipes in it: the welcome lists `stage`.
                       The folder is removed when the run ends
  --stage-wrong-marker the folder's marker file holds another token than the
                       welcome's, as a folder of another host at the same path
                       would; the program is expected to use messages
  --stage-route-after N
                       the first transfer through a pipe loses its way after N
                       bytes (`route`), and the program is expected to go on
  --stage-silent       no byte ever crosses a pipe; the program is expected to
                       take the transfer back and use messages
  --interrupt-at-pipe  ^C is typed into the pane once a pipe has been made for a
                       transfer and the program told its name
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

/// How the person gives a view a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Give {
    Drop,
    Paste,
}

impl Give {
    /// The verb of the record.
    pub fn word(self) -> &'static str {
        match self {
            Self::Drop => "drops",
            Self::Paste => "pastes",
        }
    }
}

/// What the person does with files as each view starts (specification §8.9, §8.10).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Gestures {
    /// The files the person drops or pastes on the view.
    pub gives: Vec<(Give, PathBuf)>,
    /// The view asks for the Open panel.
    pub open_panel: bool,
    /// What the person chooses in it.
    pub picks: Vec<PathBuf>,
    /// The view asks for the Save panel, with this name.
    pub save: Option<String>,
    /// Where the person saves; with none it dismisses the panel.
    pub save_in: Option<PathBuf>,
    /// ^C is typed as the session asks for this piece of a file, counting from 1.
    pub interrupt_at_read: Option<u32>,
    /// ^C is typed once a pipe of the folder has been made for a transfer.
    pub interrupt_at_pipe: bool,
}

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
    /// The host closes a view at the program's request (`dismiss`).
    pub dismiss: bool,
    /// The opens the host refuses, counting from 1.
    pub refuse_opens: Vec<u32>,
    pub gestures: Gestures,
    /// Where the host's folder is, when it keeps one (§8.11).
    pub stage: Option<PathBuf>,
    /// The flaws it is asked to have.
    pub stage_flaws: Flaws,
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
        dismiss: true,
        refuse_opens: Vec::new(),
        gestures: Gestures::default(),
        stage: None,
        stage_flaws: Flaws::default(),
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
            "--no-dismiss" => parsed.dismiss = false,
            "--pick-nothing" => parsed.gestures.open_panel = true,
            "--stage-wrong-marker" => parsed.stage_flaws.wrong_marker = true,
            "--stage-silent" => parsed.stage_flaws.silent = true,
            "--interrupt-at-pipe" => parsed.gestures.interrupt_at_pipe = true,
            "--call"
            | "--subscribe"
            | "--link"
            | "--cancel-after"
            | "--then"
            | "--on-view"
            | "--store"
            | "--size"
            | "--limit"
            | "--refuse-open"
            | "--drop"
            | "--paste"
            | "--pick"
            | "--save"
            | "--save-in"
            | "--interrupt-at-read"
            | "--stage"
            | "--stage-route-after" => {
                let value = args.next().ok_or_else(|| format!("{arg} needs a value"))?;
                option(&mut parsed, &arg, &value)?;
            }
            other => return Err(format!("unknown option {other}")),
        }
    }
    if parsed.program.is_empty() {
        return Err("no program: put it after --".to_owned());
    }
    if parsed.stage.is_none() && parsed.stage_flaws != Flaws::default() {
        return Err("a flaw of the folder needs --stage DIR".to_owned());
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
        "--refuse-open" => parsed.refuse_opens.push(count(name, value)?),
        "--drop" => parsed
            .gestures
            .gives
            .push((Give::Drop, PathBuf::from(value))),
        "--paste" => parsed
            .gestures
            .gives
            .push((Give::Paste, PathBuf::from(value))),
        "--pick" => {
            parsed.gestures.open_panel = true;
            parsed.gestures.picks.push(PathBuf::from(value));
        }
        "--save" => parsed.gestures.save = Some(value.to_owned()),
        "--save-in" => parsed.gestures.save_in = Some(PathBuf::from(value)),
        "--interrupt-at-read" => parsed.gestures.interrupt_at_read = Some(count(name, value)?),
        "--stage" => {
            if value.is_empty() {
                return Err("--stage needs a directory".to_owned());
            }
            parsed.stage = Some(PathBuf::from(value));
        }
        "--stage-route-after" => {
            parsed.stage_flaws.route_after = Some(u64::from(count(name, value)?));
        }
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
        let Outcome::Run(args) = parsed(&[
            "--no-dismiss",
            "--refuse-open",
            "2",
            "--refuse-open",
            "4",
            "--",
            "p",
        ]) else {
            panic!("a run");
        };
        assert!(!args.dismiss);
        assert_eq!(args.refuse_opens, [2, 4]);
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
    fn the_folder_and_its_flaws_are_kept() {
        let Outcome::Run(args) = parsed(&[
            "--stage",
            "/tmp/folder",
            "--stage-wrong-marker",
            "--stage-silent",
            "--stage-route-after",
            "4096",
            "--",
            "p",
        ]) else {
            panic!("did not parse");
        };
        assert_eq!(args.stage, Some(PathBuf::from("/tmp/folder")));
        assert!(args.stage_flaws.wrong_marker && args.stage_flaws.silent);
        assert_eq!(args.stage_flaws.route_after, Some(4096));
        assert!(!args.gestures.interrupt_at_pipe);
        let Outcome::Run(args) = parsed(&["--", "p"]) else {
            panic!("did not parse");
        };
        assert_eq!(args.stage, None);
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
            &["--refuse-open", "0", "--", "p"],
            &["--stage-silent", "--", "p"],
            &["--stage", "", "--", "p"],
            &["--stage", "d", "--stage-route-after", "0", "--", "p"],
            &["--wat", "--", "p"],
            &["--limit"],
        ] {
            assert!(matches!(parsed(bad), Outcome::Usage(_)), "{bad:?}");
        }
        assert!(matches!(parsed(&["--help"]), Outcome::Help));
    }
}
