//! The container list: a worked page written with `pencere-view`, and what the engine's tests are
//! gauged with. The Lua script the sandbox's tests hold to it (`kit/luau/examples/containers.lua`)
//! and the program that serves it the numbers (`examples/containers-rust`) are written against
//! this one page, so that what a list says is the page's own, not a copy in either.

pub mod containers;

pub use containers::{Container, Containers, FIELD, Reading, State, readings};
