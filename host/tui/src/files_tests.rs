//! The person's files, read and written for the program: the handles and the thread that moves the
//! bytes, run through a [`Conversation`] and the program's own [`ViewControl`], as the program asks.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use pencere::{Conversation, FileError, Options, Said, ViewControl};

use crate::files::{Disk, Handles};

const WITHIN: Duration = Duration::from_secs(10);

/// What the program asks, and the screen's side that answers: a conversation the program's calls
/// are made in, and the files the screen holds for the person.
struct Bench {
    control: ViewControl,
    conversation: Conversation,
    disk: Disk,
    handles: Arc<Handles>,
    dir: PathBuf,
}

impl Bench {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("pencere-tui-files-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let control = ViewControl::new();
        let handles = Handles::new();
        Self {
            conversation: Conversation::new(Options::new().control(control.clone())),
            disk: Disk::new(Arc::clone(&handles)),
            control,
            handles,
            dir,
        }
    }

    /// A file the person gave, by its handle.
    fn given(&self, name: &str, bytes: &[u8]) -> String {
        let path = self.dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        self.handles.give(&path, &std::fs::metadata(&path).unwrap())
    }

    /// A place the person chose, by its handle, and where it is.
    fn place(&self, name: &str) -> (String, PathBuf) {
        let target = self.dir.join(name);
        (self.handles.place(target.clone()), target)
    }

    /// The program reads `length` bytes at `offset` of `handle`: the screen answers.
    fn read(&mut self, handle: &str, offset: u64, length: usize) -> Result<Vec<u8>, FileError> {
        let (control, handle) = (self.control.clone(), handle.to_owned());
        let asking = std::thread::spawn(move || control.read(&handle, offset, length));
        let Some(Said::Read(reading)) = self.conversation.receive(WITHIN) else {
            panic!("the read said");
        };
        self.disk.read(reading);
        asking.join().unwrap()
    }

    /// The program writes `bytes` at `offset` of `handle`: the screen answers.
    fn write(
        &mut self,
        handle: &str,
        offset: u64,
        bytes: &[u8],
        end: bool,
    ) -> Result<(), FileError> {
        let (control, handle, bytes) = (self.control.clone(), handle.to_owned(), bytes.to_vec());
        let asking = std::thread::spawn(move || control.write(&handle, offset, bytes, end));
        let Some(Said::Write(writing)) = self.conversation.receive(WITHIN) else {
            panic!("the write said");
        };
        self.disk.write(writing);
        asking.join().unwrap()
    }

    /// The names in the bench's directory, sorted.
    fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&self.dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}

impl Drop for Bench {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn refused(result: Result<impl std::fmt::Debug, FileError>) -> String {
    match result {
        Err(FileError::Refused(code)) => code,
        other => panic!("not refused: {other:?}"),
    }
}

#[test]
fn a_file_given_is_read_from_where_the_person_left_it_a_piece_at_a_time() {
    let mut bench = Bench::new("read");
    let handle = bench.given("note.txt", b"0123456789");
    assert_eq!(bench.read(&handle, 3, 4).unwrap(), b"3456");
    assert_eq!(
        bench.read(&handle, 6, 100).unwrap(),
        b"6789",
        "fewer bytes than were asked for is the end of the file"
    );
    assert_eq!(
        bench.read(&handle, 10, 4).unwrap(),
        b"",
        "at the end there is nothing"
    );
    assert_eq!(refused(bench.read(&handle, 11, 4)), "range", "past it");
    assert_eq!(refused(bench.read(&handle, 0, 0)), "range", "of no bytes");
}

#[test]
fn a_file_that_is_not_what_it_was_when_it_was_given_is_gone() {
    let mut bench = Bench::new("gone");
    let handle = bench.given("a.txt", b"hello");
    std::fs::write(bench.dir.join("a.txt"), b"hello there").unwrap();
    assert_eq!(
        refused(bench.read(&handle, 0, 5)),
        "gone",
        "its length is another"
    );
    let handle = bench.given("b.txt", b"hello");
    std::fs::remove_file(bench.dir.join("b.txt")).unwrap();
    assert_eq!(
        refused(bench.read(&handle, 0, 5)),
        "gone",
        "it is not there"
    );
}

#[test]
fn a_handle_is_a_file_given_or_a_place_chosen_and_does_the_work_of_that_kind_alone() {
    let mut bench = Bench::new("kinds");
    let given = bench.given("given.txt", b"given");
    let (place, target) = bench.place("saved.txt");
    // A place is not read, and the page cannot draw it by `file:` either.
    assert_eq!(refused(bench.read(&place, 0, 5)), "handle");
    assert!(bench.handles.read(&place).is_none());
    assert!(bench.handles.given_path(&place).is_none());
    // A file given is not written to, and nothing is made where it is.
    assert_eq!(refused(bench.write(&given, 0, b"x", true)), "handle");
    assert_eq!(
        std::fs::read(bench.dir.join("given.txt")).unwrap(),
        b"given"
    );
    // A handle nobody made is neither.
    let nobody = "0".repeat(32);
    assert_eq!(refused(bench.read(&nobody, 0, 5)), "handle");
    assert_eq!(refused(bench.write(&nobody, 0, b"x", true)), "handle");
    assert!(!target.exists());
    assert_eq!(bench.names(), ["given.txt"]);
}

#[test]
fn a_save_is_written_to_a_part_beside_the_place_and_has_the_places_name_when_the_last_piece_is_in()
{
    let mut bench = Bench::new("write");
    let (handle, target) = bench.place("saved.bin");
    bench.write(&handle, 0, b"hello ", false).unwrap();
    // What is there before the last piece is a part, hidden and named for what it is, and nothing
    // is under the person's name.
    assert!(!target.exists());
    let names = bench.names();
    let [part] = names.as_slice() else {
        panic!("one part: {names:?}");
    };
    assert!(
        part.starts_with(".saved.bin.") && part.ends_with(".pencere-part"),
        "{part}"
    );
    bench.write(&handle, 6, b"world", true).unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"hello world");
    assert_eq!(bench.names(), ["saved.bin"], "no part is left");
    // The place has had its last write.
    assert_eq!(refused(bench.write(&handle, 11, b"more", true)), "handle");
    assert_eq!(std::fs::read(&target).unwrap(), b"hello world");
}

