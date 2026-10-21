/**
 * The table tool: an MCP server whose `show_table` tool has a view of one table,
 * read before the server starts. Asked to let the person choose (`select`), it also
 * has `choose_rows`, which only the view calls: the rows come back by their place in
 * the table, and `chosen` hears of them.
 *
 * Nothing in this file knows about pencere or about the pipeline: `main.ts` reads the
 * table, asks the terminal, and writes what was chosen.
 */

import { McpServer } from "@modelcontextprotocol/server";
import {
  RESOURCE_MIME_TYPE,
  registerAppResource,
  registerAppTool,
} from "@modelcontextprotocol/ext-apps/server";
import { readFile } from "node:fs/promises";
import { z } from "zod";

export const VIEW_URI = "ui://tablepen/view";

/** Where the view's files are: `index.html`, `style.css` and `app.js`. */
export const VIEW_DIRECTORY = new URL("../view/", import.meta.url);

/**
 * The most rows the view is given. A larger table is shown up to here and says so:
 * a tool result crosses the terminal's stream whole, and a page of this many rows is
 * as much as a person scrolls through anyway.
 */
export const MAX_ROWS = 10_000;

export interface Table {
  columns: string[];
  /** As read: a row may have fewer or more fields than there are columns. */
  rows: string[][];
}

/** The table of CSV records: the first is the header. `undefined` when there is none. */
export function tableOf(records: string[][]): Table | undefined {
  const [columns, ...rows] = records;
  return columns === undefined ? undefined : { columns, rows };
}

/** The table as text, its columns lined up: what is shown where there is no view. */
export function text(table: Table, widest = 40): string {
  const cut = (value: string) => (value.length > widest ? `${value.slice(0, widest - 1)}…` : value);
  const lines = [table.columns, ...table.rows].map((row) =>
    table.columns.map((_, at) => cut(row[at] ?? "")),
  );
  const widths = table.columns.map((_, at) => Math.max(...lines.map((line) => line[at]!.length)));
  return lines
    .map((line) =>
      line
        .map((value, at) => value.padEnd(widths[at]!))
        .join("  ")
        .trimEnd(),
    )
    .join("\n");
}

export interface ServerOptions {
  /** The view lets the person choose rows, and `choose_rows` takes them. */
  select: boolean;
  /** The rows the person chose, by their place in `table.rows`, in the table's order. */
  chosen(rows: number[]): void;
}

/**
 * The view as one self-contained document, for a host that reads it with
 * `resources/read` instead of taking the three files as a bundle.
 */
export async function inlineDocument(directory: URL = VIEW_DIRECTORY): Promise<string> {
  const read = (name: string) => readFile(new URL(name, directory), "utf8");
  const [page, style, script] = await Promise.all([
    read("index.html"),
    read("style.css"),
    read("app.js"),
  ]);
  const link = /<link rel="stylesheet" href="style\.css"\s*\/?>/;
  const tag = /<script src="app\.js"><\/script>/;
  if (!link.test(page) || !tag.test(page)) {
    throw new Error("index.html no longer links style.css and app.js the way inlineDocument reads");
  }
  return page
    .replace(link, () => `<style>\n${style}</style>`)
    .replace(tag, () => `<script>\n${script}</script>`);
}

export function createServer(table: Table, options: ServerOptions): McpServer {
  const server = new McpServer({ name: "tablepen", version: "1.0.0" });
  const shown = table.rows.slice(0, MAX_ROWS);
  const size = `${table.rows.length} rows, ${table.columns.length} columns`;

  registerAppTool(
    server,
    "show_table",
    {
      // What a terminal writes above the view, in place of `show_table`.
      title: "Table",
      description: "Show the table",
      _meta: { ui: { resourceUri: VIEW_URI } },
    },
    async () => ({
      content: [{ type: "text" as const, text: `${size}: ${table.columns.join(", ")}` }],
      structuredContent: {
        columns: table.columns,
        rows: shown,
        total: table.rows.length,
        select: options.select,
      },
    }),
  );

  if (options.select) {
    server.registerTool(
      "choose_rows",
      {
        title: "Choose rows",
        description: "Hand back the rows the person chose, by their place in the table",
        inputSchema: z.object({
          rows: z.array(z.number().int().min(0)).describe("Places of rows, counted from 0"),
        }),
        _meta: { ui: { visibility: ["app"] } },
      },
      async (args) => {
        const places = [...new Set(args.rows)]
          .filter((at) => at < shown.length)
          .sort((a, b) => a - b);
        if (places.length === 0) {
          return {
            isError: true,
            content: [{ type: "text" as const, text: "No row of the table was chosen." }],
          };
        }
        options.chosen(places);
        return {
          content: [{ type: "text" as const, text: `Chose ${places.length} of ${size}.` }],
        };
      },
    );
  }

  registerAppResource(server, "Table", VIEW_URI, {}, async (uri) => ({
    contents: [{ uri: uri.href, mimeType: RESOURCE_MIME_TYPE, text: await inlineDocument() }],
  }));
  return server;
}
