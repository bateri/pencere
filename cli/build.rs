//! Tells the program which target it is built for (`pencere build` copies the runner of its own
//! target and fetches the runner of any other), and takes the page bridge of the SDK for the
//! project `pencere new --page web` makes.
//!
//! The bridge is `sdk/typescript/src/page`, compiled by the TypeScript half of the repository
//! (`npx tsc -b`, which the gate runs first): one ES module with no dependencies. It is not
//! copied by hand; a build without it makes a stand-in that says what is missing, and a test
//! fails until the real one is there.

use std::path::PathBuf;
use std::{env, fs};

fn main() {
    let target = env::var("TARGET").unwrap_or_default();
    println!("cargo:rustc-env=PENCERE_TARGET={target}");
    println!("cargo:rerun-if-changed=build.rs");

    let (Some(manifest), Some(out)) = (env::var_os("CARGO_MANIFEST_DIR"), env::var_os("OUT_DIR"))
    else {
        return;
    };
    let bridge = PathBuf::from(manifest).join("../sdk/typescript/dist/page/index.js");
    println!("cargo:rerun-if-changed={}", bridge.display());
    let text = fs::read_to_string(&bridge).unwrap_or_else(|_| {
        println!(
            "cargo:warning=sdk/typescript/dist/page/index.js is missing: build the TypeScript half first (npx tsc -b)"
        );
        "throw new Error(\"pencere-page.js was not generated: the SDK was not built when this pencere was (npx tsc -b)\");\n".to_owned()
    });
    // The map the compiler pointed to is not in the project that is made.
    let text: String = text
        .lines()
        .filter(|line| !line.starts_with("//# sourceMappingURL="))
        .map(|line| format!("{line}\n"))
        .collect();
    // A write that fails leaves nothing to include, which the build then says.
    let _ = fs::write(PathBuf::from(out).join("pencere-page.js"), text);
}
