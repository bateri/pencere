# What a host does for a kit view

## The floors of the sandbox

The sandbox floors are those of the protocol ([Section 9.8](../../spec/draft/pencere.md#98-kit-views): the table of what a host gives a view at least), and the kit adds 16 stops to a gradient and 16 lengths to a dash. A view that stays within them opens on every host.

The values of this repository's implementation are `ModuleLimits::floor()` in `kit/sandbox/src/sandbox.rs`: what an author's page is written and tested against (`pencere test` holds a page to them).

A terminal gives more time: `ModuleLimits::default()` is the floors with a call 5 s and the start 10 s. Time is the wall clock's, and the wall clock of a busy machine is no measure of a page, which runs on a thread of its own that the drawing never waits for. The limit is there to stop a page that does not come back, not one that is slow.

## What a host does

Besides drawing what the engine lays out, a host does what follows.

This repository's drive (`pencere-drive`, `kit/drive`) does it for a host, all but what only the host can do:

- measure and draw text in its fonts;
- draw in its renderer and theme;
- turn its keys into the kit's;
- and be its system's input method, pasteboard, pointer, fields, menus, browser, screen reader, clock and program.

### The baseline of words

A host measures a line of words (`Measure::measure`): its width, the height of its line box and `ascent`, how far down in the line box its baseline is, in the unit of `height`. The host draws a run of words with its baseline `ascent` below the origin the kit gives it, which is the top of the line box. A page that stands its rows on a baseline (`items_baseline`, [Layout](layout.md#standing-on-a-baseline)) is aligned by this number, so a host that draws its glyphs lower or higher than its `ascent` shows the rows off by as much. A host that draws no words measures nothing (`Metrics::default()`).

### Text fields

A host puts one field of its own over each text field of the page (`Laid::fields`), of its kind, saying its hint while it is empty.

The host:

- tells the page what the person types into a field, and where the person clicks into one;
- takes the texts a reply fills the fields with, and what it selects in them;
- and, while a field has the keyboard, gives the page the keys it has no use for there, as the kit's rule says with the field's text and selection (`FieldState::takes`, [Elements](elements.md#text-field)).

A field has three boxes (`Field`), each inside the one before it, the element's own ([The box of an element](layout.md#the-box-of-an-element)): its room (`rect`: the border box), its padding box (`padding_box`: the room less its border) and its text box (`text`: the padding box less the element's padding). The whole room is what the pointer reaches, so that a press in the padding or on the border is a press in the field. A field's text is set in the text of its element (`Field::font`: the face, the size, the weight and the italic the element says, or the boxes around it do, as a style sheet's `font: inherit` has a field take them), in its text box; where nothing says, in the terminal's face at the kit's text size (`ZOOM_TEXT`), regular. The font of the field the drive hands a host (`pencere_drive::Field::font`) has its size in the host's points, the page's points times the host's zoom, as its boxes are; the font of the kit's own `Field` (`pencere_view::Field::font`) has it in the layout's pixels, the font the kit asks `Measure::field_line` of. The hint a field says while it is empty is set in the same font: the words of the text inside the field are the hint, and the size, face and weight that text says are not the host's to draw (the field's box and its baseline are laid out from that text in its own font, so a page gives it the text of the field). A field of lines starts its text at the top of that box and breaks a line that is too long at its width, at a word, keeping every space (`pencere_view::break_rows`); it cuts what it shows, and scrolls, at its padding box: the padding scrolls with the text, so that a row past the text box shows in the padding and no row shows over the border, and the caret is kept in view. A host that scrolls by whole rows shows the row before the first and the row after the last in the padding, cut at the padding box. A field of one line has its line in the middle of its text box.

The host says how tall one line of its field is, in the field's font, with the unit of its measure (`Measure::field_line`, asked the font of the field as the layout has it): a page that gives a field `rows` is laid out with that many of them, so that a host measures it in the very face that draws it, and the last line is not cut. A line is not a share of the size, so a host says it of each font its fields come in and is asked again where a field comes in another. Where it says nothing, the kit takes the height of a line of the same font that the host's measure gives.

It says too where the words of the field stand in that line: how far down from the top of the line the baseline is, in the same unit (`Measure::field_baseline`, asked of the same font). The baseline of a label and a field in a row that stands on one (`items_baseline`) is that number, as it is the host that draws the words. Where it says nothing the kit works it out from the line and the ascent of a line of the font it measures (the line has what is above and below the words in halves). A host that draws every field in one face (a terminal's, one cell high) says that face's line and baseline for every font.

The host takes what a reply asks of a field (its text, its selection, the keyboard) only of a field it has put over the page. It holds what a reply asks of a field it has not put over the page until the next frame that lays the page out has been laid and its fields put over the page, then takes it in the order it was asked ([Replies](replies.md): a reply that opens a field and fills it and sends the keyboard to it in one is done as it says). It passes over a name that frame does not have either.

### Words and the pointer's shape

A host draws a run's `underline` and `strike` where its font puts those lines, in the run's colour, and sets an `italic` run in its face's italic.

A host takes the pointer's shape a reply asks for, and tells a press's `count` and a scroll across.

### Opening links

A host opens the link a reply asks to open as the protocol says of the links of a kit view ([Section 9.8](../../spec/draft/pencere.md#98-kit-views), Links). `pencere_host::session::Session::module_link` does it, and raises `Action::OpenLink` as a page's `link` does.

### Waking a page

A host wakes a page at the time it asked to be woken at: the earliest of what its replies and frames asked, which the sandbox keeps (`pencere_sandbox::Update::wake`, on render's clock).

- The host arms one wakeup for that time and, at the first frame at or after it, wakes the page with the frame's time (`Module::wake`) before it asks for the frame's page.
- The host wakes from the frame, never more than once a frame however often a page asks (a page that asks again at once costs a frame's work, as an `animating` one does), and never before the time or for nothing (the sandbox passes over such a wake).
- While the view is not seen (its pane hidden, its window minimized or on another space), a host MAY hold a page's wakes to one a second.
- The host does not drop the wakes it holds: a wake held is given at the next one it gives, or when the view is seen again.

### Every movement the system gives

A host tells the page every movement the system gives it, and not only the ones a page of the kit asks for:

- the pointer with the modifiers held;
- the buttons of a mouse beyond the third (`extra_press` and `extra_release`, never as a `press`);
- the wheel with its phase and whether the device is a trackpad;
- and what a trackpad's fingers do: a pinch (`magnify`), a double tap (`smart_magnify`), a rotation (`rotate`), a swipe of three fingers (`swipe`) and the force of a press (`pressure`).

A host that has no device for one of them never tells it.

What the system makes of a gesture of its own (a swipe that goes back in a browser) is the host's to decide before it tells it.

A run of pinches or of rotations one gesture makes that the page is too slow to hear one by one is told as one (a factor of them all, a turn by all of them), its beginning and its end as they were (`Module`).

### Scrolling boxes

A host scrolls the boxes that scroll (`Scrolls`):

- the wheel and the trackpad move the box under the pointer, as its system does (a gesture stays with the box it started on, and where the system does so a box pulled past its end springs back);
- its scroll bars are drawn as the system's are and can be dragged;
- the page's `scroll_to` is done before the page is next laid out;
- the boxes that say the same `scroll_link` go together, in the same frame (`Scrolls`);
- and the page is told where its boxes went.

A wheel no box took is the page's (`wheel`).

### Zooming boxes

A host zooms the boxes that say a `zoom` (`Scrolls`):

- a pinch, the wheel with the command key held and a double tap with two fingers are the box's, about the point under the fingers, within the range the page gave, with the page's constants for its range, its double tap and its wheel;
- what the box holds is laid out again at the scale and not stretched;
- and the page is told the scale (`scrolled`) and asks for one (`zoom_to`).

A gesture no such box took is the page's (`magnify`, `smart_magnify`).

### The size of the page's words

A host draws the page's words as many points larger than its own text as a reply says (`text_size`, [Replies](replies.md); `Host::text_size`):

- a whole number, below 0 for smaller;
- held within what the host can draw;
- its own text as it was;
- and the host tells the page nothing of what came of it.

### Vibration of the trackpad

A host vibrates the trackpad as a reply's `haptic` says ([Haptics](haptics.md); `Host::haptic`), and holds the vibration to these rules:

- it vibrates only while a finger is on the glass of a device that can, and only for the view the person is working in;
- a vibration ends when the finger lifts, when a new one takes its place, and when the view is gone;
- a strength is a share of the strongest the device gives, and a sharpness is mapped to the band the device feels, so that a crisp tap is crisp on every one;
- a host with no device that vibrates ignores it;
- a host with no screen keeps every ask, whatever a device would have done with it;
- and the host tells the page nothing of what came of it.

A page that asks while no finger is on the glass asks for nothing: it is not told, and the vibration is not kept for when a finger comes.

### Playing sounds

A host plays the sounds a reply's `sound` asks for ([Sounds](sounds.md); `Host::sound`). It is given the samples, never a name or a file (the kit makes its own sounds and decodes the page's files, the same on every system), and holds the playing to these rules:

- it plays them as the system's volume and silence say, multiplied by the `volume` of each, and a host with no speaker, or with the sound off, ignores them;
- sounds play over each other, up to eight at once for a view (`pencere_view::sound::Voices` keeps the count): the ninth silences the one that has played longest;
- the end of every sound (`false`) is told as a sound is, in its place among them, and ends all that the view plays;
- a sound is ended when the view is gone;
- a file that was not decoded when the page asked is told later (`Drained::sounds`), and a host that is to drop what has waited too long drains the page when `Page::sounds_deadline` says;
- and the host tells the page nothing of what came of it.

A host with no screen keeps every ask, whatever a speaker would have done with it.

### Carrying out of the view

A host carries out of the view what a page's element says it carries (`carry`, [Commands and dragging](commands.md)) as its system's drag:

- from the press and the move past `DRAG_START` page points (for an element that is also `draggable`, from the pointer's leaving the view);
- in the same `input` that moved the pointer (`Host::carry`: the words or the path, the url, where the pointer is and the element's room);
- and the host holds the page's pointer events back until the drag is over, when it tells the page `carried` (`Page::carried`).

A host that does not carry says nothing (`Page::set_carry`) and the page hears every move and the release of an element that carries as of any other.

The host takes what it carries from the view as the protocol says ([Section 12.3](../../spec/draft/pencere.md#123-files-and-navigation)), and `pencere_wire::carry` is the check.

### A larger point

A host may draw a point of the page larger than a point of its screen, the page given fewer points for the same room.

A terminal draws a page as large as its own text is to the kit's (12.5 points), so a page's words are the size of the terminal's and grow and shrink with them.

`Laid::accessible` and the fields are in the page's points; the host places them.

### Moving elements

A host moves the elements that say a `motion` between the frames of the page.

The host:

- lays the page out again at each frame while one moves that takes room (a width or a height in points, the thickness of a border, the padding, the size or the colour of words);
- and draws the layout it has where what moves only paints (a fill, a border or a shadow colour, an opacity, the corners, the shadow, a slide, a coming in or going out, a turn or a size of a picture or a canvas, a loop, a spin; `Motions`).

A host draws a frame for each step of what loops or spins for as long as it is on the page.

A host says an element's `tip` while the pointer rests on it (`Laid::tips`).

### The screen reader

A host tells its system's screen reader what the page is:

- each element as what it says it is (its `role`), the other pressable elements as buttons, the fields as fields, the links of a text as links, and the words of the rest, each by its `label` where it has one, with its state (checked, selected, expanded, of no use, its value);
- and the controls inside an element that answers the pointer apart from it (`Laid::accessible`).

### Gradients and figures

A host draws a box's fill from one colour to another as a style sheet's `linear-gradient` of that angle draws it, over its plain fill and under its border.

A host draws a canvas's paths as `pencere_sandbox`'s `draw_figure` draws them (the one rasteriser, so that a figure is the same on every host: their shadows, gradients, blends, dashes and transforms as well), in the colours of its theme, drawing again only what `figure_key` says it does not hold.

### The list of commands

A host lists the commands that can be done now (`Laid::commands`: those of the element the page says has the keyboard and of the boxes around it) where a person looks for what can be done, and tells the page the one asked for there.

Before it tells the page a key, a host looks for the command those keys do (`Laid::command_for`) and tells the page that command in place of the key.

### A button held

A host keeps telling the page where the pointer is, and the release, wherever the pointer goes while a button pressed on the page is held (it does not tell a `leave` then). A drag that leaves the page and comes back, or ends outside it, is so followed to its end.

### The rest of the input

A host tells the page:

- the middle button;
- the keys that go up after it was told they went down;
- the modifiers when they alone change;
- the files dropped on the page from outside it;
- and the addresses dropped on it (`link_dropped`):
  - only an `http` or `https` link of printable ASCII without a space, at most 8192 bytes, as a link of a reply is held to;
  - any other is left and the page is not told.

### The time and the keyboard

A host says the time of day as its system does, with the person's offset from UTC (the interface's `system.wall-clock`, `Program::wall_clock`). The sandbox's own default reads the system's clock and knows no time zone, so a host that knows the person's says it.

A host says how its system makes and writes shortcuts (`system.keyboard`, `Program::keyboard`: `mac`, `windows` or `linux`).

### A soft edge

A host draws what a clip with a soft edge lets through ([`Clip`](layout.md#the-box-of-an-element)) at less than the whole of it towards the edge: a box, a picture, a line of words and a glyph, each by the share of it that the fade leaves at that pixel (`Clip::cover`). A host that cannot (the character cells of a terminal) cuts at the rectangle, as it does at a rounded corner.

### A turned picture

A host draws a picture turned where its draw list says (`Picture::turn`, degrees clockwise about the middle of its rectangle, cut at its rectangle turned with it): what spins (a canvas that spins is turned before it reaches the host, and draws as any other).

### Editors

A host puts an editor of its own (`pencere-editor`, the same in every host) in each editor's room (`Laid::editors`).

The host:

- tells the page each one it makes (`editor_ready`, its text posted first);
- keeps the editor's document as long as an editor of that name is on the page, hidden or not;
- and draws the editor in its place of the draw list (`Paint::Editor`), over the room's fill and under what covers it.

The host draws the carets (their corners, light and outline as the options say, `caret.*`), and blinks and moves them (`caret.motion`) on its own clock, as the options of the editor that has the keyboard say:

- the page is asked for no frames for it, whether the caret blinks or glides;
- a host that has a cursor of its own, as a terminal does, MAY draw and blink that at the main caret instead, and then draws none of the page's: `Page::set_host_caret`, and the box of the caret from `PageEditors::caret` and `PageEditors::rect`.

The host gives the editor its system's input method (what is composed, and the questions of where the caret is and what is around it, answered at once, in the units its system counts in).

The host gives the editor its system's pasteboard:

- text reaches an editor from it only on the person's key or the person's pick of a menu entry the host performs itself, `Host::paste`: a page cannot ask for it;
- what the page has an editor copy or cut is handed over to put on it, `Drained::copied`.

The host sends a key to the page's command for it first, the editor's own next, the text last:

- none of the page's while an input method composes;
- and every key to the page where the editor gives it its keys.

The host tells its screen reader the editor as one.

### Scrolling an editor

A host:

- scrolls an editor's room as it scrolls a box, over the content the editor laid out;
- keeps the caret in view where the editor asks;
- and tells the page the lines in view.

Where a caret an overlay is anchored to moves (or to a range of its text the page named, which also moves with the text), or an editor with one scrolls:

- the host lays the page's last tree out again without asking the page;
- and it does not show an overlay whose range is out of the room.

### The terminal's look

A host says how its terminal draws its cursor and its selection (`system.terminal-look`, `Program::terminal_look`, `pencere.terminal_look()`), as the person set them, in the words and units of an editor's options:

- a field only where it has such a setting;
- none where it has none (the default, which is a host with no settings of the kind).

The host MUST tell the page `terminal-look` when what it says changes, and only then. The host MUST NOT apply it to a page's editors itself: the page decides.

The page is told from the thread of the pane (the host's settings are reloaded on another, which wakes the pane's as it does for anything else; `Page::terminal_look_changed` is called there).

### What a host draws

A host says what it draws that not every host does (`system.draws`, `Program::draws`, `pencere.draws()`):

- `shadow` where it draws a box's soft shadow (`Quad::shadow`);
- `cells` where the page is drawn in the character cells of a terminal;
- `coarse_images` where a picture is drawn in too few pixels to read a small one by.

A host says only what it draws.

A host MAY leave the soft shadow out. A caret's `glow` is then not seen, and the page can tell (`pencere.draws()`).

The engine lays a page out in pixels whatever a host says.

### The pointer's shape under the modifiers

A host tells the drive the pointer's shape when only the modifiers changed what is under it (a link that ⌘ made: `Drained::cursor`, the shape the host takes where the pointer is over the page).

### The pointer's rest over a word

A host times the pointer's rest over a word of an editor that hears of it on its own clock, as it does the caret's blink (`Page::wake_at` has the time). It tells the page `hover` once, and its leaving.

### The browsers a host shows

A host puts a browser in each browser's room (`Laid::browsers`: the content box of its element, the border and the padding around it) where it shows browsers, and says so (`Page::set_browsers`).

The host:

- tells the page what each is and does (`Page::browser`; a host does not leave the set of addresses of [Browser](browser.md), whoever asks);
- keeps the browser's history as long as a browser of that name is on the page, hidden or not;
- draws the browser where its draw list says (`Paint::Browser`, over the room's fill and under what is inside it);
- draws none while the browser is hidden;
- gives the browser the keyboard on a press in its room;
- keeps the keys of `keys` for the page and gives the page the other keys the browser had no use for (`Page::input`);
- finds what the page asks it to find and marks the matches (the bar is the page's);
- opens its tools for the page's `devtools`;
- empties the browser's cache for `clear_cache` and a site's data for `clear_site_data`;
- goes on past a certificate for `proceed` (what the person asked the page to ask);
- tells the page the person's wish to open a link behind (`Page::browser_window`);
- and tells its screen reader the room as a browser.

A host that shows none draws the placeholder of `kit/drive` there and does nothing of what the page asks of it.

### Texts posted to the page

A host posts the page the text of each editor before it tells the page of the version it is (the copy costs nothing that grows with the document).

A host holds a reply to what one may hand the editors and the editors to the floors of the protocol ([Section 9.8](../../spec/draft/pencere.md#98-kit-views), its table).

## Pictures

What a page may name as a picture, the formats and the limits are the protocol's ([Section 9.8](../../spec/draft/pencere.md#98-kit-views), Images); [`README.md`](../README.md) says how a terminal draws them.

The host answers a `sha256:` name from the files the program attached to the view (protocol, Section 8.8) and a `file:` name from the handles it made (Section 8.9), each from its own table. A file the program lets go of (`detach`) is forgotten, and not drawn any more.
