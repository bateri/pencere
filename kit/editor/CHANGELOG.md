# Changelog

All notable changes to the `pencere-editor` crate are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate follows
[semantic versioning](https://semver.org/).

## [Unreleased]

The editor of version `2.0` of the kit (`pencere:view@2.0.0`, `lua` `2.0`).

### Added

- `Pressing::Context`: the caret rule of a press that opens a menu (`Editors::selection_holds` says
  whether a place is in a selection), and `Editors::set_control_click` for a Mac's ⌃ with the main
  button, which is that press where the page's gestures say a right press is one.
- The page's `copy` and `cut` commands (`Editors::take_copied` gives the text for the host's
  pasteboard); a key the page binds to them does what the editor's own key does.

### Fixed

- A block caret that glides (`caret.motion`) shows the text under it from the first frame, as a
  terminal's block cursor does: it covered the letters it went over and the one it came to, and the
  letter showed only when the glide ended. The text it overlaps is drawn on it, cut at the box, in
  the caret's `text` colour on every frame.
- A caret's glow (`caret.glow = 1`) gives off as much light as bateri's own cursor halo: it reached a
  little more than half as far.

- An inline widget keeps `margin` points outside its box, between it and the characters beside
  it: the box of a hint stood against the character after it.
- A gap kept open for the page is as tall as the page said and its text follows at once: the rows
  that hold it are whole, what they hold more is taken back below, so that no band is left under what
  stands in it (the rows in view, the places, the ruler and the scrolling count in pixels).
- A mark on the overview ruler is as tall as the rows of its own lines: the gap kept open after its
  last line was counted as its.

### Added

- **Where a caret is drawn is the host's to move** (`Editors::paint` takes a function that is told where
  the carets of the editor that has the keyboard are, `CaretSpots`, and answers how far from there to
  draw each): a caret that glides. A block that is away from its place does not draw its character
  again on it.

- **How a caret looks** (`caret.radius`, `glow`, `unfocused`): the caret's box has round corners and a
  soft light (the box's `shadow`, with no offset, in the caret's colour), and away from the keyboard a
  block can be an outline. `Editors::paint` takes a `CaretPhase` (`Lit`, `Dark`, `Away`) in place of a
  `bool`, and `Editors::focused_caret` gives the options the host blinks the caret by.

- **Round selections** (`selection.radius`): each range of the selection is drawn as one shape over
  the rows it covers (`pencere_view::bands`), the corners that are open rounded and the turns
  inward filled with a small figure; 0, the default, draws the square box of each row as before.

- **A gap above a line** (`ZoneSide::Above`, a code lens): between the line before it that shows and its
  line, or before the first row; a line folded away has none.

- **Text that is not the document's in its lines** (`Mark::Widget`): an inline widget takes room in its
  line before or after the caret (`Row::caret_x`, `end_x`; the rows, the wrapping, the places, the
  selection and the carets go through them), one at a line's end stands beyond the text and is cut with
  "…"; the line's measure is kept with its widgets and made again only when they change; a group's
  `show` goes by the modifiers held. A widget with a name is what a rest and a press are told of, and
  the hand over it; `anchor` names its box.

- **The modifiers held** (`Editors::set_mods`) draw the looks that are `held` and `over`, and give the
  pointer's shape over them; the pointer over a word with the modifiers of a press that is the page's
  held is told once for each word and at most once in `links.delay` (`EditorEvent::Target`), and that
  press (`gestures`: `Pressing::Target`) moves no caret.
- **A press is taken as the page's `gestures` say** (place, extend, add a caret, column, word, line,
  nothing) and as the editor did where they say nothing; the editor's keys are as the page's `keymap`
  says (a key, or a chord that waits `chord_timeout`, `EditorEvent::KeymapPending`).

- **The pointer's rest over a word** (`Editors::rest`, `rest_due`, `tick`, `end_rest`): told once after
  `hover.delay`, and its leaving, to a page that asked (`EditorEvent::Hover`); a scroll, a key, a
  press, an edit or a drag ends it and none is told until the pointer moves again; `Editors::word_at`
  is the word by the rule a double click has.
- **Names for ranges** (`Mark::Anchor`, `Editors::anchors`, `Buffer::anchors`): the box of a range
  (from where it starts across its first row, from the top of that row to the bottom of its last) is
  where an overlay stands, moves with the text, and is not shown out of the room.
- The numbers keep room to their left where they are the first column of the gutter (`Column::pad`).

- **A gutter of named columns, markers in them, an overview ruler and zones.** The columns the page
  lists are laid out in its order (the numbers, the dots and the buttons that fold as they were
  where it lists none) and a marker is drawn in the column it names: a dot, a bar along every row of
  its lines, a triangle on the edge above or below a line, or a picture of the bundle, tinted; a
  marker in lines that are folded away is on the fold's line. A press in a column that hears it is
  told to the page (`EditorEvent::Gutter`) and selects nothing, folds nothing and moves no caret,
  where the cell is empty as well; `hover` and `cursor_at` say what is under the pointer, a bar the
  pointer is over is wider and lit, and a column's preview shows in an empty cell. The ruler is drawn
  down the right edge in its lanes, whatever the scroll bars do, and a zone opens a gap of whole rows
  after its line that the rows, the numbers, the scrolling and the ruler count and `Editors::zones`
  reports.

