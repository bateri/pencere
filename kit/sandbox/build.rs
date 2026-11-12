//! Lists the kit's library (`library/`) for the crate: every module of Luau in it, under the path
//! the interpreter asks for (`@pencere/` and the file's path in the directory), with its source and
//! the bytes they come to together (`library.rs`, included by `src/library.rs`). The directory is
//! the one source: a module added to it is in the library, and nothing else names it.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::{env, fs};

/// The files of `dir` and the directories under it whose name ends in `.luau`, as paths from `root`
/// with `/` between their parts.
fn modules(root: &Path, dir: &Path, found: &mut Vec<String>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|why| format!("{}: {why}", dir.display()))?;
    for entry in entries {
        let path = entry
            .map_err(|why| format!("{}: {why}", dir.display()))?
            .path();
        if path.is_dir() {
            modules(root, &path, found)?;
        } else if path
            .extension()
            .is_some_and(|extension| extension == "luau")
        {
            let relative = path
                .strip_prefix(root)
                .map_err(|why| format!("{}: {why}", path.display()))?;
            let parts: Option<Vec<&str>> = relative.iter().map(|part| part.to_str()).collect();
            let parts =
                parts.ok_or_else(|| format!("{} is not a name in UTF-8", path.display()))?;
            found.push(parts.join("/"));
        }
    }
    Ok(())
}

/// The list as Rust, or why there is none.
fn listing(root: &Path) -> Result<String, String> {
    let mut found = Vec::new();
    modules(root, root, &mut found)?;
    found.sort();
    let mut size = 0usize;
    let mut out = String::from("pub const MODULES: &[(&str, &str)] = &[\n");
    for module in &found {
        let file = root.join(module);
        size += fs::metadata(&file)
            .map_err(|why| format!("{}: {why}", file.display()))?
            .len() as usize;
        // Debug's quotes and escapes make a literal of Rust of any name a file can have.
        let path = format!("@pencere/{module}");
        let file = format!("/library/{module}");
        let _ = writeln!(
            out,
            "    ({path:?}, include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), {file:?}))),"
        );
    }
    out.push_str("];\n");
    let _ = writeln!(out, "pub const SIZE: usize = {size};");
    Ok(out)
}

// How every engine is configured: the file the runtime takes it from.
include!("src/engine_config.rs");

/// The interpreter compiled to machine code for the target (`luau.cwasm` in the output directory),
/// for a runtime built with no code generator (no `compiler` feature) to load. The target is the
/// one the crate is built for, set explicitly so that the code is for the architecture and not for
/// the CPU of the machine that builds it.
fn precompile() -> Result<(), String> {
    let (Some(manifest), Some(out), Some(target)) = (
        env::var_os("CARGO_MANIFEST_DIR"),
        env::var_os("OUT_DIR"),
        env::var("TARGET").ok(),
    ) else {
        return Err("no manifest, output directory or target".into());
    };
    let wasm = PathBuf::from(manifest).join("luau").join("luau.wasm");
    let bytes = fs::read(&wasm).map_err(|why| format!("{}: {why}", wasm.display()))?;
    let mut config = wasmtime::Config::new();
    configure(&mut config);
    config
        .target(&target)
        .map_err(|why| format!("target {target}: {why:#}"))?;
    let engine = wasmtime::Engine::new(&config).map_err(|why| format!("{why:#}"))?;
    let compiled = engine
        .precompile_component(&bytes)
        .map_err(|why| format!("compiling the interpreter: {why:#}"))?;
    fs::write(PathBuf::from(out).join("luau.cwasm"), compiled).map_err(|why| why.to_string())
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=library");
    println!("cargo:rerun-if-changed=src/engine_config.rs");
    println!("cargo:rerun-if-changed=luau/luau.wasm");
    if env::var_os("CARGO_FEATURE_COMPILER").is_none()
        && let Err(why) = precompile()
    {
        eprintln!("the interpreter for the target: {why}");
        std::process::exit(1);
    }
    let (Some(manifest), Some(out)) = (env::var_os("CARGO_MANIFEST_DIR"), env::var_os("OUT_DIR"))
    else {
        return;
    };
    let root = PathBuf::from(manifest).join("library");
    let text = listing(&root).unwrap_or_else(|why| {
        format!(
            "compile_error!({:?});\n",
            format!("the kit's library: {why}")
        )
    });
    // A write that fails leaves nothing to include, which the build then says.
    let _ = fs::write(PathBuf::from(out).join("library.rs"), text);
}
