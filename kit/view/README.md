# pencere-view

The page engine of [pencere](https://github.com/bateri/pencere)'s native views: a tree of boxes and
lines of text, laid out as flexbox and grid, that comes out as a draw list a terminal draws.

A page is a function from the size it is given to an `Element` tree, in the chain of a style sheet's
properties (`div().row().gap(8.0).child(...)`), in points, with colours named for the theme. The
engine draws nothing and links to nothing of the terminal: the host measures the text it also draws,
resolves the names of colours in its theme and turns the list into what it draws. It depends on the
layout crate (`taffy`) and nothing else.

It is the engine of the kit; see [`kit/README.md`](../README.md) for the rest, and the specification
(Section 8.8) for how a terminal opens the views that use it.

## License

Apache-2.0 — see [LICENSE](LICENSE).
