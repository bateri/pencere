//! The interpreter of the scripting language: Luau, in a component of the page interface.
//!
//! A page can be a script instead of a module: the component here is the interpreter, built
//! from `guest/luau` (`make luau`) and kept in the repository, so that building the
//! application needs no toolchain for WebAssembly. It runs in the same sandbox as any module —
//! the same memory, the same time for a call, the same interface and nothing else — and is
//! given the script's text as the one thing a module of its kind is given (`script.source`).
//!
//! Compiling the interpreter to machine code takes the better part of a second, so it is done
//! once in a process: the compiled component is kept (serialized) and every page of a script
//! after the first loads it from there. Each page still has an engine of its own (a module's
//! time limit is its engine's clock, and must not cut off another page); engines made the same
//! way read what one another compiled.
//!
//! A build with no `compiler` feature has no code generator to do that with: the interpreter is
//! compiled while the crate is built (`build.rs`, for the target the crate is built for) and the
//! pages load it as it is: no page waits for the interpreter to be compiled.

use std::sync::Arc;
#[cfg(feature = "compiler")]
use std::sync::{Mutex, PoisonError};

use wasmtime::Engine;
use wasmtime::component::Component;

use crate::images::Assets;
use crate::sandbox::Fault;

/// The interpreter, as a component (`wit/view.wit`'s world `view`, importing `script`). A constant
/// no code names takes no room in an executable: a build with no `compiler` carries the interpreter
/// compiled (`PRECOMPILED`) and not these bytes.
pub const COMPONENT: &[u8] = include_bytes!("../luau/luau.wasm");

/// The interpreter compiled for the target by the build (`build.rs`), with the engine configuration
/// of `engine_config`: what a runtime with no code generator loads (no `compiler` feature).
#[cfg(not(feature = "compiler"))]
static PRECOMPILED: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/luau.cwasm"));

/// A page that is a script: the entry, where it is in the bundle the view came in, and that
/// bundle.
///
/// The script may load the other scripts of the bundle, and nothing else: `require` is Luau's
/// require-by-string, from `path`, and what it finds is what `bundle` says is a script (a file the
/// manifest types `text/x-lua`). The bytes it loads count against [`crate::ModuleLimits::component`],
/// the entry's too.
pub struct Script {
    /// The path of the entry, as the manifest names it: what a `require` is relative to, and
    /// what the errors of the entry say (`screens/logs.lua:12: ...`).
    pub path: String,
    /// The entry's text.
    pub source: Vec<u8>,
    /// The files of the bundle, as the host has them. `None` is a page with no bundle to look
    /// in: nothing it requires is found.
    pub bundle: Option<Arc<dyn Assets>>,
}

/// The interpreter compiled for this process's engines, once there has been a page that needed it.
#[cfg(feature = "compiler")]
static COMPILED: Mutex<Option<Arc<[u8]>>> = Mutex::new(None);

/// The interpreter for a page that runs on `engine`: compiled now, if no page before it has
/// needed it, and read from what that page left otherwise.
#[cfg(feature = "compiler")]
pub(crate) fn component(engine: &Engine) -> Result<Component, Fault> {
    let load = |why: wasmtime::Error| Fault::Load(format!("the interpreter: {why:#}"));
    // Held while compiling, so that two pages opened together compile it once.
    let mut held = COMPILED.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(compiled) = held.as_ref() {
        // SAFETY: the bytes are what `Component::serialize` made in this process, from an
        // engine configured as `sandbox::engine` configures every engine, and have not left
        // memory: nothing that could have changed them since.
        return unsafe { Component::deserialize(engine, compiled) }.map_err(load);
    }
    let component = Component::new(engine, COMPONENT).map_err(load)?;
    *held = Some(component.serialize().map_err(load)?.into());
    Ok(component)
}

/// The interpreter for a page that runs on `engine`: loaded from what the build compiled.
#[cfg(not(feature = "compiler"))]
pub(crate) fn component(engine: &Engine) -> Result<Component, Fault> {
    // SAFETY: the bytes are what `Engine::precompile_component` made in `build.rs`, from the
    // interpreter of this crate, for this target, by the same wasmtime and the configuration of
    // `engine_config` that `sandbox::engine` takes; nothing has changed them since the build.
    unsafe { Component::deserialize(engine, PRECOMPILED) }
        .map_err(|why| Fault::Load(format!("the interpreter: {why:#}")))
}
