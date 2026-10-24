//! The container list: a worked page written with `pencere-view`, and what the engine's tests are
//! gauged with. The module that runs in a terminal's sandbox (`kit/guest/containers`) and the
//! program that serves it the numbers (`examples/containers-rust`) are written against this
//! one page, so that what a list says is the page's own, not a copy in either.

pub mod containers;

pub use containers::{Container, Containers, Reading, State, readings};
