<script setup lang="ts">
import type { CallToolResult } from "@modelcontextprotocol/client";
import {
  App as McpApp,
  PostMessageTransport,
  applyDocumentTheme,
  applyHostFonts,
  applyHostStyleVariables,
} from "@modelcontextprotocol/ext-apps";
import type { McpUiHostContext } from "@modelcontextprotocol/ext-apps";
import { computed, onBeforeUnmount, onMounted, ref, shallowRef } from "vue";

const NAME = "{{name}}";
const TITLE = "{{title}}";

const count = ref<number>();
const problem = ref<string>();
const failure = ref<string>();
const busy = ref(false);
const app = shallowRef<McpApp>();
const button = ref<HTMLButtonElement>();

/** The count in a tool result: both of this tool's results carry it. */
function countOf(result: CallToolResult): number | undefined {
  const structured: unknown = result.structuredContent;
  if (typeof structured !== "object" || structured === null || !("count" in structured))
    return undefined;
  return typeof structured.count === "number" ? structured.count : undefined;
}

/** What a failed tool call says. */
function textOf(result: CallToolResult): string {
  const text = result.content.find((item) => item.type === "text");
  return text?.type === "text" ? text.text : "The tool failed.";
}

/** The host's colours, font and theme, as the variables MCP Apps defines. */
function adopt(context: McpUiHostContext | undefined) {
  if (context?.theme) applyDocumentTheme(context.theme);
  if (context?.styles?.variables) applyHostStyleVariables(context.styles.variables);
  if (context?.styles?.css?.fonts) applyHostFonts(context.styles.css.fonts);
}

onMounted(async () => {
  button.value?.focus();
  const connecting = new McpApp(
    { name: NAME, version: "0.1.0" },
    // Written for the terminal pane: the view fills it, and every key is the view's.
    // A host that does not know the name ignores it.
    { experimental: { "pencere/pane": {} } },
  );
  // The result of the tool that has this view arrives here, when the host has it. Both
  // handlers are set before the connection, so that nothing the host sends first is missed.
  connecting.ontoolresult = (result) => {
    count.value = countOf(result);
  };
  connecting.addEventListener("hostcontextchanged", adopt);
  try {
    await connecting.connect(new PostMessageTransport(window.parent, window.parent));
    adopt(connecting.getHostContext());
    app.value = connecting;
  } catch (error) {
    failure.value = error instanceof Error ? error.message : String(error);
  }
});

onBeforeUnmount(() => {
  void app.value?.close();
});

async function addOne() {
  const connected = app.value;
  if (connected === undefined || busy.value) return;
  busy.value = true;
  problem.value = undefined;
  try {
    const result = await connected.callServerTool({ name: "add_one", arguments: {} });
    if (result.isError) problem.value = textOf(result);
    else count.value = countOf(result);
  } catch (error) {
    problem.value = error instanceof Error ? error.message : String(error);
  } finally {
    busy.value = false;
  }
}

const status = computed(
  () => failure.value ?? problem.value ?? (app.value === undefined ? "Waiting for the host…" : ""),
);
</script>

<template>
  <main>
    <h1>{{ TITLE }}</h1>
    <p class="count" aria-live="polite">{{ count ?? "–" }}</p>
    <button ref="button" :aria-busy="busy" @click="addOne">Add one</button>
    <p :class="failure || problem ? 'status failed' : 'status'" role="status">{{ status }}</p>
    <p class="keys"><kbd>⏎</kbd> add one</p>
  </main>
</template>
