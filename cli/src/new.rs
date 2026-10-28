//! `pencere new`: a project to start from, in Lua, TypeScript or Rust.

use std::fs;
use std::path::Path;
use std::process::ExitCode;

/// A file of a template: where it goes in the project, and what is in it (`{{name}}` is the
/// project's name).
type Template = &'static [(&'static str, &'static str)];

/// Every project is laid out the same way: the program in `src/program/`, the page in `src/view/`
/// and the pictures in `src/assets/`; what a tool makes goes to `dist/`, never under `src/`.
const LUA: Template = &[
    (
        "src/program/app.lua",
        include_str!("../templates/lua/src/program/app.lua"),
    ),
    (
        "src/view/page.lua",
        include_str!("../templates/lua/src/view/page.lua"),
    ),
    (
        "src/assets/icon.svg",
        include_str!("../templates/lua/src/assets/icon.svg"),
    ),
    (
        "test/page_test.lua",
        include_str!("../templates/lua/test/page_test.lua"),
    ),
    ("README.md", include_str!("../templates/lua/README.md")),
    (".gitignore", include_str!("../templates/lua/gitignore")),
];

/// The Lua project with a web page: the page is HTML and JavaScript, and its bridge to the program
/// is the SDK's, as the build took it.
const LUA_WEB: Template = &[
    (
        "src/program/app.lua",
        include_str!("../templates/lua-web/src/program/app.lua"),
    ),
    (
        "src/view/index.html",
        include_str!("../templates/lua-web/src/view/index.html"),
    ),
    (
        "src/view/app.js",
        include_str!("../templates/lua-web/src/view/app.js"),
    ),
    (
        "src/view/pencere-page.js",
        include_str!(concat!(env!("OUT_DIR"), "/pencere-page.js")),
    ),
    ("README.md", include_str!("../templates/lua-web/README.md")),
    (".gitignore", include_str!("../templates/lua/gitignore")),
];

const TS: Template = &[
    ("deno.json", include_str!("../templates/ts/deno.json")),
    (
        "src/program/main.ts",
        include_str!("../templates/ts/src/program/main.ts"),
    ),
    (
        "src/view/index.html",
        include_str!("../templates/ts/src/view/index.html"),
    ),
    (
        "src/view/app.js",
        include_str!("../templates/ts/src/view/app.js"),
    ),
    ("README.md", include_str!("../templates/ts/README.md")),
    (".gitignore", include_str!("../templates/ts/gitignore")),
];

/// The TypeScript project whose view is a page of the kit, the Lua project's: the pictures are in
/// the view's directory, because the program bundles that one directory.
const TS_LUA: Template = &[
    ("deno.json", include_str!("../templates/ts/deno.json")),
    (
        "src/program/main.ts",
        include_str!("../templates/ts-lua/src/program/main.ts"),
    ),
    (
        "src/view/page.lua",
        include_str!("../templates/lua/src/view/page.lua"),
    ),
    (
        "src/view/assets/icon.svg",
        include_str!("../templates/lua/src/assets/icon.svg"),
    ),
    (
        "test/page_test.lua",
        include_str!("../templates/ts-lua/test/page_test.lua"),
    ),
    ("README.md", include_str!("../templates/ts-lua/README.md")),
    (".gitignore", include_str!("../templates/ts/gitignore")),
];

const RUST: Template = &[
    // Kept in the repository as `Cargo.toml.tmpl`: a file called `Cargo.toml` with a placeholder
    // for a name in it is a manifest that cargo reads, and refuses, in every project that takes
    // this repository by git.
    (
        "Cargo.toml",
        include_str!("../templates/rust/Cargo.toml.tmpl"),
    ),
    ("build.rs", include_str!("../templates/rust/build.rs")),
    (
        "src/program/main.rs",
        include_str!("../templates/rust/src/program/main.rs"),
    ),
    // The view is the Lua project's, the one page the templates share.
    (
        "src/view/page.lua",
        include_str!("../templates/lua/src/view/page.lua"),
    ),
    (
        "src/assets/icon.svg",
        include_str!("../templates/lua/src/assets/icon.svg"),
    ),
    (
        "test/page_test.lua",
        include_str!("../templates/lua/test/page_test.lua"),
    ),
    ("README.md", include_str!("../templates/rust/README.md")),
    (".gitignore", include_str!("../templates/rust/gitignore")),
];

