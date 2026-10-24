//! {{name}}: a program that shows a view in the pane of the terminal it runs in.
//!
//! The `pencere` library does what a program has to do to have a view — the handshake, the modes
//! of the terminal, the signals, giving the terminal back — and this program is the calls and the
//! topics of its view. `cargo run` in a terminal that hosts pencere shows it.

use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use pencere::{Bundle, Kind, Options, View};
use serde_json::json;

/// The view, a page of the kit's page API, which the terminal draws in its own font and theme.
const PAGE: &[u8] = include_bytes!("../page.lua");
const ICON: &[u8] = include_bytes!("../assets/icon.svg");

fn main() -> Result<ExitCode, pencere::Error> {
    let bundle = Bundle::new(
        "page.lua",
        [
            pencere::File::new("page.lua", PAGE),
            pencere::File::new("assets/icon.svg", ICON),
        ],
    )?;
    let outcome = pencere::serve(
        Options::new()
            .view(View::new(Kind::Lua, &["1.0"], bundle))
            .program("{{name}}")
            .title("{{name}}")
            .call("hello", |params, _call| {
                let name = params["name"].as_str().unwrap_or("world");
                Ok(json!({ "message": format!("Hello, {name}!") }))
            })
            .topic("clock", |_params, subscription| {
                let feed = subscription.clone();
                std::thread::spawn(move || {
                    // Until the view stops listening (or is gone).
                    loop {
                        let now = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map_or(0.0, |since| since.as_secs_f64());
                        if !feed.push(json!({ "now": now })) || feed.sleep(Duration::from_secs(1)) {
                            break;
                        }
                    }
                });
                Ok(())
            })
            .fallback(|_| println!("This terminal shows no views, so here is the text: Hello, world!")),
    )?;
    Ok(ExitCode::from(outcome.shell_status()))
}
