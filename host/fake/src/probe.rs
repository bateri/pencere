//! The stand-in's proof of its own route (specification §4.10): it asks the program for a listener
//! on the loopback of its machine, connects to the port the program names, and reads the token it
//! sent back. The stand-in's route is this machine's own, which is the program's, so the proof
//! holds; the options make it fail the way a route that ends elsewhere, or a program that is late,
//! would.

use std::io::Read;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::thread::sleep;
use std::time::Duration;

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
