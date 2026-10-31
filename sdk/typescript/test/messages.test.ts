/** The payloads: what the program writes is what the fixtures say, and what a host sends is read leniently. */

import { describe, expect, it } from "vitest";

import { bundleFromFiles } from "../src/bundle.js";
import type { BundleManifest } from "../src/bundle.js";
import {
  VERSIONS,
  byePayload,
  callPayload,
  cancelPayload,
  chooseView,
  dismissPayload,
  failurePayload,
  fetchPayload,
  helloPayload,
  openPayload,
  parseClosed,
  parseFetched,
  parseNeed,
  parseStaged,
  parseViewRequest,
  parseWelcome,
  progressPayload,
  pushPayload,
  readViews,
  responsePayload,
  stagePayload,
  subscribePayload,
  unstagePayload,
} from "../src/messages.js";
import type { OpenFields, WelcomeOffer } from "../src/messages.js";
import { EXAMPLE_FILES, TOKEN, TOKEN_2, WASM_HEADER, offers, payloadOf } from "./support.js";

const web = bundleFromFiles(
  Object.entries(EXAMPLE_FILES).map(([path, text]) => ({ path, data: Buffer.from(text) })),
);

const component = bundleFromFiles([{ path: "view.wasm", data: WASM_HEADER }], {
  entry: "view.wasm",
});

describe("what the program writes", () => {
  it("is the hello of the fixtures: one kind with its versions", () => {
    expect(helloPayload(offers("web"))).toBe(payloadOf("to-host/hello.frame"));
  });

  it("is the hello of three kinds, one of them in two versions, in the order given", () => {
    expect(helloPayload(offers(["wasm", "0.5", "0.4"], "lua", "web"))).toBe(
      payloadOf("to-host/hello-views.frame"),
    );
  });

  it("is the open of a page: the kind and its version, the names, what it asks to reach, the bundle", () => {
    const open: OpenFields = {
      token: TOKEN,
      view: "web",
      version: "1.0",
      program: "containers",
      title: "Containers",
      access: { connect: ["https://api.example.com"], permissions: ["clipboardWrite"] },
      bundle: web.manifest,
    };
    expect(openPayload(open)).toBe(payloadOf("to-host/open-web.frame"));
  });

  it("is the open of a kit view: no access", () => {
    const open = (view: "wasm" | "lua", bundle: BundleManifest): OpenFields => ({
      token: TOKEN,
      view,
      version: "0.5",
      program: "demopen",
      title: "Containers",
      bundle,
    });
    expect(openPayload(open("wasm", component.manifest))).toBe(
      payloadOf("to-host/open-wasm.frame"),
    );
    // The script of the fixture is not among the files here: its manifest is read from the fixture.
    const lua = payloadOf("to-host/open-lua.frame");
    const manifest = (JSON.parse(lua) as { bundle: BundleManifest }).bundle;
    expect(openPayload(open("lua", manifest))).toBe(lua);
  });

  it("is the open with an icon: the icon's members in the order of Appendix A", () => {
    const src =
      "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/q842iQAAAABJRU5ErkJggg==";
    const open: OpenFields = {
      token: TOKEN,
      view: "wasm",
      version: "0.5",
      program: "demopen",
      title: "Containers",
      // Written here in another order than the wire's.
      icons: [{ theme: "dark", sizes: ["1x1"], mimeType: "image/png", src }],
      bundle: component.manifest,
    };
    expect(openPayload(open)).toBe(payloadOf("to-host/open-icons.frame"));
  });

  it("writes the members of an open in the wire's order whatever order they were given in", () => {
    const text = openPayload({
      bundle: component.manifest,
      access: { permissions: ["camera"], connect: ["https://a.example"] },
      title: "t",
      program: "p",
      version: "0.5",
      view: "web",
      token: TOKEN,
    });
    expect(Object.keys(JSON.parse(text) as object)).toEqual([
      "token",
      "view",
      "version",
      "program",
      "title",
      "access",
      "bundle",
    ]);
    expect(text).toContain('"access":{"connect":["https://a.example"],"permissions":["camera"]}');
  });

  it("is the bye of the fixtures: the token first, then why", () => {
    expect(byePayload(TOKEN, "signal")).toBe(payloadOf("to-host/bye.frame"));
  });

  it("is the dismiss of the fixtures: the token of the view, and nothing else", () => {
    expect(dismissPayload(TOKEN)).toBe(payloadOf("to-host/dismiss.frame"));
  });

  it("are the stage, the fetch and the unstage of the fixtures", () => {
    const handle = "664de63fcac3e6e61bd577ba1ea4b4be";
    expect(stagePayload(handle, 0)).toBe(payloadOf("to-host/stage.frame"));
    expect(fetchPayload(handle, 11, 0)).toBe(payloadOf("to-host/fetch.frame"));
    expect(unstagePayload(4)).toBe(payloadOf("to-host/unstage.frame"));
  });
});

