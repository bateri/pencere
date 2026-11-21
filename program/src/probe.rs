//! The proof of the terminal's own route (specification §4.10): a listener on the loopback of the
//! program's machine that gives one token to one connection.
//!
//! The terminal asks with a `probe`. The program binds `127.0.0.1` at a port the system chooses,
//! says the port in the `probed` that answers, and hands the listener to a thread that waits for
//! the one connection: it closes the listener when the connection has come, and only then writes the
//! token, so that a second connection finds no listener. The thread also closes it when `ttl`
//! seconds have passed with no connection, and when the view that was shown has ended.

use std::io::Write;
use std::net::{Ipv4Addr, Shutdown, TcpListener};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant};

use pencere_wire::frame::Message as Frame;

use crate::link::Writer;
use crate::payload::{self, Asked, NotListening};
use crate::terminal::spawn_quiet;

/// How long to wait before looking again at a listener nobody has connected to.
const LOOK_AGAIN: Duration = Duration::from_millis(10);

/// How long the terminal's connection may take to be written the token: it has just connected and
/// reads, so this is only for one that does not.
const WRITE_WITHIN: Duration = Duration::from_secs(5);

/// The listener of the program, if it has one up (§4.10: at most one).
#[derive(Default)]
pub(crate) struct Probe {
    up: Option<Up>,
}

/// A listener that was opened.
struct Up {
    /// The view ended, or the program is leaving: the listener is to be closed.
    stop: Arc<AtomicBool>,
    /// The thread has closed the listener.
    closed: Arc<AtomicBool>,
}

impl Probe {
    /// A `probe` of the terminal: answers it with the port of a new listener, or why there is none.
    pub(crate) fn on_probe(&mut self, message: &Frame, writer: &mut Writer) {
        let Some(asked) = payload::probe(&message.payload) else {
            Self::say(message, writer, Err(NotListening::Invalid));
            return;
        };
        if self.is_up() {
            Self::say(message, writer, Err(NotListening::Busy));
            return;
        }
        let bound = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).and_then(|listener| {
            listener.set_nonblocking(true)?;
            let port = listener.local_addr()?.port();
            Ok((listener, port))
        });
        let Ok((listener, port)) = bound else {
            Self::say(message, writer, Err(NotListening::Io));
            return;
        };
        Self::say(message, writer, Ok(port));
        let (stop, closed) = (
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        let (thread_stop, thread_closed) = (Arc::clone(&stop), Arc::clone(&closed));
        let until = Instant::now() + Duration::from_secs(asked.ttl);
        // A thread that could not be made leaves the listener closed, as it drops it: what the
        // terminal connects to is then refused, which is a route that was not shown.
        let started = spawn_quiet(move || {
            wait(listener, &asked, until, &thread_stop);
            thread_closed.store(true, Ordering::Release);
        });
        if started.is_ok() {
            self.up = Some(Up { stop, closed });
        }
    }

    /// The view is over, or the program leaves: the listener, if it is up, is closed.
    pub(crate) fn end(&mut self) {
        if let Some(up) = self.up.take() {
            up.stop.store(true, Ordering::Release);
        }
    }

    fn is_up(&self) -> bool {
        self.up
            .as_ref()
            .is_some_and(|up| !up.closed.load(Ordering::Acquire))
    }

    fn say(message: &Frame, writer: &mut Writer, answer: Result<u16, NotListening>) {
        let _ = writer.message("probed", Some(message.id), None, &payload::probed(answer));
    }
}

