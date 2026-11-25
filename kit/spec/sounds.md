# Sounds

## What a page asks of its sounds

A reply's `sound` plays a sound: a short click when a toggle flips, a chime when a result lands. It is one of:

- the name of a sound of the kit's: `sound = "success"`;
- a sound of the kit's as loud and as fast as the page says: `sound = { name = "tap", volume = 0.5, pitch = 1.2 }`;
- a sound of the page's own, a file: `sound = { file = "sounds/ding.wav", volume = 0.8 }`;
- a list of these, started in the order written: `sound = { "tap", { name = "snap", pitch = 0.8 } }`;
- `false`, which ends every sound the view is playing.

`volume` is from 0 to 1 and is 1 where it is not said. `pitch` is how much faster the sound plays, from 0.5 to 2, and is 1 where it is not said: a sound at 2 is twice as fast and an octave higher, and half as long.

It is no person's permission to give and none to ask: a page that plays a sound reads nothing and takes nothing, and the worst it does is annoy. The person's remedy is the system's volume and silence, the host's own switch for sounds where it has one, or closing the view. The page is not told what came of it, or whether a speaker was there at all, or whether a switch is off.

Any reply may carry it, a press's, a wake's or a program's message's.

Sounds play over each other, as a bell and a click do. A view plays up to eight at once: the ninth silences the one that has played longest. Replies put together (`pencere.merge`, [Replies](replies.md)) keep every sound, in the order of the replies, and a `false` among them stays where it was said: what is said after it plays.

## The sounds of the kit's

`pencere.has("sound.<name>")` says whether the kit has a sound ([Accessibility](accessibility.md)). The names are the API; how each is made may change, as long as it sounds like what it is called. They are made by the kit and are the same on every system: a host never has a sound of its own for a name.

Each name is the job a sound does in an interface, not how it sounds.

| name         | what it is for                                                      | what it is not for                                         |
| ------------ | ------------------------------------------------------------------- | ---------------------------------------------------------- |
| `tap`        | A press that landed: a button, a row, a tab.                        | A press that did something that matters; that is a result. |
| `select`     | A pick among several: a list's row, a menu's item, a segment.       | A toggle, which has two sounds.                            |
| `toggle_on`  | A switch, a checkbox or a mode turned on.                           |                                                            |
| `toggle_off` | The same turned off.                                                |                                                            |
| `type`       | A key that typed: the shortest and quietest.                        | A press of a button.                                       |
| `open`       | A panel, a sheet, a menu or a window that opened.                   |                                                            |
| `close`      | The same, closed.                                                   |                                                            |
| `snap`       | Something clicked into place: a drop that landed, a size that held. | A press.                                                   |
| `copy`       | Text or a file put on the pasteboard.                               |                                                            |
| `delete`     | Something was thrown away.                                          | A cancel.                                                  |
| `success`    | It worked: a save, a payment, a sent form.                          | A step on the way to one.                                  |
| `warning`    | Look at this: something may go wrong.                               | A failure.                                                 |
| `error`      | It failed.                                                          | A person's slip the page can fix without saying it.        |
| `send`       | A message or a file went out.                                       | A saved document.                                          |
| `receive`    | A message or a file came in.                                        | A notification the person asked for.                       |
| `notify`     | Something happened that the person may want to know.                | An answer to what the person just did.                     |
| `attention`  | The person is wanted now: a call, a timer that ran out.             | Anything that can wait.                                    |
| `start`      | A process began: a recording, a run, a download.                    |                                                            |
| `complete`   | A process ended well.                                               | A step of one.                                             |
| `celebrate`  | A milestone: the longest and the brightest.                         | Everything that worked.                                    |

### How loud, and how often

The sounds are of three loudnesses, set by what they say: the ones a person makes (a tap, a toggle, a key) are quiet, because the person knows what they did; the ones that report a result are louder; and a warning, an error and a call for attention are the loudest. A page does not need to set `volume` to make them sit right with each other; it sets it to make them sit right with the rest of what the person hears.

The sounds a person hears often (`tap`, `select`, `type`, `snap` and the toggles) are a little different each time they play, in pitch and in level, as a real key is never struck the same way twice. The others are the same every time.

Use sound for what is not already clear from the screen, and use it little. A sound on every press of a thing that is pressed a hundred times a day is noise, and the person's remedy is to silence the whole view. A sound that tells the person a thing that was out of sight happened (a message came in, a long job ended) earns its place; a sound that repeats what the person just saw does not. Do not use the sounds of the results for things that are not results: a page whose every action says `success` says nothing.

Do not use a sound alone to say something the person must know: a person may have the sound off, and a host may have none ([Accessibility](accessibility.md)).

## A file of the page's own

`file` is a path of the view's bundle (`sounds/ding.wav`), `sha256:` and a digest of a file the view's program attached, or `file:` and a handle of one the person gave it, as an image's is named. The formats are WAV (8, 16, 24 and 32 bit, whole numbers or floats) and Ogg Vorbis, mono or stereo. `pencere.has("sound.wav")` and `pencere.has("sound.ogg")` say whether the kit reads them.

The kit reads the file and decodes it on a thread of its own, and a host is given samples, never the file. A file is at most 8 MiB, like every file of a bundle; the samples a view keeps of its files are at most 64 MiB (the ones read longest ago are let go of first), and a view names at most 1024 different sound files, counted apart from its images' paths.

A page does not wait for a sound: a file that is not decoded yet plays as soon as it is. One that is not decoded a second after it was asked for is dropped and the page's author is told; it stays decoded for the next ask. A `false` in a reply that comes after the ask ends the waiting sound as it ends the playing ones.

## What is a mistake in a sound

The script's mistake, which closes the page with a line of the script:

- a value that is none of the ways above (`true`, a number, a table with both `name` and `file`, a sound that also has entries, a list that has anything but its entries in it such as `{ volume = 1 }`);
- a list that has another list in it;
- a `volume` or a `pitch` out of its range, or one that is not a number;
- more than 64 sounds in one reply (replies put together are one reply: 40 and 30 are 70).

What is only told to the page's author (a diagnostic, which `pencere test` reads with `page:diagnostics()`), with the rest of the reply going through:

- a name the kit has no sound for;
- a file that is not in the bundle, is not a WAV or an Ogg Vorbis file, is too large or too long to hold, or was not decoded in time.

## How a host plays them

What a host does with a sound is in [What a host does](host.md#playing-sounds): it plays them as the system's volume and silence say, up to eight at once for a view, and ends them with the view.

A test reads what the page asked: `page:sounds()` of `pencere test` is the list of them, in order, each a table with the `name` or the `file`, the `volume` and the `pitch` the page said, and what the host was given of the sound (`seconds`, `frames`, `rate`, `channels`, `peak`), or `false` where the page ended them all. A host with no screen has no speaker, so it keeps every ask, however many play at once.

To listen to the sounds of the kit's, `pencere sounds <directory>` writes them there as WAV files.
