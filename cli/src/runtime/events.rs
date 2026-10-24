//! What other threads tell the runner's loop: the calls of the view (which come in on threads of
//! the program library's), the end of `serve`, and the watcher of `pencere dev`. The loop is
//! the one thread that runs Lua; these are queued for it and a byte in a pipe wakes it.

use std::io;
use std::os::fd::RawFd;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use pencere::{Call, Outcome, Subscription, ViewError};
use serde_json::Value;

/// A call of the view, for the handler in Lua; the answer goes back to the thread that waits.
pub struct CallEvent {
    pub method: String,
    pub params: Value,
    pub call: Call,
    pub reply: Sender<Result<Value, ViewError>>,
}

/// A subscription of the view, for the handler in Lua.
pub struct SubscribeEvent {
    pub id: u64,
    pub topic: String,
    pub params: Value,
    pub subscription: Subscription,
    /// Whether the handler started the subscription: the view hears it if it did not.
    pub started: Sender<Result<(), ViewError>>,
}

pub enum Event {
    Call(CallEvent),
    Subscribe(SubscribeEvent),
    /// The subscription ended: the view cancelled it, or the view is over.
    SubscriptionEnded(u64),
    /// `serve` returned.
    Served(Result<Outcome, pencere::Error>),
    /// The app is to start again from its files (`pencere dev`).
    Restart,
    /// Files changed under a directory the app watches.
    Watched {
        id: u64,
        changes: Vec<super::watch::Change>,
    },
}

/// A pipe a thread writes a byte into to wake the loop that polls the other end.
struct Wake {
    read: RawFd,
    write: RawFd,
}

impl Wake {
    fn new() -> io::Result<Self> {
        let mut ends = [0 as libc::c_int; 2];
        // SAFETY: fills two descriptors.
        if unsafe { libc::pipe(ends.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        for fd in ends {
            // SAFETY: flags of a descriptor of ours.
            unsafe {
                libc::fcntl(fd, libc::F_SETFL, libc::O_NONBLOCK);
                libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
            }
        }
        Ok(Self {
            read: ends[0],
            write: ends[1],
        })
    }
}

impl Drop for Wake {
    fn drop(&mut self) {
        // SAFETY: closes what `new` opened.
        unsafe {
            libc::close(self.read);
            libc::close(self.write);
        }
    }
}

/// The way to the loop, for any thread.
#[derive(Clone)]
pub struct Events {
    sender: Sender<Event>,
    wake: Arc<Wake>,
}

/// The loop's end of it.
pub struct Inbox {
    receiver: Receiver<Event>,
    wake: Arc<Wake>,
}

pub fn channel_pair() -> io::Result<(Events, Inbox)> {
    let (sender, receiver) = channel();
    let wake = Arc::new(Wake::new()?);
    Ok((
        Events {
            sender,
            wake: Arc::clone(&wake),
        },
        Inbox { receiver, wake },
    ))
}

impl Events {
    /// Hands `event` to the loop; whether the loop is still there to take it.
    pub fn send(&self, event: Event) -> bool {
        let sent = self.sender.send(event).is_ok();
        if sent {
            self.poke();
        }
        sent
    }

    /// Wakes the loop with nothing for it: it looks at what it waits for again.
    pub fn poke(&self) {
        // SAFETY: writes a byte to a descriptor of ours. A full pipe already holds a wake.
        unsafe {
            libc::write(self.wake.write, [1u8].as_ptr().cast(), 1);
        }
    }
}

impl Inbox {
    /// The descriptor the loop polls.
    pub fn fd(&self) -> RawFd {
        self.wake.read
    }

    /// Takes what is waiting, and empties the pipe.
    pub fn take(&self) -> Vec<Event> {
        let mut scratch = [0u8; 64];
        loop {
            // SAFETY: reads into a buffer of ours from a descriptor of ours.
            let got =
                unsafe { libc::read(self.wake.read, scratch.as_mut_ptr().cast(), scratch.len()) };
            if got <= 0 {
                break;
            }
        }
        self.receiver.try_iter().collect()
    }
}
