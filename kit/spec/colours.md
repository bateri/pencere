# Colours

## The forms of a colour

A colour is said in one of these ways, wherever one is taken:

| Form                                                | Meaning                                                                                           |
| --------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| `"accent"`, `"red"`, `"raised2"`, …                 | A **role** of the theme. The way to prefer: it follows the theme the pane wears.                  |
| `"#rrggbb"`, `"#rrggbbaa"`                          | A **value**, sRGB, in either letter case; the last pair is the alpha.                             |
| `{ light = "#…", dark = "#…" }`                     | A value for a light theme and, if the page wants one, another for a dark theme (`dark` optional). |
| `pencere.mix(colour, toward, amount)`               | Any of the first three taken `amount` (`0` to `1`) of the way toward another of them.             |
| `{ colour, alpha }`, `pencere.alpha(colour, alpha)` | Any of the above with an opacity of `0` to `1` on top.                                            |

## Opacity and mixing

The opacity is **multiplied** with the alpha of a value: `pencere.alpha("#80808080", 0.5)` is drawn at a quarter of full opacity.

A mix is made in sRGB, channel by channel, a value's alpha with them, and then the opacity is put on it. A host MUST resolve it so (`Color::resolve` in the engine does).

A mix is how a highlight comes in on its ground and how a word brightens: `pencere.mix("dim", "foreground", level)`.

## The roles

The roles are those of the theme and three tones derived from them:

- the theme's own:
  - `background`, `foreground`, `dim`, `accent`, `cursor`, `success`, `error`, `info` and `warning`;
  - `selection`, the ground of what the person selected, and `border`, the colour of rules and borders.

  These have the **same names as in a page's `context`** (protocol, Sections 9.5 and 11.2);

- the sixteen colours of the ANSI palette by xterm's names: `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white`, and the same eight prefixed `bright-`;
- the kit's three derived tones, below: `raised<n>` (`pencere.raised(n)`), `quiet` and `shadow`.

A name that is none of these is an error at the line of the script that wrote it.

## The derived tones (RECOMMENDED)

The host resolves a role in its theme, and may choose the derived ones as it likes. A host SHOULD use these formulas, so that the same page looks alike on every host.

`mix(a, b, t)` is the colour `t` of the way from `a` to `b`. Each channel is rounded to the nearest value (halves away from zero), and the channels are in sRGB.

| Tone        | Meaning                                                                   | Formula                                                |
| ----------- | ------------------------------------------------------------------------- | ------------------------------------------------------ |
| `raised(n)` | A surface `n` steps above the background; a card is 1, a field on it is 2 | `mix(background, foreground, 0.04 × min(n, 8))`        |
| `quiet`     | Text a step quieter than `dim`: readable and not standing out             | `mix(dim, background, 1/3)`                            |
| `shadow`    | The colour a shadow is cast in                                            | black on a dark theme; the `foreground` on a light one |

### The upper bound

`raised(n)` has an **upper bound**. A host MUST accept any `n` from 0 to 255,[^byte] but treats an `n` above 8 as 8. A page uses `1` to `4` in practice.

[^byte]: The interface carries a byte, and so does the Lua name `raised<n>`.

Because a surface is lighter than the ground on a dark theme and darker on a light one, the formula uses the foreground as the end it moves toward.

### Border

`border` is not a derived tone but a role of the theme, with the name and the meaning it has in a page's `context`. Where the theme gives none, the host makes it up as the protocol says (Section 11.2): `mix(background, foreground, 1/5)`.

The three tones above are for what no theme gives.
