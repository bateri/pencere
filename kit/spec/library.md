# The kit's library

`require("@pencere/<name>")` is a module of the **kit's library**: Luau that the kit writes on this page API alone ([the topics of the page API](../SPEC.md), and nothing of the interpreter's besides), at the kit's version, for what every page would otherwise write again.

A host that runs the kit's interpreter MUST give a script the library of its own kit's version; the kit's sandbox does (`pencere_sandbox::library`). A page needs nothing else to use it, and MAY write its own instead.

- The alias is `@pencere`, in any letter case, as Luau's aliases are.
- A module of the library requires another with `@pencere/`, `./` or `@self/`, as the scripts of a bundle require one another, and reaches no script of the bundle: a path past the library's root is an error.
- A script of the bundle reaches the library through the alias alone, as no path of a bundle has an `@` in it.
- A module made of several files has the others as its parts (`@pencere/languages/sql`).
  - A script of the bundle cannot require them (`@pencere/languages` is the module, and the only name it has); only the library's own modules can.
  - What they are may change with any version of the kit.
- A name the library has nothing at is an error, raised at the line of the `require`, that says so (`the kit's library has no module "x"`).
- A module runs once in a page, as a script of the bundle does.
- An error at a line of a module says the module and the line (`@pencere/syntax:12: …`). An error a module raises for what its caller gave it says the caller's line.
- The modules do not count toward the size of a script ([What a host does for a kit view](host.md): 4 MiB). The kit holds them to 1 MiB together.
- The kit's parts are in four layers: the host, the prelude, the library and the app ([`README.md`](../README.md), "Where a part of the kit goes").
  - What a layer uses is below it:
    - a module of the library is what an app could have written on the page API, and requires no script of an app;
    - the prelude requires no module of the library.

## `@pencere/syntax`

A language that colours an editor is the page's, and `@pencere/syntax` makes one the way CodeMirror's `StreamLanguage` does.

### Defining a language

`Syntax.define { name =, start =, token =, copy =, equal =, blank =, styles = }` (with `local Syntax = require("@pencere/syntax")`) reads a line a token at a time:

- `token(stream, state)` takes at least a character from the stream and says the name of its style, or nil.
- The state carries what a line leaves open to the next:
  - `start()` is the state before the first line;
  - `copy` and `equal` copy and compare two states, by default tables all the way down;
  - `blank(state)` is told of a line with nothing on it.

### The styles of a language

`styles` gives a name a look, a decoration's (`color`, `background`, `weight`, `italic`, `underline`, `strike`):

- a name may be several apart (`"keyword strong"`, the later's look over the earlier's);
- a dotted one falls back to what is before its last dot (`"string.special"` to `"string"`);
- and one no style says is not coloured.

### The stream

The stream is the line, its `\r` left out (`string`), and where it is (`pos`, and `start` where the token began, bytes from 1). It has:

- `sol()`, `eol()`, `peek()` and `next()` (a character is a whole UTF-8 sequence);
- `eat(match)` and `eat_while(match)` (a character is itself, a longer string a Lua pattern the character matches, a function says yes or no);
- `eat_space()`, `skip_to_end()`, `skip_to(text)`;
- `backup(count)` (characters);
- `column()` and `indentation()` (a tab to its next stop);
- `match(pattern, consume, case_insensitive)` (a Lua pattern read where the stream is: the text matched or its captures, taken unless `consume` is false);
- and `current()`.

A `token` that takes nothing fails the page, as any error of the language's does, in words that say the language's name and the line (`sql: line 12: …`).

### The tokens of a text

- `language:tokens(text, state, tab_width)` is a line's tokens, `{ from =, to =, name = }` in bytes from 1 with the end not in the range ([Counting](counting.md)), so that `string.sub(text, from, to - 1)` is the token, for a text that is not in an editor.
  - `state` is the state before the text, `start()`'s where nil.
  - `tab_width` is what a tab is to the stream's columns, 4.
- `language:look(name)` is the look its styles give a token's name (a copy; nil for none), found as the syntax finds it.

### Colouring an editor

`Syntax.new(language, id, { layer =, step =, margin =, tab_width = })` colours the editor `id` with the language.

A syntax keeps the state before every 32nd line of the document (a checkpoint) and before each line of the window it colours, and no other, so that what it holds grows with a document 32 lines at a time. A line whose state it does not keep is read from the checkpoint before it.

The syntax is given every event of the page (`syntax:handle(event)`):