describe("what a host says of a stage and of a fetch", () => {
  const pipe = "0b5d7c1e9a3f4682d0e1c7a95b3f8d24";

  it("is read from the fixtures: the pipe, then the end or the reason", () => {
    expect(parseStaged(payloadOf("to-program/staged-pipe.frame"))).toEqual({ kind: "pipe", pipe });
    expect(parseStaged(payloadOf("to-program/staged-sent.frame"))).toEqual({
      kind: "sent",
      sent: 11,
    });
    expect(parseStaged(payloadOf("to-program/staged-error.frame"))).toEqual({
      kind: "failed",
      code: "gone",
    });
    expect(parseFetched(payloadOf("to-program/fetched-pipe.frame"))).toEqual({
      kind: "pipe",
      pipe,
    });
    expect(parseFetched(payloadOf("to-program/fetched-saved.frame"))).toEqual({
      kind: "saved",
      saved: 11,
    });
    expect(parseFetched(payloadOf("to-program/fetched-route.frame"))).toEqual({
      kind: "failed",
      code: "route",
      at: 4,
    });
  });

  it("is nothing when it is none of those, and a pipe that is not 32 hex digits is not one", () => {
    for (const parse of [parseStaged, parseFetched]) {
      expect(parse("{}")).toBeUndefined();
      expect(parse("[]")).toBeUndefined();
      expect(parse('{"pipe":"../x"}')).toBeUndefined();
      expect(parse('{"pipe":"0B5D7C1E9A3F4682D0E1C7A95B3F8D24"}')).toBeUndefined();
      expect(parse('{"sent":-1,"saved":1.5}')).toBeUndefined();
    }
  });
});

