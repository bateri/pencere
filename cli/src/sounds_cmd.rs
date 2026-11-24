//! `pencere sounds DIR`: the kit's own sounds as WAV files, to be listened to.
//!
//! The sounds are made by the kit (a page asks for one by its name), the same on every system;
//! this writes each as `NAME.wav` so that a person can hear what a name says, and decide which to
//! ask for. The command with no terminal in it does not hold the kit, and says so.

use std::fs;
use std::path::Path;
use std::process::ExitCode;

/// What `pencere sounds` does.
pub fn command(args: &[String]) -> ExitCode {
    let [directory] = args else {
        return crate::usage("sounds takes the directory to write the sounds into");
    };
    match write(Path::new(directory)) {
        Ok(names) => {
            println!("wrote {} sounds to {directory}:", names.len());
            println!("  {}", names.join(" "));
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("pencere: {why}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(feature = "terminal")]
fn palette() -> Result<Vec<(&'static str, Vec<u8>)>, String> {
    Ok(pencere_tui::palette_wavs())
}

#[cfg(not(feature = "terminal"))]
fn palette() -> Result<Vec<(&'static str, Vec<u8>)>, String> {
    Err(
        "this command was built with no terminal, and holds none of the kit's sounds: \
         the one built with its terminal writes them"
            .into(),
    )
}

/// Writes each sound of the palette into `directory` and says their names.
fn write(directory: &Path) -> Result<Vec<&'static str>, String> {
    let sounds = palette()?;
    fs::create_dir_all(directory)
        .map_err(|why| format!("{}: could not be made: {why}", directory.display()))?;
    let mut names = Vec::new();
    for (name, bytes) in sounds {
        let file = directory.join(format!("{name}.wav"));
        fs::write(&file, bytes).map_err(|why| format!("{}: {why}", file.display()))?;
        names.push(name);
    }
    Ok(names)
}
