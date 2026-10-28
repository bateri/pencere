-- The view of gutter-lua: an editor whose gutter has columns the page declares, and what a page
-- builds on them with no more of the kit than the page API:
--
-- * `breakpoints`, a column of its own at the left: a press in it, on an empty cell as well, puts a
--   breakpoint there or takes it away (a dot, with a label for a screen reader and a tooltip);
--   while the pointer is over an empty cell the column shows a faint dot, the breakpoint to be put;
-- * `changes`, between the numbers and the buttons that fold: bars along the lines changed (blue
--   modified, green added), a triangle where lines were taken away (red), and the same on the
--   overview ruler; a press on one opens a peek, a zone under the line with the lines as they were,
--   and Esc closes it;
-- * a zone: the peek is an element of the page, a child of the editor's, put in the gap the editor
--   keeps open after the line, where it scrolls with the text.

local b = pencere

local CODE = "code"
local PEEK = "peek"

local lines = {}
for n = 1, 40 do
	lines[n] = "line " .. n .. ": " .. string.rep("abc ", n % 5 + 1)
end
local text = table.concat(lines, "\n")

-- Where each line starts, in characters (the text is ASCII).
local function start_of(line)
	local at = 0
	for n = 1, line do
		at += #lines[n] + 1
	end
	return at
end

-- The changes the page shows: lines (from 0) and what happened to them.
local CHANGES = {
	{ first = 3, last = 5, kind = "modified" },
	{ first = 12, last = 13, kind = "added" },
	{ first = 20, last = 20, kind = "deleted", was = "the lines that were here" },
}
local COLORS = { modified = "blue", added = "green", deleted = "red" }

local breakpoints = {}
local peek = nil
local version = 0

local function changes_layer()
	local marks = {}
	for _, change in CHANGES do
		local color = COLORS[change.kind]
		if change.kind == "deleted" then
			marks[#marks + 1] = {
				from = start_of(change.first), to = start_of(change.first), gutter = color, column = "changes",
				shape = "above", id = "change", label = "Lines were deleted here",
			}
		else
			marks[#marks + 1] = {
				from = start_of(change.first), to = start_of(change.last + 1), gutter = color,
				column = "changes", shape = "bar", id = "change", label = "Lines " .. change.kind,
			}
		end
		marks[#marks + 1] = {
			from = start_of(change.first), to = start_of(change.last + 1), ruler = color, lane = "left",
		}
	end
	return { decorate = "changes", version = version, from = 0, to = #text, decorations = marks }
end

local function breakpoints_layer()
	local marks = {}
	for line in breakpoints do
		marks[#marks + 1] = {
			from = start_of(line), to = start_of(line), gutter = "red", column = "breakpoints",
			id = "breakpoint " .. line, label = "Breakpoint on line " .. (line + 1),
		}
		marks[#marks + 1] = { from = start_of(line), to = start_of(line), ruler = "red", lane = "right" }
	end
	return { decorate = "breakpoints", version = version, from = 0, to = #text, decorations = marks }
end

local function peek_layer()
	local marks = {}
	if peek then
		marks[1] = { from = start_of(peek.line), to = start_of(peek.line), zone = PEEK, height = 60 }
	end
	return { decorate = "peek", version = version, from = 0, to = #text, decorations = marks }
end

local OPTIONS = {
	gutters = {
		{
			name = "breakpoints", width = 14, events = { "press", "menu" },
			hover = { shape = "dot", color = { "red", 0.35 } },
		},
		"numbers",
		{ name = "changes", width = 8 },
		"folds",
	},
	marks = { bar = 4, hover = 7, ruler = 6 },
}

local function render()
	local editor = b.editor(CODE, OPTIONS):grow(1):bg("raised1"):focused(true)
	if peek then
		editor = editor:command("peek-close", "Close the peek", "escape")
		editor = editor:child(
			b.div():id(PEEK):zone(PEEK):h(60):bg("raised2"):border(1, "border"):p(6)
				:child(b.text("Was: " .. peek.was):color("dim"))
				:child(b.text("Esc closes"):color("quiet"))
		)
	end
	return b.div():size_full():child(editor)
end

local function handle(event)
	local kind = event.type
	if kind == "editor_ready" and event.editor == CODE then
		return { editor = { [CODE] = { { load = text } } } }
	elseif kind == "edit" and event.editor == CODE then
		version = event.version
		return { editor = { [CODE] = { changes_layer(), breakpoints_layer() } } }
	elseif kind == "gutter" and event.kind == "press" and event.button == "left" then
		if event.column == "breakpoints" then
			breakpoints[event.line] = not breakpoints[event.line] or nil
			return { redraw = true, editor = { [CODE] = { breakpoints_layer() } } }
		elseif event.column == "changes" then
			for _, change in CHANGES do
				if event.line >= change.first and event.line <= change.last then
					peek = { line = change.last, was = change.was or "(the lines as they were)" }
					return { redraw = true, editor = { [CODE] = { peek_layer() } } }
				end
			end
		end
	elseif kind == "command" and event.id == "peek-close" then
		peek = nil
		return { redraw = true, editor = { [CODE] = { peek_layer() } } }
	end
	return nil
end

return { render = render, handle = handle }
