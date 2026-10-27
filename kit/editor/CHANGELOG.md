# Changelog

All notable changes to the `pencere-editor` crate are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate follows
[semantic versioning](https://semver.org/).

## [Unreleased]

The editor of version `2.0` of the kit (`pencere:view@2.0.0`, `lua` `2.0`).

### Added

- **The document.** `Buffer`: a text in a rope, places counted in characters, every place a page
  sends checked before it is used, the units of UTF-16 turned into places and back. Each change is a
  version; the steps of the last 1 024 are kept.
- **Changes of the page.** `Buffer::change(base, changes)`: a set made against an older version is
  carried over what was done since (each range's start after what came in at it, its end before),
  and done. What is not something it can do (a range the wrong way round, a place past the end,
  changes that overlap, a version too old or yet to come, a document too large) is refused, the page
  told why (`Note::Refused`), and nothing of it done.
- **Selections.** Several carets and ranges, in order, those that meet made one; a move up and down
  keeps the place across it started at (`Placement`).
- **Undoing.** Groups by kind and by time (typing that pauses longer than a second is a group of its
  own), the page's changes apart; a group undone is carried over what was done after it.
- **An input method.** `set_marked`, `unmark`, `insert`, `abort_marked`: what it composes is in the
  document, as one group to undo, and the page is told it composes (`Selection::composing`).
- **Commands.** Moves by characters as a person reads them, by words, lines, pages and to a matching
  bracket; deleting by character and by word; indent and outdent; lines moved, copied and commented;
  brackets and quotes that close themselves; a line break that keeps the indent; a caret added above
  or below; the next of the selected text selected; finding with words or regular expressions.
- **Decorations.** Layers the page names, each sent for a range in place of what the layer had there
  and carried over every change; regions that fold, which the person folds and unfolds and the
  editor carries over changes.
- **Loading.** A document loaded in pieces.
- **A copy of the text** for the page to read (`Snapshot`), which costs nothing that grows with the
  document.
- **The editors of a page** (`Editors`), as a host keeps them: one for each editor's room on the page
  laid out, its document kept while a room of its name is there, hidden or not; the page laid out
  with what they laid out (`Editors::measure`); each drawn in its place of the draw list
  (`Editors::paint`), its words in their own colours with the opacity of the room beside them
  (`Run::opacity`), so that a host's floor of contrast does not keep them from fading with it.
- **Layout.** Lines measured by the host (`Measure::offsets`), a tab to the stop the width of a space
  gives, broken after the last space that fits where the editor wraps; only the rows in view laid
  out; folded lines taking none; a gutter of line numbers, the page's marks and fold buttons; the
  page told the lines in view (`EditorEvent::Viewport`). A row is as tall as the page says, 1.6 times
  the font's size where it says nothing (the golden ratio editors of code read comfortably at): its
  text in the middle, the caret as tall as the text, the grounds (the caret's line, a selection, what was found)
  the whole row.
- **How it looks is the page's** (`EditorOptions`): the room around the text, the rows kept about
  the caret when it scrolls, a caret that is a bar, a block (its character drawn again on it) or a
  line under the character, line numbers counted from the caret's line (a folded region one line) or
  none, the caret's line drawn across the text, the gutter or both, and the colour of every ground,
  ring, number and button it draws, or none; what the page leaves out is drawn in the theme's
  colours.
- **The pointer.** A click puts the caret at the nearest boundary across; two select a word, three
  a line; shift selects to it, ⌘ adds a caret, ⌥ selects a column (on each line, from the boundary
  nearest one place across to the boundary nearest the other); a drag past the room scrolls it.
- **The keys.** In order: the page's command for a key, the editor's own (the keys of a Mac's text
  views, and those of editors for code), the text (`Route`): a character types with ⇧ or ⌥ held (⌥
  is how a Mac's keyboards type `@`, `[`, `{`, `|`, `~` on many of them), not with ⌘ or ⌃; none of
  the page's while an input method composes, every one the page's where the editor gives it its
  keys (`Keys::Page`).
- **Scrolling** is the host's: the room scrolls as a box does, over the content the editor says,
  and the editor asks for the caret to be kept in view (`Editors::take_scrolls`). Where the caret an
  overlay stands by moves with no frame of the page, the host lays the page out again as it is
  (`Editors::wants_layout`).
- **An input method's questions** in units of UTF-16 (`selected_utf16`, `marked_utf16`,
  `text_utf16`, `rect_utf16`, `set_marked_utf16`), so that no host turns places into them itself.
- **Events of the page.** What `Editors` tells is an `Event` of the page's (`From<EditorEvent>`); the
  example `examples/editor-lua` is a page of SQL that colours, folds and completes, run by the
  sandbox's tests against `Editors`.