/// Waits for the one connection, and gives it the token: the listener is closed before the token is
/// written, and also when `until` comes or `stop` is set.
fn wait(listener: TcpListener, asked: &Asked, until: Instant, stop: &AtomicBool) {
    while !stop.load(Ordering::Acquire) && Instant::now() < until {
        match listener.accept() {
            Ok((mut stream, _)) => {
                drop(listener);
                // What an accepted socket inherits of the listener's mode is the system's.
                if stream.set_nonblocking(false).is_ok()
                    && stream.set_write_timeout(Some(WRITE_WITHIN)).is_ok()
                    && stream.write_all(asked.token.as_bytes()).is_ok()
                {
                    let _ = stream.shutdown(Shutdown::Write);
                }
                return;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => sleep(LOOK_AGAIN),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return,
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use std::io::{ErrorKind, Read};
    use std::net::{SocketAddr, TcpStream};
    use std::thread;

    use super::*;

    const TOKEN: &str = "3f9c2a7be1d84056a9c4e7b1d2f80a65";

    /// A listener on the loopback, waited on by a thread as a `probe` has it, and its address.
    fn up(ttl: Duration, stop: &Arc<AtomicBool>) -> (SocketAddr, thread::JoinHandle<()>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("a listener");
        listener.set_nonblocking(true).expect("non-blocking");
        let address = listener.local_addr().expect("an address");
        let asked = Asked {
            token: TOKEN.to_owned(),
            ttl: 1,
        };
        let stop = Arc::clone(stop);
        let thread = thread::spawn(move || wait(listener, &asked, Instant::now() + ttl, &stop));
        (address, thread)
    }

    /// A `probe` of the terminal with this id and payload, as the loop reads it.
    fn probe_of(id: u32, payload: &str) -> Frame {
        use pencere_wire::frame::{Direction, Event, Outgoing, Split, StreamDecoder, encode};
        let mut bytes = Vec::new();
        encode(
            Direction::ToProgram,
            &Outgoing {
                kind: "probe",
                id,
                re: None,
                digest: None,
                file: pencere_wire::frame::FileParams::NONE,
                payload,
            },
            Split::Fill,
            &mut bytes,
        )
        .expect("a frame");
        let mut events = Vec::new();
        StreamDecoder::new(Direction::ToProgram).feed(&bytes, &mut events);
        let Some(Event::Message(message)) = events.pop() else {
            panic!("no message");
        };
        message
    }

    #[test]
    fn a_probe_is_answered_with_the_port_of_a_listener_one_at_a_time_and_invalid_when_it_is_not_one()
     {
        use std::os::fd::AsRawFd;
        let (mut read, write) = std::os::unix::net::UnixStream::pair().expect("a pair of sockets");
        let mut writer = Writer::new(write.as_raw_fd());
        let mut probe = Probe::default();
        let good = format!(r#"{{"token":"{TOKEN}","ttl":5}}"#);
        probe.on_probe(&probe_of(3, &good), &mut writer);
        // A listener is up: the next is busy. One that is not a probe is invalid whatever else.
        probe.on_probe(&probe_of(4, &good), &mut writer);
        probe.on_probe(&probe_of(5, r#"{"token":"x","ttl":5}"#), &mut writer);
        while writer.pending() {
            writer.pump().expect("a piece");
        }
        probe.end();
        drop(write);
        let mut got = Vec::new();
        read.read_to_end(&mut got).expect("what was written");
        let mut events = Vec::new();
        pencere_wire::frame::StreamDecoder::new(pencere_wire::frame::Direction::ToHost)
            .feed(&got, &mut events);
        let answers: Vec<(u32, Option<u32>, String)> = events
            .into_iter()
            .filter_map(|event| match event {
                pencere_wire::frame::Event::Message(m) if m.kind == "probed" => {
                    Some((m.id, m.re(), m.payload.clone()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(answers.len(), 3, "{answers:?}");
        assert_eq!(answers[0].1, Some(3));
        let port: serde_json::Value = serde_json::from_str(&answers[0].2).expect("JSON");
        assert!(
            port["port"]
                .as_u64()
                .is_some_and(|port| (1..=65535).contains(&port))
        );
        assert_eq!(
            (answers[1].1, answers[1].2.as_str()),
            (Some(4), r#"{"code":"busy"}"#)
        );
        assert_eq!(
            (answers[2].1, answers[2].2.as_str()),
            (Some(5), r#"{"code":"invalid"}"#)
        );
    }

    #[test]
    fn the_one_connection_is_given_the_token_and_the_next_finds_no_listener() {
        let stop = Arc::new(AtomicBool::new(false));
        let (address, thread) = up(Duration::from_secs(10), &stop);
        let mut first = TcpStream::connect(address).expect("it listens on the loopback");
        assert!(address.ip().is_loopback());
        let mut bytes = Vec::new();
        first.read_to_end(&mut bytes).expect("the token");
        assert_eq!(bytes, TOKEN.as_bytes());
        thread.join().expect("the thread ends");
        // The listener was closed before the token was written: no connection is made now.
        let second = TcpStream::connect(address);
        assert_eq!(
            second.err().map(|e| e.kind()),
            Some(ErrorKind::ConnectionRefused)
        );
    }

    #[test]
    fn a_listener_nobody_connects_to_is_closed_when_its_time_is_up() {
        let stop = Arc::new(AtomicBool::new(false));
        let (address, thread) = up(Duration::from_millis(100), &stop);
        thread.join().expect("the thread ends");
        let late = TcpStream::connect(address);
        assert_eq!(
            late.err().map(|e| e.kind()),
            Some(ErrorKind::ConnectionRefused)
        );
    }

    #[test]
    fn a_listener_is_closed_when_the_view_ends() {
        let stop = Arc::new(AtomicBool::new(false));
        let (address, thread) = up(Duration::from_secs(30), &stop);
        stop.store(true, Ordering::Release);
        thread.join().expect("the thread ends");
        let late = TcpStream::connect(address);
        assert_eq!(
            late.err().map(|e| e.kind()),
            Some(ErrorKind::ConnectionRefused)
        );
    }
}
