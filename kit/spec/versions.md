# Versions

| Kind  | The version is                                                                                       | Source of truth               |
| ----- | ---------------------------------------------------------------------------------------------------- | ----------------------------- |
| `lua` | `0.1`: the page API whose `pencere.version` is the string `"0.1"`, in Luau `0.740` (`kit/luau/pins`) | `luau/prelude`, this document |

`lua` is the one kind of the kit (protocol, Section 5.4), and `0.1` the one version a host opens
while the protocol is a draft. `lua` `0.x` is bound to the release of Luau that the kit pins: a
script that needs syntax or a library function a later Luau added is not a `lua` `0.1` script, and
such a Luau arrives with a new version of `lua`. A host MUST run the kit's own interpreter, the component
`kit/sandbox/luau/luau.wasm`, so that a script means one thing on every host. The interface
[`wit/view.wit`](../wit/view.wit) is the link between that interpreter and the sandbox that runs it:
an internal one, which no view is written against and whose version is no promise to anyone.

While the protocol is a draft, the versions of `lua` are `0.x` (protocol, Section 5.4). A change
that an existing view can notice moves the minor number (`0.2` after `0.1`), and an addition does
not: a new function, a new element, a new colour role, a new event. A host opens one version. A
program written for a version the host does not open is not opened, and says so in a sentence that
names both sides. A script that uses something a later version added asks first whether it exists
(`if pencere.x then`).

From the first general release on, the versions follow the rules of the protocol (Section 5.4):

- A change that no existing view can notice is a new minor version (`1.1`): a new function, a new
  element, a new colour role. A minor version only adds.
- A host opens every published minor version of a major, and the newest major together with the
  one before it.
- A new major version (`2.0`) comes only when a function of the page API has to mean something else
  and a new name cannot do it.
- Every change is one of two sorts: one that applies to every view (a fix of security, of a limit
  or of a bug), or one that applies only to a view opened at the version that carries it (a
  behaviour). The `CHANGELOG` of the package that carries the change says which.
- A new kind of event reaches only a view opened at the version that adds it.
- A script that uses something a later version added asks first whether it exists
  (`if pencere.x then`).
- The kit's library (`@pencere/*`, [The kit's library](library.md)) is a part of the page API: it comes with the host
  and follows the same rules.

The regular expressions of a view (an editor's search, the rules of its indent) are in the syntax of
Rust's `regex` crate, `1.x` at the release the kit pins (its `Cargo.lock`), and one may take at most
4 MiB compiled (`pencere_view::edit::limits::PATTERN_SIZE`): a pattern that would take more is none.
A release of `regex` whose syntax a view could notice (a pattern it reads otherwise, or one it takes
that an earlier refused) arrives with a new version of `lua`, as a later Luau does.
