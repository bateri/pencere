//! `pencere-view`: the engine of the UI kit's pages — a tree of boxes and lines of text, laid out
//! as flexbox and grid, that comes out as a draw list.
//!
//! A page is written as a function from the size it is given to an [`Element`] tree, in the
//! chain of a style sheet's properties (`div().row().gap(8.0).child(...)`), in **points**,
//! with colours named for the theme ([`Role`]) or, where a role does not say it, given as a value
//! ([`Rgba`]). [`layout`] puts the tree on a page of pixels —
//! asking the host, through [`Measure`], how large each line of text is — and
//! [`Laid::paint`] turns it into a [`DrawList`] of boxes and lines of text.
//!
//! The engine draws nothing and links to nothing of the terminal: not its fonts (the host
//! measures what it also draws), not its colours (the host resolves the names in the pane's
//! theme), not its renderer (the host turns the list into what it draws). It depends only on
//! the layout crate and stands apart from the rest of the workspace, so that what a page is
//! written in can be taken, as it is, to wherever a page is shown.
//!
//! A page is not only drawn: it answers the pointer, the wheel and the keyboard. Elements are
//! named ([`Id`]) and the ones that answer the pointer say so ([`Element::press`]); what a page
//! keeps between builds — what is hovered, pressed, focused, how far a list is scrolled, how
//! far a highlight has faded — is a [`Ui`] kept by those names; a long list builds only the
//! rows near what is in view ([`List`], over [`Rows`] and [`Built`]). What the host tells a page
//! is an [`Input`], and what the page tells back a [`Reply`].
//!
//! A page shows images too ([`image`]): files of the bundle it came in, which the host decodes
//! and says the size of ([`Measure::image`]); the engine makes the room for an image and says
//! how it fills it ([`Picture`]), and has no pixels of its own.
//!
//! A page can hold an editor of text, as it holds a text field: the host's, which edits, lays out and
//! draws its document (the `pencere-editor` crate); [`edit`] is what the page and the editor say to
//! each other.
//!
//! A page need not live in the host that shows it: [`wire`] lays an [`Element`] tree down as a flat
//! list and picks it up again, and [`Queries`] is what a page asks of itself laid out — the pair a
//! page kept in a sandbox is written against, as [`Laid`] answers the same questions to one that
//! is not.

mod color;
mod command;
pub mod edit;
mod element;
mod fades;
mod field;
mod figure;
mod geometry;
mod id;
mod input;
mod layout;
mod motion;
mod paint;
mod rich;
mod rows;
mod scrolls;
mod style;
mod system;
mod text;
mod ui;
pub mod wire;

pub use color::{Color, Mix, Rgba, Role, Source};
pub use command::{Command, Shortcut};
pub use element::{
    Element, FieldKind, Fit, Image, State, canvas, div, image, rich, spacer, spark, text,
};
pub use fades::{FADE_SECONDS, Fades};
pub use field::{FieldState, Fill};
pub use figure::{Path, Segment, Stroke, path};
pub use geometry::{Point, Rect, Size};
pub use id::Id;
pub use input::{Button, Cursor, Event, Input, Key, Keyboard, Menu, MenuEntry, Mods, Place, Reply};
pub use layout::{Accessible, AccessibleRole, EditorRoom, Extent, Field, Laid, Queries, layout};
pub use motion::{Ease, Motion, Motions, Shift, Tip};
pub use paint::{DrawList, EditorSlot, Figure, Paint, Picture, Quad, Run};
pub use rich::{Span, span};
pub use rows::{Built, List, Rows};
pub use scrolls::{Bars, Phase, ScrollTo, Scrolls, Wheeled};
pub use style::{Anchor, Border, Gradient, Shadow, Side, Track};
pub use system::{KeyConvention, WallTime};
pub use text::{Align, EditorMetrics, Family, Font, Measure, Metrics};
pub use ui::{Drag, Dropped, Ui};

#[cfg(test)]
mod commands_tests;
#[cfg(test)]
mod editor_tests;
#[cfg(test)]
mod image_tests;
#[cfg(test)]
mod interact_tests;
#[cfg(test)]
mod margins_tests;
#[cfg(test)]
mod motion_tests;
#[cfg(test)]
mod overlay_tests;
#[cfg(test)]
mod rich_tests;
#[cfg(test)]
mod scroll_tests;
#[cfg(test)]
mod tests;
