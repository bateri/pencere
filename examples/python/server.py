"""A tasks tool with a view, written in Python with the official MCP SDK.

Nothing in this file knows about pencere. It is an ordinary MCP server: the tool
`show_tasks` names a view (`_meta.ui.resourceUri`), and the view is a `ui://`
resource. Started by an MCP Apps host (Claude Desktop, VS Code) the view shows
there. Started by `pencere run`, the same server shows the same view in the
terminal pane you ran it in:

    pencere run -- python server.py
"""

from pathlib import Path

from mcp.server.apps import Apps
from mcp.server.mcpserver import MCPServer
from mcp.types import CallToolResult, TextContent

VIEW_URI = "ui://tasks/view"
VIEW = Path(__file__).with_name("view.html")

apps = Apps()
tasks = [
    {"id": "spec", "title": "Read the pencere specification", "done": True},
    {"id": "tool", "title": "Write a tool in Python", "done": True},
    {"id": "view", "title": "Show its view in a terminal pane", "done": False},
    {"id": "ssh", "title": "Run it over ssh on a server", "done": False},
]


def listing(note: str) -> CallToolResult:
    """The tasks, for the view as structured data and for everyone else as text."""
    mark = {True: "[x]", False: "[ ]"}
    lines = [f"{mark[task['done']]} {task['title']}" for task in tasks]
    return CallToolResult(
        content=[TextContent(type="text", text="\n".join([note, *lines]))],
        structured_content={"tasks": tasks},
    )


@apps.tool(resource_uri=VIEW_URI, title="Tasks", description="Show the tasks and which are done")
def show_tasks() -> CallToolResult:
    return listing(f"{sum(task['done'] for task in tasks)} of {len(tasks)} tasks are done.")


# The view is one document, with its stylesheet and script in it, read when the server starts.
apps.add_html_resource(VIEW_URI, VIEW.read_text(encoding="utf-8"), name="Tasks")

mcp = MCPServer("tasks", extensions=[apps])


# Only the view calls this one: the model is not offered a tool that is a checkbox. It names
# no view of its own, so `pencere run` finds `show_tasks` as the one tool with a view.
@mcp.tool(
    title="Toggle a task",
    description="Mark a task done, or not done again",
    meta={"ui": {"visibility": ["app"]}},
)
def toggle_task(id: str) -> CallToolResult:
    task = next((task for task in tasks if task["id"] == id), None)
    if task is None:
        return CallToolResult(
            content=[TextContent(type="text", text=f'There is no task "{id}".')],
            is_error=True,
        )
    task["done"] = not task["done"]
    return listing(f"{'Finished' if task['done'] else 'Reopened'}: {task['title']}")


if __name__ == "__main__":
    mcp.run("stdio")