- an `edit` makes it read again from the first line the change touched (from the checkpoint before it, where that line's state is not kept) until a line whose state is kept ends in the state it ended in before;
- a `viewport` says what to colour;
- for those two it does a call's work at once;
- any other event (the pointer moving, a key) costs it nothing, what is left being its timer's.

The syntax colours the lines in view and `margin` lines on each side (as many as are in view) in the layer `layer` (`"syntax"`), a line again only where it or the state before it changed, and takes away what it coloured out of them.

### What a call reads

- One call reads at most `step` bytes (20 000), the lines to colour first, and a line to its first 10 000 characters (the state after it is the state there).
- The syntax holds at most 10 000 decorations for the page, half of what a reply may hand the editors (a line that would take it past that waits for the next call).

Where the lines in view are further on than a call reads to:

- they are coloured from the state the language starts in;
- and the calls after read on to them and colour again what that got wrong.

### What it asks of the editor

What a syntax asks of the editor goes in the page's reply (`syntax:reply(reply)`, its asks before the page's own; or `syntax:asks()`).

While there is more to do (`syntax:busy()`) the syntax goes on by itself, a call a frame:

- a timer of its own (`pencere.time.after(0, …)`) wakes the page;
- the syntax does the next call;
- and hands what it found to the editor with the wake's reply, so that a large document is coloured to its end with no event of the person's.

`syntax:work()` does a call at once, for a page that wants one.

### Ending a syntax

`syntax:close()` ends the syntax for a page that colours the editor another way from then on (the language of the file changed):

- its timer is cut;
- `handle`, `work`, `reply` and `asks` do nothing after the close;
- and it returns the list for the reply's `editor[id]` that takes away what it coloured (one ask that empties the layer; an empty list where it had coloured nothing, and from a second `close`).

That list goes out in a reply before the colours of the syntax that takes its place. That syntax has not seen the viewport the editor told, and is given it (`syntax:handle(event)` with the last `viewport` event) before it colours anything.

## `@pencere/languages`

The languages an editor reads are the page's, and `@pencere/languages` (with `local languages = require("@pencere/languages")`) knows the kit's as CodeMirror's `@codemirror/language-data` does.

A **description** of each says what it is called and how a file in it is told, and costs nothing to keep. The **language** is built from one only when a page loads it.

### A description

A description is a frozen table:

- `name`;
- `aliases`, its other names;
- `extensions`, what a file's name ends in after its last dot;
- `filenames`, whole names of files in it (`"Makefile"`, `".bashrc"`);
- `patterns`, Lua patterns a file's whole name in lower case is in it by (`"^dockerfile[%.%-]"`);
- `interpreters`, the programs a `#!` first line runs a script in it with, without their version (`"python"`);
- and `first_line`, a Lua pattern the first line of a file in it matches (`"^%s*<%?xml"`), or nil.

Names, aliases, extensions and file names are matched in any letter case.

### The kit's languages

The kit's languages are C, C#, C++, CSS, Diff, Dockerfile, Go, HTML, INI, Java, JavaScript, JSON, JSX, Kotlin, Less, Lua, Makefile, Markdown, PHP, PostgreSQL, Python, Ruby, Rust, SCSS, Shell, SQL, SQLite, Swift, TOML, TSX, TypeScript, XML and YAML. None is plain text: that is the page's word for a file in none of them.

### Finding a language

