# Counting

A page counts what it is told and what it says by one rule, wherever a number is a place in a text.

## A place is a byte

A place is a byte of the text's UTF-8, the unit of a Luau string. So what `string.find` returns is a place of the editor with no `utf8` call between them.

A place is counted **from 1**: the first byte is place 1, and the end of a text of `n` bytes is place `n + 1`.

A place must be where a character starts (or the end). A place inside a character is an error at the line of the script where a call reads the text (`layout.editor_text`, `line_of`, `word_at`), and an ask that names one is refused (`editor_refused`, `"mid-character"`). It is never rounded to a character, which would hide the mistake.

A place of an older version (`base`, `version`) is carried over what was done since as bytes and only then checked. Where text deleted since swallowed it, it is carried to the deletion and cannot be told from one that was a character's start.

The lengths the interface tells are bytes (`editor_length`).

## A range is two places

A range is two places, `[from, to)`. The end is not in it, so that `string.sub(text, from, to - 1)` is what the range holds, and an empty range (`from == to`) is a place text is put in.

A match that `string.find` reports as `i, j` is the range `i, j + 1`.

## A line

A line is counted from 1, as a place is: `line_start`, `line_of`, the `line` of a `gutter` or a `hover` event, and the `first` and `last` of a `viewport`.

So is the index into a list the page keeps: `main` of a selection, `current` of a search.

## Where it applies

What speaks of places says "Counting" and means this:

- the editor's events and asks ([The editor](editor.md) and [Events](events.md#what-a-page-is-told));
- a field's `select`;
- the places of the kit's library ([The kit's library](library.md)).

What counts something else says so:

- the points of a layout;
- the offsets of a list's rows (`Rows`, in points from 0);
- the columns of a line, which a screen draws and a page counts in characters or in cells.
