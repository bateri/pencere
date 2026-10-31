//! The calls of the system that the fast way of a file needs and `std` does not have: who the
//! process is, waiting on a descriptor, and cloning a file (specification §8.11, §8.9).

use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
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

/// Opens `name`, a name of one part, in the directory `dir` that is open, without following a link
/// at it: what is opened is in that directory and not where a path to it may lead by now.
pub(crate) fn open_at(dir: &File, name: &str, flags: i32) -> io::Result<File> {
    if name.is_empty() || name.contains(['/', '\0']) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a name of one part",
        ));
    }
    let name = std::ffi::CString::new(name)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "a NUL in a name"))?;
    // SAFETY: a descriptor that is open for the call and a NUL-terminated name that lives for it.
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `fd` was just opened here and nothing else owns it.
    Ok(unsafe { File::from_raw_fd(fd) })
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
    // Not a link, if `from` is one, as on the other systems: the file is the one given.
    let Ok(source) = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(from)
    else {
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
