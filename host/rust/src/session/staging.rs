//! The folder of the host on the program's machine (specification §8.11): the way a file's bytes
//! cross when the program and the person's terminal are far apart, without passing through the
//! terminal's stream.
//!
//! A host that has such a folder says so ([`Session::offer_stage`]) in the `welcome`, with where it
//! is and the token its `marker` file holds. The program asks for the file of a handle with a
//! `stage`, or sends the file it wants saved in a place of a handle with a `fetch`; the session
//! checks the handle and counts what is out, and asks the terminal to make a pipe in the folder and
//! to move the bytes ([`Action::StageFile`], [`Action::FetchFile`]). Each is answered twice, as an
//! `attach` is: first the name of the pipe, once it exists ([`Session::pipe_ready`]), then how it
//! ended ([`Session::staged`], [`Session::fetched`]).
//!
//! The session names the pipes (128 bits of the terminal's randomness, as a handle is), so that a
//! name is never the person's or the program's, and does none of the I/O. A view's transfers end
//! with it, whatever ended it (§10.2).

use super::files::{Handle, MAX_OPEN_TRANSFERS, Saving, Use, hex};
use super::{Action, Session};
use crate::bundle::Store;
use crate::frame::{Message, is_handle};
use crate::message::{self, Fetch, Stage, Unstage};
use crate::token::Entropy;

pub use crate::message::StageDir;

/// What a `stage` or a `fetch` that is out is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Way {
    /// The file of a handle goes to the program.
    Stage,
    /// The program's file goes to the place of a handle; `size` is its whole length.
    Fetch { size: u64 },
}

/// A `stage` or a `fetch` that is out: the program's message, the handle, the pipe.
pub(super) struct Transfer {
    /// The id of the program's message, for the `re` of what answers it.
    pub(super) id: u32,
    handle: String,
    way: Way,
    /// The name of the pipe in the folder.
    pipe: String,
    /// Whether the program has been told the pipe's name.
    announced: bool,
}

/// Why a file did not cross to the program (the codes of `staged`, specification §8.11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StageError {
    /// The file is not what it was when the person gave it, or cannot be read any more.
    Gone,
    /// The `offset` is past the end of the file.
    Size,
    /// The way to the program's machine is gone for the time being; the program asks again from
    /// where its part ends.
    Route,
    /// Any other failure.
    Io,
}

impl StageError {
    fn code(self) -> &'static str {
        match self {
            Self::Gone => "gone",
            Self::Size => "size",
            Self::Route => "route",
            Self::Io => "io",
        }
    }
}

/// Why a file did not arrive from the program (the codes of `fetched`, specification §8.11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FetchError {
    /// The pipe ended before `size` bytes had come, or the pipe held more.
    Size,
    /// The place the person chose has no room.
    Space,
    /// The way to the program's machine is gone for the time being. The terminal keeps the part
    /// it wrote, which holds `at` bytes of the file; the program sends the rest from there.
    Route {
        /// The length of the file the terminal holds.
        at: u64,
    },
    /// Any other failure.
    Io,
}

impl FetchError {
    fn code(self) -> &'static str {
        match self {
            Self::Size => "size",
            Self::Space => "space",
            Self::Route { .. } => "route",
            Self::Io => "io",
        }
    }
}

impl<S: Store, E: Entropy> Session<S, E> {
    /// Sets where the host's folder on the program's machine is (§8.11), or takes it back with
    /// `None`. A host that has one lists `stage` in the `features` of its `welcome`s from now on,
    /// and says where the folder is; a `stage`, a `fetch` or an `unstage` that comes to one that
    /// does not is ignored.
    pub fn offer_stage(&mut self, stage: Option<StageDir>) {
        self.stage = stage;
    }

