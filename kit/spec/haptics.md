# Haptics

## What a page asks

A reply's `haptic` asks the trackpad to vibrate: a tap under the finger when something clicks into place, a long buzz while a thing is dragged. It is one of:

- the name of a ready pattern: `haptic = "snap"`;
- a ready pattern with its strongest moment as strong as the page says: `haptic = { name = "thud", intensity = 0.5 }`;
- a pattern of the page's own: `haptic = { events = { … } }`;
- `false`, which ends what the view is playing.

It is no person's permission to give and none to ask: a page that vibrates reads nothing and takes nothing, and the worst it does is annoy. The person's remedy is to close the view, or the host's own switch for vibration where it has one. The page is not told what came of it, or whether a device could vibrate at all, or whether a switch is off.

Any reply may carry it, a press's, a wake's or a program's message's.

A view has one vibration at a time: a new one takes the place of the one playing. Replies put together (`pencere.merge`, [Replies](replies.md)) keep the last that says one, and `false` is one.

## The ready patterns

`pencere.has("haptic.<name>")` says whether the kit has a pattern ([Accessibility](accessibility.md)). The names are the API; what each is made of may change, as long as it feels like what it is called.

| name        | what it is for                                                                         |
| ----------- | -------------------------------------------------------------------------------------- |
| `tick`      | The lightest and crispest: a thing under the finger moved by a step, a pick in a list. |
| `tap`       | A press that landed.                                                                   |
| `thud`      | Something heavy set down: a dull knock that dies away.                                 |
| `snap`      | Something clicked into place: the strongest and crispest.                              |
| `bump`      | The finger ran into an edge: a soft, dull knock.                                       |
| `double`    | Two taps close together.                                                               |
| `success`   | It worked: a light tap, then a stronger one.                                           |
| `warning`   | Look at this: two strong taps, the second weaker.                                      |
| `error`     | It failed: three heavy, dull knocks.                                                   |
| `rise`      | Building: a held buzz that grows stronger and sharper.                                 |
| `fall`      | Fading: a held buzz that dies away and dulls.                                          |
| `rumble`    | A steady dull growl.                                                                   |
| `heartbeat` | Two beats and a rest, twice.                                                           |

The ready patterns are light: the strongest moment of any of them is 0.3 of what the device gives ([How strong](#how-strong)). `{ name = "thud", intensity = 0.5 }` is the pattern with its strongest moment at 0.5 and every other moment in the proportion it had to the strongest, so its shape stays: the times of its events and how sharp they are are not touched. It can be made stronger than it was as well as lighter: `{ name = "tick", intensity = 1 }` is a tick as strong as the device goes. Where `intensity` is not said, the pattern is as the kit made it.

A name that is not one of these is not the script's error: the page's author is told (a diagnostic, which `pencere test` reads with `page:diagnostics()`) and the rest of the reply goes through.

## A pattern of the page's own

`events` is a list of events, each a table:

| field           | what it is                                                                          | default     |
| --------------- | ----------------------------------------------------------------------------------- | ----------- |
| `at`            | Seconds from the start of the pattern.                                              | 0           |
| `seconds`       | Seconds the event is held. 0 is a tap.                                              | 0           |
| `intensity`     | How strong it is, 0 to 1, where the event begins ([How strong](#how-strong)).       | 0.3         |
| `intensity_end` | How strong it is where the event ends.                                              | `intensity` |
| `sharpness`     | How sharp it is, from 0 (a dull thud) to 1 (a crisp click), where the event begins. | 0.5         |
| `sharpness_end` | How sharp it is where the event ends.                                               | `sharpness` |

A held event goes from its beginning to its end in a straight line, in both strength and sharpness. The events may be in any order: the pattern is played in the order of their `at`, and those at one time as they are written.

A pattern is at most 256 events and lasts at most 60 seconds (the last event is over by then). A list with no events plays nothing, which ends what was playing, as `false` does.

Strength and sharpness are what a person feels, not what a motor does: how a sharpness comes to a frequency is the host's to know.

## How strong

`intensity` is a share of the strongest the device gives: 1 is all of it, 0 is nothing, and 0.5 is half of what the device can do. It is the same scale for the ready patterns, for a ready pattern's `intensity` and for a pattern of the page's own, and it does not change with the host: another host's 0.3 is 0.3 of its own device's strongest.

On bateri, on a Mac's trackpad, 0.18 is a light tick and 0.3 a firm click, and a short, sharp tap under 0.1 is hardly felt. So `intensity` is 0.3 where an event does not say, and the ready patterns top out at 0.3. 1 is the whole of the motor: use it for a moment, not for a long buzz.

## What is a mistake

The script's mistake, which closes the page with a line of the script:

- a value that is none of the four ways above (`true`, a number, a table with both `name` and `events` or with neither);
- a number out of its range, or one that is not a number;
- an event that is not a table;
- more than 256 events, or one that ends after 60 seconds.

## How a host does it

What a host does with a vibration is in [What a host does](host.md#vibration-of-the-trackpad): it vibrates only while a finger is on the glass of a device that can, and only for the view the person is working in.

A test reads what the page asked: `page:haptics()` of `pencere test` is the list of them, in order, each the events of a pattern or `false` where the page ended one. A host with no screen has no finger and no device, so it keeps every ask.
