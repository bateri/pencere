/**
 * The proof of the host's own route (specification §4.10): a listener on the loopback of the
 * program's machine that gives one token to one connection, closes before it writes, and closes
 * when its time is up or the view ends.
 */

import { connect } from "node:net";
import type { Socket } from "node:net";
import { describe, expect, it } from "vitest";

import { parseProbe, probedPayload } from "../src/messages.js";
import { Probe } from "../src/probe.js";
import { fixture } from "./support.js";

const TOKEN = "3f9c2a7be1d84056a9c4e7b1d2f80a65";

/** Connects to the loopback at `port`; resolves with the bytes read until the end, or the error code. */
function read(port: number): Promise<string | { readonly code: string }> {
  return new Promise((resolve) => {
    const chunks: Buffer[] = [];
    const socket: Socket = connect({ host: "127.0.0.1", port });
    socket.on("data", (chunk: Buffer) => chunks.push(chunk));
    socket.on("end", () => resolve(Buffer.concat(chunks).toString("latin1")));
    socket.on("error", (error: NodeJS.ErrnoException) => resolve({ code: error.code ?? "?" }));
  });
}

describe("what the host asks and the program answers", () => {
  it("reads a probe as the fixture has it, and a token or a ttl that is not one is not asked", () => {
    const text = fixture("to-program/probe.frame").toString("latin1");
    const payload = /;\{.*\}/.exec(text)?.[0].slice(1) ?? "";
    expect(parseProbe(payload)).toEqual({ token: TOKEN, ttl: 10 });
    expect(parseProbe(`{"token":"${TOKEN}","ttl":30}`)?.ttl).toBe(30);
    for (const bad of [
      `{"token":"${TOKEN}","ttl":0}`,
      `{"token":"${TOKEN}","ttl":31}`,
      `{"token":"${TOKEN}","ttl":1.5}`,
      `{"token":"${TOKEN}","ttl":"10"}`,
      `{"token":"${TOKEN.toUpperCase()}","ttl":10}`,
      `{"token":"${TOKEN.slice(1)}","ttl":10}`,
      `{"ttl":10}`,
      `[]`,
      `not json`,
    ])
      expect(parseProbe(bad), bad).toBeUndefined();
  });

  it("writes a probed as the fixtures have it", () => {
    expect(probedPayload(43211)).toBe('{"port":43211}');
    expect(probedPayload("busy")).toBe('{"code":"busy"}');
    expect(probedPayload("invalid")).toBe('{"code":"invalid"}');
  });
});

describe("the listener", () => {
  it("gives the token to the one connection and refuses the next", async () => {
    const probe = new Probe();
    const port = await probe.open({ token: TOKEN, ttl: 5 });
    expect(typeof port).toBe("number");
    const got = await read(port as number);
    expect(got).toBe(TOKEN);
    // The listener was closed before the token was written: nothing listens now.
    expect(await read(port as number)).toEqual({ code: "ECONNREFUSED" });
    probe.end();
  });

  it("is one at a time: a second is busy until the first is closed", async () => {
    const probe = new Probe();
    const port = await probe.open({ token: TOKEN, ttl: 5 });
    expect(await probe.open({ token: TOKEN, ttl: 5 })).toBe("busy");
    probe.end();
    expect(await read(port as number)).toEqual({ code: "ECONNREFUSED" });
    const again = await probe.open({ token: TOKEN, ttl: 5 });
    expect(typeof again).toBe("number");
    probe.end();
  });

  it("is closed when its time is up", async () => {
    const probe = new Probe();
    const port = await probe.open({ token: TOKEN, ttl: 1 });
    await new Promise((done) => setTimeout(done, 1300));
    expect(await read(port as number)).toEqual({ code: "ECONNREFUSED" });
    // And another may be asked for.
    const again = await probe.open({ token: TOKEN, ttl: 1 });
    expect(typeof again).toBe("number");
    probe.end();
  });

  it("is closed when the view ends", async () => {
    const probe = new Probe();
    const port = await probe.open({ token: TOKEN, ttl: 30 });
    probe.end();
    expect(await read(port as number)).toEqual({ code: "ECONNREFUSED" });
  });

  it("is on the loopback and on no other address", async () => {
    const probe = new Probe();
    const port = (await probe.open({ token: TOKEN, ttl: 5 })) as number;
    // `::1` is not listened on.
    const other = await new Promise<string | { readonly code: string }>((resolve) => {
      const socket = connect({ host: "::1", port });
      socket.on("connect", () => {
        socket.destroy();
        resolve("connected");
      });
      socket.on("error", (error: NodeJS.ErrnoException) => resolve({ code: error.code ?? "?" }));
    });
    expect(other).not.toBe("connected");
    probe.end();
  });
});
