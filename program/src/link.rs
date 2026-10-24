//! The stream between the program and its terminal: the frames it writes, in order and one
//! writer, and the frames and the terminal's own answers that it reads.
//!
//! **One thread writes.** The handlers of a program run on threads of their own and hand what
//! they have to say to the event loop, which writes it; so a frame is never cut by another's and
//! the signal handler's rule — whatever is queued is dropped, and the `bye` goes first — has one
//! queue to apply to.
//!
//! **A write never waits for a terminal that is not reading.** The loop writes a few bytes when
//! the tty says it has room and goes on reading in between (a terminal that waits for the
//! program to read before it reads is not deadlocked by a program that waits for it to read).

use std::collections::VecDeque;
use std::io;
use std::os::fd::RawFd;
use std::time::{Duration, Instant};

use pencere_wire::frame::{
    Direction, Event, Ids, Message, Outgoing, Split, StreamDecoder, WriteError, encode,
};

use crate::terminal::{self, write_piece};

/// The frames of the program, waiting to be written.
pub(crate) struct Writer {
    fd: RawFd,
    ids: Ids,
    queue: VecDeque<Vec<u8>>,
    /// The bytes being written and how many are written already.
    current: Option<(Vec<u8>, usize)>,
    /// The bytes waiting, the one being written included.
    waiting: usize,
    /// Nothing is queued any more but what is forced: the program is leaving.
    muted: bool,
}

impl Writer {
    pub(crate) fn new(fd: RawFd) -> Self {
        Self {
            fd,
            ids: Ids::new(),
            queue: VecDeque::new(),
            current: None,
            waiting: 0,
            muted: false,
        }
    }

    /// Queues a message of the protocol; its id. A muted writer drops it (the id is still
    /// taken).
    pub(crate) fn message(
        &mut self,
        kind: &str,
        re: Option<u32>,
        digest: Option<&str>,
        payload: &str,
    ) -> Result<u32, WriteError> {
        let id = self.ids.next_id();
        let mut bytes = Vec::new();
        encode(
            Direction::ToHost,
            &Outgoing {
                kind,
                id,
                re,
                digest,
                payload,
            },
            Split::Fill,
            &mut bytes,
        )?;
        if !self.muted {
            self.push(bytes);
        }
        Ok(id)
    }

    /// Queues the message that ends the program's use of the view, ahead of anything that is
    /// waiting and whether or not the writer is muted: the last words (§10.3).
    pub(crate) fn last_words(&mut self, kind: &str, payload: &str) -> Result<u32, WriteError> {
        let id = self.ids.next_id();
        let mut bytes = Vec::new();
        encode(
            Direction::ToHost,
            &Outgoing {
                kind,
                id,
                re: None,
                digest: None,
                payload,
            },
            Split::Fill,
            &mut bytes,
        )?;
        self.waiting += bytes.len();
        self.queue.push_front(bytes);
        Ok(id)
    }

    /// Queues bytes that are not a frame (the escape that switches the screen).
    pub(crate) fn raw(&mut self, bytes: &[u8]) {
        if !self.muted {
            self.push(bytes.to_vec());
        }
    }

    fn push(&mut self, bytes: Vec<u8>) {
        self.waiting += bytes.len();
        self.queue.push_back(bytes);
    }

    /// Whether something waits to be written.
    pub(crate) fn pending(&self) -> bool {
        self.waiting > 0
    }

    /// How many bytes wait: the loop takes no more from the handlers while this is large, which
    /// is how a handler that pushes faster than the terminal reads is slowed to its pace.
    pub(crate) fn waiting(&self) -> usize {
        self.waiting
    }

