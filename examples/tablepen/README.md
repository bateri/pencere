# tablepen

A table in the middle of a pipeline. Rows come in on stdin as CSV, the table opens in
the terminal pane, and with `--select` the person chooses rows that go out on stdout, for
whatever comes next:

```sh
tablepen --select servers.csv | tail -n +2 | cut -d, -f1 | xargs -n1 ping -c1
```

```
tablepen [--select] [FILE]
```

- Reads CSV, the first line its header, from `FILE` or from stdin.
- In the view: type to filter (<kbd>/</kbd> puts the cursor there), click a heading to sort,
  <kbd>↑</kbd> <kbd>↓</kbd> to move. With `--select`, <kbd>space</kbd> chooses a row,
  <kbd>a</kbd> chooses every row shown, <kbd>⏎</kbd> or **Choose** hands them back, and
  <kbd>esc</kbd> or **Cancel** hands back nothing.
- With `--select`, the chosen rows are written to stdout as CSV, header first, in the
  table's order, and the status is 0. Nothing chosen writes nothing and ends with 1; ⌃C ends
  it as a signal does.

## Nothing chosen, nothing sent

Every command of a pipeline starts at once: `mail` is already waiting when the table
opens, and it sends whatever it is given when the input ends, even nothing. A step that must
not run without a choice waits for it:

```sh
rows=$(tablepen --select < hosts.csv) && printf '%s\n' "$rows" | mail -s hosts ops@example.com
```

## How it works

stdin and stdout belong to the pipeline, so the program asks for its view on the terminal
itself, its controlling terminal (`/dev/tty`): `serve(server, { filter: true, … })`, in
`src/main.ts`. The terminal sees an ordinary pencere program, and the pipe sees only CSV.

- `src/tool.ts` is an ordinary MCP server: `show_table` (the tool with the view, whose result
  carries the table) and, with `--select`, `choose_rows`, which only the view calls, with the
  places of the chosen rows.
- `src/main.ts` reads the table, opens the view, and once `choose_rows` has the rows closes
  the view itself (`ViewControl.close`) and writes them.
- `view/` is the view, written by hand against the MCP Apps messages, with no build step.

The whole input is read before the view opens, and the view is given at most 10,000 rows
(it says how many there were). Because its stdin is data, a filter is not an MCP server a
chat client can start; `examples/containers` is that kind of tool.

Without a terminal that hosts pencere, `tablepen` prints the table as text, and
`tablepen --select` says it needs one and ends with 2.

## Try it without a terminal that hosts pencere

From the root of the repository, after `make setup` once:

```sh
npx tsc -b
cargo run -p pencere-fake-host -- --call 'choose_rows={"rows":[0,2]}' -- sh -c \
  "printf 'name,size\nweb,3\ndb,40\ncache,5\n' | node examples/tablepen/dist/main.js --select > chosen.csv"
cat chosen.csv
```

The [fake host](../../host/fake/README.md) plays the terminal and a scripted view that
chooses the first and the third row; `chosen.csv` holds the header and those two rows.
