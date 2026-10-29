//! `pencere-editor`: the editor of text of pencere's native views — a document, its selections,
//! its history and what an input method composes, which a host puts in the room a page gives it
//! (`pencere_view::Element::editor`), the same in every host.
//!
//! The editor is the host's, as a text field is, so that an input method's questions (where the
//! caret is, what is composed, what text is around it) are answered at once, while the page asks
//! of it, and is told by it, across the border between them: what changed ([`Note::Edited`]),
//! where the carets are ([`Note::Selection`]), and what it would not do ([`Note::Refused`]). The
//! page changes the text against the version it saw ([`Buffer::change`]: carried over what the
//! person typed since), colours it, marks it and says where it folds, by layers
//! ([`Buffer::decorate`]), and reads it through a [`Snapshot`] the host posts it. What the words
//! mean (the language, completions, errors) is the page's or its program's: the editor knows none,
//! and does what the page's options say of how the language is written (its comments, the rules of
//! its indent, folding by the indent).
//!
//! The editor has no clock and does nothing on its own: what depends on time (which changes are
//! undone together) is told the time by the host.
//!
//! A place is a character (a code point), as [`pencere_view::edit`] says; the units of UTF-16 an
//! input method of macOS counts in are turned into places here ([`Buffer::place_of_utf16`]).

mod buffer;
mod changes;
mod commands;
mod decorations;
mod editors;
mod find;
mod guides;
mod history;
mod indent;
mod lines;
mod matches;
mod segments;
mod text;
mod view;

pub use buffer::{Buffer, Columns, Note, Placement};
pub use editors::{EditorEvent, Editors, Measured, Route};
pub use find::FIND_MATCHES;
pub use history::{UNDO_DEPTH, UNDO_PAUSE};
pub use text::Snapshot;
pub use view::CaretPhase;
