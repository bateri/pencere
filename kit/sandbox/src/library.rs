//! The kit's library: modules of Luau that a script requires as `@pencere/<name>`.
//!
//! A module of the library is written on the Lua page API alone (what the prelude gives every
//! page), as an app could have written it itself: a part that every app would otherwise write
//! again, kept right once, in the kit. It is not in the interpreter: the application has it, at its
//! kit's version, and gives it to a script that asks as it gives the scripts of the bundle
//! (`script.has-module`, `script.module`), under paths that start with `@pencere/`, which no path
//! of a bundle can (a bundle's paths have no `@`). So a module is mended without rebuilding the
//! interpreter, and a page that does not ask for one pays nothing for it.
//!
//! The library does not count toward what a page's own scripts may come to (the size of a script,
//! 4 MiB): it is the kit's, held to [`LIBRARY_SIZE`] when the crate is built.

/// The most bytes the library's modules may come to together. The crate does not build past it.
pub const LIBRARY_SIZE: usize = 1 << 20;

/// The prefix of the path the interpreter asks for a module of the library by.
pub const PREFIX: &str = "@pencere/";

// The modules, `MODULES`: the path the interpreter asks for (`@pencere/` and the module's file in
// `library/`, as a script's `require` looks for one) and the source; and `SIZE`, the bytes they come
// to together. The build script lists the directory, which is all that names a module.
include!(concat!(env!("OUT_DIR"), "/library.rs"));

const _: () = assert!(
    SIZE <= LIBRARY_SIZE,
    "the kit's library is larger than LIBRARY_SIZE"
);

/// The source of the module of the library at `path` (`@pencere/syntax.luau`), if there is one.
pub fn source(path: &str) -> Option<&'static str> {
    path.starts_with(PREFIX)
        .then(|| MODULES.iter().find(|(name, _)| *name == path))
        .flatten()
        .map(|(_, text)| *text)
}
