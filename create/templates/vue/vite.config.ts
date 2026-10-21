import vue from "@vitejs/plugin-vue";
import { defineConfig } from "vite";
import { viteSingleFile } from "vite-plugin-singlefile";

// The view is built twice, into two directories: `vite build` writes many files, which a
// pencere terminal keeps by content, and `vite build --mode single` writes one document,
// which every other MCP Apps host reads.
export default defineConfig(({ mode }) => {
  const single = mode === "single";
  return {
    // Relative, so that the view's files are found wherever its document is served from.
    base: "./",
    plugins: [vue(), ...(single ? [viteSingleFile()] : [])],
    build: { outDir: single ? "dist/single" : "dist/view" },
  };
});
