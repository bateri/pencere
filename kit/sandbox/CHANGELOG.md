# Changelog

All notable changes to the `pencere-sandbox` crate are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate follows
[semantic versioning](https://semver.org/).

## [Unreleased]

### Added

- The package carries what it is built from and nothing of the repository around it: the interface
  it reads is its own copy (`wit/view.wit`, equal to the kit's `kit/wit/view.wit`, which `make check`
  keeps so), and the tests of `src/`, which read files of the repository, are left out.
- **Images.** `Images` decodes the files of a view's bundle (`Assets`, a path to bytes) for the
  `image` element: PNG, JPEG and SVG, off the thread that draws, once for each path. It gives the
  host the size of an image for the layout (`Images::size`), the pixels to draw (`Images::bitmap`,
  premultiplied RGBA with an `id`; an SVG is drawn at the size it is asked and again for another),
  and one wake for what arrived (`Images::poll`: `relayout`, `redraw`, `diagnostics`). Held to
  `ImageLimits`: 4096 pixels a side, 64 MiB of decoded pixels, 8 MiB a file, 1024 paths. An SVG
  reads nothing it names, draws no text and loads no font. The interface and the Lua page API have
  the element (`pencere.image { src = …, fit = "contain" | "cover", tint = …, opacity = … }`); the
  interface is still `pencere:view@1.0.0`.
- New dependencies, all of them MIT, Apache-2.0, BSD or Zlib: `png` 0.18, `zune-jpeg` 0.5.15 with
  `zune-core`, and `resvg` 0.48.1 with no default features (so no text, no raster images and no
  compressed files) and what it brings: `usvg`, `tiny-skia` and `tiny-skia-path` (BSD-3-Clause),
  `svgtypes`, `kurbo`, `roxmltree`, `simplecss`, `data-url`, `strict-num`, `imagesize`, `rgb`,
  `float-cmp`, `euclid`, `polycool`, `siphasher`, `pico-args`, `bytemuck`, `arrayref`, `flate2`,
  `zlib-rs`, `fdeflate`, `miniz_oxide`, `adler2`, `simd-adler32`, `num-traits`, `autocfg`.

- The interface and the Lua page API carry the new colours: the theme's ANSI palette and selection
  as roles (`"red"`, `"bright-red"`, `"selection"`), and values (`"#rrggbb"`, `"#rrggbbaa"`,
  `{ light = "#…", dark = "#…" }`, with an opacity as before: `pencere.alpha(colour, 0.5)`). The
  interface is still `pencere:view@1.0.0`, because it has not been published.

- The crate itself, taken from the terminal that wrote it first: the sandbox, its limits and
  watchdog, the thread a module runs on, and the embedded interpreter. The interface is
  `pencere:view@1.0.0` and the Lua global is `pencere`.
- A helper of the Lua page API that is given the wrong argument (`pencere.text` of a value that is
  not a string or a number, `pencere.spark` of a non-list, `children` of a non-list) raises the
  error at the line of the script that called it, not at a line of the prelude.
