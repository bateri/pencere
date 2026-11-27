# Elements

An element is a plain table that a chain of builder methods fills. Lengths are points.

## The builders

| Constructor                      | Element                                                                                                                              |
| -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| `pencere.div()`                  | A box: a column of its children until told otherwise.                                                                                |
| `pencere.text(string \| number)` | One line of text, in the font, size and colour of the boxes it is in; with `wrap`, as many lines as its box needs.                   |
| `pencere.spark(values, color)`   | A sparkline of numbers, newest last; `:peak(value)` is its largest.                                                                  |
| `pencere.image(path)`            | A picture: a file of the view's own bundle (protocol, Section 9.8); a page of it, for a PDF: `:page(n)`.                             |
| `pencere.rich(parts)`            | A text of many looks: its parts (`pencere.span(words)`, or a string) one after the other, as one paragraph.                          |
| `pencere.canvas(paths)`          | A box with paths drawn in it (`pencere.path()`), in its own points. It has no size of its own.                                       |
| `pencere.editor(id, options)`    | The room of an editor of text the host keeps, lays out and draws ([The editor](editor.md)); `div():editor(id, options)` the same.    |
| `pencere.browser(id, options)`   | The room of a browser the host keeps and draws ([Browser](browser.md)); `div():browser(id, options)` the same. `if pencere.browser`. |
| `pencere.spacer()`               | A box that takes the room left over.                                                                                                 |

The builders return the element:

| Kind                                    | Builders                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| --------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Layout                                  | `row`, `col`, `wrap`, `grid(columns)`, `grid_rows(rows)`, `span`, `hidden`, `items_start\|center\|end\|stretch`, `justify_start\|center\|end\|between`, `gap`, `gap_x`, `gap_y`, `grow`, `no_shrink`, `w`, `h`, `size`, `w_pct`, `h_pct`, `w_full`, `h_full`, `size_full`, `min_w`, `min_h`, `max_w`, `max_h`, `p`, `px`, `py`, `pt\|pr\|pb\|pl`, `m`, `mx`, `my`, `mt\|mr\|mb\|ml`, `absolute`, `inset`, `overlay`, `anchor(id, side, { gap =, align = })`, `clip`, `scroll()`, `zoom({ min =, max =, smart =, wheel = })` |
| Looks                                   | `bg`, `bg_gradient(angle, from, to)`, `border(width, color)`, `rounded`, `rounded_corners`, `shadow(dx, dy, blur, color)`, `opacity(0..1)`, `transform({ rotate =, scale = })` (a picture or a canvas)                                                                                                                                                                                                                                                                                                                      |
| Text, which the elements inside inherit | `text_size`, `weight`, `color`, `family("system" \| "terminal")`, `align("start" \| "center" \| "end")`, `italic`, `underline`, `strike`, `truncate`                                                                                                                                                                                                                                                                                                                                                                        |
| Images                                  | `fit("contain" \| "cover")`, `tint(color)`, `page(n)`                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| Names and answers                       | `id`, `press(id)`, `focusable`, `focused`, `text_field(id, "line" \| "lines" \| "secret")`, `label(words)`, `tip(words, keys)`, `command(id, title, keys, enabled)`, `draggable(id)`, `drop_target(id)`, `carry({ text = \| path =, url = })`                                                                                                                                                                                                                                                                               |
| To a screen reader                      | `role(name, level)`, `checked(on)`, `selected(chosen)`, `expanded(open)`, `disabled(on)`, `value(words)`, `mixed()`, `level(n)`, `position(i, n)`, `table_size(r, c)`, `cell_at(r, c)`, `sorted(order)`, `role_description(words)`, `layer(modal)`, `fallback_name(words)`                                                                                                                                                                                                                                                  |
| Motion                                  | `motion({...})`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| Children                                | `child`, `children`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |

