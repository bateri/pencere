//! How much memory the process holds, said in the trace (`PENCERE_TUI_TRACE`): what a pen that
//! shows its page in a terminal costs the person who has it open.
//!
//! The two calls macOS has for it are declared here (`proc_pidinfo` of `libproc.h`, `getrusage` of
//! `sys/resource.h`: their layouts are the system's and do not change), so that the crate needs no
//! library of the system's types for a line of a trace; Linux says it in `/proc`.

/// What the system's calls fill: more room than either of their structures (`proc_taskinfo` is 96
/// bytes, `rusage` 144).
#[cfg(target_os = "macos")]
type Room = [u64; 20];

/// The bytes the process has in memory now, where the system says.
#[cfg(target_os = "macos")]
pub(crate) fn resident() -> Option<u64> {
    unsafe extern "C" {
        fn proc_pidinfo(
            pid: i32,
            flavor: i32,
            arg: u64,
            buffer: *mut std::ffi::c_void,
            size: i32,
        ) -> i32;
    }
    /// `PROC_PIDTASKINFO`, the flavour that answers with a `proc_taskinfo`.
    const TASK_INFO: i32 = 4;
    /// The size of `proc_taskinfo`: the call answers with all of it or fails.
    const SIZE: i32 = 96;
    let mut room: Room = [0; 20];
    let pid = i32::try_from(std::process::id()).ok()?;
    // SAFETY: `room` is larger than the `SIZE` bytes the call may write.
    #[allow(unsafe_code)]
    let written = unsafe { proc_pidinfo(pid, TASK_INFO, 0, room.as_mut_ptr().cast(), SIZE) };
    // `pti_resident_size` is the second word of `proc_taskinfo`.
    (written == SIZE).then_some(room[1])
}

/// The most bytes the process has had in memory, where the system says.
#[cfg(target_os = "macos")]
pub(crate) fn peak() -> Option<u64> {
    unsafe extern "C" {
        fn getrusage(who: i32, usage: *mut std::ffi::c_void) -> i32;
    }
    /// `RUSAGE_SELF`.
    const SELF: i32 = 0;
    let mut room: Room = [0; 20];
    // SAFETY: `room` is larger than a `rusage`, which the call fills.
    #[allow(unsafe_code)]
    let done = unsafe { getrusage(SELF, room.as_mut_ptr().cast()) };
    // `ru_maxrss` follows the two times of `rusage` (16 bytes each), and is in bytes on macOS.
    (done == 0).then_some(room[4])
}

/// A figure of `/proc/self/status`, in bytes.
#[cfg(target_os = "linux")]
fn status(name: &str) -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find_map(|line| line.strip_prefix(name))?;
    let kilobytes: u64 = line
        .trim_start_matches(':')
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    Some(kilobytes * 1024)
}

/// The bytes the process has in memory now, where the system says.
#[cfg(target_os = "linux")]
pub(crate) fn resident() -> Option<u64> {
    status("VmRSS")
}

/// The most bytes the process has had in memory, where the system says.
#[cfg(target_os = "linux")]
pub(crate) fn peak() -> Option<u64> {
    status("VmHWM")
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(crate) fn resident() -> Option<u64> {
    None
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(crate) fn peak() -> Option<u64> {
    None
}

/// `bytes` as the trace says them: in MiB, or nothing where the system did not say.
pub(crate) fn mib(bytes: Option<u64>) -> String {
    bytes.map_or_else(
        || "?".to_owned(),
        |bytes| format!("{:.1}", bytes as f64 / (1024.0 * 1024.0)),
    )
}
