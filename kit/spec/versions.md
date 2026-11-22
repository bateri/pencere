# Versions

| Kind  | The version is                                                                                       | Source of truth                                  |
| ----- | ---------------------------------------------------------------------------------------------------- | ------------------------------------------------ |
| `lua` | `0.1`: the page API whose `pencere.version` is the string `"0.1"`, in Luau `0.740` (`kit/luau/pins`) | `luau/prelude`, [the kit's document](../SPEC.md) |

`lua` is the one kind of the kit, and `0.1` the one version a host opens while the protocol is a
draft; the rules the versions follow are the protocol's ([Section 5.4](../../spec/draft/pencere.md#54-versions)). `lua` `0.x` is bound to the release of Luau that the kit pins: a
script that needs syntax or a library function a later Luau added is not a `lua` `0.1` script, and
such a Luau arrives with a new version of `lua`. The interpreter a host runs is the kit's own ([Section 9.8](../../spec/draft/pencere.md#98-kit-views)). The interface
[`wit/view.wit`](../wit/view.wit) is the link between that interpreter and the sandbox that runs it:
an internal one, which no view is written against and whose version is no promise to anyone.

What moves the minor number and what the major one, and which versions a host opens, in the draft
(`0.x`) and from the first general release on, is the protocol's rule ([Section 5.4](../../spec/draft/pencere.md#54-versions)).
A new event is an addition too. The kit's library (`@pencere/*`, [The kit's library](library.md)) is a
part of the page API and follows the same rules.

The regular expressions of a view (an editor's search, the rules of its indent) are in the syntax of
Rust's `regex` crate, `1.x` at the release the kit pins (its `Cargo.lock`), and one may take at most
4 MiB compiled (`pencere_view::edit::limits::PATTERN_SIZE`): a pattern that would take more is none.
A release of `regex` whose syntax a view could notice (a pattern it reads otherwise, or one it takes
that an earlier refused) arrives with a new version of `lua`, as a later Luau does.
