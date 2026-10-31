//! `pencere-sandbox`: a page of the UI kit that lives in a sandbox.
//!
//! A page's logic can be a module — a WebAssembly component of the interface in `wit/view.wit`
//! — that the application runs on a thread of its own and drives with what the person does,
//! and that answers with the page to draw: a tree the engine ([`pencere_view`]) lays out and paints
//! in the terminal's own font and theme. The module reaches nothing but that interface:
//! no file, no network, no clock, no environment. It is held to a memory limit and a time for
//! every call, and a module that goes past either, traps or hands out something that is not a
//! page is closed — the application is told why and goes on.
//!
//! A page can also be a **script**: the logic is then text in the Lua language (Luau) that an
//! interpreter, itself such a module (`LUAU_COMPONENT`), runs under the same terms and hands the
//! same kind of page out of. A script may `require` the other scripts of its bundle and the modules
//! of the kit's library ([`library`]), and nothing else ([`Script`]). A module or a script that stops says why — a script's error comes with its
//! file and its line — and that is what the page's program is told.
//!
//! This crate knows no window, no renderer and no terminal: it depends on the engine and on the
//! runtime, and moves out of the workspace with the engine.

mod decode;
mod figure;
mod images;
pub mod library;
mod luau;
mod module;
mod sandbox;
mod watchdog;
mod world;

#[cfg(test)]
mod guests_tests;
#[cfg(test)]
mod images_tests;
#[cfg(test)]
mod library_fuzzy_tests;
#[cfg(test)]
mod library_keys_tests;
#[cfg(test)]
mod library_languages_tests;
#[cfg(test)]
mod library_markdown_tests;
#[cfg(test)]
mod library_terminal_tests;
#[cfg(test)]
mod library_tests;
#[cfg(test)]
mod luau_containers_tests;
#[cfg(test)]
mod luau_editor_tests;
#[cfg(test)]
mod luau_require_tests;
#[cfg(test)]
mod luau_tests;

pub use figure::draw_figure;
pub use images::{Arrival, Assets, Bitmap, Diagnostic, ImageLimits, Images};
pub use luau::{COMPONENT as LUAU_COMPONENT, Script};
pub use module::{Answer, Frame, Module, Runner, Update};
pub use sandbox::{Fault, ModuleLimits, Program, TimeLimit, Unbounded};
