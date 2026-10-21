/**
 * The table tool as an MCP client sees it: the table comes in the result of
 * `show_table`, and `choose_rows`, there only when the person may choose, hands
 * the rows back by their place.
 */

import { Client } from "@modelcontextprotocol/client";
import type { Transport } from "@modelcontextprotocol/server";
import { describe, expect, it } from "vitest";

import { MAX_ROWS, createServer, tableOf, text } from "../src/tool.js";
import type { Table } from "../src/tool.js";

type Message = Parameters<Transport["send"]>[0];

/** Two transports, each delivering what the other sends. */
function pair(): [Transport, Transport] {
  const make = (): Transport & { peer?: Transport } => ({
    async start() {},
    async send(message: Message) {
      const peer = this.peer;
      queueMicrotask(() => peer?.onmessage?.(message));
    },
    async close() {
      this.onclose?.();
    },
  });
  const a = make();
  const b = make();
  a.peer = b;
  b.peer = a;
  return [a, b];
}

const TABLE: Table = {
  columns: ["name", "size"],
  rows: [
    ["web", "3"],
    ["db", "40"],
    ["cache", "5"],
  ],
};

async function connect(table: Table, select: boolean) {
  const heard: number[][] = [];
  const server = createServer(table, { select, chosen: (rows) => heard.push(rows) });
  const [serverSide, clientSide] = pair();
  await server.connect(serverSide);
  const client = new Client({ name: "tool-test", version: "0.0.0" });
  await client.connect(clientSide);
  return { client, heard };
}

describe("the table tool", () => {
  it("gives the view the table, and whether the person may choose", async () => {
    const { client } = await connect(TABLE, false);
    const result = await client.callTool({ name: "show_table", arguments: {} });
    expect(result.structuredContent).toEqual({
      columns: ["name", "size"],
      rows: TABLE.rows,
      total: 3,
      select: false,
    });
    const { tools } = await client.listTools();
    expect(tools.map((tool) => tool.name)).toEqual(["show_table"]);
  });

  it("takes the rows the person chose by their place, once each, in the table's order", async () => {
    const { client, heard } = await connect(TABLE, true);
    const { tools } = await client.listTools();
    expect(tools.find((tool) => tool.name === "choose_rows")?._meta).toMatchObject({
      ui: { visibility: ["app"] },
    });
    const result = await client.callTool({
      name: "choose_rows",
      arguments: { rows: [2, 0, 2, 9] },
    });
    expect(result.isError).toBeFalsy();
    expect(heard).toEqual([[0, 2]]);
    const none = await client.callTool({ name: "choose_rows", arguments: { rows: [7] } });
    expect(none.isError).toBe(true);
    expect(heard).toHaveLength(1);
  });

  it("gives a large table up to its limit and says how many rows there are", async () => {
    const rows = Array.from({ length: MAX_ROWS + 5 }, (_, at) => [String(at)]);
    const { client } = await connect({ columns: ["n"], rows }, false);
    const result = await client.callTool({ name: "show_table", arguments: {} });
    const table = result.structuredContent as { rows: unknown[]; total: number };
    expect(table.rows).toHaveLength(MAX_ROWS);
    expect(table.total).toBe(MAX_ROWS + 5);
  });
});

describe("without a view", () => {
  it("is the header and the rows of CSV, lined up", () => {
    expect(tableOf([])).toBeUndefined();
    expect(text(tableOf([["name", "size"], ["web", "3"], ["cache"]])!)).toBe(
      "name   size\nweb    3\ncache",
    );
  });
});
