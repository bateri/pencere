# The pencere kit: the page API, colours and sandbox floors

The kit of the kind `lua` has one file for each topic, listed here. [The Lua page API](spec/page.md) says what the document is and how to read it.

## The page

- [The Lua page API](spec/page.md): What a script returns, what a page can use, and how a call of the page API fails.
- [Elements](spec/elements.md): The elements a page is made of and the builders that fill them: boxes, text, fields, tips, overlays, rich text and gradients.
- [Canvas](spec/canvas.md): Paths drawn in a box: strokes, fills, gradients, shadows and dashes.
- [Accessibility](spec/accessibility.md): Labels, roles and groups for a screen reader, and `pencere.has`, which asks whether the kit has a thing.
- [Colours](spec/colours.md): How a colour is said, the theme's roles and the derived tones.
- [Counting](spec/counting.md): How a page counts places in a text: bytes, lines, and what a number means where it is a place.

## Behaviour

- [Events](spec/events.md): What the host tells a page, event by event, and the replies that answer them.
- [Replies](spec/replies.md): What a reply asks of the host: fill a field, open a link, take files, pick a file, wake the page, draw larger.
- [Commands and dragging](spec/commands.md): Commands, what can be dragged, and what can be carried out of the view.
- [Motion](spec/motion.md): How a change of an element moves instead of jumping: looks, `auto_size` and transforms.
- [Layout and scrolling](spec/layout.md): What a page remembers between frames, what it asks of the layout, timers, scrolling and zooming.

## Editor and browser

- [The editor](spec/editor.md): The host's editor of text: its language, its look and its options.
- [Marks, zones and searches](spec/editor-marks.md): What a page puts on an editor: decorations, markers, zones and widgets; its keys, its commands and its searches.
- [Browser](spec/browser.md): The host's web view in a room of the page: its options, what a page asks of it and what it is told.

## Library

- [The kit's library](spec/library.md): The modules a page requires from the kit: syntax, languages, fuzzy finding, keys, Markdown, the terminal's look, components and charts.

## The program

- [The page and its program](spec/program.md): How a page talks to its program, what `print` and `warn` do, and the other scripts of a bundle.

## Versions and hosts

- [Versions](spec/versions.md): The version of the kind `lua`, what it pins and when it changes.
- [What a host does for a kit view](spec/host.md): What a host gives a view at least: the sandbox floors, what it draws and does for a page, and the pictures.
