/**
 * The tool side of pencere: an MCP server running in a terminal asks the
 * terminal whether it hosts pencere views and, if it does, shows its MCP App
 * view in its own pane, over the terminal's own stream.
 *
 * ```ts
 * const server = new McpServer({ name: "containers", version: "1.0.0" });
 * // ... registerAppTool, registerAppResource ...
 * await serve(server, {
 *   tool: "list_containers",
 *   bundle: await bundleFromDirectory(new URL("./view/", import.meta.url)),
 *   fallback: () => console.log(table()),
 * });
 * ```
 */

export { serve } from "./serve.js";
export type { Connectable, EndedBy, NoViewReason, ServeOptions, ServeResult } from "./serve.js";
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
  ViewMeta,
} from "./bundle.js";
export type { ToolCall } from "./messages.js";
