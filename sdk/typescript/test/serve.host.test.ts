/**
 * `serve` tells the program which terminal it runs in: the `host` of the last `welcome` it acted
 * on (§5.2), again when a restart `welcome` names another (§10.7).
 */

import { describe, expect, it } from "vitest";

import { toWire } from "../src/json.js";
import { ViewControl } from "../src/control.js";
import type { Host } from "../src/messages.js";
import { serveWith } from "../src/serve.js";
import {
  DA_REPLY,
  Rig,
  TOKEN,
  TOKEN_2,
  options,
  payloadOf,
  start,
  welcomePayload,
} from "./support.js";

const closed = (reason: string) => toWire({ reason });
const need = () => toWire({ missing: [] });

const BATERI: Host = { name: "bateri", version: "0.9.1" };
const UPDATED: Host = { name: "bateri", version: "0.9.2" };
const ELSEWHERE: Host = { name: "ghostty", version: "1.1.0" };

/** The `welcome` of a host that starts the view again, with the `host` it names (none: `undefined`). */
const restart = (host: unknown, token = TOKEN_2) =>
  welcomePayload(token, undefined, undefined, { restart: true, host });

/** Starts `serve` whose host is `host` and runs the view until it is shown. */
async function shownBy(host: unknown) {
  const rig = new Rig();
  const control = new ViewControl();
  const told: Host[] = [];
  const seen: (Host | undefined)[] = [];
  const onHost = (host: Host) => {
    told.push(host);
    seen.push(control.host);
  };
  const served = serveWith(options({ control, onHost }), rig.environment());
  const hello = await rig.next("hello");
  rig.say("welcome", welcomePayload(TOKEN, undefined, undefined, { host }), hello.id);
  rig.sayBytes(DA_REPLY);
  const open = await rig.next("open");
  rig.say("need", need(), open.id);
  await rig.settle();
  return { rig, control, served, told, seen };
}

/** The host restarts the view with `payload`: the program opens again, and the host has it. */
async function restarted(rig: Rig, payload: string) {
  rig.say("welcome", payload);
  const open = await rig.next("open");
  rig.say("need", need(), open.id);
  await rig.settle();
}

describe("the terminal the program runs in (§5.2)", () => {
  it("is told at the first welcome, and a control has it, already, inside the notice", async () => {
    const { rig, control, served, told, seen } = await shownBy(BATERI);
    expect(told).toEqual([BATERI]);
    expect(seen).toEqual([BATERI]);
    expect(control.host).toEqual(BATERI);
    rig.say("closed", closed("user"));
    expect(await served).toMatchObject({ kind: "closed", by: "user" });
    // With no `serve` there is no terminal known.
    expect(control.host).toBeUndefined();
  });

  it("is not a reason to stop when the welcome names none, or names it badly", async () => {
    for (const host of [
      undefined,
      "bateri",
      { name: "bateri" },
      { name: 7, version: "1" },
      { name: "bateri", version: null },
      [],
    ]) {
      const { rig, control, served, told } = await shownBy(host);
      expect(told).toEqual([]);
      expect(control.host).toBeUndefined();
      rig.say("closed", closed("user"));
      expect(await served).toMatchObject({ kind: "closed", by: "user" });
    }
  });

  it("skips the members of a host that it does not know", async () => {
    const { told } = await shownBy({ ...BATERI, build: 7 });
    expect(told).toEqual([BATERI]);
  });

  it("is told again when a restart welcome names another host, and not when it names the same", async () => {
    const { rig, control, told } = await shownBy(BATERI);
    await restarted(rig, restart(BATERI));
    expect(told).toEqual([BATERI]);
    await restarted(rig, restart(UPDATED, TOKEN));
    expect(told).toEqual([BATERI, UPDATED]);
    expect(control.host).toEqual(UPDATED);
    await restarted(rig, restart(ELSEWHERE));
    expect(told).toEqual([BATERI, UPDATED, ELSEWHERE]);
    expect(control.host).toEqual(ELSEWHERE);
  });

  it("is unknown after a restart that names none, and told again by the next that names one", async () => {
    const { rig, control, told } = await shownBy(BATERI);
    await restarted(rig, restart(undefined));
    expect(control.host).toBeUndefined();
    expect(told).toEqual([BATERI]);
    await restarted(rig, restart(BATERI, TOKEN));
    expect(control.host).toEqual(BATERI);
    expect(told).toEqual([BATERI, BATERI]);
  });

  it("is told before the program falls back, by a host that shares no version", async () => {
    const rig = new Rig();
    const told: Host[] = [];
    const served = serveWith(options({ onHost: (host) => told.push(host) }), rig.environment());
    const hello = await rig.next("hello");
    rig.say("welcome", payloadOf("to-program/welcome-unsupported.frame"), hello.id);
    expect(await served).toEqual({ kind: "no-view", reason: "no-common-version" });
    expect(told).toEqual([{ name: "pencere-test", version: "0.0.0" }]);
  });

  it("is told by a host that opens none of the kinds the program can show", async () => {
    const rig = new Rig();
    const told: Host[] = [];
    const { served } = await start(rig, { onHost: (host) => told.push(host) }, { views: [] });
    expect(await served).toEqual({ kind: "no-view", reason: "no-common-view" });
    expect(told).toEqual([{ name: "pencere-test", version: "0.0.0" }]);
  });

  it("is the program's own error when the notice throws, and the terminal is given back after the late reply", async () => {
    const rig = new Rig();
    const served = serveWith(
      options({
        onHost: () => {
          throw new Error("no host for me");
        },
      }),
      rig.environment(),
    );
    const failed = served.then(
      () => undefined,
      (error: unknown) => error,
    );
    const hello = await rig.next("hello");
    rig.say("welcome", welcomePayload(TOKEN), hello.id);
    // The terminal's answer to `CSI c` is still on its way: it is waited for, as for any program
    // that ends, and does not reach the shell as typed text.
    await new Promise<void>((resolve) => setImmediate(resolve));
    expect(rig.terminal.calls).toEqual(["enter"]);
    rig.sayBytes(DA_REPLY);
    expect(String(await failed)).toContain("no host for me");
    expect(rig.terminal.calls).toEqual(["enter", "discard", "restore"]);
  });
});