describe("the six messages", () => {
  it("are written as the fixtures have them", () => {
    expect(responsePayload(1, { containers: [{ id: "c1", name: "web", state: "running" }] })).toBe(
      payloadOf("to-host/msg-response.frame"),
    );
    expect(pushPayload(3, { id: "c1", state: "exited" })).toBe(payloadOf("to-host/msg-push.frame"));
    expect(progressPayload(4, 4, 10, "layer 4 of 10")).toBe(
      payloadOf("to-host/msg-progress.frame"),
    );
    expect(callPayload(1, "list", { all: true })).toBe(payloadOf("to-program/msg-call.frame"));
    expect(subscribePayload(2, "stats", { name: "web" })).toBe(
      payloadOf("to-program/msg-subscribe.frame"),
    );
    expect(cancelPayload(2)).toBe(payloadOf("to-program/msg-cancel.frame"));
  });

  it("have their members in the order of §9.2, and leave out the optional ones that are not given", () => {
    expect(callPayload(1, "m")).toBe('{"call":{"id":1,"method":"m"}}');
    expect(subscribePayload(2, "t")).toBe('{"subscribe":{"id":2,"topic":"t"}}');
    expect(progressPayload(4, 1)).toBe('{"progress":{"id":4,"done":1}}');
    expect(progressPayload(4, 1, undefined, "n")).toBe('{"progress":{"id":4,"done":1,"note":"n"}}');
    expect(failurePayload(5, "gone")).toBe('{"response":{"id":5,"error":{"code":"gone"}}}');
    expect(failurePayload(5, "gone", "it is gone")).toBe(
      '{"response":{"id":5,"error":{"code":"gone","message":"it is gone"}}}',
    );
  });

  it("never have a result that is null: nothing to say is {}", () => {
    expect(responsePayload(1, undefined)).toBe('{"response":{"id":1,"result":{}}}');
    expect(responsePayload(1, null)).toBe('{"response":{"id":1,"result":{}}}');
    expect(responsePayload(1, 0)).toBe('{"response":{"id":1,"result":0}}');
    expect(responsePayload(1, false)).toBe('{"response":{"id":1,"result":false}}');
    expect(pushPayload(1, undefined)).toBe('{"push":{"id":1,"value":null}}');
  });

  it("carry text the wire may not as escapes", () => {
    const text = responsePayload(1, { s: "é\u007f\u{1F600}\n" });
    expect(text).toBe('{"response":{"id":1,"result":{"s":"\\u00e9\\u007f\\ud83d\\ude00\\n"}}}');
    expect(/^[\x20-\x7e]*$/.test(text)).toBe(true);
  });

  it("are read from the view: a call, a subscription, a cancel", () => {
    expect(parseViewRequest(payloadOf("to-program/msg-call.frame"))).toEqual({
      type: "call",
      id: 1,
      method: "list",
      params: { all: true },
    });
    expect(parseViewRequest(payloadOf("to-program/msg-subscribe.frame"))).toEqual({
      type: "subscribe",
      id: 2,
      topic: "stats",
      params: { name: "web" },
    });
    expect(parseViewRequest(payloadOf("to-program/msg-cancel.frame"))).toEqual({
      type: "cancel",
      id: 2,
    });
  });

  it("are read from a longer message of the view, its text unescaped", () => {
    const request = parseViewRequest(payloadOf("to-program/msg-chunked.frame"));
    expect(request).toMatchObject({ type: "call", id: 4, method: "stop" });
    const params = (request as { params: { id: string; reason: string } }).params;
    expect(params.id).toBe("c3");
    expect(params.reason.startsWith("Arrêt demandé depuis la vue. ")).toBe(true);
  });

  it("have params only when the view sent them, and any value counts, null too", () => {
    expect(parseViewRequest('{"call":{"id":1,"method":"m"}}')).toEqual({
      type: "call",
      id: 1,
      method: "m",
    });
    expect(parseViewRequest('{"call":{"id":1,"method":"m","params":null}}')).toEqual({
      type: "call",
      id: 1,
      method: "m",
      params: null,
    });
  });

  it("are ignored from the view when they are not what §9.2 says", () => {
    const ignored = [
      "not json",
      "[]",
      "null",
      "1",
      '"call"',
      "{}",
      // another member, or the program's own messages: the view does not send them
      '{"response":{"id":1,"result":{}}}',
      '{"push":{"id":1,"value":1}}',
      '{"progress":{"id":1,"done":1}}',
      '{"close":{}}',
      '{"context":{}}',
      // two members: not one of the six
      '{"call":{"id":1,"method":"m"},"cancel":{"id":1}}',
      '{"call":{"id":1,"method":"m"},"x":1}',
      // the value is not an object
      '{"call":1}',
      '{"call":null}',
      '{"call":[]}',
      '{"cancel":"1"}',
      // the id is missing, zero, negative, fractional, not a number, too large to be exact
      '{"call":{"method":"m"}}',
      '{"call":{"id":0,"method":"m"}}',
      '{"call":{"id":-1,"method":"m"}}',
      '{"call":{"id":1.5,"method":"m"}}',
      '{"call":{"id":"1","method":"m"}}',
      '{"call":{"id":1e300,"method":"m"}}',
      '{"call":{"id":9007199254740992,"method":"m"}}',
      '{"cancel":{"id":0}}',
      '{"cancel":{}}',
      // the other members are missing or of the wrong kind
      '{"call":{"id":1}}',
      '{"call":{"id":1,"method":2}}',
      '{"subscribe":{"id":1}}',
      '{"subscribe":{"id":1,"topic":null}}',
    ];
    for (const text of ignored) expect(parseViewRequest(text), text).toBeUndefined();
  });

  it("take the greatest id a JSON number holds exactly, and no more (§9.2)", () => {
    expect(parseViewRequest('{"cancel":{"id":9007199254740991}}')).toEqual({
      type: "cancel",
      id: 9007199254740991,
    });
  });

  it("ignore a message nested 128 levels deep, the outermost counting as the first (§4.5)", () => {
    const nested = (levels: number): string =>
      `{"call":{"id":1,"method":"m","params":${"[".repeat(levels - 2)}${"]".repeat(levels - 2)}}}`;
    expect(parseViewRequest(nested(127))).toMatchObject({ type: "call", id: 1 });
    expect(parseViewRequest(nested(128))).toBeUndefined();
    // A bracket in a string is not a level.
    expect(parseViewRequest(`{"call":{"id":1,"method":"${"[".repeat(300)}"}}`)).toMatchObject({
      type: "call",
    });
  });

  it("ignore members they do not know inside the object", () => {
    expect(parseViewRequest('{"cancel":{"id":3,"why":"bored"}}')).toEqual({
      type: "cancel",
      id: 3,
    });
  });
});

