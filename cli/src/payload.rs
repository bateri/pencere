//! The app inside the executable: a section that `pencere build` writes into a copy of the
//! runner, and that the runner reads from itself when it starts.
//!
//! The section is made by `libsui`, the crate the Deno authors made for `deno compile`. A
//! section, and not bytes added to the end of the file, because a Mac refuses to run a signed
//! executable that has grown: on an Apple silicon machine the kernel kills it, and `codesign
//! --verify` says it "failed strict validation". `libsui` writes the section into the Mach-O file,
//! signs the result with an ad hoc signature in this process (so that a Linux machine can make the
//! file of a Mac, and no `codesign` is needed), and writes it into an ELF file the same way.

use std::fs::{self, File};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use libsui::{Elf, Macho};

/// The name of the section.
const SECTION: &str = "pencere";

/// The executable formats a runner can be in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// A 64-bit Mach-O file: macOS.
    MachO,
    /// An ELF file: Linux.
    Elf,
}

/// The format of an executable, from its first bytes.
pub fn format_of(runner: &[u8]) -> Option<Format> {
    match runner.get(..4)? {
        [0xcf, 0xfa, 0xed, 0xfe] => Some(Format::MachO),
        [0x7f, b'E', b'L', b'F'] => Some(Format::Elf),
        _ => None,
    }
}

/// The app inside this executable, if there is one.
pub fn own() -> Option<&'static [u8]> {
    match libsui::find_section(SECTION) {
        Ok(Some(payload)) if !payload.is_empty() => Some(payload),
        _ => None,
    }
}

/// Writes the executable `runner` with `payload` in it to `out` (made executable).
pub fn embed(runner: Vec<u8>, payload: Vec<u8>, out: &Path) -> io::Result<()> {
    let format = format_of(&runner).ok_or_else(|| {
        io::Error::other("the runner is neither a Mach-O nor an ELF executable (a 64-bit one)")
    })?;
    // Written beside the destination and moved over it: a file that is running is never written
    // into, and a destination that fails half way is not left half made.
    let mut partial_name = out.as_os_str().to_owned();
    partial_name.push(".partial");
    let partial = PathBuf::from(partial_name);
    let result = (|| {
        let mut file = File::create(&partial)?;
        match format {
            Format::MachO => {
                Macho::from(runner)
                    .map_err(io::Error::other)?
                    .write_section(SECTION, payload)
                    .map_err(io::Error::other)?
                    .build_and_sign(&mut file)
                    .map_err(io::Error::other)?;
            }
            Format::Elf => {
                Elf::new(&runner)
                    .append(SECTION, &payload, &mut file)
                    .map_err(io::Error::other)?;
            }
        }
        file.flush()?;
        drop(file);
        make_executable(&partial)?;
        fs::rename(&partial, out)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&partial);
    }
    result
}

fn make_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
}