/// Whether `name` can be a directory and a package: letters, digits, `-` and `_`, not starting
/// with a digit or a dash.
fn is_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && name.len() <= 64
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Writes the project `name` of language `lang` (and, for Lua and TypeScript, what the view is:
/// `page` is `lua`, a page of the kit, or `web`, a web page; a Lua project's is a page of the kit and
/// a TypeScript project's a web page unless told) in the directory `into`.
pub fn make(into: &Path, name: &str, lang: &str, page: Option<&str>) -> Result<usize, String> {
    if let Some(page) = page
        && !matches!(page, "lua" | "web")
    {
        return Err(format!("--page is lua or web, not {page:?}"));
    }
    if page == Some("web") && lang == "rust" {
        return Err("--page is for Lua and TypeScript projects: a Rust program chooses its view in its own code".into());
    }
    let template = match (lang, page) {
        ("lua", Some("web")) => LUA_WEB,
        ("lua", _) => LUA,
        ("ts", Some("lua")) => TS_LUA,
        ("ts", _) => TS,
        ("rust", _) => RUST,
        (other, _) => return Err(format!("--lang is lua, ts or rust, not {other:?}")),
    };
    if !is_name(name) {
        return Err(format!(
            "{name:?} is not a name for a project: letters, digits, - and _, starting with a letter"
        ));
    }
    if into.exists() {
        return Err(format!("{} is already there", into.display()));
    }
    for (path, text) in template {
        let target = into.join(path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|why| format!("{}: {why}", parent.display()))?;
        }
        fs::write(&target, text.replace("{{name}}", name))
            .map_err(|why| format!("{}: {why}", target.display()))?;
    }
    Ok(template.len())
}

