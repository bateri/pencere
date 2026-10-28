//! The files of a page's bundle, read from a directory: what a page under the headless host
//! requires and shows, as a program carries them.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pencere_sandbox::Assets;

/// The files of a bundle, from a directory of this machine. A path of the bundle is a path under
/// the directory; one that climbs out of it (`..`), names nothing or is absolute is no file. The
/// media type is the extension's, as a program gives it: a script is `text/x-lua`, a picture its
/// image type.
///
/// [`Files::app`] is the bundle a pencere app's view is: `src/view/` at the top, `src/assets/`
/// under `assets/`.
#[derive(Clone, Debug)]
pub struct Files {
    /// Where each part of the bundle is: a prefix of its paths, and the directory it is in.
    roots: Vec<(String, PathBuf)>,
}

impl Files {
    /// The files under `dir`, at the top of the bundle.
    pub fn dir(dir: impl Into<PathBuf>) -> Self {
        Self {
            roots: vec![(String::new(), dir.into())],
        }
    }

    /// The bundle of the view of the app in `dir`, as the program carries it: `src/view/` at the
    /// top, `src/assets/` under `assets/`.
    pub fn app(dir: impl AsRef<Path>) -> Self {
        let dir = dir.as_ref();
        Self {
            roots: vec![
                ("assets/".to_owned(), dir.join("src/assets")),
                (String::new(), dir.join("src/view")),
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
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return None;
        }
        let (rest, dir) = self
            .roots
            .iter()
            .find_map(|(prefix, dir)| path.strip_prefix(prefix.as_str()).map(|rest| (rest, dir)))?;
        let file = dir.join(rest);
        file.is_file().then_some(file)
    }
}

impl Assets for Files {
    fn read(&self, path: &str) -> Option<Arc<[u8]>> {
        std::fs::read(self.file(path)?).ok().map(Into::into)
    }

    fn media_type(&self, path: &str) -> Option<String> {
        self.file(path)?;
        let extension = path
            .rsplit('/')
            .next()
            .and_then(|name| name.rsplit_once('.'))
            .map(|(_, extension)| extension.to_ascii_lowercase())
            .unwrap_or_default();
        let media = match extension.as_str() {
            "lua" | "luau" => "text/x-lua",
            "svg" => "image/svg+xml",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            _ => "application/octet-stream",
        };
        Some(media.to_owned())
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
        for outside in [
            "../src/view/page.lua",
            "/etc/hosts",
            "",
            "a//b",
            "./page.lua",
        ] {
            assert!(files.read(outside).is_none(), "{outside}");
        }
        assert!(files.read("assets/missing.svg").is_none());
    }
}
