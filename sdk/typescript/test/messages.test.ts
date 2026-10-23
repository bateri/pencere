/** The payloads: what the program writes is what the fixtures say, and what a host sends is read leniently. */

import { describe, expect, it } from "vitest";

import { bundleFromFiles } from "../src/bundle.js";
import {
  VERSIONS,
  byePayload,
  dismissPayload,
  helloPayload,
  openPayload,
  parseClosed,
  parseNeed,
  parseWelcome,
} from "../src/messages.js";
import { EXAMPLE_FILES, TOKEN, payloadOf } from "./support.js";

const exampleBundle = () =>
  bundleFromFiles(
    Object.entries(EXAMPLE_FILES).map(([path, text]) => ({ path, data: Buffer.from(text) })),
    { meta: { csp: { connectDomains: ["https://api.example.com"] }, prefersBorder: false } },
  );

describe("what the program writes", () => {
  it("is the hello of the fixtures", () => {
    expect(helloPayload()).toBe(payloadOf("to-host/hello.frame"));
  });

  it("is the open of the fixtures, with and without a bundle", () => {
    const tool = { name: "list_containers" };
    expect(openPayload({ token: TOKEN, view: "web", tool })).toBe(payloadOf("to-host/open.frame"));
    expect(openPayload({ token: TOKEN, view: "web", tool, bundle: exampleBundle().manifest })).toBe(
      payloadOf("to-host/open-bundle.frame"),
    );
  });

  it("is the bye of the fixtures: the token first, then why", () => {
    expect(byePayload(TOKEN, "signal")).toBe(payloadOf("to-host/bye.frame"));
  });

  it("is the dismiss of the fixtures: the token of the view, and nothing else", () => {
    expect(dismissPayload(TOKEN)).toBe(payloadOf("to-host/dismiss.frame"));
  });

  it("writes the arguments of the tool as given", () => {
    expect(
      openPayload({
        token: TOKEN,
        view: "web",
        tool: { name: "t", arguments: { all: true, n: 2 } },
      }),
    ).toBe(`{"token":"${TOKEN}","view":"web","tool":{"name":"t","arguments":{"all":true,"n":2}}}`);
  });
});

describe("a welcome", () => {
  it("with a version and a token is an offer", () => {
    expect(parseWelcome(payloadOf("to-program/welcome.frame"), VERSIONS)).toEqual({
      kind: "offer",
      version: 1,
      token: TOKEN,
      features: ["bundle", "dismiss"],
      views: ["web"],
      restart: false,
    });
  });

  it("lists the kinds of view the program can open, in the order it listed them", () => {
    const welcome = payloadOf("to-program/welcome-wasm.frame");
    expect(parseWelcome(welcome, VERSIONS)).toMatchObject({ views: ["web"] });
    expect(parseWelcome(welcome, VERSIONS, ["web", "wasm"])).toMatchObject({
      views: ["wasm", "web"],
    });
  });

  it("with no kind of view this program can show is not an offer", () => {
    expect(parseWelcome(payloadOf("to-program/welcome-no-views.frame"), VERSIONS)).toEqual({
      kind: "no-views",
    });
    expect(parseWelcome(payloadOf("to-program/welcome-wasm.frame"), VERSIONS, ["lua"])).toEqual({
      kind: "no-views",
    });
    expect(
      parseWelcome(`{"version":1,"token":"${TOKEN}","features":["bundle"]}`, VERSIONS),
    ).toEqual({ kind: "no-views" });
  });

  it("that asks to start again says so", () => {
    expect(parseWelcome(payloadOf("to-program/welcome-restart.frame"), VERSIONS)).toMatchObject({
      kind: "offer",
      restart: true,
    });
  });

  it("with supported and no version shares no version", () => {
    expect(parseWelcome(payloadOf("to-program/welcome-unsupported.frame"), VERSIONS)).toEqual({
      kind: "unsupported",
      supported: [1],
    });
  });

  it("with a version the program did not offer shares none either", () => {
    expect(parseWelcome(payloadOf("to-program/welcome.frame"), [2])).toEqual({
      kind: "unsupported",
      supported: [],
    });
  });

  it("is ignored when it cannot be understood", () => {
    for (const payload of [
      "not json",
      "[]",
      "{}",
      '{"version":1}',
      '{"version":1,"token":7}',
      '{"version":1,"token":"short"}',
      `{"version":"1","token":"${TOKEN}"}`,
      `{"version":0,"token":"${TOKEN}"}`,
      `{"version":1.5,"token":"${TOKEN}"}`,
    ])
      expect(parseWelcome(payload, VERSIONS), payload).toBeUndefined();
  });

  it("takes no feature that is not a string", () => {
    expect(
      parseWelcome(
        `{"version":1,"token":"${TOKEN}","features":["bundle",1,null],"views":["web",2]}`,
        VERSIONS,
      ),
    ).toMatchObject({ features: ["bundle"], views: ["web"] });
    expect(
      parseWelcome(
        `{"version":1,"token":"${TOKEN}","features":"bundle","views":["web"]}`,
        VERSIONS,
      ),
    ).toMatchObject({ features: [] });
  });
});

describe("a need", () => {
  it("is the digests in the host's order", () => {
    expect(parseNeed(payloadOf("to-program/need.frame"))).toEqual([
      "bcc81d22a717681517bc99bbad72438377e02c19583fae67522475fd8b3ca62c",
    ]);
    expect(parseNeed(payloadOf("to-program/need-empty.frame"))).toEqual([]);
    expect(parseNeed(payloadOf("to-program/need-chunked.frame"))).toHaveLength(12);
  });

  it("is ignored without a list, and skips what is no digest", () => {
    expect(parseNeed("{}")).toBeUndefined();
    expect(parseNeed('{"missing":"x"}')).toBeUndefined();
    expect(parseNeed(`{"missing":["nope",7,"${"a".repeat(64)}","${"A".repeat(64)}"]}`)).toEqual([
      "a".repeat(64),
    ]);
  });
});

describe("a closed", () => {
  it("has its reason and its code", () => {
    expect(parseClosed(payloadOf("to-program/closed-refused.frame"))).toEqual({
      reason: "refused",
      code: "token",
    });
    expect(parseClosed(payloadOf("to-program/closed-error.frame"))).toEqual({
      reason: "error",
      code: "bundle",
    });
    expect(parseClosed(payloadOf("to-program/closed-user.frame"))).toEqual({ reason: "user" });
    expect(parseClosed(payloadOf("to-program/closed-program.frame"))).toEqual({
      reason: "program",
    });
  });

  it("with a reason this version does not know still ends the view, as an error", () => {
    expect(parseClosed('{"reason":"later"}')).toEqual({ reason: "error" });
  });

  it("is ignored without a reason", () => {
    for (const payload of ["not json", "[]", "{}", '{"reason":1}'])
      expect(parseClosed(payload), payload).toBeUndefined();
  });
});