- `languages.list()` is every description, sorted by name in any case, a new list each call.
- `languages.find(name)` is the description whose name or one of whose aliases is `name`, in any case, or nil.
- `languages.detect(path, first_line)` is the description of the file at `path` (its whole name, after the last `/` or `\`; nil for a document with no name) whose first line is `first_line` (or nil), by:
  - its whole name (`filenames`);
  - then `patterns`;
  - then its extension;
  - then its first line: the program of a `#!` line (the one `env` is given, past its options and settings; `python3.12` is `python`), then `first_line`.
  - The result is nil where none of them says a language.
- `languages.load(which)` is the language `which` says (a name or an alias in any case, or a description), built the first time it is asked for and the same table after; nil for a name the module does not know.
- `languages.loaded(which)` is the language where it has been built, nil otherwise: for a page that would not build one in a call that is short of time.
- An argument that is neither a name nor one of the module's descriptions is an error at the caller's line.

### A language

A language is a frozen `{ name =, description =, syntax =, config = }`:

- `syntax` is a language of `@pencere/syntax`: `Syntax.new(language.syntax, id)` colours an editor with it, and `language.syntax:tokens(line, state)` a text that is not in one.
- `config` is what an editor is told of the language, in the editor's own options ([The editor](editor.md#an-editor-and-its-language)):
  - `comment`, `indent` (in Rust's syntax), `folds` (`"indent"`), `pairs`;
  - and `hard_tabs` and `tab_width` only where the language or its one formatter fixes how it indents (Go and the Makefile, tabs; Rust and Python, four spaces; YAML, two spaces and no tabs).

A page gives `config` to an editor as it is (`pencere.editor(id, language.config)`), its own options in a copy (`table.clone(language.config)`). A page that gives the editor another `tab_width` gives `Syntax.new` the same.

### Loading a language

Requiring the module builds no language: it reads its descriptions alone, and the module of readers a language is built with is required when it is first loaded.

A language that holds another loads the other by name, through the module, when it first meets it:

- HTML loads its scripts and styles by the tag's `lang` or `type` (`lang="ts"`, `type="module"`, `type="text/css"`), a script being read to the tag that closes it, on its own line too;
- Markdown loads the code in a fence, by the first word of the fence's info.

A held language the module does not know is read as text.

A reader takes at least a character a token and does work in proportion to the line. Where nothing is open at a line's end, a reader leaves the empty state `{}`, which the lines after it share.

### SQL

SQL is three descriptions that share one reader.

- `PostgreSQL` and `SQLite` each read the characters the two databases read apart as its own does:
  - `[` (a subscript or `ARRAY[…]` in PostgreSQL, a quoted name in SQLite);
  - `?` (an operator of `jsonb` in PostgreSQL, a parameter in SQLite);
  - and `@name`, `$name` (parameters of SQLite).
- `SQL`, for a file that does not say its database, reads the forms of both:
  - a `[` right after a name or a bracket is a subscript and any other opens a name;
  - `?|` and `?&` are operators and any other `?` is a parameter.
- Each of the three reads:
  - `--` and `/* */` comments (nested in PostgreSQL);
  - strings in `'` over lines (`''` a quote in one);
  - `E'…'` and `X'…'`;
  - `$tag$…$tag$` (PostgreSQL's and SQL's);
  - quoted names in `"` and in `` ` `` (SQLite's and SQL's);
  - `$1` and `:name` parameters and `::` casts.
- `#` is no comment: PostgreSQL's `#>` is an operator.

### The looks of the tokens

`languages.styles` is the one table of looks every language names its tokens from (a look is what `Syntax.define`'s styles say one is), frozen, in the theme's roles:

| Tokens                                                       | Look                             |
| ------------------------------------------------------------ | -------------------------------- |
| `keyword`, `heading`                                         | `accent` (`heading` bold)        |
| `keyword.modifier`, `variable.special`                       | `accent`, italic                 |
| `string`, `code`, `inserted`                                 | `success`                        |
| `number`, `atom`, `constant`, `attribute`, `list`, `changed` | `warning`                        |
| `type`                                                       | `info`                           |
| `link`, `url`                                                | `info` (`link` underlined)       |
| `tag`, `deleted`                                             | `error`                          |
| `invalid`                                                    | `error`, a wavy underline        |
| `comment`, `quote`                                           | `dim` toward `quiet`, italic     |
| `punctuation`, `operator`, `meta`                            | `foreground` toward `background` |
| `function`, `property`, `string.special`, `escape`, `regexp` | mixed from two roles             |
| `emphasis`, `strong`, `strike`                               | the face's alone                 |

`languages.look(name)` is the look a token named `name` has by `styles`, as `language:look` finds it ([The kit's library](library.md#penceresyntax), `@pencere/syntax`), or nil. It is for a page that colours code that is not in an editor.

## `@pencere/fuzzy`

What a person types, found in texts the way a picker, a palette or a filter finds it (`local fuzzy = require("@pencere/fuzzy")`).

### What matches

A pattern is in a text where each of its characters is there, in order, with anything between them. A space in the pattern is not looked for.

### How a match scores

Of the ways a pattern is in a text, the one that scores best is taken, as fzf's second algorithm scores it:

- a character scores more at the start of the text and of a word (after a space, after one of `/ \ : , ; |`, after any other character that is not a letter or a digit, a capital after a small letter, a digit after what is not one);
- a run scores more than the same characters apart;
- a gap costs;
- and a first character late in the text costs a point for each character before it, at most five.

### Matching one text

`fuzzy.match(pattern, text, options)` is the score and the places of the pattern's characters, or nil where it is not there.

- The places are bytes from 1, in order, the byte each character starts at, as in [Counting](counting.md); in a text that is not UTF-8 they are a byte a character.
- A text is matched in its first 64 KiB.
- An empty pattern is in every text, with a score of 0 and no places.
- `options.case` is `"smart"` (the default: a capital is told from its small letter only where the pattern has a capital), `"ignore"` or `"respect"`.
  - A letter is lowered by Unicode's simple mapping for ASCII, Latin-1, Latin Extended-A, the letters in pairs of Latin Extended-B and Latin Extended Additional, Greek, Cyrillic, Armenian and the full-width Latin letters, and any other is compared as it is.
- With `options.path` the text is a path:
  - a `/` and a `\` are one character;
  - a pattern with a `/` in it is matched against the whole path;
  - and any other pattern, where it is in the path's last part (the name; a separator at the end left out), scores above any match that is not.
- `options.highlight` says which places (the score is the same):
  - `"best"` (the default), those that gave the score;
  - or `"first"`, where the pattern is first all there, from as late a first character as that allows.
- A match whose table of scores would pass 32 768 places is scored along the places `"first"` gives.

### Filtering a list

`fuzzy.filter(pattern, items, options)` is the items the pattern is in, `{ { item =, index =, score =, positions = }, ... }`:

- a higher score first, and of two the same the earlier item;
- with an empty pattern every item in order.

Its options are those of `match` and two of its own:

- `text` (an item's text where the items are not strings: a function of the item, or the name of its field; a number is its digits);
- and `limit` (how many to keep at most: the best, kept as they are found).

The call does all at once.

### Searching a step at a time

`fuzzy.search(pattern, items, options)` (the same options) goes through the items a list holds when it is made a step at a time:

- `search:step(budget)` goes through at least one item, until what it compared (bytes of text and places of the tables of scores) comes to `budget` (60 000 unless said, some milliseconds), and says whether it is over;
- `search:results()` is the best so far, in order (a list without a `limit` is sorted again as it grows: a long one is searched with a limit);
- `search:progress()` is how many items it went through and of how many;
- `search:done()` is whether it goes no further (it went through every item or was cancelled);
- `search:cancel()` stops it, keeping what it found;
- `search:run(on_progress)` does a step at once and returns what `on_progress(search)` returns for it (a reply, for the page to answer with). It then goes on by itself, a step a frame: a timer of its own (`pencere.time.after(0, …)`) wakes the page, does a step and tells `on_progress`, whose reply is the wake's, until it is done or cancelled;
- `search:narrow(pattern)` is a search for `pattern` in its place, with its items and options, and cancels it:
  - where every text the new pattern is in, the old one's is in as well (its characters in the new pattern's in order, told apart in case no less), it goes only through what the old one found and the items it had not gone through.

An error a list's item gives (no text) is the caller's, at its line; in a step `run` goes on with, the page's.

### Marking the places

`fuzzy.spans(text, positions, look, from, to)` is the text as parts of a text of many looks (`pencere.rich`):

- the characters at the places in `look`, what a `pencere.span` says (`color`, `weight`, `italic`, `underline`, `strike`, `family`, `bg`, `link`; `{ color = "accent", weight = 700 }` where none is given);
- the rest in the element's own look.

The places are bytes from 1, as `fuzzy.match` gives them.

`from` and `to` (a range `[from, to)` of bytes, `1` and `#text + 1` where left out) take a part of the text, the places counted in the whole:

- places outside the part are passed over;
- and one inside a character of a text in UTF-8, as an end of the part is, is an error at the caller's line.

The call makes at most 1 024 parts, the rest plain in the last.

## `@pencere/keys`

Keys as a page writes them (`"cmd+shift+k"`, [Commands and dragging](commands.md)), as a person reads them (`local keys = require("@pencere/keys")`).

### Reading keys

`keys.parse(words)` is `pencere.keys(words)` with the key checked and named as a key event names it (`"page_up"`, `"delete"`, `"enter"`, `"escape"`, `" "`, `"f1"` to `"f20"`, or one character, a letter in its small form: `"CMD+Shift+K"` is the key `"k"`), or nil and why for words that are not keys.

### Caps and labels

- `keys.caps(words, options)` is a cap for each key (`{ "⇧", "⌘", "K" }`).
- `keys.label(words, options)` is the label: on a Mac the caps with nothing between (`"⇧⌘K"`), elsewhere joined by `+` (`"Ctrl+Shift+K"`).

`words` are words or what `keys.parse` made; words that are not keys are an error at the caller's line.

### On a Mac

A Mac writes the modifiers in the order ⌃ ⌥ ⇧ ⌘ and the arrows `↑ ↓ ← →`.

A Mac writes the other keys that are not characters as words, as any other platform does (`Enter`, `Esc`, `Tab`, `Backspace`, `Delete`, `Space`, `Home`, `End`, `PageUp`, `PageDown`), unless the page asks for their glyphs (`options.symbols`, below):

- `⇞ ⇟` (page up, down);
- `↖ ↘` (home, end);
- `↩` (enter);
- `⎋` (escape);
- `⇥` (tab);
- `⌫` (backspace);
- `⌦` (delete);
- `␣` (space).

### On any other platform

Any other platform writes `Ctrl`, `Alt`, `Shift`, `Super` (`Win` on Windows) in that order and `Up`, `Down`, `Left`, `Right`, `PageUp`, `PageDown`, `Home`, `End`, `Enter`, `Esc`, `Tab`, `Backspace`, `Delete`, `Space`.

### Function keys and letters

A Mac and any other platform both write a function key `F5` and a letter as a capital (ASCII, Latin-1, Latin Extended-A, Greek and Cyrillic; another character as it is).

### The command key and the main modifier

`cmd` is the command key, matched as it is and never as control: elsewhere than on a Mac it is the system's logo key, `Super` (`Win` on Windows).

`mod+…` is the host's main modifier ([Commands and dragging](commands.md)): `⌘` on a Mac, `Ctrl` elsewhere.

### The symbols and the platform

- `options.symbols` is whether a Mac writes the glyphs of the keys above (false where it is not given, unless `keys.set_symbols(true)` said so for the page; `keys.symbols()` says which).
- `options.platform` is `"mac"`, `"windows"`, `"linux"` or `"other"` (a platform a page does not name: written as Linux is).
  - Where it is not given it is `keys.platform()`, which `keys.set_platform(name)` sets for the page and which is the host's keyboard convention, `pencere.keyboard()` ([Commands and dragging](commands.md)), or `"mac"` where the page API says none.
  - Windows writes the command key `Win`, Linux and `"other"` `Super`.

### Matching a key event

`keys.matches(event, words)` is whether a key event (`key` or `key_up`, or a table with no `type`) is those keys:

- the same modifiers, no more and no fewer;
- and the same key, a character in either case, as the host matches a command's keys.

An event of another type is not those keys.

## `@pencere/markdown`

`@pencere/markdown` (with `local markdown = require("@pencere/markdown")`) reads Markdown and draws
it. The dialect is CommonMark (0.31.2) with GitHub's extensions (GFM): tables, their columns aligned
as the delimiter row says (`\|` a pipe in a cell); task lists; strikethrough (`~` or `~~`); and the
links that need no brackets (`www.…`, `http://…`, `https://…`, `ftp://…`, an address of mail). HTML in
the text, a block of it or a tag in a line, is kept as the text it is and drawn so, never run. A
reference to a character is the character HTML names (every name of WHATWG's list, and numbers).

`markdown.parse(text)` is the tree of the text, read at once:
`{ type = "document", children = { block, ... } }`. Every block says the lines of the text it came
from: `line` (from 1, [Counting](counting.md)) and `lines`. A block is a `heading` (`level`, `children`), a `paragraph`
(`children`), a `quote` (`children`), a `list` (`ordered`, `start`, `tight`, `children`: its items),
an `item` (`checked`: nil, or true or false for a task; `children`), `code` (`language`: the first word
of its fence's info, or nil; `info`; `text`, its last line break left out), `html` (`text`), a `rule`
or a `table` (`align`: a list of `"left"`, `"center"`, `"right"` or false; `head`: a list of cells;
`rows`: a list of lists of cells; a cell is a list of inlines). An inline is `text` (`text`),
`emphasis`, `strong` or `strike` (`children`), `code` (`text`), a `link` (`href`, `title`,
`children`), an `image` (`src`, `title`, `alt`: its description as plain text), `html` (`text`), a
`softbreak` or a `hardbreak`. An address is written as commonmark.js writes it: a byte an address may
not hold, or one that is not printable ASCII, is `%XX`.

`markdown.render(document, options)` is the element of a document, a column of its blocks: a
paragraph a text of many looks that wraps; a heading such a text, bold, in the size `headings` gives
its level (`{ 22, 18, 16, 14.5, 13.5, 13 }`), with the role of a heading; a list a column of rows, a
marker and the item's blocks (a task's marker a checkbox); a quote its blocks beside a bar; a table a
grid with a column for each of its columns (at most 64); a block of code a box in the terminal's
font; HTML a box of its text; a rule a line. Its `options` are each optional: `id`, the start of the
names of its elements (`"markdown"`); `colors`, roles of the theme: `text` (its words; the box's
where nil), `quiet` (`"dim"`), `link` (`"accent"`), `code` (`"raised2"`), `head` (`"raised1"`),
`quote` and `rule` (`"border"`); `gap`, the points between blocks (12); `size`, the size of the text
the page sets, for what it works out before the layout (13); `link`, `code`, `image` and
`languages`, below; and `limit`, the most elements it makes (10 000), what is past it left out and
an "…" in its place. An option it has no such name for, or one of the wrong kind, fails the page at
the line of the page's call. Two draws on one page have different ids.

A link is a part of a text named `<id>/link/<n>`. Given the page's events, `markdown.handle(event)`
answers a click on a link (the left button pressed and let go on it) with what `link(href)` returns:
by default `{ link = href }` for an `http` or `https` address, nil for any other; it answers nil to
every other event. `markdown.href(id)` is the address of the link of that name, for a page that finds
what was clicked its own way. An image is its description, as words of the text, since a page reaches
only the pictures of its bundle; where `image(src, alt, title)` is given, an image is the element it
returns, and the text around it stands in a row that wraps. A block of code is the element
`code(language, text)` returns; where that is not given, the block is coloured line by line by the
language `languages` finds (`find(word)`, then `load(description)`, which returns `{ syntax = }`, as
`@pencere/languages` does; by default that module, looked for when code is first drawn), a token in
the look `syntax:look(name)` gives. A draw colours at most 20 000 bytes of code and draws the rest
plain.

`markdown.document()` is a document kept for a text that changes.

- `doc:set(text)` reads again what changed. It starts at the block before the first line that
  changed, where that line may go on with that block (the line right after it, or, after blank lines,
  a list or code indented; a table whose header was the last line of a paragraph is read with that
  paragraph), and otherwise at the line after that block. It reads until, in the lines it did not
  change, a block begins at a line where one began before; the blocks after that are the ones it had,
  moved by the lines the change added or took. A change inside a fence, a list or a quote so reads
  that block again, and no more; where a definition of a link changes, the blocks that looked up its
  label have their inlines read again.
- One call reads at most 20 000 bytes (a line and a block cost besides their bytes). The rest is read
  by a timer of its own (`pencere.time.after(0, …)`), a call a frame, each asking for a frame, while
  `doc:busy()`; `doc:work()` does a call's work at once. Until it is done, `doc:tree()` is the
  document as far as it is read; `doc:reread()` is how many lines and blocks the last change was read
  in, so far.
- `doc:render(options)` is `markdown.render`'s element of it: a block that is as it was, drawn with
  the same options, is the element it was. Code left plain for want of what a draw colours is
  coloured in the draws after, and a frame is asked for each.
- `doc:list(id, options)` is a box named `id` that scrolls and holds the blocks as a long list
  (`box:list`, [Layout and scrolling](layout.md)), which the page sizes with the builders after it
  (`doc:list("preview"):size_full()`). A block is a row named `<id>/block/<n>`, as tall as it is
  likely to be until the layout says; `doc:settle()`, which the page's `settle` returns, reads how tall
  the rows made came out, and is true where one changed. `doc:reveal(line)` scrolls the list to the
  block that holds line `line` of the text, and again as the layout says where that block stands. A
  document's id is `markdown<n>` unless the options say otherwise.

## `@pencere/terminal`

Editors that look as the person's terminal draws its cursor and its selection
(`local terminal = require("@pencere/terminal")`). The host says how it draws them
(`pencere.terminal_look()`, [Layout and scrolling](layout.md)) and applies none of it; this is for a page that wants its
editors to take it where the page says nothing.

- `terminal.editor(options, look)` is a copy of `options` (an editor's options; none: `{}`) with each
  field of `look` that the page did not write put in it, a field at a time: `caret` and `selection`
  are merged, not replaced, so that a page that writes `caret = { shape = "bar" }` keeps its bar and
  takes the rest of the terminal's. What the page wrote stays, `false` and `0` included. `look` is
  `pencere.terminal_look()` where none is given (a page that shows the look itself, a settings view,
  gives its own); a host that says nothing gives `options` back as they were. A table that is not
  options or a look, or an `options.caret` that is not a table, is an error at the caller's line.
- A page calls it from `render`, so that the options follow the setting: it asks again when the page
  is told `terminal_look`. A page in Rust (`pencere_page`) asks `terminal_look()` and puts it in the
  options with `TerminalLook::fill`: a field of the options at the editor's default is taken for one the page
  said nothing of (a Rust page cannot tell a default it wrote from one it left, as a Lua page's table can).

## `@pencere/ui`

The components of a page (`local ui = require("@pencere/ui")`), written on the page API alone: a
component is a method of a context that the page makes once (`local u = ui.new()`) and calls in
`render` with a name and its props, and what the person does with it comes back from `handle` as
**actions**. The context sees to what the hand did (`pencere.Ui`, the keyboard's place, the layers in
front); the page keeps what it would miss if it were lost (`props.state`, a table the component changes in
place, or, where none is given, one the context keeps by the name for as long as the component is drawn).

- `u:begin(size, now, reduce_motion)` and `u:finish(root)` (`root`, `animating`, `wake_at`: what `render`
  returns) bracket a render; `u:settle()` is `settle`'s; `u:handle(event)` is the reply, the actions
  (`{ from = name, kind = …, … }`) and whether a component took the event, and the page does the
  rest of the event where none did.
- Components are drawn in the theme's roles and a few sizes (`ui.new({ theme =, density = "regular" |
"compact" })`), never in values; the host keeps their words readable. A component inside another
  says its action to the one around it first, which may take it for its own; one it does not take is
  the page's.
- A `popover` stands by an element (`anchor = "name"`), by the caret of an editor
  (`anchor = pencere.caret_of(editor)`) or by a range of its text (`anchor = pencere.range_of(editor,
name)`), and says `{ kind = "dismiss" }` at what `dismiss` lists: `"escape"`, `"away"` (a press outside it
  that is not on its anchor, a click into a field outside it among them) and, by an editor, `"edit"`, `"caret"` (the selection moved), `"scroll"` (the
  lines in view changed) and `"leave"`; by an editor all of them, by an element the first two. `"leave"`
  waits `hide_delay` (0.3 seconds): the card stays if the pointer came onto it or rested on another word
  in that time (the page forwards the editor's `hover` events and its `wake` to `u:handle`, as it
  forwards every event). `focus = true` writes the commands `up`, `down`, `page_up` and `page_down`
  (while it is open) which scroll the card's body, and the keyboard stays in the editor. A card of a
  range that is not in the room is not shown. `step` (40 points) and `page` (0.9 of the card) say how far
  the keys of a card that has the focus scroll it, `max_height` (320 points) how tall it grows. A card
  does not close of itself when the pointer was on it as `hide_delay` ended and goes off it later: it goes at
  what else `dismiss` lists.
- `u:editor_menu(id, { editor =, at =, read_only =, before =, after = })` is the menu of an editor, the same
  in every editor of every page: the reply that asks the host for Cut, Copy, Paste, Select All, Toggle Line
  Comment and Select All Occurrences, each with the keys the editor has for it now (`layout.keymap`, as the
  page changed them), the page's own lines before and after (`before`, `after`, `"-"` for a rule). The page
  opens it from the `press` of a right button in an editor written with `gestures = { ["right+press"] =
"context" }` (`at` is the press's `x` and `y`), or, without `at`, under the caret for the keyboard
  (`ui.EDITOR_MENU_COMMAND`, Shift F10, is the command of that). Copy, Cut and the rest are not greyed for an
  empty selection, as the keys are not (Copy and Cut take the caret's line); `read_only` greys Cut, Paste and
  Toggle Line Comment. Paste is the host's line (`paste = editor`): the page neither reads the pasteboard nor
  asks the editor to paste, and the line is on only in the menu a right press opens: in the one Shift F10
  asks for it is greyed. `u:editor_menu_pick(act)` is the reply that asks the editor for the line picked
  (nil for the page's lines and Paste).
- `button`, `icon_button`, `toggle`, `badge`, `kbd`, `rule`, `banner`; `input`, `segments`, `tabs`,
  `picker`; `modal`, `dialog`, `toasts`, `menu` (the host's), `editor_menu`, `popover`; `list`, `palette`,
  `completions`, `tree`, `grid`; `split`, `status`; `find_bar`. `ui.commands(list)` writes the page's commands once for the host, the
  palette and the help, and holds them while a modal is in front.
- `u:picker(id, { anchor =, items =, multi =, state =, label = })` is a card by the control that
  opened it (`anchor`, an element's name) with a field that narrows its items and the items under it,
  to choose one or, with `multi`, several, as a field of labels or the values of a filter does: the
  host's menu closes at every pick, the picker stays. Its `items` are
  `{ key, title, hint, icon, on, enabled }`, `on` what is chosen now, which the page holds. A row of
  one is `pick { item }` (and the page stops drawing it), a row of several `toggle { item, on }` with
  `on` what it is to become, each a click or enter; the keyboard stays in the field, enter selects what was typed, and escape or a press
  outside it that is not on its anchor is `dismiss`. `search = false` leaves the field out (the space
  bar chooses as enter does); `side`, `gap`, `align`, `width`, `rows`, `height`, `inset`, `hint` and
  `empty` place, size and word it.
- `u:menu(id, entries, { near =, at = })` is the reply that asks the host for a menu at an element
  (below its left edge) or at a place (`at = { x =, y = }`): where `near` is not laid out it opens at
  `at`, and where there is no `at` either there is no menu (nil); with neither given, it is an error. A
  press of the right button on a tab, a row, a cell or a line of a tree is the action `{ kind = "context",
… at = { x =, y = } }`, which the page answers with `u:menu`; what is picked in a menu comes back as
  `{ kind = "menu", entry = key }`. `list` and `tree` take `key_map = { j = "down" }` for keys of their
  own, and a list with `multi` says `mark { marked }`. A `split`'s edge and a `grid`'s column say
  `resize { size, final }` (`{ column, width, final }`): `final = false` while it is dragged, `true` when
  it is let go after it moved and at each step of the keys; a press that does not move it says nothing.
  A `grid`'s row is read by the words of its cells (its number first, no column's title; "Row N, loading"
  where none is read yet), and each cell apart from it in its column (`cell`, with the controls in it
  under it), so that a screen reader goes from cell to cell; `row_label(row)` gives the words to
  say it by where the page has better. A `grid`'s edge is a `slider` said "<title> column width", with its
  width in points, and a `tree`'s arrow a button said "Expand <name>" or "Collapse <name>".
- An `icon` that a component or one of its items takes is an element the page made, a path of the
  page's own bundle (`"icons/save.svg"`: an SVG or a PNG, tinted in the colour the component draws it
  in), or the name of one of the library's glyphs: `check`, `chevron_down`, `chevron_left`,
  `chevron_right`, `chevron_up`, `close`, `dash`, `dot`, `info`, `plus`, `warning`.
- An icon-only button and a field must say what they are in words (`label`), or the page fails at the
  line that drew them. A layer takes the keyboard when it opens and gives it back when it closes.

## `@pencere/charts`

Charts drawn on a canvas, made in two halves: what a chart is (its model) and how it is painted (a
painter) (`local charts = require("@pencere/charts")`). The kit paints smooth lines and fills; a page,
or a kit of its own, may give another painter and keep the chart's scale, model and pointer as they are.

- `charts.area(spec)` is a box `spec.w` by `spec.h` points (a chart has no size of its own) with a
  canvas in it: a line for each of `spec.series` (`{ values =, role = }`, the values oldest first, the
  newest at the right edge; a line shorter than `count` starts further right, and a series of fewer
  than two values draws nothing) and the area under it, between 0 and `ceiling`. `count` is how many
  samples the box is across (the longest series', where not given; 2 to 4000, the steps a path takes),
  `ceiling` the top of the scale (the largest value shown where not given, and 1 where it, or every
  value, is 0 or less), `rules` the
  fractions of the height a quiet rule is drawn at (`{ 0.5 }`), `marker` the sample under the pointer,
  counted back from the newest (none outside the samples), `painter` how the chart is drawn
  (`charts.smooth`), and `label` what a screen reader says of it. A chart with an `id` keeps its
  drawing while what it shows is the same: a pointer that moves paints the marker again and nothing
  else, and a new sample draws it again. A drawing is kept for the frame it was drawn in and the one
  after, a frame being seen to begin when a chart it drew is asked for again (two charts on a page
  have two ids), and for 64 charts a frame at most: one the page stops drawing is let go once the page draws
  its charts again, and the last a page drew are kept while it draws none.
- `charts.model(spec)` is the model a painter is given, in the points of the box:
  `{ w =, h =, rules = { y, … }, series = { { role =, points = { { x, y }, … } } }, marker = { x =, points = { { x, y, role }, … } } }`,
  x rising, a pixel at the top and the foot left clear, a rule's `y` on the middle of a pixel, and no
  `marker` where the chart has none.
- A painter is `{ area = function(model) -> paths, marker = function(model) -> paths }`: `area` is
  given the model with no marker, `marker` the model with one. `charts.painter(options)` makes the
  kit's, each look an option: `line` (how wide a line is, 1.5 points), `fill` (how opaque the area
  under a line is, in the line's colour, 0.2), `ink` (the colour of the rules and the marker,
  `"foreground"`), `rule` (0.07) and `marker` (0.45), how opaque they are, and `dot` (the side of the
  marker's dot on each line, 4 points). `charts.smooth` is `charts.painter()`. A page makes a painter
  once, not in `render`: a chart's drawing is kept for the painter that drew it.
- `charts.back_at({ w =, count = }, x)` is the sample under the point `x` of the box (0 at its left
  edge), counted back from the newest as `marker` takes it, or nil outside the box. Where the pointer
  is, is the page's to tell (its `move` events, [Events](events.md#what-a-page-is-told), and `pencere.layout.bounds`).
- A page that may run where the kit has no such module asks for it with
  `pcall(require, "@pencere/charts")`, which is false there.
