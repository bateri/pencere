/**
 * The containers program: what its view may call and subscribe to. The containers are
 * made up, so nothing here needs Docker.
 *
 * Nothing in this file knows about a terminal. `main.ts` is the only place that asks one
 * to show the view.
 */

import { ViewError } from "@pencere/sdk";
import type { CallHandler, Icon, TopicHandler } from "@pencere/sdk";

/**
 * Three stacked containers, 64 pixels a side, as a PNG in a `data:` URI. A terminal
 * draws an icon only from the bytes it is given and never fetches one.
 */
export const ICON: Icon = {
  src: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAEAAAABACAYAAACqaXHeAAAAy0lEQVR42u3YsQ3CMBBG4QzowhJTsIMrClgD0dEyAYUHSEXrUYwcKR1VSOP4e9JJVqr4j+zcvWkCAAD4QUg5hpRLSLl2Wu3d4z+brwepuCWAcqAAypYA6pFqtwBOt3c9P19LtfVQAbQNX+ZHvX7uS7V1LyHsEkD76uvm12rPBOAIuATHCWCk3+DwjdDYrfDwwxDAB/ABfAAfoBU2DBmHBeAIuAT5AD6AD+ADwAfwAXwAH8AH8AF8AB/AB/ABfAAfwAfwAXwAAAAAOucLnVucWIVm/igAAAAASUVORK5CYII=",
  mimeType: "image/png",
  sizes: ["64x64"],
};

/** How often the `stats` topic pushes, in milliseconds, unless the view asks for another. */
export const STATS_EVERY = 1000;

export interface Container {
  id: string;
  name: string;
  image: string;
  state: "running" | "stopped";
  ports: string;
}

export function sampleContainers(): Container[] {
  return [
    { id: "web", name: "web", image: "nginx:1.27", state: "running", ports: "8080→80" },
    { id: "api", name: "api", image: "node:22-alpine", state: "running", ports: "3000" },
    { id: "db", name: "db", image: "postgres:17", state: "running", ports: "5432" },
    { id: "cache", name: "cache", image: "redis:7", state: "stopped", ports: "" },
    { id: "worker", name: "worker", image: "python:3.13", state: "stopped", ports: "" },
  ];
}

/** The containers as text: what the program prints where there is no view to show them. */
export function table(containers: readonly Container[]): string {
  const width = (pick: (container: Container) => string) =>
    Math.max(...containers.map((container) => pick(container).length));
  const [name, image] = [width((c) => c.name), width((c) => c.image)];
  return containers
    .map(
      (c) =>
        `${c.state === "running" ? "●" : "○"} ${c.name.padEnd(name)}  ${c.image.padEnd(image)}  ` +
        `${c.state.padEnd(7)}  ${c.ports}`.trimEnd(),
    )
    .join("\n");
}

/** What a running container uses at a moment: a share of one CPU, in percent, and memory in MiB. */
export interface Stat {
  id: string;
  cpu: number;
  memory: number;
}

/** What each made-up container uses when it is quiet. */
const IDLE: Readonly<Record<string, { cpu: number; memory: number }>> = {
  web: { cpu: 4, memory: 38 },
  api: { cpu: 12, memory: 142 },
  db: { cpu: 7, memory: 286 },
};

/**
 * What the running containers use at the `tick`th second: a slow wave around what each
 * container uses when it is quiet, so that a view has something to show moving and a test
 * has something to expect.
 */
export function stats(containers: readonly Container[], tick: number): Stat[] {
  return containers
    .filter((container) => container.state === "running")
    .map((container, index) => {
      const idle = IDLE[container.id] ?? { cpu: 2, memory: 64 };
      const wave = Math.sin((tick + index * 3) / 2);
      return {
        id: container.id,
        cpu: Math.max(0, Math.round(idle.cpu * (1 + wave / 2) * 10) / 10),
        memory: Math.round(idle.memory * (1 + wave / 20)),
      };
    });
}

/** The id a call names, or an error that says what was missing. The params are the view's words. */
function containerOf(containers: readonly Container[], params: unknown): Container {
  const id = typeof params === "object" && params !== null ? (params as { id?: unknown }).id : null;
  if (typeof id !== "string") {
    throw new ViewError("bad-params", "Say which container, as {id}.");
  }
  const container = containers.find((candidate) => candidate.id === id);
  if (container === undefined) {
    throw new ViewError("no-such-container", `There is no container "${id}".`);
  }
  return container;
}

/** How many milliseconds a `stats` subscription asks to hear at, or the default. */
function intervalOf(params: unknown): number {
  const every =
    typeof params === "object" && params !== null ? (params as { every?: unknown }).every : null;
  return typeof every === "number" && Number.isFinite(every) && every >= 10 ? every : STATS_EVERY;
}

/**
 * What the view may call and subscribe to, over `containers`, which the calls change in
 * place:
 *
 * - `list` answers `{ containers }`;
 * - `stop` and `start` take `{ id }` and answer `{ containers, message }`, or fail with
 *   `no-such-container`;
 * - the topic `stats` pushes `{ stats }`, the use of every running container, at once
 *   and then once a second (or every `every` milliseconds, if the subscription says).
 */
export function createProgram(containers: Container[] = sampleContainers()): {
  calls: Record<string, CallHandler>;
  topics: Record<string, TopicHandler>;
} {
  const change = (state: Container["state"], verb: string): CallHandler => {
    return (params) => {
      const container = containerOf(containers, params);
      container.state = state;
      return { containers, message: `${verb} ${container.name} (${container.image})` };
    };
  };
  return {
    calls: {
      list: () => ({ containers }),
      stop: change("stopped", "Stopped"),
      start: change("running", "Started"),
    },
    topics: {
      stats: (params, subscription) => {
        let tick = 0;
        const push = () => subscription.push({ stats: stats(containers, tick++) });
        push();
        const timer = setInterval(push, intervalOf(params));
        return () => clearInterval(timer);
      },
    },
  };
}
