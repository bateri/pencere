//! Where a machine keeps its faces, and which of them are a picture's families: the files of its
//! font directories by name (no font library of the system's is asked: a server with none has the
//! directories and the files, which is all this reads), as pure functions of a list of directories,
//! so that a machine's layout is held to a directory made for the test.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::fonts::{Layout, Origin, Slots, Src};

/// What a machine's faces are called where it keeps them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Platform {
    Mac,
    Linux,
}

impl Platform {
    pub(super) fn here() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else {
            Self::Linux
        }
    }

    /// The directories the machine's faces are in, in the order they are looked in.
    pub(super) fn dirs(self, home: Option<&Path>) -> Vec<PathBuf> {
        let under = |tail: &str| home.map(|home| home.join(tail));
        let mut dirs: Vec<PathBuf> = match self {
            Self::Mac => ["/System/Library/Fonts", "/Library/Fonts"]
                .into_iter()
                .map(PathBuf::from)
                .collect(),
            Self::Linux => [
                "/usr/share/fonts",
                "/usr/local/share/fonts",
                "/run/current-system/sw/share/X11/fonts",
            ]
            .into_iter()
            .map(PathBuf::from)
            .collect(),
        };
        match self {
            Self::Mac => dirs.extend(under("Library/Fonts")),
            Self::Linux => {
                dirs.extend(under(".local/share/fonts"));
                dirs.extend(under(".fonts"));
            }
        }
        dirs
    }
}

/// The font files of some directories: by their names (lower case), and all of them in order.
pub(super) struct Files {
    by_name: HashMap<String, PathBuf>,
    all: Vec<PathBuf>,
}

/// How deep in a directory a face is looked for (a font directory is a tree, a link may be a loop).
const DEPTH: usize = 6;

/// How many files are looked at at most (a machine with every face in the world installed).
const MOST: usize = 4000;

fn is_face(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            ["ttf", "otf", "ttc", "otc"]
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
}

fn walk(dir: &Path, depth: usize, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries
        .filter_map(|entry| Some(entry.ok()?.path()))
        .collect();
    entries.sort();
    for path in entries {
        if into.len() >= MOST {
            return;
        }
        if path.is_dir() {
            if depth > 0 {
                walk(&path, depth - 1, into);
            }
        } else if is_face(&path) && path.is_file() {
            into.push(path);
        }
    }
}

impl Files {
    /// The font files under `dirs`, the first directory's first.
    pub(super) fn in_dirs(dirs: &[PathBuf]) -> Self {
        let mut all = Vec::new();
        for dir in dirs {
            walk(dir, DEPTH, &mut all);
        }
        let mut by_name = HashMap::new();
        for path in &all {
            if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                by_name
                    .entry(name.to_lowercase())
                    .or_insert_with(|| path.clone());
            }
        }
        Self { by_name, all }
    }

    fn named(&self, name: &str) -> Option<&PathBuf> {
        self.by_name.get(&name.to_lowercase())
    }

    /// The face of the file called `name`, the first of its collection.
    fn face(&self, name: &str) -> Option<Src> {
        self.named(name).map(|path| Src {
            origin: Origin::File(path.clone()),
            index: 0,
        })
    }

    /// The upright, bold, italic and bold italic files of a family, by their names; where there is
    /// no upright one, no family.
    fn family(&self, [upright, bold, italic, bold_italic]: [&str; 4]) -> Option<Slots> {
        let upright = self.face(upright)?;
        Some(Slots {
            upright: Some(upright),
            bold: self.face(bold),
            italic: self.face(italic),
            bold_italic: self.face(bold_italic),
        })
    }

    /// The first of `families` the machine has.
    fn first(&self, families: &[[&str; 4]]) -> Option<Slots> {
        families.iter().find_map(|names| self.family(*names))
    }
}

/// A collection's four faces (regular, bold, italic, bold italic) as one family.
fn collection(path: &Path) -> Slots {
    let face = |index| {
        Some(Src {
            origin: Origin::File(path.to_path_buf()),
            index,
        })
    };
    Slots {
        upright: face(0),
        bold: face(1),
        italic: face(2),
        bold_italic: face(3),
    }
}