    pub(super) fn on_stage(&mut self, message: &Message) {
        if self.stage.is_none() {
            self.ignored();
            return;
        }
        let Some(stage) = crate::json::from_object::<Stage>(&message.payload) else {
            self.ignored();
            return;
        };
        let refusal = match self.files() {
            None => Some("handle"),
            Some(files) => match files.handles.get(&stage.handle) {
                Some(Handle {
                    usage: Use::Read { size },
                    ..
                }) if is_handle(&stage.handle) => {
                    if stage.offset > *size {
                        Some("size")
                    } else if files.transfers.len() >= MAX_OPEN_TRANSFERS {
                        Some("busy")
                    } else {
                        None
                    }
                }
                _ => Some("handle"),
            },
        };
        if let Some(code) = refusal {
            self.answer_transfer(
                Way::Stage,
                message.id,
                &message::transfer_failed(code, None),
            );
            return;
        }
        let pipe = hex(&self.entropy.token_bytes());
        let id = message.id;
        if let Some(files) = self.files_mut() {
            files.transfers.push(Transfer {
                id,
                handle: stage.handle.clone(),
                way: Way::Stage,
                pipe: pipe.clone(),
                announced: false,
            });
        }
        self.actions.push(Action::StageFile {
            stage: id,
            handle: stage.handle,
            offset: stage.offset,
            pipe,
        });
    }

    pub(super) fn on_fetch(&mut self, message: &Message) {
        if self.stage.is_none() {
            self.ignored();
            return;
        }
        let Some(fetch) = crate::json::from_object::<Fetch>(&message.payload) else {
            self.ignored();
            return;
        };
        let refusal = match self.files() {
            None => Some("handle"),
            Some(files) => match files.handles.get(&fetch.handle) {
                Some(Handle {
                    usage: Use::Save,
                    saving: Saving::Open { next, .. },
                }) if is_handle(&fetch.handle) => {
                    // The file is sent in order and without a gap, whether it is the first of it
                    // or the rest of it after a `route`: the offset is where the part ends.
                    if fetch.offset != *next || fetch.offset > fetch.size {
                        Some("size")
                    } else if files.transfers.len() >= MAX_OPEN_TRANSFERS {
                        Some("busy")
                    } else {
                        None
                    }
                }
                _ => Some("handle"),
            },
        };
        if let Some(code) = refusal {
            self.answer_transfer(
                Way::Fetch { size: 0 },
                message.id,
                &message::transfer_failed(code, None),
            );
            return;
        }
        let pipe = hex(&self.entropy.token_bytes());
        let id = message.id;
        if let Some(files) = self.files_mut() {
            if let Some(h) = files.handles.get_mut(&fetch.handle) {
                h.saving = Saving::Fetching;
            }
            files.transfers.push(Transfer {
                id,
                handle: fetch.handle.clone(),
                way: Way::Fetch { size: fetch.size },
                pipe: pipe.clone(),
                announced: false,
            });
        }
        self.actions.push(Action::FetchFile {
            fetch: id,
            handle: fetch.handle,
            offset: fetch.offset,
            size: fetch.size,
            pipe,
        });
    }

    /// The program takes back a `stage` or a `fetch`: the terminal stops it ([`Action::Unstage`])
    /// and the program is told `cancelled`. What the id names being no transfer that is out (it
    /// ended, or never was), the message is ignored.
    pub(super) fn on_unstage(&mut self, message: &Message) {
        if self.stage.is_none() {
            self.ignored();
            return;
        }
        let Some(unstage) = crate::json::from_object::<Unstage>(&message.payload) else {
            self.ignored();
            return;
        };
        let Some(transfer) = self.take_transfer(unstage.id, |_| true) else {
            self.ignored();
            return;
        };
        self.actions.push(Action::Unstage {
            transfer: transfer.id,
        });
        if let Way::Fetch { .. } = transfer.way {
            // What was written of the file is thrown away, and the place is as it was before the
            // fetch: the program that gave up the pipe may send the file in `write`s, or fetch
            // again, from the start.
            self.set_saving(
                &transfer.handle,
                Saving::Open {
                    next: 0,
                    written: false,
                },
            );
            self.actions.push(Action::AbortWrite {
                handle: transfer.handle.clone(),
            });
        }
        self.answer_transfer(
            transfer.way,
            transfer.id,
            &message::transfer_failed("cancelled", None),
        );
    }

