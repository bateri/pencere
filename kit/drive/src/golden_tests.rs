//! Golden readings: the text a page's reading is expected to be, kept in a file of `src/goldens/`
//! for each, and the line-by-line difference printed where it is not.
//!
//! A change that is meant to change what is read (a role, a state, a line gone or added) shows as
//! the change of those files in the diff of the commit, which is where it is reviewed; one that is
//! not meant to shows as a test that fails with the lines that differ. `PENCERE_BLESS=1` writes the
//! files again from what the pages read now.

use std::fs;
use std::path::PathBuf;

/// The file of the golden `name`.
fn file(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/goldens")
        .join(format!("{name}.txt"))
}

/// Sets `actual` against the golden `name`: it passes where they are the same, and fails with the
/// lines that differ where they are not (or there is no golden yet).
pub(crate) fn check(name: &str, actual: &str) {
    let path = file(name);
    if std::env::var_os("PENCERE_BLESS").is_some() {
        fs::create_dir_all(path.parent().expect("a folder")).expect("a folder of goldens");
        fs::write(&path, actual).expect("a golden written");
        return;
    }
    let Ok(expected) = fs::read_to_string(&path) else {
        panic!(
            "there is no golden reading `{name}` ({}): PENCERE_BLESS=1 writes it from what is read now:\n{actual}",
            path.display()
        );
    };
    if expected != actual {
        panic!(
            "the reading `{name}` is not its golden (src/goldens/{name}.txt; PENCERE_BLESS=1 writes it again):\n{}",
            difference(&expected, actual)
        );
    }
}

/// The lines of `expected` and `actual` that differ, with a line of what stays on either side:
/// `-` what the golden has and is not read, `+` what is read and the golden has not.
pub(crate) fn difference(expected: &str, actual: &str) -> String {
    let (old, new): (Vec<&str>, Vec<&str>) = (expected.lines().collect(), actual.lines().collect());
    // The longest run of lines the two have in common, from the end.
    let mut common = vec![vec![0usize; new.len() + 1]; old.len() + 1];
    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            common[i][j] = if old[i] == new[j] {
                common[i + 1][j + 1] + 1
            } else {
                common[i + 1][j].max(common[i][j + 1])
            };
        }
    }
    let mut marked: Vec<(char, &str)> = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < old.len() || j < new.len() {
        if i < old.len() && j < new.len() && old[i] == new[j] {
            marked.push((' ', old[i]));
            i += 1;
            j += 1;
        } else if i < old.len() && (j == new.len() || common[i + 1][j] >= common[i][j + 1]) {
            marked.push(('-', old[i]));
            i += 1;
        } else {
            marked.push(('+', new[j]));
            j += 1;
        }
    }
    let mut out = String::new();
    let mut last = None;
    for (at, (mark, line)) in marked.iter().enumerate() {
        let near = marked[at.saturating_sub(1)..(at + 2).min(marked.len())]
            .iter()
            .any(|(mark, _)| *mark != ' ');
        if *mark == ' ' && !near {
            continue;
        }
        if last.is_some_and(|last| last + 1 != at) {
            out.push_str("  ...\n");
        }
        out.push_str(&format!("{mark} {line}\n"));
        last = Some(at);
    }
    out
}

#[test]
fn what_differs_is_told_by_its_lines_and_what_is_the_same_is_not() {
    assert_eq!(difference("a\nb\nc", "a\nb\nc"), "");
    assert_eq!(
        difference("a\nb\nc\nd\ne\nf", "a\nb\nX\nd\ne\nf"),
        "  b\n- c\n+ X\n  d\n"
    );
    assert_eq!(difference("a\nb", "a"), "  a\n- b\n");
}
