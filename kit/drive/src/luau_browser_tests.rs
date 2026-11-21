//! A Lua page with a browser, driven by the host with no screen: the room laid out as a browser's,
//! what the page asks of it kept for the test, what the browser would tell the page sent to it, and
//! what stands in the room of a host that shows none. The page is the example's
//! (`examples/browser-lua`), and some short scripts besides. Heavy: the interpreter is compiled and
//! run (`make check-heavy`).

use pencere_sandbox::Fault;
use pencere_view::{
    BrowserAsk, BrowserRefusal, BrowserState, BrowserWindows, Event, Id, Key, Mods, Paint,
};

use crate::UNSUPPORTED;
use crate::headless::{Error, Focus, Headless};

const PAGE: &str = include_str!("../../../examples/browser-lua/src/view/page.lua");

/// A script page whose room of a browser is `room`, and which keeps what it is told.
fn script(room: &str) -> String {
    format!(
        r#"
local told = {{}}
return {{
	render = function()
		return pencere.div():size_full():child({room}:grow(1))
	end,
	handle = function(event)
		if event.type == "browser" or event.type == "browser_refused" or event.type == "browser_window" then
			told[#told + 1] = event.type
			if event.type == "browser" and event.status == "ready" and #told == 1 then
				return {{ browser = {{ web = {{ go = "https://example.com/" }} }} }}
			end
			if event.type == "browser_window" then
				return {{ browser = {{ web = {{ go = event.url }} }} }}
			end
		end
		if event.type == "key" then
			return {{ browser = {{ web = {{ go = "file:///etc/passwd" }} }} }}
		end
	end,
}}
"#
    )
}

fn open(source: &str) -> Result<Headless, Error> {
    Headless::script(source).size(800.0, 600.0).open()
}

fn browser_events(page: &Headless) -> Vec<&Event> {
    page.told()
        .iter()
        .filter(|event| {
            matches!(
                event,
                Event::Browser { .. } | Event::BrowserRefused { .. } | Event::BrowserWindow { .. }
            )
        })
        .collect()
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_room_is_a_browser_with_the_options_the_page_made_it_by() -> Result<(), Error> {
    let page = open(&script(
        r#"pencere.browser("web", { profile = "work-1", windows = "page", keys = { "mod+l", "f5" } })"#,
    ))?;
    let [room] = page.browsers() else {
        panic!("one browser: {:?}", page.browsers());
    };
    assert_eq!(room.id, Id::named("web"));
    assert_eq!(room.options.profile.as_deref(), Some("work-1"));
    assert_eq!(room.options.windows, BrowserWindows::Page);
    assert_eq!(room.options.keys.len(), 2);
    assert_eq!((room.rect.w, room.rect.h), (800.0, 600.0));
    // The picture the host draws is its engine's: the draw list has the place, and the host with no
    // screen has no placeholder for it, since it says it shows browsers.
    assert!(
        page.list()
            .items
            .iter()
            .any(|item| matches!(item, Paint::Browser(slot) if slot.id == room.id))
    );
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn what_the_browser_says_reaches_the_page_and_what_the_page_asks_reaches_the_host()
-> Result<(), Error> {
    let mut page = open(&script(r#"pencere.browser("web")"#))?;
    // A browser that is made says nothing until its host does; and what is asked of it before the page
    // has heard that it is ready is nothing.
    assert!(browser_events(&page).is_empty());
    assert!(page.browser_asks().is_empty());
    page.browser_state("web", BrowserState::default())?;
    assert_eq!(
        page.browser_asks(),
        [(
            Id::named("web"),
            BrowserAsk::Go("https://example.com/".to_owned())
        )]
    );
    // What it is told is all of it, as the host said it.
    let state = BrowserState {
        url: "https://example.com/".into(),
        title: "Example".into(),
        loading: true,
        back: true,
        ..BrowserState::default()
    };
    page.browser_state("web", state.clone())?;
    assert!(matches!(
        browser_events(&page).last(),
        Some(Event::Browser { browser, state: told }) if *browser == Id::named("web") && **told == state
    ));
    // A link that opens a window is the page's to follow.
    page.browser_window("web", "https://example.org/")?;
    assert_eq!(
        page.browser_asks().last(),
        Some(&(
            Id::named("web"),
            BrowserAsk::Go("https://example.org/".to_owned())
        ))
    );
    assert!(matches!(
        page.browser_state("nowhere", state),
        Err(Error::NoSuch(_))
    ));
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn an_address_a_browser_does_not_go_to_is_refused_to_the_page_and_is_not_asked_of_the_host()
-> Result<(), Error> {
    let mut page = open(&script(r#"pencere.browser("web")"#))?;
    page.browser_state("web", BrowserState::default())?;
    let asked = page.browser_asks().len();
    // Any key makes the script ask for a file.
    page.key(Key::Character('x'), Mods::default())?;
    assert_eq!(
        page.browser_asks().len(),
        asked,
        "the host was asked nothing"
    );
    assert!(matches!(
        browser_events(&page).last(),
        Some(Event::BrowserRefused { browser, reason: BrowserRefusal::Scheme }) if *browser == Id::named("web")
    ));
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_browser_takes_the_keyboard_on_a_press_and_the_keys_the_page_kept_are_the_pages()
-> Result<(), Error> {
    let mut page = open(&script(r#"pencere.browser("web", { keys = { "mod+l" } })"#))?;
    assert_eq!(page.focus(), Focus::Page);
    page.click("web")?;
    assert_eq!(page.focus(), Focus::Browser(Id::named("web")));
    // `cmd+t` is not the page's: the browser takes it, and the page is told nothing.
    let command = Mods {
        command: true,
        ..Mods::default()
    };
    page.key(Key::Character('t'), command)?;
    assert_eq!(
        page.browser_keys(),
        [(Id::named("web"), Key::Character('t'), command)]
    );
    // `cmd+l` is: the script answers every key it is told by asking for a file, which is refused.
    page.key(Key::Character('l'), command)?;
    assert_eq!(page.browser_keys().len(), 1);
    assert!(matches!(
        browser_events(&page).last(),
        Some(Event::BrowserRefused { .. })
    ));
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_host_that_shows_no_browser_is_asked_nothing_and_has_a_placeholder_in_the_room()
-> Result<(), Error> {
    let mut page = Headless::script(script(r#"pencere.browser("web")"#))
        .size(800.0, 600.0)
        .browsers(false)
        .open()?;
    // The page is told the status once, with the sentence for the person.
    let told: Vec<_> = browser_events(&page)
        .into_iter()
        .filter_map(|event| match event {
            Event::Browser { state, .. } => Some((state.status.clone(), state.message.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        told,
        [("unsupported".to_owned(), Some(UNSUPPORTED.to_owned()))]
    );
    assert!(page.says(UNSUPPORTED), "{:?}", page.words());
    assert!(
        page.list()
            .items
            .iter()
            .all(|item| !matches!(item, Paint::Browser(_)))
    );
    // What the page asks of it (here a refused address as well as a good one) never reaches a host
    // that shows none.
    page.key(Key::Character('x'), Mods::default())?;
    page.browser_window("web", "https://example.org/")?;
    assert!(page.browser_asks().is_empty());
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_browser_of_another_profile_under_the_same_name_is_another_browser_and_is_told_so()
-> Result<(), Error> {
    let source = r#"
local profile = "a"
return {
	render = function()
		return pencere.div():size_full():child(pencere.browser("web", { profile = profile }):grow(1))
	end,
	handle = function(event)
		if event.type == "key" then
			profile = "b"
			return true
		end
	end,
}
"#;
    let mut page = Headless::script(source)
        .size(400.0, 300.0)
        .browsers(false)
        .open()?;
    assert_eq!(browser_events(&page).len(), 1);
    page.key(Key::Character('x'), Mods::default())?;
    assert_eq!(page.browsers()[0].options.profile.as_deref(), Some("b"));
    assert_eq!(browser_events(&page).len(), 2, "told again, as a new one");
    Ok(())
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn a_page_that_names_a_browser_wrongly_fails_where_it_does() {
    for (room, why) in [
        (r#"pencere.browser("web", { profile = "Work" })"#, "profile"),
        (r#"pencere.browser("web", { profile = "" })"#, "profile"),
        (
            &format!(
                r#"pencere.browser("web", {{ profile = "{}" }})"#,
                "a".repeat(33)
            ),
            "profile",
        ),
        (r#"pencere.browser("web", { windows = "tab" })"#, "windows"),
        (r#"pencere.browser("web", { keys = { "mod+" } })"#, "key"),
        (
            r#"pencere.browser("web", { url = "https://example.com" })"#,
            "no option url",
        ),
        (r#"pencere.browser()"#, "named"),
    ] {
        let failed = match open(&script(room)).map(drop) {
            Err(Error::Ended(Fault::Failed(failed))) => failed,
            other => panic!("{room}: expected the page to fail, it is {other:?}"),
        };
        assert!(failed.contains(why), "{room}: {failed}");
    }
}

#[test]
#[ignore = "heavy: compiles the interpreter and drives a page; make check-heavy"]
fn the_example_asks_for_its_first_address_when_the_browser_is_ready_and_says_what_it_was_told()
-> Result<(), Error> {
    let mut page = open(PAGE)?;
    page.browser_state(
        "web",
        BrowserState::with_status(BrowserState::STARTING, Some("The browser is starting.")),
    )?;
    assert!(page.browser_asks().is_empty());
    assert!(page.says("The browser is starting."));
    page.browser_state("web", BrowserState::default())?;
    assert_eq!(
        page.browser_asks(),
        [(
            Id::named("web"),
            BrowserAsk::Go("https://example.com/".to_owned())
        )]
    );
    Ok(())
}
