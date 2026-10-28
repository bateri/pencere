//! The kit's library as the sandbox keeps it: every module of `library/` in the crate, under the
//! path the interpreter asks for, and given to a script whatever its bundle, outside what its own
//! scripts may come to. What a script's `require` makes of it is `luau_require_tests`'.

use std::collections::BTreeSet;
use std::sync::Arc;

use crate::library::{self, LIBRARY_SIZE, MODULES, PREFIX, SIZE};
use crate::sandbox::Host;
use crate::world::pencere::view::script::Host as ScriptHost;
use crate::{ModuleLimits, Script, Unbounded};

#[test]
fn the_library_is_every_module_of_its_directory_and_nothing_else() {
    let directory = concat!(env!("CARGO_MANIFEST_DIR"), "/library");
    let mut on_disk = BTreeSet::new();
    let mut pending = vec![std::path::PathBuf::from(directory)];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).expect("the library's directory") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let relative = path
                .strip_prefix(directory)
                .expect("under the directory")
                .to_str()
                .expect("a path in UTF-8")
                .replace('\\', "/");
            if relative.ends_with(".luau") {
                on_disk.insert(relative);
            }
        }
    }
    let listed: BTreeSet<String> = MODULES
        .iter()
        .map(|(path, _)| {
            path.strip_prefix(PREFIX)
                .expect("a module's path starts with @pencere/")
                .to_owned()
        })
        .collect();
    assert_eq!(listed, on_disk, "library::MODULES lists library/");
    for (path, source) in MODULES {
        let file = format!("{directory}/{}", &path[PREFIX.len()..]);
        assert_eq!(
            std::fs::read_to_string(&file).expect("the module's file"),
            *source,
            "{path}"
        );
    }
}

#[test]
fn the_library_is_within_its_size_and_found_by_its_whole_path_alone() {
    let total: usize = MODULES.iter().map(|(_, source)| source.len()).sum();
    assert_eq!(total, SIZE);
    assert!(total <= LIBRARY_SIZE);
    assert!(library::source("@pencere/syntax.luau").is_some());
    for missing in [
        "@pencere/syntax",
        "@pencere/syntax.lua",
        "syntax.luau",
        "@pencere/",
        "@pencere/../syntax.luau",
        "@PENCERE/syntax.luau",
        "",
    ] {
        assert!(library::source(missing).is_none(), "{missing:?}");
    }
}

/// A host of a script page with no bundle, whose scripts may come to `most` bytes, `given` of them
/// given already.
fn script_host(given: usize, most: usize) -> Host {
    let mut host = Host::new(&ModuleLimits::default(), Arc::new(Unbounded));
    host.script = Some(Script {
        path: "page.lua".to_owned(),
        source: b"return {}".to_vec(),
        bundle: None,
    });
    host.given = given;
    host.most = most;
    host
}

#[test]
fn a_script_is_given_the_library_with_no_bundle_and_not_counted_with_its_scripts() {
    let mut host = script_host(9, 10);
    assert!(ScriptHost::has_module(
        &mut host,
        "@pencere/syntax.luau".to_owned()
    ));
    let given = ScriptHost::module(&mut host, "@pencere/syntax.luau".to_owned())
        .expect("the library's module");
    assert_eq!(
        given,
        library::source("@pencere/syntax.luau").unwrap().as_bytes()
    );
    // Far more than the page's room, and none of it counted.
    assert!(given.len() > 10);
    assert_eq!(host.given, 9);
    for missing in ["@pencere/nothing.luau", "@pencere/syntax", "@pencere/"] {
        assert!(!ScriptHost::has_module(&mut host, missing.to_owned()));
        assert!(ScriptHost::module(&mut host, missing.to_owned()).is_err());
    }
}

#[test]
fn a_module_that_runs_no_script_is_given_no_library() {
    let mut host = Host::new(&ModuleLimits::default(), Arc::new(Unbounded));
    assert!(!ScriptHost::has_module(
        &mut host,
        "@pencere/syntax.luau".to_owned()
    ));
    assert!(ScriptHost::module(&mut host, "@pencere/syntax.luau".to_owned()).is_err());
}
