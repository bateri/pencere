import { describe, expect, it } from "vitest";

import { createRouter } from "../src/router.js";
import type { RouterHandlers } from "../src/router.js";
import { TAKE_TOPIC, parseTake, takeKind } from "../src/take.js";
import { payloadOf } from "./support.js";

/** A router and what it said, parsed. */
function rig(handlers: RouterHandlers) {
  const sent: Record<string, unknown>[] = [];
  const router = createRouter(handlers, (payload) => {
    sent.push(JSON.parse(payload));
    return true;
  });
  const subscribe = (id: number) => router.handle({ subscribe: { id, topic: TAKE_TOPIC } });
  return { router, sent, subscribe };
}

describe("what the host hands a program that runs for it", () => {
  it("reads the takes of the fixtures", () => {
    expect(parseTake(payloadOf("to-program/take-url.frame"))).toEqual({
      url: "http://localhost:3000/",
    });
    expect(parseTake(payloadOf("to-program/take-file.frame"))).toEqual({
      file: "/home/alice/notes.txt",
      line: 12,
      column: 3,
    });
    expect(parseTake(payloadOf("to-program/take-folder.frame"))).toEqual({
      folder: "/home/alice/src",
    });
  });

  it("takes the scheme in either letter case, as a link of a view is", () => {
    expect(parseTake('{"url":"HTTPS://EXAMPLE.COM"}')).toEqual({ url: "HTTPS://EXAMPLE.COM" });
    expect(parseTake('{"url":"hTtP://example.com"}')).toEqual({ url: "hTtP://example.com" });
    const longest = `http://${"a".repeat(8192 - 7)}`;
    expect(parseTake(JSON.stringify({ url: longest }))).toEqual({ url: longest });
  });

  it("reads one of the three and nothing else, and a member of another name is no matter", () => {
    expect(takeKind({ url: "http://a.example" })).toBe("url");
    expect(takeKind({ file: "/a" })).toBe("file");
    expect(takeKind({ folder: "/a" })).toBe("folder");
    expect(parseTake('{"url":"https://a.example","later":[1]}')).toEqual({
      url: "https://a.example",
    });
    for (const bad of [
      "{}",
      "[]",
      "null",
      '{"url":"https://a.example","file":"/a"}',
      '{"url":7}',
      '{"url":"ftp://a.example"}',
      '{"url":"https://a.example/ b"}',
      '{"url":"https:/a.example"}',
      '{"url":"https://a.example/\\u00e9"}',
      `{"url":"https://${"a".repeat(8192)}"}`,
      '{"url":"https://a.example","line":1}',
      '{"folder":"/a","line":1}',
      '{"file":"/a","column":3}',
      '{"file":"/a","line":0}',
      '{"file":"/a","line":"3"}',
      '{"file":"/a","line":1.5}',
      '{"file":"/a","line":4294967296}',
      '{"file":""}',
      '{"file":"notes.txt"}',
      '{"folder":"../src"}',
      '{"file":"/a\\nb"}',
      '{"folder":"/a\\u007f"}',
      `{"file":"/${"a".repeat(4096)}"}`,
    ])
      expect(parseTake(bad), bad).toBeUndefined();
  });
});

describe("the topic of what the host hands the program", () => {
  it("pushes each take at once to every subscription, and keeps it for the first when there is none", () => {
    const { router, sent, subscribe } = rig({ takes: ["url", "file"] });
    expect(router.take({ url: "http://localhost:3000/" })).toBe(true);
    expect(sent).toEqual([]);
    subscribe(1);
    expect(sent).toEqual([{ push: { id: 1, value: { url: "http://localhost:3000/" } } }]);
    subscribe(2);
    sent.length = 0;
    router.take({ file: "/a", line: 2 });
    expect(sent).toEqual([
      { push: { id: 1, value: { file: "/a", line: 2 } } },
      { push: { id: 2, value: { file: "/a", line: 2 } } },
    ]);
    // Cancelled: hears no more. A kind the program did not say it takes is not told.
    router.handle({ cancel: { id: 1 } });
    sent.length = 0;
    expect(router.take({ folder: "/srv" })).toBe(false);
    router.take({ url: "https://example.com/" });
    expect(sent).toEqual([{ push: { id: 2, value: { url: "https://example.com/" } } }]);
  });

  it("keeps the last eight for a view that has not subscribed, and none for the next view", () => {
    const { router, sent, subscribe } = rig({ takes: ["folder"] });
    for (let n = 1; n <= 10; n++) router.take({ folder: `/srv/${n}` });
    subscribe(1);
    expect(sent.map((m) => (m.push as { value: { folder: string } }).value.folder)).toEqual(
      [3, 4, 5, 6, 7, 8, 9, 10].map((n) => `/srv/${n}`),
    );
    router.end();
    router.take({ folder: "/srv/11" });
    router.end();
    sent.length = 0;
    subscribe(1);
    expect(sent).toEqual([]);
  });

  it("is not a topic of a program that takes nothing", () => {
    const { router, sent, subscribe } = rig({});
    expect(router.take({ url: "http://localhost:3000/" })).toBe(false);
    subscribe(1);
    expect(sent).toMatchObject([{ response: { id: 1, error: { code: "not-found" } } }]);
  });
});
