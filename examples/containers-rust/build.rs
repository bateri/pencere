//! Carries the page's WebAssembly module into the program, when it has been built.
//!
//! The module is made by `make guest` (it needs a toolchain for WebAssembly that `make check`
//! does not), which builds it before this program. Where it is not there the program is built
//! without it and says so when it is asked for the view it would show.

use std::path::PathBuf;
use std::{env, fs};

fn main() {
    let (Some(manifest), Some(out)) = (env::var_os("CARGO_MANIFEST_DIR"), env::var_os("OUT_DIR"))
    else {
        return;
    };
    let module = PathBuf::from(manifest).join("../../kit/guest/dist/containers.wasm");
    println!("cargo:rerun-if-changed={}", module.display());
    println!("cargo:rerun-if-changed=build.rs");
    let bytes = fs::read(&module).unwrap_or_default();
    // A write that fails leaves nothing to include, which the build then says.
    let _ = fs::write(PathBuf::from(out).join("containers.wasm"), bytes);
}
