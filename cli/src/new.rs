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

const RUST: Template = &[
    // Kept in the repository as `Cargo.toml.tmpl`: a file called `Cargo.toml` with a placeholder
    // for a name in it is a manifest that cargo reads, and refuses, in every project that takes
    // this repository by git.
    (
        "Cargo.toml",
        include_str!("../templates/rust/Cargo.toml.tmpl"),
    ),
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

/// Writes the project `name` of language `lang` (and, for Lua, whose `page` the view is: `lua` or
/// `web`) in the directory `into`.
pub fn make(into: &Path, name: &str, lang: &str, page: &str) -> Result<usize, String> {
    if !matches!(page, "lua" | "web") {
        return Err(format!("--page is lua or web, not {page:?}"));
    }
    if page == "web" && lang != "lua" {
        return Err("--page is for Lua projects: a TypeScript or Rust program chooses its view in its own code".into());
    }
    let template = match (lang, page) {
        ("lua", "web") => LUA_WEB,
        ("lua", _) => LUA,
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
    let mut page = "lua".to_owned();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--lang" => match args.next() {
                Some(value) => lang.clone_from(value),
                None => return crate::usage("--lang needs lua, ts or rust"),
            },
            "--page" => match args.next() {
                Some(value) => page.clone_from(value),
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
    match make(Path::new(&name), &name, &lang, &page) {
        Ok(count) => {
            println!(
                "pencere: made {name}/ ({count} files, {lang}{})",
                if lang == "lua" && page == "web" {
                    " with a web page"
                } else {
                    ""
                }
            );
            match lang.as_str() {
                "lua" => println!(
                    "  cd {name} && pencere dev       run it in this pane\n  pencere build                  make one executable of it: dist/{name}"
                ),
                "ts" => println!(
                    "  cd {name} && deno task dev     run it in this pane\n  deno task build                make one executable of it: dist/{name}"
                ),
                _ => println!("  cd {name} && cargo run         run it in this pane"),
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
            let count = make(&into, "demo-app", lang, "lua").expect("a project");
            assert!(count >= 4, "{lang}");
            let text = fs::read_to_string(into.join(match lang {
                "lua" => "src/program/app.lua",
                "ts" => "src/program/main.ts",
                _ => "src/program/main.rs",
            }))
            .expect("the program");
            assert!(text.contains("demo-app"), "{lang}");
            assert!(!text.contains("{{name}}"), "{lang}");
            assert!(
                make(&into, "demo-app", lang, "lua").is_err(),
                "{lang} twice"
            );
        }
        assert!(make(&dir.join("x"), "demo", "go", "lua").is_err());
        assert!(make(&dir.join("y"), "demo", "rust", "web").is_err());
        assert!(make(&dir.join("z"), "demo", "lua", "native").is_err());
        let web = dir.join("web");
        make(&web, "demo-web", "lua", "web").expect("a project with a web page");
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
}
