//! The calls of the system that the fast way of a file needs and `std` does not have: who the
//! process is, waiting on a descriptor, and cloning a file (specification §8.11, §8.9).

use std::os::fd::RawFd;
use std::path::Path;

/// The effective user id of the process: the user that the folder of the terminal and its pipes
/// must belong to.
pub(crate) fn euid() -> u32 {
    // SAFETY: `geteuid` reads the process's credentials and has no way to fail.
    unsafe { libc::geteuid() }
}

/// Waits up to `millis` milliseconds for `fd` to be readable, or writable if `for_write`. True when
/// it is, or when it has hung up or failed (a read or a write then says so); false when the time
/// ran out or a signal came.
pub(crate) fn ready(fd: RawFd, for_write: bool, millis: i32) -> bool {
    let mut poll = libc::pollfd {
        fd,
        events: if for_write {
            libc::POLLOUT
        } else {
            libc::POLLIN
        },
        revents: 0,
    };
    // SAFETY: one valid `pollfd` that lives for the call, and a count of one.
    unsafe { libc::poll(&raw mut poll, 1, millis) > 0 }
}

/// Makes `to`, which must not exist, a copy-on-write clone of the file at `from`, where the file
/// system can share the blocks of the two (APFS, Btrfs, XFS with reflinks). True when it did; the
/// clone then has `from`'s bytes and no others, and when it did not, `to` is not there.
pub(crate) fn clone_file(from: &Path, to: &Path) -> bool {
    clone_file_by_system(from, to)
}

#[cfg(target_os = "macos")]
fn clone_file_by_system(from: &Path, to: &Path) -> bool {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    /// Not a link, if `from` is one: the file is the one given, not where it points.
    const CLONE_NOFOLLOW: u32 = 0x0001;
    let (Ok(from), Ok(to)) = (
        CString::new(from.as_os_str().as_bytes()),
        CString::new(to.as_os_str().as_bytes()),
    ) else {
        return false;
    };
    // SAFETY: two NUL-terminated paths that live for the call.
    unsafe { libc::clonefile(from.as_ptr(), to.as_ptr(), CLONE_NOFOLLOW) == 0 }
}

#[cfg(target_os = "linux")]
fn clone_file_by_system(from: &Path, to: &Path) -> bool {
    use std::fs;
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;
    let Ok(source) = fs::File::open(from) else {
        return false;
    };
    let Ok(dest) = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(to)
    else {
        return false;
    };
    // SAFETY: two descriptors that are open for the call; `FICLONE` takes the source's.
    let done = unsafe { libc::ioctl(dest.as_raw_fd(), libc::FICLONE, source.as_raw_fd()) } == 0;
    if !done {
        drop(dest);
        let _ = fs::remove_file(to);
    }
    done
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn clone_file_by_system(_from: &Path, _to: &Path) -> bool {
    false
}
