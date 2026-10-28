//! `pencere-drive`: a page of pencere's native views, driven.
//!
//! A page of the kit is a module or a script in a sandbox ([`pencere_sandbox`]) that hands out a
//! tree of boxes and lines of text ([`pencere_view`]) and answers what the person does. Between the
//! two stands what the kit's specification (Section 4) asks of every host: the page laid out, its
//! boxes scrolled, its editors (`pencere-editor`) put in their rooms, a key sent to the page's
//! command for it, an editor's or the page, its replies done, its wakes given, what it says to its
//! program carried. That is this crate, written once, so that every host does it the same way:
//! [`Page`].
//!
//! What only a host can do stays the host's: measuring text and drawing the draw list in its fonts
//! and colours, its keyboard and input method, its pasteboard, its pointer, its fields, its menus,
//! its browser, its screen reader, its clock and its program. The drive tells the host what to do
//! ([`Drained`], [`Host`]) and never waits for it.
//!
//! The crate knows no window, no renderer, no font and no terminal.

mod editing;
pub mod headless;
mod host;
mod outline;
mod page;
mod scrolling;
#[cfg(feature = "shot")]
pub mod shot;
mod waking;

pub use host::{Drained, Host};
pub use outline::{Field, Outline, Placement, Tip, ZOOM_TEXT};
pub use page::{FrameAt, NoText, Page, PageEditors, Painted, Setup};
pub use pencere_editor::Route;
pub use scrolling::Scrolling;

#[cfg(test)]
mod headless_tests;
#[cfg(all(test, feature = "program"))]
mod in_process_tests;
#[cfg(test)]
mod library_languages_tests;
#[cfg(test)]
mod luau_editor_tests;
#[cfg(test)]
mod page_tests;
#[cfg(all(test, feature = "shot"))]
mod shot_tests;
