//! The stand-in's proof of its own route (specification §4.10): it asks the program for a listener
//! on the loopback of its machine, connects to the port the program names, and reads the token it
//! sent back. The stand-in's route is this machine's own, which is the program's, so the proof
//! holds; the options make it fail the way a route that ends elsewhere, or a program that is late,
//! would.

use std::io::Read;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::thread::sleep;
use std::time::{Duration, Instant};

use pencere_fake_host::log;

/// How long a connection, or a wait for what it gives, may take.
const WITHIN: Duration = Duration::from_secs(3);

/// What the command line asks of the proof.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Probing {
    /// Ask the program for a listener that stays up this many seconds, as each view starts
    /// (`--probe`).
    pub ttl: Option<u8>,
    /// Ask a second time before the first is answered, so that the program has one up (`--probe-twice`).
    pub twice: bool,
    /// Connect only after the listener's time is up (`--probe-late`).
    pub late: bool,
    /// Connect a second time once the token is read (`--probe-reconnect`).
    pub reconnect: bool,
}

impl Probing {
    /// Whether any option besides `ttl` is set.
    pub fn has_flaws(&self) -> bool {
        self.twice || self.late || self.reconnect
    }
}

/// How long the stand-in waits for the program's `probed` (§4.10 has the wait bounded): a program
/// that does not know the kind, or cannot listen, never answers, and what the script does next does
/// not wait for it for ever. Long enough for any program that does answer, on a busy machine.
pub const ANSWER_WAIT: Duration = Duration::from_secs(10);

/// The `probe` that is out, and since when the stand-in has waited for its answer.
#[derive(Debug, Default)]
pub struct Waiting(Option<(u32, Instant)>);

impl Waiting {
    /// The `probe` of this id was sent at `now`: the one out from now on.
    pub fn sent(&mut self, probe: u32, now: Instant) {
        self.0 = Some((probe, now));
    }

    /// Whether a `probe` is out.
    pub fn out(&self) -> bool {
        self.0.is_some()
    }

    /// The program answered the `probe` of this id: whether it is the one out, which is out no more.
    /// The answer to one given up is not believed.
    pub fn answered(&mut self, probe: u32) -> bool {
        self.0.take_if(|(out, _)| *out == probe).is_some()
    }

    /// Nothing is waited for: the view the `probe` was out for ended.
    pub fn clear(&mut self) {
        self.0 = None;
    }

    /// Whether the wait for the answer was over at `now`: the `probe` is given up then.
    pub fn given_up(&mut self, now: Instant) -> bool {
        self.0
            .take_if(|(_, since)| now.duration_since(*since) >= ANSWER_WAIT)
            .is_some()
    }
}

/// What came of a connection to the listener.
enum Told {
    /// The 32 bytes the listener gave.
    Token(String),
    /// What went wrong, in words.
    Failed(String),
}

fn read_token(port: u16) -> Told {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let stream = match TcpStream::connect_timeout(&address, WITHIN) {
        Ok(stream) => stream,
        Err(error) => return Told::Failed(format!("could not be reached ({:?})", error.kind())),
    };
    let _ = stream.set_read_timeout(Some(WITHIN));
    let mut bytes = Vec::new();
    // The listener writes the token and closes: what comes before the end is all there is.
    if let Err(error) = stream.take(64).read_to_end(&mut bytes) {
        return Told::Failed(format!("gave nothing in time ({:?})", error.kind()));
    }
    match String::from_utf8(bytes) {
        Ok(text) => Told::Token(text),
        Err(_) => Told::Failed("gave bytes that are not text".to_owned()),
    }
}

/// Connects to the listener at `port` of the loopback, by the route this stand-in has, and says
/// whether it gave `token` back.
pub fn prove(port: u16, token: &str, how: &Probing) {
    if how.late {
        let waits = u64::from(how.ttl.unwrap_or(1)) + 1;
        log::line("host", format!("waits {waits} seconds before it connects"));
        sleep(Duration::from_secs(waits));
    }
    match read_token(port) {
        Told::Token(text) if text == token => {
            log::line(
                "host",
                format!(
                    "the route reaches the program's loopback: the listener at port {port} gave the token back"
                ),
            );
        }
        Told::Token(text) => log::line(
            "host",
            format!(
                "no route: the listener at port {port} gave {} bytes that are not the token",
                text.len()
            ),
        ),
        Told::Failed(why) => log::line(
            "host",
            format!("no route: the listener at port {port} {why}"),
        ),
    }
    if how.reconnect {
        match read_token(port) {
            Told::Failed(why) => log::line(
                "host",
                format!("a second connection to port {port} is refused: it {why}"),
            ),
            Told::Token(_) => log::line(
                "host",
                format!("a second connection to port {port} was given a token"),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_probe_is_waited_for_a_bounded_time_and_an_answer_after_it_is_not_believed() {
        let at = Instant::now();
        let mut waiting = Waiting::default();
        assert!(!waiting.out() && !waiting.given_up(at + ANSWER_WAIT));
        waiting.sent(3, at);
        assert!(waiting.out());
        assert!(!waiting.given_up(at + ANSWER_WAIT - Duration::from_millis(1)));
        assert!(waiting.given_up(at + ANSWER_WAIT));
        assert!(!waiting.out(), "given up once");
        assert!(!waiting.given_up(at + ANSWER_WAIT * 2));
        assert!(!waiting.answered(3), "a late answer is not believed");
        // The answer to the one out, and not to another.
        waiting.sent(4, at);
        assert!(!waiting.answered(3));
        assert!(waiting.answered(4));
        assert!(!waiting.out());
        waiting.sent(5, at);
        waiting.clear();
        assert!(!waiting.out() && !waiting.answered(5));
    }
}
