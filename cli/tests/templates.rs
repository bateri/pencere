//! The projects `pencere new` makes are ones that build: the Lua project is run by the end-to-end
//! tests of the stand-in terminal; the Rust project is compiled here, against the libraries of
//! this repository instead of the registry's (it asks for `pencere = "0.2"`).

// A test fails by panicking, and the helpers here are the test's.
#![allow(clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;
use std::process::Command;

#[test]
#[ignore = "heavy: compiles a project made by `pencere new --lang rust`"]
fn the_rust_project_pencere_new_makes_compiles() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    // Outside the repository: cargo would take a project under it for a member of its workspace.
    let directory =
        std::env::temp_dir().join(format!("pencere-template-rust-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("a directory");
    let made = Command::new(env!("CARGO_BIN_EXE_pencere"))
        .args(["new", "hello-rust", "--lang", "rust"])
        .current_dir(&directory)
        .output()
        .expect("pencere runs");
    assert!(
        made.status.success(),
        "{}",
        String::from_utf8_lossy(&made.stderr)
    );
    let project = directory.join("hello-rust");
    // The registry does not have these versions yet: the project is built against this checkout.
    let manifest = project.join("Cargo.toml");
    let mut text = fs::read_to_string(&manifest).expect("a manifest");
    text.push_str(&format!(
        "\n[patch.crates-io]\npencere = {{ path = {:?} }}\npencere-wire = {{ path = {:?} }}\n",
        root.join("program"),
        root.join("wire")
    ));
    fs::write(&manifest, text).expect("a manifest is written");
    let checked = Command::new(env!("CARGO"))
        .args(["check", "--offline", "--quiet", "--target-dir"])
        .arg(directory.join("target"))
        .current_dir(&project)
        .output()
        .expect("cargo runs");
    let said = String::from_utf8_lossy(&checked.stderr).into_owned();
    let _ = fs::remove_dir_all(&directory);
    assert!(
        checked.status.success(),
        "the Rust project does not compile:\n{said}"
    );
}