/// Faces by file name, none of them (`""`) where a family has no such one.
const MAC_MONO: &[[&str; 4]] = &[["SFNSMono.ttf", "", "SFNSMonoItalic.ttf", ""]];
const MAC_SYSTEM: &[[&str; 4]] = &[["SFNS.ttf", "", "SFNSItalic.ttf", ""]];

const LINUX_MONO: &[[&str; 4]] = &[
    [
        "JetBrainsMono-Regular.ttf",
        "JetBrainsMono-Bold.ttf",
        "JetBrainsMono-Italic.ttf",
        "JetBrainsMono-BoldItalic.ttf",
    ],
    [
        "JetBrainsMono[wght].ttf",
        "",
        "JetBrainsMono-Italic[wght].ttf",
        "",
    ],
    [
        "DejaVuSansMono.ttf",
        "DejaVuSansMono-Bold.ttf",
        "DejaVuSansMono-Oblique.ttf",
        "DejaVuSansMono-BoldOblique.ttf",
    ],
    [
        "LiberationMono-Regular.ttf",
        "LiberationMono-Bold.ttf",
        "LiberationMono-Italic.ttf",
        "LiberationMono-BoldItalic.ttf",
    ],
    ["NotoSansMono-Regular.ttf", "NotoSansMono-Bold.ttf", "", ""],
    [
        "UbuntuMono-R.ttf",
        "UbuntuMono-B.ttf",
        "UbuntuMono-RI.ttf",
        "UbuntuMono-BI.ttf",
    ],
    ["FiraMono-Regular.ttf", "FiraMono-Bold.ttf", "", ""],
];

const LINUX_SYSTEM: &[[&str; 4]] = &[
    [
        "Inter-Regular.ttf",
        "Inter-Bold.ttf",
        "Inter-Italic.ttf",
        "Inter-BoldItalic.ttf",
    ],
    [
        "NotoSans-Regular.ttf",
        "NotoSans-Bold.ttf",
        "NotoSans-Italic.ttf",
        "NotoSans-BoldItalic.ttf",
    ],
    [
        "Roboto-Regular.ttf",
        "Roboto-Bold.ttf",
        "Roboto-Italic.ttf",
        "Roboto-BoldItalic.ttf",
    ],
    [
        "Ubuntu-R.ttf",
        "Ubuntu-B.ttf",
        "Ubuntu-RI.ttf",
        "Ubuntu-BI.ttf",
    ],
    [
        "LiberationSans-Regular.ttf",
        "LiberationSans-Bold.ttf",
        "LiberationSans-Italic.ttf",
        "LiberationSans-BoldItalic.ttf",
    ],
    [
        "DejaVuSans.ttf",
        "DejaVuSans-Bold.ttf",
        "DejaVuSans-Oblique.ttf",
        "DejaVuSans-BoldOblique.ttf",
    ],
];

/// The faces a character no face of a family has is looked for in, before the others, in order:
/// symbols, then the scripts of East Asia, then everything one face carries.
const MAC_NAMED: &[&str] = &[
    "Apple Symbols.ttf",
    "ヒラギノ角ゴシック W3.ttc",
    "Hiragino Sans GB.ttc",
    "STHeiti Light.ttc",
    "AppleSDGothicNeo.ttc",
    "Arial Unicode.ttf",
    "Menlo.ttc",
    "Monaco.ttf",
];
const MAC_EMOJI: &[&str] = &["Apple Color Emoji.ttc"];

const LINUX_NAMED: &[&str] = &[
    "NotoSansSymbols-Regular.ttf",
    "NotoSansSymbols2-Regular.ttf",
    "NotoSansMath-Regular.ttf",
    "NotoSansCJK-Regular.ttc",
    "NotoSansCJKsc-Regular.otf",
    "NotoSerifCJK-Regular.ttc",
    "wqy-microhei.ttc",
    "wqy-zenhei.ttc",
    "DroidSansFallbackFull.ttf",
    "DejaVuSans.ttf",
    "DejaVuSansMono.ttf",
    "Symbola.ttf",
    "unifont.ttf",
];
const LINUX_EMOJI: &[&str] = &["NotoColorEmoji.ttf", "TwemojiMozilla.ttf"];

