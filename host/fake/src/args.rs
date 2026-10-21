//! The command line: which program to run in the pane, and what the stand-in
//! host does around it.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;

pub const USAGE: &str = "\
usage: pencere-fake-host [options] -- <program> [args...]

Runs <program> on a pty and plays the host of its views: it answers the
program's capability query, hands out the token, takes the bundle and starts a
view that is a script, not a web view. What happens is written to stderr, one
line each, in the order it happens (program:, host:, view:, screen:).

options:
  --call NAME[=JSON]   the view calls tool NAME (with the arguments JSON, an
                       object) once it has the first result; repeatable, in order
  --then ACTION        what happens when the calls are done: wait (the default;
                       the person ends it), interrupt (^C is typed into the
                       pane), kill (the program is killed with SIGKILL), close
                       (the person closes the view)
  --store DIR          keep verified bundle files in DIR, as a terminal that
                       remembers them between runs does (default: in memory)
  --no-host            act as a terminal that does not host pencere: answer
                       only the reply to CSI c
  --allow-network      grant the domains and permissions a view declares
                       (default: refuse them)
  --passthrough        copy what the program writes to this program's stdout
  --size COLSxROWS     the pane's size (default: this terminal's, else 80x24)
  --limit SECONDS      kill the program and give up after this long (default 30)
  -h, --help           this text
";

/// What the view does when its calls are done.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Then {
    Wait,
    Interrupt,
    Kill,
    Close,
}

/// A tool the scripted view calls.
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug)]
pub struct Args {
    pub program: Vec<String>,
    pub calls: Vec<Call>,
    pub then: Then,
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

fn call(text: &str) -> Result<Call, String> {
    let (name, arguments) = match text.split_once('=') {
        Some((name, json)) => {
            let arguments: Value = serde_json::from_str(json)
                .map_err(|error| format!("--call {name}: the arguments are not JSON: {error}"))?;
            if !arguments.is_object() {
                return Err(format!(
                    "--call {name}: the arguments must be a JSON object"
                ));
            }
            (name, arguments)
        }
        None => (text, Value::Object(serde_json::Map::new())),
    };
    if name.is_empty() {
        return Err("--call needs a tool name".to_owned());
    }
    Ok(Call {
        name: name.to_owned(),
        arguments,
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
        then: Then::Wait,
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
            "--call" | "--then" | "--store" | "--size" | "--limit" => {
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

fn option(parsed: &mut Args, name: &str, value: &str) -> Result<(), String> {
    match name {
        "--call" => parsed.calls.push(call(value)?),
        "--then" => {
            parsed.then = match value {
                "wait" => Then::Wait,
                "interrupt" => Then::Interrupt,
                "kill" => Then::Kill,
                "close" => Then::Close,
                other => {
                    return Err(format!(
                        "--then {other}: not one of wait, interrupt, kill, close"
                    ));
                }
            };
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
    fn a_call_has_a_name_and_optional_arguments() {
        let Outcome::Run(args) = parsed(&[
            "--call",
            "stop_container={\"id\":\"web\"}",
            "--call",
            "list_containers",
            "--",
            "p",
        ]) else {
            panic!("did not parse");
        };
        assert_eq!(args.calls.len(), 2);
        assert_eq!(args.calls[0].name, "stop_container");
        assert_eq!(args.calls[0].arguments["id"], "web");
        assert_eq!(args.calls[1].arguments, serde_json::json!({}));
    }

    #[test]
    fn mistakes_say_what_they_are() {
        for bad in [
            &["--"][..],
            &["--then", "later", "--", "p"],
            &["--call", "x=[1]", "--", "p"],
            &["--size", "80", "--", "p"],
            &["--wat", "--", "p"],
            &["--limit"],
        ] {
            assert!(matches!(parsed(bad), Outcome::Usage(_)), "{bad:?}");
        }
        assert!(matches!(parsed(&["--help"]), Outcome::Help));
    }
}
