// A program with a web view, for the end-to-end tests against the stand-in host: the
// containers of a host that does not exist. What the page of the stand-in asks for is
// here: a call that answers at once (`list`, `stop`, `echo`), one that takes a while,
// reports how far it has got and stops when it is cancelled (`slow`), one that writes
// to the terminal and reports as it goes (`say`), one that has the view shown again once
// it has answered (`reload`), and a topic that goes on pushing until the page cancels it or
// the view is over (`ticks`).
//
//   node containers.mjs
//
// It says how it ended on the screen it gave back and what it stopped doing, and `counts`
// tells the page how many calls were cancelled and how many topics have ended so far, so
// that a test can read from the page's side that a `cancel` was taken while the view ran.
import { ViewControl, ViewError, bundleFromDirectory, serve } from "@pencere/sdk";

// Three stacked containers, 64 pixels a side, as a PNG in a `data:` URI.
const ICON =
  "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAEAAAABACAYAAACqaXHeAAAAy0lEQVR42u3YsQ3CMBBG4QzowhJTsIMrClgD0dEyAYUHSEXrUYwcKR1VSOP4e9JJVqr4j+zcvWkCAAD4QUg5hpRLSLl2Wu3d4z+brwepuCWAcqAAypYA6pFqtwBOt3c9P19LtfVQAbQNX+ZHvX7uS7V1LyHsEkD76uvm12rPBOAIuATHCWCk3+DwjdDYrfDwwxDAB/ABfAAfoBU2DBmHBeAIuAT5AD6AD+ADwAfwAXwAH8AH8AF8AB/AB/ABfAAfwAfwAXwAAAAAOucLnVucWIVm/igAAAAASUVORK5CYII=";

const containers = [
  { id: "web", name: "web", image: "nginx:1.27", state: "running" },
  { id: "api", name: "api", image: "node:22-alpine", state: "running" },
  { id: "cache", name: "cache", image: "redis:7", state: "stopped" },
];

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

let cancelledCalls = 0;
let endedTopics = 0;
let reloads = 0;
const control = new ViewControl();

const result = await serve({
  views: [
    {
      kind: "web",
      bundle: await bundleFromDirectory(new URL("./view/", import.meta.url)),
    },
  ],
  program: "containers",
  title: "Containers",
  icons: [{ src: ICON, mimeType: "image/png", sizes: ["64x64"] }],
  calls: {
    list: () => ({ containers }),
    stop: (params) => {
      const container = containers.find((candidate) => candidate.id === params?.id);
      if (container === undefined) {
        throw new ViewError("no-such-container", `There is no container "${params?.id}".`);
      }
      container.state = "stopped";
      return { stopped: container.id };
    },
    echo: (params) => params ?? {},
    counts: () => ({ cancelledCalls, endedTopics }),
    // The first call has the view shown again, after the answer has gone: the view that
    // comes asks again and is left as it is.
    reload: () => {
      reloads += 1;
      if (reloads === 1) setImmediate(() => void control.reopen());
      return { reloads };
    },
    slow: async (params, call) => {
      const steps = params?.steps ?? 5;
      let reported = 0;
      call.signal.addEventListener(
        "abort",
        () => {
          cancelledCalls += 1;
          console.log(`slow: cancelled after ${reported} of ${steps}`);
        },
        { once: true },
      );
      for (let step = 1; step <= steps; step += 1) {
        await sleep(params?.ms ?? 100);
        if (call.signal.aborted) return undefined;
        reported = step;
        call.progress(step, steps, `step ${step} of ${steps}`);
      }
      console.log(`slow: finished ${steps} steps`);
      return { steps };
    },
    say: (params, call) => {
      const lines = params?.lines ?? 1;
      for (let line = 1; line <= lines; line += 1) {
        process.stdout.write(`say: ${params?.text ?? "hello"} ${line}\r\n`);
        call.progress(line, lines);
      }
      return { written: lines };
    },
  },
  topics: {
    ticks: (params, subscription) => {
      let count = 0;
      const timer = setInterval(() => {
        count += 1;
        subscription.push({ n: count });
      }, params?.every ?? 30);
      return () => {
        clearInterval(timer);
        endedTopics += 1;
        console.log(`ticks: stopped after ${count} pushes`);
      };
    },
  },
  control,
  fallback: () => {
    for (const container of containers) {
      console.log(`${container.name.padEnd(6)} ${container.image.padEnd(16)} ${container.state}`);
    }
  },
});
console.log(`serve: ${JSON.stringify(result)}`);