/// The machine's faces the pictures' families are: the terminal's (SF Mono on a Mac, as bateri's own
/// is; on Linux the first mono face of a short list), the system's (SF; the first sans one), and
/// the faces a missing character is looked for in, then every other face the directories have.
pub(super) fn find(platform: Platform, files: &Files) -> Layout {
    let named =
        |names: &[&str]| -> Vec<Src> { names.iter().filter_map(|n| files.face(n)).collect() };
    let (terminal, system, named_faces, emoji) = match platform {
        Platform::Mac => (
            files
                .first(MAC_MONO)
                .or_else(|| files.named("Menlo.ttc").map(|path| collection(path))),
            files.first(MAC_SYSTEM),
            named(MAC_NAMED),
            named(MAC_EMOJI),
        ),
        Platform::Linux => (
            files.first(LINUX_MONO),
            files.first(LINUX_SYSTEM),
            named(LINUX_NAMED),
            named(LINUX_EMOJI),
        ),
    };
    let scan = files
        .all
        .iter()
        .map(|path| Src {
            origin: Origin::File(path.clone()),
            index: 0,
        })
        .collect();
    Layout {
        terminal: terminal.unwrap_or_default(),
        system: system.unwrap_or_default(),
        named: named_faces,
        emoji,
        scan,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path_of(src: &Option<Src>) -> Option<String> {
        match src.as_ref().map(|src| &src.origin) {
            Some(Origin::File(path)) => path
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned),
            _ => None,
        }
    }

    fn dir_with(files: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pencere-faces-{}-{}",
            std::process::id(),
            files.len()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        for file in files {
            let path = dir.join("truetype").join(file);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
            std::fs::write(&path, b"not a face, only a name").expect("a file");
        }
        dir
    }

    /// A Linux machine's layout, from the directory of a made-up one: the first family of the list
    /// it has, its four files by name, the faces a character is looked for in in their order, and
    /// every other file after them. (Not run on a Linux machine of any kind: held to the names.)
    #[test]
    fn a_linux_machines_families_are_found_by_the_names_of_their_files() {
        let dir = dir_with(&[
            "DejaVuSansMono.ttf",
            "DejaVuSansMono-Bold.ttf",
            "DejaVuSansMono-Oblique.ttf",
            "LiberationMono-Regular.ttf",
            "NotoSans-Regular.ttf",
            "NotoColorEmoji.ttf",
            "NotoSansCJK-Regular.ttc",
            "Other.OTF",
            "readme.txt",
        ]);
        let found = find(Platform::Linux, &Files::in_dirs(std::slice::from_ref(&dir)));
        assert_eq!(
            path_of(&found.terminal.upright).as_deref(),
            Some("DejaVuSansMono.ttf")
        );
        assert_eq!(
            path_of(&found.terminal.bold).as_deref(),
            Some("DejaVuSansMono-Bold.ttf")
        );
        assert_eq!(
            path_of(&found.terminal.italic).as_deref(),
            Some("DejaVuSansMono-Oblique.ttf")
        );
        assert!(found.terminal.bold_italic.is_none(), "no file, no face");
        assert_eq!(
            path_of(&found.system.upright).as_deref(),
            Some("NotoSans-Regular.ttf")
        );
        assert_eq!(found.emoji.len(), 1);
        assert_eq!(
            found.named.len(),
            2,
            "the CJK face and DejaVu Sans Mono are there of the named"
        );
        assert_eq!(found.scan.len(), 8, "every face file, and no other");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_machine_with_no_faces_has_no_families() {
        let found = find(
            Platform::Mac,
            &Files::in_dirs(&[PathBuf::from("/no/such/directory")]),
        );
        assert!(found.terminal.upright.is_none() && found.system.upright.is_none());
        assert!(found.named.is_empty() && found.emoji.is_empty() && found.scan.is_empty());
    }
}