describe("a welcome", () => {
  const offer = (name: string): WelcomeOffer => {
    const welcome = parseWelcome(payloadOf(name), VERSIONS);
    if (welcome?.kind !== "offer") throw new Error(`${name} is no offer`);
    return welcome;
  };

  it("that lists stage and says where the folder is carries it, and one that does not list it has none", () => {
    expect(offer("to-program/welcome-stage.frame")).toMatchObject({
      features: ["dismiss", "stage"],
      stage: {
        dir: expect.stringMatching(/^\//) as string,
        token: expect.stringMatching(/^[0-9a-f]{32}$/) as string,
      },
    });
    expect(offer("to-program/welcome.frame")).not.toHaveProperty("stage");
    const token = "0b5d7c1e9a3f4682d0e1c7a95b3f8d24";
    const welcome = (features: string, stage: string): string =>
      `{"version":1,"token":"${TOKEN}","features":[${features}],${stage}"views":[]}`;
    const stageOf = (text: string): unknown =>
      (parseWelcome(text, VERSIONS) as { stage?: unknown } | undefined)?.stage;
    expect(stageOf(welcome('"stage"', `"stage":{"dir":"/a","token":"${token}"},`))).toEqual({
      dir: "/a",
      token,
    });
    // Not listed, not said, or said badly: no offer.
    expect(
      stageOf(welcome('"dismiss"', `"stage":{"dir":"/a","token":"${token}"},`)),
    ).toBeUndefined();
    expect(stageOf(welcome('"stage"', ""))).toBeUndefined();
    for (const bad of [
      `"stage":{"dir":"a","token":"${token}"},`,
      `"stage":{"dir":"/a","token":"${token.toUpperCase()}"},`,
      '"stage":{"dir":"/a","token":"0b5d"},',
      `"stage":{"token":"${token}"},`,
      '"stage":"/a",',
    ]) {
      expect(stageOf(welcome('"stage"', bad)), bad).toBeUndefined();
    }
  });

  it("with a version and a token is an offer, with the kinds the host opens and their versions", () => {
    expect(parseWelcome(payloadOf("to-program/welcome.frame"), VERSIONS)).toEqual({
      kind: "offer",
      version: 1,
      token: TOKEN,
      host: { name: "pencere-test", version: "0.0.0" },
      features: ["dismiss"],
      views: [{ kind: "web", versions: ["1.0"] }],
      opens: [],
      restart: false,
    });
  });

  it("lists the kinds in the order the host wrote them, each with its versions in its order", () => {
    expect(offer("to-program/welcome-wasm.frame").views).toEqual([
      { kind: "wasm", versions: ["0.5"] },
      { kind: "web", versions: ["1.0"] },
    ]);
    expect(offer("to-program/welcome-versions.frame").views).toEqual([
      { kind: "wasm", versions: ["0.5", "0.4"] },
      { kind: "web", versions: ["1.0"] },
    ]);
  });

  it("with no kind in common is still an offer: there is a host, and nothing to open", () => {
    expect(offer("to-program/welcome-no-views.frame")).toMatchObject({ token: TOKEN, views: [] });
  });

  it("unsolicited, with restart, is an offer that says so", () => {
    expect(offer("to-program/welcome-restart.frame")).toMatchObject({
      token: TOKEN_2,
      restart: true,
    });
  });

  it("with only the versions the host speaks is an unsupported one", () => {
    expect(parseWelcome(payloadOf("to-program/welcome-unsupported.frame"), VERSIONS)).toEqual({
      kind: "unsupported",
      supported: [1],
    });
  });

  it("with a version the program did not offer is an unsupported one", () => {
    const welcome = payloadOf("to-program/welcome.frame").replace('"version":1', '"version":7');
    expect(parseWelcome(welcome, VERSIONS)).toEqual({ kind: "unsupported", supported: [] });
    expect(parseWelcome(welcome, [7])).toMatchObject({ kind: "offer", version: 7 });
  });

  it("is not read when it is not a welcome", () => {
    const good = payloadOf("to-program/welcome.frame");
    expect(parseWelcome("not json", VERSIONS)).toBeUndefined();
    expect(parseWelcome("[]", VERSIONS)).toBeUndefined();
    expect(parseWelcome("{}", VERSIONS)).toBeUndefined();
    expect(parseWelcome(good.replace(TOKEN, "short"), VERSIONS)).toBeUndefined();
    expect(parseWelcome(good.replace('"token":"', '"tok":"'), VERSIONS)).toBeUndefined();
    expect(parseWelcome(good.replace('"version":1', '"version":0'), VERSIONS)).toBeUndefined();
    expect(parseWelcome(good.replace('"version":1', '"version":"1"'), VERSIONS)).toBeUndefined();
  });

  it("reads the kinds leniently: what is not an entry of §5.2 is left out, a kind counts once", () => {
    expect(
      readViews([
        { kind: "web", versions: ["1.0", "nine", 2, "1.0.0"] },
        { kind: "web", versions: ["2.0"] },
        { kind: "wasm" },
        { kind: "lua", versions: [] },
        { kind: 3, versions: ["0.5"] },
        "wasm",
        null,
        { kind: "lua", versions: ["0.5"] },
      ]),
    ).toEqual([
      { kind: "web", versions: ["1.0"] },
      { kind: "lua", versions: ["0.5"] },
    ]);
    expect(readViews(undefined)).toEqual([]);
    expect(readViews({ kind: "web", versions: ["1.0"] })).toEqual([]);
  });
});

describe("choosing the view", () => {
  const mine = offers(["wasm", "0.6", "0.5", "0.4"], "lua", "web");

  it("is the first kind of the program's list that the host lists, at the first version it lists", () => {
    // session/several-versions: the host opens wasm at 0.5 and 0.4, and web.
    const welcome = parseWelcome(payloadOf("to-program/welcome-versions.frame"), VERSIONS);
    if (welcome?.kind !== "offer") throw new Error("no offer");
    expect(chooseView(mine, welcome.views)).toEqual({ kind: "wasm", version: "0.5" });
  });

  it("follows the program's order of kinds and not the host's", () => {
    expect(chooseView(offers("web", ["wasm", "0.5"]), offers(["wasm", "0.5"], "web"))).toEqual({
      kind: "web",
      version: "1.0",
    });
  });

  it("follows the program's order of versions when the host lists its own order (§10.7)", () => {
    expect(chooseView(offers(["wasm", "0.5", "0.4"]), offers(["wasm", "0.4", "0.5"]))).toEqual({
      kind: "wasm",
      version: "0.5",
    });
  });

  it("goes on to the next kind when a kind has no version in common (session/no-common-version)", () => {
    expect(chooseView(offers(["wasm", "0.9"], "web"), offers("web"))).toEqual({
      kind: "web",
      version: "1.0",
    });
    expect(chooseView(offers(["wasm", "0.9"]), offers(["wasm", "0.4", "0.5"]))).toBeUndefined();
  });

  it("is nothing when the host lists nothing, or a kind the program did not list", () => {
    expect(chooseView(mine, [])).toBeUndefined();
    expect(chooseView(offers("web"), offers(["wasm", "0.5"]))).toBeUndefined();
  });
});

describe("a need", () => {
  it("lists the digests the host lacks, in its order", () => {
    expect(parseNeed(payloadOf("to-program/need.frame"))).toEqual([
      "bcc81d22a717681517bc99bbad72438377e02c19583fae67522475fd8b3ca62c",
    ]);
    expect(parseNeed(payloadOf("to-program/need-empty.frame"))).toEqual([]);
    expect(parseNeed(payloadOf("to-program/need-chunked.frame"))).toHaveLength(12);
  });

  it("leaves out what is not a digest, and is not read without a list", () => {
    expect(parseNeed('{"missing":["xyz",1,"' + "a".repeat(64) + '"]}')).toEqual(["a".repeat(64)]);
    expect(parseNeed("{}")).toBeUndefined();
    expect(parseNeed("not json")).toBeUndefined();
  });
});

describe("a closed", () => {
  it("has a reason, a code when there is one, and words when a view failed", () => {
    expect(parseClosed(payloadOf("to-program/closed-refused.frame"))).toEqual({
      reason: "refused",
      code: "token",
    });
    expect(parseClosed(payloadOf("to-program/closed-user.frame"))).toEqual({ reason: "user" });
    expect(parseClosed(payloadOf("to-program/closed-program.frame"))).toEqual({
      reason: "program",
    });
    expect(parseClosed(payloadOf("to-program/closed-error.frame"))).toEqual({
      reason: "error",
      code: "bundle",
    });
    expect(parseClosed(payloadOf("to-program/closed-error-view.frame"))).toEqual({
      reason: "error",
      code: "view",
      message: "script:12: attempt to index nil with 'x'",
    });
  });

  it("with a reason this version does not know still ends the view, as an error", () => {
    expect(parseClosed('{"reason":"later"}')).toEqual({ reason: "error" });
  });

  it("is not read without a reason", () => {
    expect(parseClosed("{}")).toBeUndefined();
    expect(parseClosed('{"reason":3}')).toBeUndefined();
    expect(parseClosed("[]")).toBeUndefined();
  });
});
