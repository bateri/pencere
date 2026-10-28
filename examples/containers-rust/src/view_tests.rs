//! The program's handlers and its Lua page in one process, under the host with no screen
//! (`pencere-drive`'s `headless`, with the program served by `pencere::Conversation`): no terminal,
//! no pty, the same handlers `serve` would give a terminal's view. Heavy: the interpreter is
//! compiled and run (`make check-heavy`).

use std::sync::Arc;
use std::time::Duration;

use pencere_drive::headless::{Error, Headless, InProcess};

use crate::{SCRIPT, Upload, handlers};

/// The page shows the readings the program pushes, and ⇧U's four mebibytes, sent in pieces, are
/// counted by the program and the count shown.
#[test]
#[ignore = "heavy: compiles the interpreter and drives the page; make check-heavy"]
fn the_lua_page_hears_its_program_in_this_process() -> Result<(), Error> {
    let upload = Arc::new(Upload::default());
    let script = String::from_utf8_lossy(SCRIPT).into_owned();
    let mut page = Headless::script(script)
        .size(1000.0, 600.0)
        .peer(InProcess::new(handlers(false, None, &upload)))
        .open()?;
    page.wait_for(Duration::from_secs(30), |page| {
        page.received()
            .iter()
            .any(|said| said.contains(r#""push""#))
            && page.says("%")
    })?;
    page.type_text("U")?;
    page.wait_for(Duration::from_secs(120), |page| {
        page.says("containers-rust counted 4194304 bytes")
    })?;
    assert_eq!(upload.total(), 4 * 1024 * 1024);
    Ok(())
}
