//! The room a Mach-O runner has in its header for the segment `pencere build` adds.
//!
//! The app goes into the runner as a section, which is a new load command in the header. The
//! linker leaves little room between the last load command and the first section, and `libsui`,
//! which writes the section, does not look: it takes the bytes right after the load commands to be
//! padding and writes the new command over them. A header with less room than the command needs
//! has the first functions of the program there, and the app built from it stops at its first call
//! of one of them. This reads the header to ask, so that a runner without the room is refused and
//! not turned into an app that does not run.
//!
//! This file has nothing of the crate in it: the integration tests include it to ask the runner
//! the build script made.

// The tests that include this file use some of it.
#![allow(dead_code)]

/// The size of the load command `libsui` adds on a 64-bit Arm Mac: a `segment_command_64` (72
/// bytes) and the `section_64` (80) in it.
pub const NEW_COMMAND: usize = 72 + 80;

const MACH_HEADER_64: usize = 32;
const MAGIC_64: [u8; 4] = [0xcf, 0xfa, 0xed, 0xfe];
const CPU_TYPE_ARM_64: u32 = 0x0100_000c;
const LC_SEGMENT_64: u32 = 0x19;
const SEGMENT_COMMAND_64: usize = 72;
const SECTION_64: usize = 80;

fn word(bytes: &[u8], at: usize) -> Option<u32> {
    let part: [u8; 4] = bytes.get(at..at.checked_add(4)?)?.try_into().ok()?;
    Some(u32::from_le_bytes(part))
}

/// Whether the executable is a 64-bit Mach-O for 64-bit Arm: the one `libsui` writes a load command
/// into. (An Intel one gets the app at its end, and needs no room.)
pub fn is_arm_64(runner: &[u8]) -> bool {
    runner.get(..4) == Some(&MAGIC_64[..]) && word(runner, 4) == Some(CPU_TYPE_ARM_64)
}

/// How many bytes there are between the end of the load commands and the contents of the first
/// section that has any: the room a new load command can be written in. None where the header
/// cannot be read.
pub fn header_room(runner: &[u8]) -> Option<usize> {
    if runner.get(..4) != Some(&MAGIC_64[..]) {
        return None;
    }
    let count = word(runner, 16)?;
    let commands_size = usize::try_from(word(runner, 20)?).ok()?;
    let commands_end = MACH_HEADER_64.checked_add(commands_size)?;
    let mut first: Option<usize> = None;
    let mut at = MACH_HEADER_64;
    for _ in 0..count {
        let command = word(runner, at)?;
        let size = usize::try_from(word(runner, at.checked_add(4)?)?).ok()?;
        if size < 8 || at.checked_add(size)? > commands_end {
            return None;
        }
        if command == LC_SEGMENT_64 {
            let sections = usize::try_from(word(runner, at.checked_add(64)?)?).ok()?;
            for index in 0..sections {
                let section = at
                    .checked_add(SEGMENT_COMMAND_64)?
                    .checked_add(index.checked_mul(SECTION_64)?)?;
                // The contents of a section: an empty one, and a zero-filled one (which has no
                // bytes in the file and says 0), are nowhere.
                let length = word(runner, section.checked_add(40)?)?;
                let offset = usize::try_from(word(runner, section.checked_add(48)?)?).ok()?;
                if length > 0 && offset > 0 {
                    first = Some(first.map_or(offset, |had| had.min(offset)));
                }
            }
        }
        at += size;
    }
    first?.checked_sub(commands_end)
}

/// A header with one segment that has the sections given, as (offset, length) each.
#[cfg(test)]
pub fn header(commands: &[(u32, u32)], arm: bool) -> Vec<u8> {
    let size = SEGMENT_COMMAND_64 + commands.len() * SECTION_64;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&MAGIC_64);
    let cpu = if arm { CPU_TYPE_ARM_64 } else { 0x0100_0007 };
    for value in [cpu, 0, 2, 1, u32::try_from(size).unwrap_or(0), 0, 0] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    let mut segment = vec![0u8; size];
    segment[..4].copy_from_slice(&LC_SEGMENT_64.to_le_bytes());
    segment[4..8].copy_from_slice(&u32::try_from(size).unwrap_or(0).to_le_bytes());
    segment[64..68].copy_from_slice(&u32::try_from(commands.len()).unwrap_or(0).to_le_bytes());
    for (index, (offset, length)) in commands.iter().enumerate() {
        let at = SEGMENT_COMMAND_64 + index * SECTION_64;
        segment[at + 40..at + 44].copy_from_slice(&length.to_le_bytes());
        segment[at + 48..at + 52].copy_from_slice(&offset.to_le_bytes());
    }
    bytes.extend_from_slice(&segment);
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_room_is_what_lies_between_the_commands_and_the_first_contents() {
        // Four sections, so the commands end at 32 + 72 + 4 * 80 = 424. Out of order; an empty
        // section and a zero-filled one (which says 0) have no contents to be in the way.
        let bytes = header(&[(40_000, 8), (524, 8), (0, 8), (300, 0)], true);
        assert_eq!(header_room(&bytes), Some(100));
        assert!(is_arm_64(&bytes));
        assert!(!is_arm_64(&header(&[(1, 1)], false)));
        assert_eq!(header_room(b"not a header"), None);
        assert_eq!(header_room(bytes.get(..40).unwrap_or_default()), None);
    }

    #[test]
    fn a_header_with_less_room_than_the_command_is_told_apart_from_one_with_enough() {
        let commands_end = MACH_HEADER_64 + SEGMENT_COMMAND_64 + SECTION_64;
        let tight = header(&[(commands_end as u32 + 20, 8)], true);
        assert!(header_room(&tight).is_some_and(|room| room < NEW_COMMAND));
        let roomy = header(&[(commands_end as u32 + 0x4000, 8)], true);
        assert!(header_room(&roomy).is_some_and(|room| room >= NEW_COMMAND));
    }
}
