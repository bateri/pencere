//! The files of a page's bundle, read from a directory: what a page under the headless host
//! requires and shows, as a program carries them.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pencere_sandbox::Assets;
use pencere_wire::bundle::{LUA_MEDIA_TYPE, media_type_of};

/// The files of a bundle, from a directory of this machine. A path of the bundle is a path under
/// the directory; one that climbs out of it (`..`), names nothing, is absolute or is hidden (a part
/// of it starts with a dot) is no file, as a program leaves a hidden file out of its bundle. The
/// media type is the one a program gives the extension (`pencere_wire::bundle::media_type_of`).
///
/// [`Files::app`] is the bundle a pencere app's page of the kit is, as the program carries it: the
/// scripts of `src/view/` at the top, and `src/assets/` under `assets/`.
#[derive(Clone, Debug)]
pub struct Files {
    roots: Vec<Root>,
}

/// A part of a bundle: a prefix of its paths, the directory it is in, and whether only the scripts
/// a page may `require` are in it.
#[derive(Clone, Debug)]
struct Root {
    prefix: String,
    dir: PathBuf,
    scripts: bool,
}

impl Files {
    /// The files under `dir`, at the top of the bundle.
    pub fn dir(dir: impl Into<PathBuf>) -> Self {
        Self {
            roots: vec![Root {
                prefix: String::new(),
                dir: dir.into(),
                scripts: false,
            }],
        }
    }

    /// The bundle of the page of the kit of the app in `dir`, as the program carries it: the
    /// scripts (`.lua`, `.luau`) of `src/view/` at the top, `src/assets/` under `assets/`. A file
    /// of `src/view/` that is not a script is not in it, as it is not in the program's.
    pub fn app(dir: impl AsRef<Path>) -> Self {
        let dir = dir.as_ref();
        Self {
            roots: vec![
                Root {
                    prefix: "assets/".to_owned(),
                    dir: dir.join("src/assets"),
                    scripts: false,
                },
                Root {
                    prefix: String::new(),
                    dir: dir.join("src/view"),
                    scripts: true,
                },
            ],
        }
    }

    /// The file a path of the bundle is, if it is one.
    fn file(&self, path: &str) -> Option<PathBuf> {
        if path.is_empty()
            || path.starts_with('/')
            || path.contains('\\')
            || path
                .split('/')
                .any(|part| part.is_empty() || part.starts_with('.'))
        {
            return None;
        }
        let (rest, root) = self.roots.iter().find_map(|root| {
            path.strip_prefix(root.prefix.as_str())
                .map(|rest| (rest, root))
        })?;
        if root.scripts && media_type_of(path) != LUA_MEDIA_TYPE {
            return None;
        }
        let file = root.dir.join(rest);
        file.is_file().then_some(file)
    }
}

impl Assets for Files {
    fn read(&self, path: &str) -> Option<Arc<[u8]>> {
        std::fs::read(self.file(path)?).ok().map(Into::into)
    }

    fn media_type(&self, path: &str) -> Option<String> {
        self.file(path)?;
        Some(media_type_of(path).to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_apps_bundle_is_its_view_at_the_top_and_its_assets_under_assets() {
        let app = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/containers-lua");
        let files = Files::app(&app);
        assert!(files.read("page.lua").is_some());
        assert_eq!(files.media_type("page.lua").as_deref(), Some("text/x-lua"));
        assert_eq!(
            files.media_type("assets/box.svg").as_deref(),
            Some("image/svg+xml")
        );
        assert_eq!(
            files.media_type("assets/icon.png").as_deref(),
            Some("image/png")
        );
        assert!(
            files.read("README.md").is_none(),
            "not a script of the view"
        );
        for outside in [
            "../src/view/page.lua",
            "/etc/hosts",
            "",
            "a//b",
            "./page.lua",
            ".hidden.lua",
        ] {
            assert!(files.read(outside).is_none(), "{outside}");
        }
        assert!(files.read("assets/missing.svg").is_none());
    }
}
