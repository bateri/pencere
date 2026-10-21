import type { CallToolResult } from "@modelcontextprotocol/client";
import { useApp, useHostStyles } from "@modelcontextprotocol/ext-apps/react";
import { useState } from "react";

const NAME = "{{name}}";
const TITLE = "{{title}}";

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

export function App() {
  const [count, setCount] = useState<number>();
  const [problem, setProblem] = useState<string>();
  const [busy, setBusy] = useState(false);

  const { app, error } = useApp({
    appInfo: { name: NAME, version: "0.1.0" },
    // Written for the terminal pane: the view fills it, and every key is the view's.
    // A host that does not know the name ignores it.
    capabilities: { experimental: { "pencere/pane": {} } },
    onAppCreated: (created) => {
      // The result of the tool that has this view arrives here, when the host has it.
      created.ontoolresult = (result) => setCount(countOf(result));
    },
  });
  // The host's colours and font, as the variables MCP Apps defines.
  useHostStyles(app, app?.getHostContext());

  async function addOne() {
    if (app === null || busy) return;
    setBusy(true);
    setProblem(undefined);
    try {
      const result = await app.callServerTool({ name: "add_one", arguments: {} });
      if (result.isError) setProblem(textOf(result));
      else setCount(countOf(result));
    } catch (failure) {
      setProblem(failure instanceof Error ? failure.message : String(failure));
    } finally {
      setBusy(false);
    }
  }

  const status = error?.message ?? problem ?? (app === null ? "Waiting for the host…" : "");
  return (
    <main>
      <h1>{TITLE}</h1>
      <p className="count" aria-live="polite">
        {count ?? "–"}
      </p>
      <button autoFocus aria-busy={busy} onClick={addOne}>
        Add one
      </button>
      <p className={error || problem ? "status failed" : "status"} role="status">
        {status}
      </p>
      <p className="keys">
        <kbd>⏎</kbd> add one
      </p>
    </main>
  );
}