    /// Drops what is queued but the frame the writer is in the middle of: that one is written to
    /// its end so that the stream is not left inside a frame (what the `bye` would then be
    /// written into), and no more of its message.
    pub(crate) fn mute(&mut self) {
        self.muted = true;
        self.queue.clear();
        if let Some((bytes, written)) = self.current.as_mut() {
            // A frame ends with `ESC \`, and nothing inside a frame is an ESC.
            let from = written.saturating_sub(1);
            let end = bytes
                .get(from..)
                .and_then(|rest| rest.windows(2).position(|w| w == b"\x1b\\"))
                .map_or(bytes.len(), |at| from + at + 2);
            bytes.truncate(end);
        }
        self.waiting = self
            .current
            .as_ref()
            .map_or(0, |(bytes, written)| bytes.len().saturating_sub(*written));
    }

    /// Writes a few bytes. Call it when the tty says it has room.
    ///
    /// # Errors
    ///
    /// When the tty cannot be written any more (it is gone).
    pub(crate) fn pump(&mut self) -> io::Result<()> {
        if self.current.is_none() {
            self.current = self.queue.pop_front().map(|bytes| (bytes, 0));
        }
        let Some((bytes, written)) = self.current.as_mut() else {
            return Ok(());
        };
        let taken = write_piece(self.fd, bytes.get(*written..).unwrap_or_default())?;
        *written += taken;
        self.waiting = self.waiting.saturating_sub(taken);
        if *written >= bytes.len() {
            self.current = None;
        }
        Ok(())
    }

    /// Writes what is queued, giving up after `within`: the way a program that has been asked to
    /// end says its last words to a terminal that may not be reading.
    pub(crate) fn flush_within(&mut self, within: Duration) -> io::Result<()> {
        let until = Instant::now() + within;
        while self.pending() {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(io::ErrorKind::TimedOut.into());
            }
            let ready = terminal::wait_for_room(self.fd, left)?;
            if ready {
                self.pump()?;
            }
        }
        Ok(())
    }

    /// Forgets everything that is queued, the frame under way included: the stream is going away.
    pub(crate) fn forget_everything(&mut self) {
        self.queue.clear();
        self.current = None;
        self.waiting = 0;
    }
}

/// Finds the terminal's answer to `CSI c` — `ESC [ ? … c` — in a stream, however the stream is
/// cut into reads. A frame has no `ESC` inside it, so the pattern cannot be part of one.
#[derive(Debug, Default)]
pub(crate) struct ReplyScanner {
    state: Scan,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Scan {
    #[default]
    Ground,
    Escape,
    Bracket,
    Parameters,
}

impl ReplyScanner {
    /// Feeds bytes up to the end of the first reply in them: where it ended, or `None` when
    /// all of `bytes` were read and none did.
    pub(crate) fn find(&mut self, bytes: &[u8]) -> Option<usize> {
        for (at, &byte) in bytes.iter().enumerate() {
            self.state = match (self.state, byte) {
                (_, 0x1b) => Scan::Escape,
                (Scan::Escape, b'[') => Scan::Bracket,
                (Scan::Bracket, b'?') | (Scan::Parameters, b'0'..=b'9' | b';') => Scan::Parameters,
                (Scan::Parameters, b'c') => {
                    self.state = Scan::Ground;
                    return Some(at + 1);
                }
                _ => Scan::Ground,
            };
        }
        None
    }
}

/// What the terminal wrote, in the order it wrote it.
#[derive(Debug)]
pub(crate) enum Item {
    /// A message of the protocol.
    Message(Message),
    /// The terminal's answer to `CSI c`.
    Reply,
}

/// What the terminal wrote: the frames in it, and its answer to `CSI c`.
pub(crate) struct Reader {
    decoder: StreamDecoder,
    scanner: ReplyScanner,
}

impl Reader {
    pub(crate) fn new() -> Self {
        Self {
            decoder: StreamDecoder::new(Direction::ToProgram),
            scanner: ReplyScanner::default(),
        }
    }