- `w`, `h`, the least and the most a box is, and `border(width, color)` are of the box with its border: the border takes room inside the size a page gives, as the padding does, and what the box holds stands inside both ([The box of an element](layout.md#the-box-of-an-element)). A page that depends on it asks `pencere.has("layout.border_box")` first ([Accessibility](accessibility.md#asking-whether-the-kit-has-what-a-page-needs)).
- `scroll()` is a box whose content scrolls behind it, down and across. The host scrolls it, by its id ([Layout and scrolling](layout.md)).
- `zoom(…)` is a box that scrolls and lets the person zoom what it holds. The host zooms it ([Layout and scrolling](layout.md)). A page's editor or browser does not go inside one.
- `opacity` is of the element with all inside it. Each thing it draws is that much fainter, its words too, past a host's floor of contrast.
- `page(n)` is a whole number from 1: which page of a PDF the picture is.
  - A file that has no pages is its only page.
  - A page it has not leaves the box empty, with a diagnostic that says how many pages the file has.
  - A page that asks whether the terminal draws PDFs asks whether `pencere.div().page` exists, as for any later addition.
  - A host built with no PDF reader (`pencere-sandbox` without its `pdf` feature) leaves the box of a PDF empty and says in a diagnostic that the build has no PDF reader.
- A grid track is a number (points), `pencere.fr(n)`, `pencere.range(least, n)` or `"auto"`.

### On and off

These builders say a thing is so or not:

- `wrap`, `hidden`, `no_shrink`, `absolute`, `overlay`, `clip` and `scroll`;
- `italic`, `underline`, `strike` and `truncate`;
- `focusable`, `focused`, `checked`, `selected`, `expanded` and `disabled`;
- a part's `bold`, `italic`, `underline` and `strike`, and a path's `crisp`.

Called with nothing, one turns it on. Otherwise it is what it is given: `false` and `nil` are off, so that `:selected(row.chosen)` with nothing in the field is not selected.

`italic(false)`, `underline(false)` and `strike(false)` take back what the element would inherit.

`hidden(false)`, `no_shrink(false)`, `clip(false)`, `scroll(false)` and a part's `bold(false)` take back only what they said themselves:

- `:row():hidden(false)` is a row;
- `:hidden():row()` is hidden;
- `:weight(300):bold(false)` is 300.

`checked` and `expanded` called at all say that the element is a thing that is on or off, open or shut. Not called, they say that it is not.

## Text

A text that does not fit its one line runs past its box, or, with `truncate`, ends in "…".

### Wrapping

`wrap` on a text puts its words in new lines where its box is too narrow for them, and at its line breaks. Its box is as tall as the lines. In a row it gives its room away down to its longest word.

### A text in a field

A text inside a `text_field` element is what the host's field says while it is empty, as in `div():text_field("filter"):child(text("Filter images"))`. The host draws it, the page does not.

## Text field

A **text field** is a field of the host's own. It is put over the room of the element that says it is one, and named by that element's id (`text_field(id)`, or the element's own id). A page has as many as it names, and an element that says it is a field and has no name is not one.

### Its kinds

- `"line"` (the default) is one line;
- `"lines"` breaks the line at enter and scrolls;
- `"secret"` is one line whose characters are not shown and are never copied.

### How tall it is, and where its text stands

A field is as tall as its element's style says, or, for a field of lines, as many lines as the page says: `text_field(id, "lines", { rows = 3 })` (a whole number from 1 to 100; `Element::field_rows` in Rust). The host says how tall one line of its field is (`Measure::field_line`, [Host](host.md#text-fields)), and the element is that many of them, and its padding and border besides: it is as tall as the lines whatever the size of the text and the host's zoom, and the page does not work a height out. Rows take the place of the height: a page that gives an element both (`h`, `h_pct`, `h_full`, `size_full`, `size` with a height) has failed, in whichever order it says them; the least and the greatest height (`min_h`, `max_h`) still hold. Rows are for a field of lines only. A page that uses them asks `pencere.has("text_field.rows")` first ([Accessibility](accessibility.md#asking-whether-the-kit-has-what-a-page-needs)).

The padding of the element is the field's: the text stands in the room less the border and the padding (`Field::text`), and the whole room is the field's for the pointer, so a press in the padding is a press in the field. A field of lines starts its text at the top of that box and breaks a line that is too long at a word, as the host's own fields do; it cuts what it shows, and scrolls, at the room less the border (`Field::padding_box`), the padding scrolling with the text ([Host](host.md#text-fields)). A field of one line has its line in the middle of its text box.

### What the page is told and does

The page is told what a field holds when the person changes it (`text`).

The page sends the keyboard to one (`keyboard = { field = name }`), and puts a text in one, selects in one, or both (`fill`, [Replies](replies.md)).[^fill]

[^fill]: A dialog that renames a file comes up with the name in its field and the stem selected, and a palette opened again with its query selected, so that what is typed takes its place.

### Keys

While a field has the keyboard, the keys it has no use for are the page's, as `key` events (a command's keys first). The field keeps the keyboard unless the reply sends it elsewhere.

A field has a use for a key that changes its text or what is selected in it, and for no other:

- Escape, tab and the function keys are the page's in every field.
- In one line (`"line"`, `"secret"`) so are enter, up, down, page up and page down, whatever is held with them.[^caret]
- In lines, enter is the field's without command or control, and up and page up, down and page down, are as left and right are below.
- A character is the field's. With command or control it is the field's only where it is one of the field's own chords:
  - select all, paste, undo and redo;
  - copy and cut, where something is selected in a field whose text may be copied.

  Every other chord is the page's.

- Backspace, left and home are the page's where nothing is selected and the caret is at the start, and delete, right and end where it is at the end, whatever is held with them. An empty field gives backspace to the page. Elsewhere they are the field's.

[^caret]: In one line they would only take the caret to an end.

So a list with a filter above it takes its commands while the person types.[^filter]

[^filter]: The arrows move in the list, enter chooses, ⌘K opens a palette, and ⌫ in the empty filter of a path goes up a folder.

A host asks the rule with its field's state (`pencere_view::FieldState::takes`). One whose system gives its fields chords besides these may keep those too.[^mac]

[^mac]: A Mac's ⌃A and ⌃E are two.

## Tip

A **tip** is what the host says of an element while the pointer rests on it, with the keys that do the same where there are some (`tip("Containers", "1")`). It is a tooltip of the host's own. Its keys are never a name.

## Overlay

An **overlay** (`overlay()`) is taken out of the flow and out of every box that holds it.

### Where an overlay goes

- It is placed by its `inset` against the page (`inset(0, 0, 0, 0)` is the whole page, the ground of a dialog).
- It is drawn after everything else, so over it.
- It is cut by no box that holds it.
- It is found by the pointer before what is under it.

What it is written inside still holds it for the rest. It is as faint as that is, and hidden with it. The path a key takes goes out through it (`layout.path_of`).

### Beside an element

`anchor(id, side, options)` makes an element an overlay **beside the element named `id`**, on `side` of it (`"below"`, the default, `"above"`, `"right"` or `"left"`). It is `options.gap` points away. Its start, middle or end is lined up with the element's (`options.align`: `"start"`, the default, `"center"` or `"end"`).

Where the page has not the room for it on that side and has it on the other, it goes on the other. Along the side it is moved as far as it takes to stay on the page. Where no element of that name is on the page, it stands where its `inset` puts it.

A menu that drops from a button is `div():anchor("sort", "below", { gap = 4 }):child(...)`.

## Rich text

In the one paragraph of a `pencere.rich`, a part (`pencere.span(words)`) says only what it changes of the element's look:

- `color`, `weight` (`bold`), `italic`, `underline`, `strike` and `family`;
- a ground behind its words alone (`bg`);
- `link(id)`, which makes it answer the pointer by that name (`layout.hit` says it, a screen reader says a link) and underlines it unless it says not (`underline(false)`).

The size is the element's for every part.

### Lines

Where the element `wrap`s, the parts run on from line to line as the words of one text do. A word made of two parts with nothing between them (`**bold**,`) is not broken between them.

Where it does not, it is one line for each line break, cut with "…" where it may be (`truncate`).

## Gradient

A **fill from one colour to another** (`bg_gradient(angle, from, to)`) runs along a line at `angle` degrees.[^gradient] `180` is from the top down, and `90` from the left to the right. The fill is over the box's `bg`. A line of text on it is read against its colour half way along.

[^gradient]: A style sheet's `linear-gradient` runs along a line in the same way.
