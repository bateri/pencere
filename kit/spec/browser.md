# Browser

## What a browser is

A **browser** (`pencere.browser(id, options)`) is the host's, as a text field and an editor are. The room of the element is where the host puts a browser. The browser:

- loads and draws web pages;
- and keeps the person's way of using them (scrolling, selecting, the pasteboard, the page's own keyboard; the page asks it to find, and draws the bar that does it).

The page gets nothing of what is loaded, not a document, a console or a cookie, only where the browser is and what it is doing.

The box's fill and border are the page's, and what the page puts inside the room is drawn over the browser.

## Its name and its life

A browser is named by the element's id (the element's own, or the `id` given), and an element that says it is one and has no name is not one.

The same `id` is the same browser:

- it lives as long as an element of that name is on the page, hidden or not (the host keeps its page and its history, and draws nothing while it is hidden);
- and it is closed, its history gone, when the page no longer has such an element.

## The options

`options` are all optional:

- `profile` is a name (1 to 32 of `a` to `z`, `0` to `9` and `-`) under which what the browser's sites keep (cookies, storage) is kept, apart from every other name's, so that a view of a program is a place the person signs in to and signs out of as of its own.
  - Where it is kept is the host's, and the page never reads it: on the computer that shows the page and, for a program that runs on a server, at that server too unless the person has chosen to keep none there, so that every computer that shows the program finds the same sessions (specification, Section 12).
  - The option is read when the browser is made: another `profile` under the same `id` closes the browser and makes another.
  - None given is a browser with a profile of its own that is gone when the browser is.
  - A name that is not one is the page's mistake, where `pencere.browser` is called.
- `windows` is what a link that opens a new window does:
  - `"here"` (the default) opens it in the same browser;
  - `"page"` has the host open none and tell the page (`browser_window`), which decides.
- `keys` is a list of keys written as a command's are (`{ "mod+l", "mod+t", "mod+w" }`, at most 32; none by default).
  - While the browser has the keyboard, the keys of this list are always the page's, told as `key` events. The browser, and the web page in it, never see them.
  - Every other key goes to the browser first. It comes to the page as a `key` event only where the web page and the browser have no use for it (an arrow in a page that does not scroll, ⌘← where the browser would go back): as a text field's and an editor's keys they have no use for do.
  - The browser's engine runs apart from the host, so such a key comes a frame or two after it was typed.

## What a page asks of a browser

Where a browser goes is not an option, since a page gives its options again at every frame and the person's links take the browser away from any address the page wrote: it is asked, in a reply (`browser = { [name] = ask, or { ask, ... } }`, [Events](events.md#what-a-reply-may-ask)).

A page asks for the first address when the browser says it is ready. The browser tells the page what it is and does in `browser` events.

What a page asks is:

- `{ go = address }` asks the browser to go to the address.
- The commands `{ command = "back" }`, `"forward"` and `"stop"`.
- `"reload"`: with `cache = false` the browser loads the page again from where it is, and not from what it keeps of it.
- `{ zoom = size }` shows the page at that much of its size.
  - The size is from 0.25 to 5, and a number outside it is taken to the nearest end.
  - The steps are the page's.
  - The zoom is kept for the pages the browser goes to.
- `{ command = "find", pattern =, case = }` marks what the browser finds and goes to the first match at or after where the page is scrolled to.
  - The pattern is plain text, in the case written where `case` is not `false`, as an editor's is.
  - An empty `pattern` ends the search.
  - A pattern longer than 1024 bytes or with a control character in it is refused, with the reason `"pattern"`.
- `"find_next"` and `"find_previous"` go round at the ends.
- `"stop_finding"` takes the marks away.
- `{ command = "devtools" }` opens the host's tools for looking into the page.
  - Their key is the host's: a page that shows a command for them shows the host's key and binds none.
- `{ command = "clear_cache" }` empties what the browser keeps to load pages faster, so that what the person is signed in with and what a page saved stay.
  - The command empties the files of every site the browser has been to, since the engine keeps them together.
  - The command empties the service workers and cache storage of the site the browser is at.
  - The command loads nothing again.
- `{ command = "clear_site_data" }` deletes what the sites of the name the browser is at keep, at every port (cookies, local storage, databases, service workers), so that the person is signed out of it.
  - A page that offers the command asks the person first.
  - The host does not say whether a profile is shared with other computers, so the page says no more than that the person is signed out of the site.
- `{ command = "proceed" }` goes on to the page the browser did not load for its certificate (`error.certificate`).
  - The host accepts that certificate for that site for as long as it runs, writes it nowhere and tells it to no other computer, and loads the page again.
  - The host cannot tell whether a person pressed anything, so a page asks only on the person's action, never of itself.
  - Where the last load did not fail for a certificate nothing happens.

An ask the kit does not know (a command a later kit has, or a table that is none of these asks) is not an error: it is refused, and the page is told (`browser_refused`, `"ask"`), so that a page that runs on a host with an older kit learns what it lacks.

## The status of a browser

The **status** of a browser is one of:

- `"ready"`: the browser shows pages;
- `"starting"`: the host is getting what it shows pages with ready;
- `"crashed"`: what it showed pages with ended, and a `reload` makes it again;
- `"no_route"`: the host cannot, or may not, reach where the page's program runs from this view;
- `"unsupported"`: the host shows no browsers.

The set is open, and a status a page does not know is a browser that is not ready.

In every status but `"ready"` the host gives a `message`, a sentence for the person (the page may show it, or write its own). The page, not the browser, says what it does about each.

A host that shows no browser draws a placeholder in the room and tells the page the status `"unsupported"` once for each browser it makes, so a page waits for no state that no host will give.

## The addresses a browser may go to

The page can only ask a browser to go where a person could not be hurt by it going:

- a `go` is an `http`, `https`, `about:blank` or `data:` address (printable ASCII with no space, at most 8 192 bytes);
- and any other address (`file:`, `chrome:`, `devtools:`, `view-source:`, `javascript:` …) is not given to the browser, and the page is told (`browser_refused`, `"scheme"` or `"address"`).

The reasons for a refusal are an open set (`"ask"` and `"pattern"` are others, above), and a page takes one it does not know as a refusal.

The host does not leave the allowed addresses where the page it shows goes elsewhere of itself (a redirect, a link): the engine runs on the person's machine and the page's program may not, so what a `file:` address names is the person's, not the program's.

## The keyboard of a browser

While a browser has the keyboard its keys are the browser's first, but two kinds:

- the ones the page listed in `keys`, which are always the page's;
- and the ones the browser has no use for, which become the page's (above).

The browser keeps the keyboard unless the reply sends it elsewhere (`keyboard = "page"`, or a field's, editor's or browser's name). A person's press in the room gives it to the browser, and a press elsewhere takes it back (`focus`, [Events](events.md#what-a-page-is-told)), as for a field.

A reply sends the keyboard to a browser, as to a field or an editor, with `keyboard = { field = name }`: the name is the element's.
