//! The names of the sounds the kit has of its own: the palette a page asks for by name and the
//! kit's own operations may ask for with the same words.
//!
//! The names are the interface jobs a sound does (a tap, a toggle, a result that landed), not how
//! it is made, so that a recipe may change and a name stays what it says. The crate holds the
//! names and plays nothing: the sounds themselves are made by the sandbox, which holds the
//! synthesis and the decoders, and are the same on every system.

/// The names of the sounds the kit has, in the order a page is told of them.
pub const NAMES: [&str; 20] = [
    "tap",
    "select",
    "toggle_on",
    "toggle_off",
    "type",
    "open",
    "close",
    "snap",
    "copy",
    "delete",
    "success",
    "warning",
    "error",
    "send",
    "receive",
    "notify",
    "attention",
    "start",
    "complete",
    "celebrate",
];

/// Whether the kit has a sound by this name.
#[must_use]
pub fn is_name(name: &str) -> bool {
    NAMES.contains(&name)
}