#[test]
fn a_save_whose_pieces_come_out_of_order_or_too_large_fails_and_leaves_nothing_behind() {
    let mut bench = Bench::new("order");
    let (handle, target) = bench.place("saved.bin");
    bench.write(&handle, 0, b"abc", false).unwrap();
    assert_eq!(
        refused(bench.write(&handle, 9, b"x", true)),
        "order",
        "a gap"
    );
    assert_eq!(
        bench.names(),
        Vec::<String>::new(),
        "the part is thrown away"
    );
    // And what comes for the handle is for a handle that is nothing.
    assert_eq!(refused(bench.write(&handle, 3, b"def", true)), "handle");
    assert!(!target.exists());
    // A first piece that is not the start of the file is a gap too.
    let (second, _) = bench.place("second.bin");
    assert_eq!(refused(bench.write(&second, 5, b"x", false)), "order");
    // A piece of more than a megabyte is `size`.
    let (third, _) = bench.place("third.bin");
    let big = vec![7_u8; (1 << 20) + 1];
    assert_eq!(refused(bench.write(&third, 0, &big, true)), "size");
    assert_eq!(bench.names(), Vec::<String>::new());
}

#[test]
fn a_link_where_the_part_goes_is_replaced_and_not_written_through() {
    let mut bench = Bench::new("link");
    let (handle, target) = bench.place("saved.bin");
    let victim = bench.dir.join("victim.txt");
    std::fs::write(&victim, b"the person's").unwrap();
    // Where the part will be, a link to a file that is not the program's to write.
    let part = bench.dir.join(format!(
        ".saved.bin.{}.pencere-part",
        &handle[handle.len() - 8..]
    ));
    std::os::unix::fs::symlink(&victim, &part).unwrap();
    bench.write(&handle, 0, b"from the program", true).unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"from the program");
    assert_eq!(std::fs::read(&victim).unwrap(), b"the person's");
    assert_eq!(bench.names(), ["saved.bin", "victim.txt"]);
}

#[test]
fn the_saves_under_way_are_given_up_when_the_view_is_shown_again_and_when_the_screen_ends() {
    let mut bench = Bench::new("abandon");
    let (handle, target) = bench.place("a.bin");
    bench.write(&handle, 0, b"abc", false).unwrap();
    assert_eq!(bench.names().len(), 1);
    bench.disk.abandon();
    // The next thing the thread is asked is after that: the part is gone and the place is nothing.
    assert_eq!(refused(bench.write(&handle, 3, b"def", true)), "handle");
    assert_eq!(bench.names(), Vec::<String>::new());
    assert!(!target.exists());
    // The screen ends with a save under way: no part is left.
    let (handle, _) = bench.place("b.bin");
    bench.write(&handle, 0, b"abc", false).unwrap();
    assert_eq!(bench.names().len(), 1);
    let Bench { disk, dir, .. } = &mut bench;
    drop(std::mem::replace(disk, Disk::new(Handles::new())));
    let left: Vec<_> = std::fs::read_dir(dir).unwrap().collect();
    assert!(left.is_empty(), "{left:?}");
}

#[test]
fn a_name_that_is_nearly_as_long_as_a_name_may_be_still_has_a_part_that_is_a_name() {
    let mut bench = Bench::new("long");
    let name = "n".repeat(250);
    let (handle, target) = bench.place(&name);
    bench.write(&handle, 0, b"abc", false).unwrap();
    let names = bench.names();
    let [part] = names.as_slice() else {
        panic!("one part: {names:?}");
    };
    assert!(part.len() <= 255, "{}", part.len());
    bench.write(&handle, 3, b"def", true).unwrap();
    assert_eq!(std::fs::read(Path::new(&target)).unwrap(), b"abcdef");
}
