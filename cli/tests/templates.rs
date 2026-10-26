//! The projects `pencere new` makes are ones that build: the Lua projects are built here into
//! `dist/` and run (with no terminal, so that they say their text), and the Lua example is run by
//! the end-to-end tests of the stand-in terminal; the TypeScript project is checked for the paths
//! its tasks name (Deno asks the registry for the SDK, which these tests do not); the Rust project
//! is compiled here, against the libraries of this repository instead of the registry's (it asks
//! for `pencere = "0.3"`).

// A test fails by panicking, and the helpers here are the test's.
#![allow(clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A directory of the test's own, empty.
fn fresh(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pencere-template-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a directory");
    dir
}

/// `pencere` with these arguments in `directory`: whether it succeeded, and what it said.
fn pencere(directory: &Path, arguments: &[&str]) -> (bool, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_pencere"))
        .args(arguments)
        .current_dir(directory)
        .stdin(Stdio::null())
        .output()
        .expect("pencere runs");
    (
        output.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    )
}

#[test]
fn the_lua_projects_are_made_with_the_layout_and_build_into_dist_and_run() {
    for (page, entry) in [("lua", "src/view/page.lua"), ("web", "src/view/index.html")] {
        let directory = fresh(page);
        let (made, said) = pencere(&directory, &["new", "hello", "--page", page]);
        assert!(made, "{said}");
        let project = directory.join("hello");
        for file in ["src/program/app.lua", entry, "README.md", ".gitignore"] {
            assert!(project.join(file).is_file(), "{page}: {file}");
        }
        // Nothing but the layout is at the top: the sources are in `src/`.
        assert!(!project.join("app.lua").exists(), "{page}");
        let (built, said) = pencere(&project, &["build"]);
        assert!(built, "{page}: {said}");
        let app = project.join("dist/hello");
        assert!(app.is_file(), "{page}: {said}");
        // Where there is no terminal that hosts views the app says its text, from its own name.
        let ran = Command::new(&app)
            .stdin(Stdio::null())
            .output()
            .expect("the app runs");
        let text = String::from_utf8_lossy(&ran.stdout).into_owned();
        assert!(ran.status.success(), "{page}: {text}");
        assert!(text.contains("Hello, world!"), "{page}: {text}");
        let _ = fs::remove_dir_all(&directory);
    }
}

#[test]
fn the_page_can_come_from_a_build_directory_and_the_app_can_be_named() {
    let directory = fresh("web-and-name");
    let program = directory.join("src/program");
    fs::create_dir_all(&program).expect("a directory");
    fs::write(
        program.join("app.lua"),
        r#"local pencere = require("pencere")
print(pencere.name, table.concat(pencere.app.list("view"), ","), pencere.app.exists("view/main.tsx"))
"#,
    )
    .expect("a program");
    // The sources of the page, which are not what is packed, and what a tool built from them.
    fs::create_dir_all(directory.join("src/view")).expect("a directory");
    fs::write(
        directory.join("src/view/index.html"),
        "<script src=main.tsx>",
    )
    .expect("a source");
    fs::write(directory.join("src/view/main.tsx"), "source").expect("a source");
    fs::create_dir_all(directory.join("dist/view/assets")).expect("a directory");
    fs::write(
        directory.join("dist/view/index.html"),
        "<script src=assets/a.js>",
    )
    .expect("a page");
    fs::write(directory.join("dist/view/assets/a.js"), "built").expect("a page");
    let (built, said) = pencere(
        &directory,
        &["build", "--web", "dist/view", "--name", "shown"],
    );
    assert!(built, "{said}");
    // The name is the executable's, in `dist/`, and not the directory's.
    let app = directory.join("dist/shown");
    assert!(app.is_file(), "{said}");
    let ran = Command::new(&app).output().expect("the app runs");
    let text = String::from_utf8_lossy(&ran.stdout).into_owned();
    assert!(ran.status.success(), "{text}");
    assert_eq!(
        text, "shown\tview/assets/a.js,view/index.html\tfalse\n",
        "the page of the build directory is packed instead of src/view/"
    );
    // `-o` says where, and creates the directory.
    let (built, said) = pencere(
        &directory,
        &["build", "--web", "dist/view", "-o", "out/elsewhere"],
    );
    assert!(built, "{said}");
    assert!(directory.join("out/elsewhere").is_file(), "{said}");
    // The simple form takes no build directory, and says so.
    let simple = directory.join("simple");
    fs::create_dir_all(&simple).expect("a directory");
    fs::write(simple.join("app.lua"), "").expect("a program");
    let (built, said) = pencere(&simple, &["build", "--web", "../dist/view"]);
    assert!(!built && said.contains("--web"), "{said}");
    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn the_typescript_project_names_the_paths_the_layout_has() {
    let directory = fresh("ts");
    let (made, said) = pencere(&directory, &["new", "hello-ts", "--lang", "ts"]);
    assert!(made, "{said}");
    let project = directory.join("hello-ts");
    for file in [
        "src/program/main.ts",
        "src/view/index.html",
        "src/view/app.js",
    ] {
        assert!(project.join(file).is_file(), "{file}");
    }
    let tasks = fs::read_to_string(project.join("deno.json")).expect("deno.json");
    for named in ["src/program/main.ts", "--include src/view", "dist/hello-ts"] {
        assert!(tasks.contains(named), "{named}: {tasks}");
    }
    // The program reads its view from the directory beside its own.
    let main = fs::read_to_string(project.join("src/program/main.ts")).expect("main.ts");
    assert!(main.contains("../view/"), "{main}");
    let _ = fs::remove_dir_all(&directory);
}

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