- **What an editor's standard keys do in CodeMirror and VS Code, and a page's changes out of the
  history.** The commands `FoldAll` (the outermost regions), `DeleteLines`, `SelectLine` (a range
  that is whole lines takes the next), `DuplicateLinesUp`, `JoinLines`, `InsertBlankLine`,
  `SplitLine`, `TransposeChars`, `DeleteToLineStart` and `DeleteToLineEnd`, none with a key of its
  own. `Buffer::change_as` makes a page's change with a `tag`, told back in its `Edited`, and, where
  not `history`, out of what is undone: what can be undone and done again is carried over it, so
  that a file read again keeps what was done to it before. `Editors` tells `EditorEvent::Ready` when
  it makes an editor, before anything else of it; a reveal goes where its `RevealAlign` says
  (nearest, start, center, end). The other places of the main selection's text are drawn where the
  editor's `matches` asks (`Buffer::matches_in`), under what was found and what is selected.
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
- **Finding, as a find widget needs it.** A search (`EditCommand::Find`) for words or a regular
  expression, in either case or the one written, and for whole words only (`word`: no letter or
  digit, of any script, and no `_` just before or after the match). It goes on as the document
  changes: an editor whose document changed since it searched searches again when it is next
  synced (`Editors::sync`, `Buffer::search_again`), once a frame however many keys came, what it
  found carried over the changes until then. What it finds is told (`Note::Found`,
  `EditorEvent::Found`): how many matches (at most `FIND_MATCHES`), which of them the main
  selection is on (from 1), whether there may be more; after every search asked for and whenever it
  changes, a count of 0 when the search stops, and nothing to an editor that does not search.
  `Replace` puts a text in place of the main selection where it is a match and selects the next
  (a regular expression's groups put in by `$1`, `${1}`, `$name`, `${name}`, `$$` a `$`; a plain
  search's text as it is), `ReplaceAll` every match of the document (not only the `FIND_MATCHES`
  a search marks) as one change undone as one, `SelectFound`
  selects every match and `SelectOccurrences` every place of the main selection's text, or of the
  caret's word (the one `SelectNext` takes) as a whole word.
- **What a page says of its language, done.** The comment commands take the page's
  `CommentTokens`: `ToggleComment` comments the lines of each range with the line comment, or, in a
  language of block comments alone, puts them in one (CodeMirror's `toggleComment`);
  `ToggleBlockComment` puts each range in the block comment (a caret's, its line's text) or takes it
  out, its tokens just outside the range or its ends, a space or a tab beside them taken too. The
  rules of the indent (`IndentRules`) are compiled once for each text the page says: a line break
  after a line that matches `increase` indents a level more (and puts what matches `decrease` after
  the caret on a line of its own), and a line that what is typed makes match `decrease` goes a level
  out from what the line above gives, undone with what was typed. `Folds::Indent` folds by the
  indent, besides the page's folds: `Buffer::fold_region` and `Buffer::fold_lines` work out what
  folds from a line when it is asked (the gutter's lines in view, a fold), never the whole document
  at a key, and so follow every change; a region folded by the indent stays folded while the page
  decorates. An editor that says none of them does as it did.
- **What is not seen, drawn.** `Whitespace` shows the spaces and tabs VS Code's `renderWhitespace`
  shows (`Boundary`, `Selection`, `Trailing`, `All`): a space as a dot in the middle of its place, a
  tab as an arrow as wide as it is, any other space as a box as wide as it is, the controls not at
  all. `indent_guides` draws a line a pixel wide down each level of indent, on a line with no text
  those of the block it is in (VS Code's rule), on the first row of a line that wraps and nowhere a
  fold hides; the guide of the block the main caret is in has its own colour while the editor has
  the keyboard. Both are worked out for the rows drawn, over the grounds and under the text, and an
  editor that says neither draws what it drew.
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
- **Layout.** Lines measured by the host (`Measure::offsets`), a tab to the stop the width of a
  space gives, broken after the last space that fits where the editor wraps; only the rows in view
  laid out; folded lines taking none; a gutter of line numbers, the page's marks and fold buttons;
  the page told the lines in view (`EditorEvent::Viewport`). A row is as tall as the page says, 1.6
  times the font's size where it says nothing (the golden ratio editors of code read comfortably
  at): its text in the middle; the caret (2 points wide, as Zed draws it) and the grounds (the
  caret's line, a selection, what was found) the whole row.
- **How it looks is the page's** (`EditorOptions`): the room around the text, the rows kept about
  the caret when it scrolls, a caret that is a bar, a block (its character drawn again on it) or a
  line under the character, as tall as its row or its text, line numbers counted from the caret's
  line (a folded region one line) or none, the caret's line drawn across the text, the gutter or
  both, and the colour of every ground, ring, number and button it draws, or none; what the page
  leaves out is drawn in the theme's colours.
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
