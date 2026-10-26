# Changelog

All notable changes to the `pencere-sandbox` crate are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate follows
[semantic versioning](https://semver.org/).

## [Unreleased]

### Added

- `Module::wait_for_frame(within)`: waits, at most `within`, for a page the host has not taken.
  For a resized pane, so the page laid out for the old size is not drawn for a frame.

### Changed

- The interface (`pencere:view@1.0.0`, a draft until a host other than this repository's runs it)
  says what `pencere-view` added: a colour's `mix`, an element's `opacity` (an image's is its
  element's), `italic`, `underline` and `strike`, named fields of three kinds (`field` and `label`
  on a node, `text` and `focus` events with the field, `keyboard.field(id)`, `reply.fill`), a
  press's `count`, a scroll's `dx`, an extent across as well as down, the keys `backspace`,
  `delete` and `function`, and six more shapes of the pointer. `layout.text-field` is gone: a
  field's box is `bounds` of its name. The interpreter (`luau.wasm`) is built from it, and its page
  API has `pencere.mix`, `opacity` on any element, `italic`, `underline`, `strike`, `label`,
  `text_field(id, kind)`, `scroll_x`, and `Ui:scroll_by(laid, at, dx, dy)`.

## [0.2.0] - 2026-10-25

### Added

- The crate itself, and its first release (the pencere crates are released together at one version,
  which began at 0.2.0: there is no 0.1.0 of this one). It is taken from the terminal that wrote it
  first: the sandbox, its limits and watchdog, the thread a module runs on, and the embedded
  interpreter. The interface is `pencere:view@1.0.0` and the Lua global is `pencere`.
- `kit/SPEC.md`: the Lua page API, the colour strings, the recommended formulas of the derived tones and the sandbox floors.
- **A frame says what it says once.** The words of every line of text and the paths of pictures,
  the values of sparklines and the tracks of grids are three lists of the frame, and a menu's
  titles one string; an element (an entry) names the part it means (`slice`: a start and a
  length). A module cannot make the application copy one buffer for every element that
  points at it: the lists are taken in once, as large as the module's memory, and what the
  elements read of them is counted each time against `ModuleLimits::frame_bytes` (4 MiB). A part
  past its list, one that begins or ends inside a character, or a frame past the limit is not a page.
- **`ModuleLimits::compile`.** Compiling a component is held to a time (20 s); a component that takes
  longer is refused, in words, and its compiling ends on its own thread. A component written
  against another version of the interface than this one's is refused with a sentence that names both
  (`this view is written against `pencere:view@0.5.0`; this terminal opens `pencere:view@1.0.0``),
  and not with the runtime's words.
- **Images.** `ImageLimits::work_bytes` (the memory one image may be worked in: what a decoder
  holds, the layers an SVG is drawn on), `sizes` (an SVG is drawn at a few sizes, each its own
  raster, and a size past them is the nearest) and `idle` (how long pixels nothing asks for are
  kept when their room is needed). A drawing that unfolds into more than a hundred thousand parts is
  refused, and its parsed shapes are counted in the pixels of the view. A JPEG is held the way its
  EXIF orientation says, as a web view shows it.
- The package carries what it is built from and nothing of the repository around it: the interface
  it reads is its own copy (`wit/view.wit`, equal to the kit's `kit/wit/view.wit`, which `make check`
  keeps so), and the tests of `src/`, which read files of the repository, are left out.
- **`Program::now`.** An application that keeps a clock it can read from any thread says what it
  reads (the same seconds `render` is given as `now`), and the page that is made again because its
  program said something is made for that time. Without it (the default, `None`) the time is that
  of the last frame and what the system's clock says has passed since: right as long as
  the application's clock goes as the system's does, which a clock that stands still or is driven by
  hand does not. A time that is not a number, or is earlier than the last frame's, is not taken.
- **Images.** `Images` decodes the files of a view's bundle (`Assets`, a path to bytes) for the
  `image` element: PNG, JPEG and SVG, off the thread that draws, once for each path. It gives the
  host the size of an image for the layout (`Images::size`), the pixels to draw (`Images::bitmap`,
  premultiplied RGBA with an `id`; an SVG is drawn at the size it is asked and again for another),
  and one wake for what arrived (`Images::poll`: `relayout`, `redraw`, `diagnostics`). Held to
  `ImageLimits`: 4096 pixels a side, 64 MiB of decoded pixels, 8 MiB a file, 1024 paths. An SVG
  reads nothing it names, draws no text and loads no font. The interface and the Lua page API have
  the element (`pencere.image(src):fit("contain" | "cover"):tint(…):opacity(…)`); the
  interface is `pencere:view@1.0.0`.
- New dependencies, all of them MIT, Apache-2.0, BSD or Zlib: `png` 0.18, `zune-jpeg` 0.5.15 with
  `zune-core`, and `resvg` 0.48.1 with no default features (so no text, no raster images and no
  compressed files) and what it brings: `usvg`, `tiny-skia` and `tiny-skia-path` (BSD-3-Clause),
  `svgtypes`, `kurbo`, `roxmltree`, `simplecss`, `data-url`, `strict-num`, `imagesize`, `rgb`,
  `float-cmp`, `euclid`, `polycool`, `siphasher`, `pico-args`, `bytemuck`, `arrayref`, `flate2`,
  `zlib-rs`, `fdeflate`, `miniz_oxide`, `adler2`, `simd-adler32`, `num-traits`, `autocfg`.
- The interface and the Lua page API carry the new colours: the theme's ANSI palette and selection
  as roles (`"red"`, `"bright-red"`, `"selection"`, and `"border"` for the colour of rules and borders), and values (`"#rrggbb"`, `"#rrggbbaa"`,
  `{ light = "#…", dark = "#…" }`, with an opacity as before: `pencere.alpha(colour, 0.5)`). The
  interface is `pencere:view@1.0.0`.
- `pencere.json` of the Lua page API reads and writes arrays and objects nested up to 127 levels; at 128 it is neither (specification §4.5).
- **`require` in a script.** A script may load the other scripts of its bundle, and nothing else:
  Luau's own require-by-string (`require("./screens/logs")`, `require("../parts/table")`, directories
  with an `init` script, `@self`), relative to the path of the script that asks, over the files the
  bundle's manifest types `text/x-lua`. A path that leaves the bundle's root, an absolute path, an
  alias (there is no configuration file), a file that is not a script and a name the bundle has
  nothing at are errors that name the path, at the line of the `require`. Each script runs once per
  page, in a thread with globals of its own, and a cycle is an error that names the chain. An error
  in a required script (a syntax error too) says that script's own file and line; the entry's says
  its path too (`page.lua:12:`). The bytes of the scripts a page loads, the
  entry's included, are held together to `ModuleLimits::component` (4 MiB); a script that is
  alone over it is not started. For this the interface `pencere:view@1.0.0` has
  `script.path`, `script.has-module` and `script.module` beside `script.source`, `Module::from_script`
  takes a `Script` (the entry, its path in the bundle and the bundle) and `Assets` has
  `media_type`, which says what the manifest typed a file. The interpreter carries Luau's
  `Require` and `Config` libraries (about 190 KB more).
- A helper of the Lua page API that is given the wrong argument (`pencere.text` of a value that is
  not a string or a number, `pencere.spark` of a non-list, `children` of a non-list) raises the
  error at the line of the script that called it, not at a line of the prelude.