    /// The pipe of the transfer whose `stage` or `fetch` this is exists (the terminal made it for
    /// [`Action::StageFile`] or [`Action::FetchFile`]): the program is told its name, and opens it.
    /// A transfer that is not out, or whose pipe has been told, is ignored.
    pub fn pipe_ready(&mut self, transfer: u32) {
        let told = self
            .files_mut()
            .and_then(|files| files.transfers.iter_mut().find(|t| t.id == transfer))
            .filter(|t| !t.announced)
            .map(|t| {
                t.announced = true;
                (t.way, t.pipe.clone())
            });
        match told {
            Some((way, pipe)) => {
                self.answer_transfer(way, transfer, &message::transfer_pipe(&pipe))
            }
            None => self.ignored(),
        }
    }

    /// What came of the [`Action::StageFile`] whose `stage` this is: the file went through the pipe
    /// to its end, which is `end` bytes into it, or it did not and why. The program is told, and
    /// the transfer is over. A result for a transfer that is not out (its view ended, or the
    /// program took it back) is ignored.
    pub fn staged(&mut self, transfer: u32, result: Result<u64, StageError>) {
        if self
            .take_transfer(transfer, |way| way == Way::Stage)
            .is_none()
        {
            self.ignored();
            return;
        }
        let payload = match result {
            Ok(end) => message::transfer_sent(end),
            Err(error) => message::transfer_failed(error.code(), None),
        };
        self.answer_transfer(Way::Stage, transfer, &payload);
    }

    /// What came of the [`Action::FetchFile`] whose `fetch` this is: the file is whole in the place
    /// the person chose and `saved` bytes long, or it did not arrive and why. The program is told,
    /// and the transfer is over. A failure other than a `route` throws away what was written
    /// ([`Action::AbortWrite`]), as a write that failed does; after a `route` the part stays, for
    /// the program to send the rest of the file. A result for a transfer that is not out is
    /// ignored.
    pub fn fetched(&mut self, transfer: u32, result: Result<u64, FetchError>) {
        let Some(Transfer { handle, way, .. }) =
            self.take_transfer(transfer, |way| matches!(way, Way::Fetch { .. }))
        else {
            self.ignored();
            return;
        };
        let payload = match result {
            Ok(saved) => {
                self.set_saving(&handle, Saving::Ended);
                message::transfer_saved(saved)
            }
            Err(FetchError::Route { at }) => {
                let saving = Saving::Open {
                    next: at,
                    written: true,
                };
                self.set_saving(&handle, saving);
                message::transfer_failed("route", Some(at))
            }
            Err(error) => {
                self.save_failed(&handle);
                message::transfer_failed(error.code(), None)
            }
        };
        self.answer_transfer(way, transfer, &payload);
    }

    fn set_saving(&mut self, handle: &str, saving: Saving) {
        if let Some(h) = self.files_mut().and_then(|f| f.handles.get_mut(handle)) {
            h.saving = saving;
        }
    }

    /// Takes the transfer with this id, if it is one of a way `accept` takes, out of the ones that
    /// are out.
    fn take_transfer(&mut self, id: u32, accept: impl Fn(Way) -> bool) -> Option<Transfer> {
        let files = self.files_mut()?;
        let index = files
            .transfers
            .iter()
            .position(|t| t.id == id && accept(t.way))?;
        Some(files.transfers.remove(index))
    }

    /// Writes the `staged` or the `fetched` that answers the message with this id.
    fn answer_transfer(&mut self, way: Way, re: u32, payload: &str) {
        let kind = match way {
            Way::Stage => "staged",
            Way::Fetch { .. } => "fetched",
        };
        self.send(kind, Some(re), payload);
    }
}
