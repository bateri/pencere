// @ts-check
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { defineConfig } from "astro/config";
import { theme } from "./src/lib/code.js";
import { repository } from "./scripts/watch.mjs";

// The sketches the site came from are kept beside it by whoever works on it (lab/), and are not in
// the repository. Where they are, they are shown while the site is watched; they are never built.
const lab = existsSync(fileURLToPath(new URL("./lab/lab.mjs", import.meta.url)))
  ? [(await import("./lab/lab.mjs")).lab()]
  : [];

export default defineConfig({
  site: "https://pencere.dev",
  server: { port: 4823 },
  // The pages come out as kit/elements.html: a host of static files serves them at /kit/elements.
  build: { format: "file" },
  compressHTML: false,
  // The specification is Markdown, read from the repository as it is; its code takes the site's colours.
  // A block of Rust that a document marks for rustdoc ("rust,no_run": compiled, not run) is Rust.
  markdown: { shikiConfig: { theme, langAlias: { "rust,no_run": "rust" } } },
  // While the site is watched, a change to a file of the repository that a page shows is shown too.
  vite: { plugins: [repository()] },
  integrations: lab,
});
