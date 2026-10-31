/**
 * The program side of pencere: a program running in a terminal asks the terminal
 * whether it hosts pencere views and, if it does, shows a view of its own in the
 * program's pane, over the terminal's own stream. The view is a WebAssembly
 * component (`wasm`), a Lua script (`lua`) or a web page (`web`); whichever it is, it
 * and the program talk in six small messages: the view calls the program and
 * subscribes to its topics, and the program answers and pushes.
 *
 * ```ts
 * const result = await serve({
 *   views: [
 *     { kind: "wasm", versions: ["0.5", "0.4"], bundle: (version) => wasmBundle(version) },
 *     { kind: "web", bundle: await bundleFromDirectory(new URL("./view/", import.meta.url)) },
 *   ],
 *   program: "containers",
 *   title: "Containers",
 *   calls: { list: () => ({ containers: listContainers() }) },
 *   topics: {
 *     stats: (_params, subscription) => {
 *       const timer = setInterval(() => subscription.push(readStats()), 1000);
 *       return () => clearInterval(timer);
 *     },
 *   },
 *   fallback: () => console.log(table()),
 * });
 * ```
 */

export { serve, ViewError } from "./serve.js";
export { ViewControl } from "./control.js";
export { FileError, MAX_PIECE } from "./files.js";
export type { Transfer } from "./files.js";
export { PipeError, StageFolder } from "./stage.js";
export { createRouter } from "./router.js";
export type { Router, RouterHandlers, Send } from "./router.js";
export type { CloseResult, ReopenOptions, ReopenResult } from "./control.js";
export type {
  BundleSource,
  CallContext,
  CallHandler,
  EndedBy,
  NoView,
  NoViewReason,
  ServeOptions,
  ServeResult,
  ShownView,
  Subscription,
  TopicHandler,
  ViewSpec,
} from "./serve.js";
export {
  BUNDLE_LIMITS,
  Bundle,
  BundleError,
  bundleFromDirectory,
  bundleFromFiles,
} from "./bundle.js";
export type {
  BundleFile,
  BundleManifest,
  BundleOptions,
  DirectoryOptions,
  ManifestFile,
} from "./bundle.js";
export type { Access, Icon, Permission, ViewKind, ViewMessage } from "./messages.js";