pub fn command(args: &[String]) -> ExitCode {
    let mut name = None;
    let mut lang = "lua".to_owned();
    let mut page: Option<String> = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--lang" => match args.next() {
                Some(value) => lang.clone_from(value),
                None => return crate::usage("--lang needs lua, ts or rust"),
            },
            "--page" => match args.next() {
                Some(value) => page = Some(value.clone()),
                None => return crate::usage("--page needs lua or web"),
            },
            other if other.starts_with('-') => {
                return crate::usage(format!("{other:?} is not an option of `new`"));
            }
            other if name.is_none() => name = Some(other.to_owned()),
            other => return crate::usage(format!("{other:?}: one name is enough")),
        }
    }
    let Some(name) = name else {
        return crate::usage("`new` needs the name of the project");
    };
    match make(Path::new(&name), &name, &lang, page.as_deref()) {
        Ok(count) => {
            println!(
                "pencere: made {name}/ ({count} files, {lang}{})",
                match (lang.as_str(), page.as_deref()) {
                    ("lua", Some("web")) => " with a web page",
                    ("ts", Some("lua")) => " with a page of the kit",
                    _ => "",
                }
            );
            match lang.as_str() {
                "lua" => println!(
                    "  cd {name} && pencere dev       run it in this pane\n  pencere build                  make one executable of it: dist/{name}{}",
                    if page.as_deref() == Some("web") {
                        ""
                    } else {
                        "\n  pencere test                   run its tests (test/)"
                    }
                ),
                "ts" => println!(
                    "  cd {name} && deno task dev     run it in this pane\n  deno task build                make one executable of it: dist/{name}{}",
                    if page.as_deref() == Some("lua") {
                        "\n  pencere test                   run the tests of its page (test/)"
                    } else {
                        ""
                    }
                ),
                _ => println!(
                    "  cd {name} && cargo run         run it in this pane\n  pencere test                   run the tests of its page (test/)"
                ),
            }
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("pencere: {why}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_template_is_a_manifest_that_cargo_or_npm_would_read_in_the_repository() {
        // A project that takes this repository by git has cargo walk every `Cargo.toml` of it, and
        // a template with `{{name}}` for a package name is an error on every build there.
        fn walk(dir: &std::path::Path, found: &mut Vec<std::path::PathBuf>) {
            for entry in fs::read_dir(dir).expect("a directory") {
                let path = entry.expect("an entry").path();
                if path.is_dir() {
                    walk(&path, found);
                } else {
                    found.push(path);
                }
            }
        }
        let mut files = Vec::new();
        walk(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("templates"),
            &mut files,
        );
        assert!(files.len() > 10, "{files:?}");
        for file in files {
            let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("");
            assert!(
                !matches!(
                    name,
                    "Cargo.toml" | "Cargo.lock" | "package.json" | "package-lock.json"
                ),
                "{} would be read as a manifest",
                file.display()
            );
        }
    }

    #[test]
    fn names_are_what_a_directory_and_a_package_can_be() {
        for good in ["a", "containers", "my-app_2", "A"] {
            assert!(is_name(good), "{good}");
        }
        for bad in [
            "",
            "2fast",
            "-x",
            "a b",
            "a/b",
            "../x",
            "ünï",
            &"a".repeat(65),
        ] {
            assert!(!is_name(bad), "{bad}");
        }
    }

    #[test]
    fn a_project_is_made_with_its_name_in_it_and_not_over_another() {
        let dir = std::env::temp_dir().join(format!("pencere-new-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        for lang in ["lua", "ts", "rust"] {
            let into = dir.join(lang);
            let count = make(&into, "demo-app", lang, None).expect("a project");
            assert!(count >= 4, "{lang}");
            let text = fs::read_to_string(into.join(match lang {
                "lua" => "src/program/app.lua",
                "ts" => "src/program/main.ts",
                _ => "src/program/main.rs",
            }))
            .expect("the program");
            assert!(text.contains("demo-app"), "{lang}");
            assert!(!text.contains("{{name}}"), "{lang}");
            assert!(make(&into, "demo-app", lang, None).is_err(), "{lang} twice");
        }
        assert!(make(&dir.join("x"), "demo", "go", None).is_err());
        assert!(make(&dir.join("y"), "demo", "rust", Some("web")).is_err());
        assert!(make(&dir.join("z"), "demo", "lua", Some("native")).is_err());
        let web = dir.join("web");
        make(&web, "demo-web", "lua", Some("web")).expect("a project with a web page");
        for file in [
            "src/program/app.lua",
            "src/view/index.html",
            "src/view/app.js",
            "src/view/pencere-page.js",
        ] {
            assert!(web.join(file).is_file(), "{file}");
        }
        // The bridge is the SDK's, not a stand-in: the gate builds the SDK before the tests.
        let bridge = fs::read_to_string(web.join("src/view/pencere-page.js")).expect("the bridge");
        assert!(
            !bridge.contains("was not generated"),
            "build the TypeScript half first (npx tsc -b)"
        );
        assert!(
            bridge.contains("export function subscribe") || bridge.contains("export { "),
            "{}",
            &bridge[..200.min(bridge.len())]
        );
        fs::remove_dir_all(&dir).expect("cleaned");
    }

    #[test]
    fn a_typescript_project_has_a_web_page_unless_it_is_told_a_page_of_the_kit_and_it_then_has_a_test()
     {
        let dir = std::env::temp_dir().join(format!("pencere-new-ts-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let web = dir.join("web");
        make(&web, "demo-ts", "ts", None).expect("a project");
        assert!(web.join("src/view/index.html").is_file());
        assert!(
            !web.join("test").exists(),
            "a web page is not opened by a host with no screen"
        );
        let kit = dir.join("kit");
        make(&kit, "demo-ts", "ts", Some("lua")).expect("a project with a page of the kit");
        for file in [
            "src/program/main.ts",
            "src/view/page.lua",
            "src/view/assets/icon.svg",
            "test/page_test.lua",
            "deno.json",
        ] {
            assert!(kit.join(file).is_file(), "{file}");
        }
        assert!(!kit.join("src/view/index.html").exists());
        let main = fs::read_to_string(kit.join("src/program/main.ts")).expect("the program");
        assert!(
            main.contains("kind: \"lua\"") && main.contains("demo-ts"),
            "{main}"
        );
        assert!(!main.contains("{{name}}"));
        // The Lua and the Rust projects come with the test of their page.
        for lang in ["lua", "rust"] {
            let into = dir.join(lang);
            make(&into, "demo-app", lang, None).expect("a project");
            let test = fs::read_to_string(into.join("test/page_test.lua")).expect("a test");
            assert!(
                test.contains("pencere.test") && !test.contains("{{name}}"),
                "{lang}"
            );
        }
        fs::remove_dir_all(&dir).expect("cleaned");
    }
}
