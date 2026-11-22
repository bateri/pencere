# motion-lua

How a change of a page moves instead of jumping, a section for each thing an element's `motion`
follows. A page of one Lua file, `src/view/page.lua`, run by `pencere`:

```sh
pencere dev          # run it in this pane, and again when a file changes
pencere build        # one executable: dist/motion-lua
dist/motion-lua      # the view, in a terminal that hosts pencere views
pencere test         # the page's tests, under the host with no screen
```

- a spring that bounces (`ease = "spring"`, `bounce`) under three segments, whose width and place
  are sent elsewhere while they are on their way and keep their speed;
- a panel as high as its tab (`size`);
- a card whose corners and shadow follow the pointer (`rounded`, `shadow`, `color`);
- a header whose padding and words follow a press (`padding`, `text_size`);
- an arrow that turns (`:transform({ rotate = })` with `transform = true`);
- what goes round and round: a status dot that fades in and out and bobbing arrow (`loop`), and a
  spinner (`spin`).

The host does the moving, frame by frame, without asking the page. Nothing moves, and nothing
loops, under Reduce Motion (the person's setting, so the page has no button for it).