    /// Reads `bytes`; what is in them, in order: a `welcome` that comes ahead of the reply is a
    /// host, one that comes after it is not (specification §5.3).
    pub(crate) fn feed(&mut self, bytes: &[u8]) -> Vec<Item> {
        let mut items = Vec::new();
        let mut rest = bytes;
        loop {
            let end = self.scanner.find(rest);
            let (part, after) = match end {
                Some(end) => rest.split_at(end.min(rest.len())),
                None => (rest, &[][..]),
            };
            let mut events = Vec::new();
            self.decoder.feed(part, &mut events);
            items.extend(events.into_iter().filter_map(|event| match event {
                Event::Message(message) => Some(Item::Message(message)),
                // A frame that was dropped is one the terminal did not write properly.
                Event::Error(_) => None,
            }));
            if end.is_none() {
                return items;
            }
            items.push(Item::Reply);
            rest = after;
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn the_answer_to_the_fence_is_told_from_noise_however_it_is_cut() {
        let reply = b"\x1b[?62;4c";
        for cut in 0..=reply.len() {
            let mut scanner = ReplyScanner::default();
            let (a, b) = reply.split_at(cut);
            assert!(
                scanner.find(a).is_some() || scanner.find(b).is_some(),
                "cut at {cut}"
            );
        }
        let mut scanner = ReplyScanner::default();
        assert_eq!(scanner.find(b"ls\r\x1b[?1;2c"), Some(10));
        for not in [
            &b"\x1b[?62;4"[..],
            b"ls\r",
            b"\x1b[0c",
            b"\x1b[?62;x4c",
            b"?62;4c",
            // The `c` of a word is not the end of a reply.
            b"\x1b_pencere;welcome;id=1;{}\x1b\\",
        ] {
            let mut scanner = ReplyScanner::default();
            assert_eq!(scanner.find(not), None, "{not:?}");
        }
    }

    #[test]
    fn a_welcome_ahead_of_the_reply_is_read_ahead_of_it_and_one_behind_it_after() {
        let welcome = b"\x1b_pencere;welcome;id=1,re=1;{\"version\":1}\x1b\\";
        let reply = b"\x1b[?62;4c";
        let mut host_first = welcome.to_vec();
        host_first.extend_from_slice(reply);
        let mut reply_first = reply.to_vec();
        reply_first.extend_from_slice(welcome);
        let shape = |bytes: &[u8]| -> String {
            Reader::new()
                .feed(bytes)
                .iter()
                .map(|item| match item {
                    Item::Message(_) => 'm',
                    Item::Reply => 'r',
                })
                .collect()
        };
        assert_eq!(shape(&host_first), "mr");
        assert_eq!(shape(&reply_first), "rm");
    }

    #[test]
    fn a_frame_in_the_middle_of_being_written_is_finished_when_the_rest_is_dropped() {
        use std::io::Read;
        use std::os::fd::AsRawFd;
        use std::os::unix::net::UnixStream;

        let (mut read, write) = UnixStream::pair().expect("a pair of sockets");
        let mut writer = Writer::new(write.as_raw_fd());
        let big = "x".repeat(2000);
        writer
            .message("msg", None, None, &big)
            .expect("a message that is written");
        writer
            .message("msg", None, None, "second")
            .expect("another");
        writer.pump().expect("a piece");
        writer.mute();
        writer.last_words("bye", "{}").expect("the last words");
        while writer.pending() {
            writer.pump().expect("a piece");
        }
        drop(write);
        let mut got = Vec::new();
        read.read_to_end(&mut got).expect("what was written");
        let mut events = Vec::new();
        StreamDecoder::new(Direction::ToHost).feed(&got, &mut events);
        let kinds: Vec<String> = events
            .iter()
            .filter_map(|e| match e {
                Event::Message(m) => Some(m.kind.clone()),
                Event::Error(_) => None,
            })
            .collect();
        // The `msg` that was under way is whole, the second never started, and the `bye` is the
        // last thing in the stream.
        assert_eq!(kinds, ["msg", "bye"]);
    }
}
