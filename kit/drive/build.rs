//! Packs the faces a picture carries (`fonts/`) with zlib, into the build's own folder: the crate
//! keeps the upstream files as they are, the program carries them packed (about half their size) and
//! unpacks one where a machine has no face of its own. Nothing is done without the feature `shot`.

use std::error::Error;
use std::path::PathBuf;

/// The files of `fonts/` that are carried.
const CARRIED: &[&str] = &[
    "JetBrainsMono[wght].ttf",
    "JetBrainsMono-Italic[wght].ttf",
    "InterVariable.ttf",
    "InterVariable-Italic.ttf",
];

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=fonts");
    if std::env::var_os("CARGO_FEATURE_SHOT").is_none() {
        return Ok(());
    }
    let from = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").ok_or("no manifest folder")?)
        .join("fonts");
    let into = PathBuf::from(std::env::var_os("OUT_DIR").ok_or("no build folder")?);
    for name in CARRIED {
        let packed =
            miniz_oxide::deflate::compress_to_vec_zlib(&std::fs::read(from.join(name))?, 10);
        std::fs::write(into.join(format!("{name}.z")), packed)?;
    }
    Ok(())
}
