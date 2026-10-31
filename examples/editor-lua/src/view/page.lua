-- The view of editor-lua: an editor of SQL, which the terminal keeps, lays out and draws, and the
-- page makes a language of. It is a page of the kit's page API, and an example of every hook the
-- editor gives a page:
--
-- * the language: SQL is the kit's (`@pencere/languages`): what the editor is told of it (its
--   comments, the rules of its indent, the brackets that close themselves) is the language's
--   `config`, and its colours its `syntax`, read a line at a time with a state carried from line to
--   line (a block comment, a string goes on over lines) by `@pencere/syntax`, which colours the
--   lines in view (`viewport`) as a layer of decorations (`decorate = "syntax"`), reads again from
--   the line an edit touched until the state is what it was, and goes on by itself in the frames
--   after where there is more than one call may do (the kit's timer wakes the page for it);
-- * folds: each statement of more than one line is a region that folds (a layer of its own);
-- * completions: a popup anchored to the caret (`pencere.caret_of`), whose keys (↑ ↓ ↩ ⇥ Esc) are
--   commands of the page only while it is open, and which puts the word in with a change against
--   the version it was made for (`change`, `base`);
-- * the program: ⌘Enter sends the statement under the caret to the program, and shows what it says;
-- * a large document, loaded in pieces (`load`, `more`) once the editor is made (`editor_ready`);
-- * the look of the person's terminal: the host says how its terminal draws its cursor and its
--   selection (`pencere.terminal_look()`) and applies none of it; this page takes all of it
--   (`@pencere/terminal`), where it says nothing of its own, and reads it again when the host says it
--   changed (`terminal_look`).

local Syntax = require("@pencere/syntax")
local languages = require("@pencere/languages")
local terminal = require("@pencere/terminal")

local b = pencere
local layout = b.layout

local SQL = "sql"
local POPUP = "completions"

-- --- The language ------------------------------------------------------------------------------

local language = languages.load("SQL")
local syntax = Syntax.new(language.syntax, SQL)

-- What the editor is told: the language's, and the page's word for an empty editor.
local options = table.clone(language.config)
options.hint = "Write SQL…"

-- The words the popup offers, the page's own.
local KEYWORDS = {
	"select", "from", "where", "group", "by", "order", "having", "limit", "offset", "insert",
	"into", "values", "update", "set", "delete", "create", "table", "drop", "alter", "join",
	"left", "right", "inner", "outer", "on", "and", "or", "not", "null", "is", "in", "as",
	"distinct", "count", "sum", "avg", "min", "max", "case", "when", "then", "else", "end",
	"begin", "commit", "rollback", "union", "all", "exists", "between", "like",
}

-- --- The document ------------------------------------------------------------------------------

-- What the page knows of the editor: the version it was last told, and where the main caret is (a
-- place: a byte of the text, from 1, as Luau counts a string).
local version = 0
local caret = 1
local composing = false
-- What is to be loaded still, in pieces, and the size of a piece.
local PIECE = 8 * 1024
local pending = nil
local note = "⌘Enter runs the statement under the caret."

-- A sample to load: many statements, so that only what is in view is coloured.
local function sample()
	local parts = {
		"-- A sample of SQL: the page colours what is in view, and folds each statement.\n",
	}
	for n = 1, 400 do
		parts[#parts + 1] = string.format(
			"select name, count(*) as n\nfrom orders\nwhere total > %d and state = 'open'\ngroup by name;\n\n",
			n * 10
		)
	end
	return table.concat(parts)
end

-- The next piece of what is to be loaded, as an ask; nil when it is all there.
local function next_piece()
	if not pending or pending == "" then
		pending = nil
		return nil
	end
	-- Cut where a character starts, at most a piece of bytes.
	local cut = math.min(#pending, PIECE)
	while cut < #pending and utf8.len(string.sub(pending, 1, cut)) == nil do
		cut -= 1
	end
	local piece = string.sub(pending, 1, cut)
	pending = string.sub(pending, cut + 1)
	return { load = piece, more = pending ~= "" }
end

-- The statements of more than one line, as regions that fold.
local function folds()
	local length = layout.editor_length(SQL)
	if not length then
		return nil
	end
	local regions = {}
	local text = layout.editor_text(SQL, 1, length + 1) or ""
	local start = 1
	while true do
		local semicolon = string.find(text, ";", start, true)
		if not semicolon then
			break
		end
		-- A region is from after the last `;` to after this one; the places are the bytes of `text`.
		local _, lines = string.gsub(string.sub(text, start, semicolon), "\n", "")
		if lines > 0 then
			regions[#regions + 1] = { from = start, to = semicolon + 1, fold = true }
		end
		start = semicolon + 1
	end
	return { decorate = "folds", version = layout.editor_version(SQL), from = 1, to = length + 1, decorations = regions }
end

-- --- Completions -------------------------------------------------------------------------------

-- The popup: the words that start as the word before the caret does, the one chosen, and where
-- that word starts.
local popup = nil

-- The word before the caret: where it starts, and its letters.
local function word_before()
	local line = layout.line_of(SQL, caret)
	local start = line and layout.line_start(SQL, line)
	local text = start and layout.editor_text(SQL, start, caret) or ""
	local word = string.match(text, "([%a_][%w_]*)$")
	if not word then
		return nil
	end
	return caret - #word, word
end

local function complete()
	popup = nil
	local from, word = word_before()
	if not from or #word < 2 then
		return
	end
	local lower = string.lower(word)
	local found = {}
	for _, keyword in KEYWORDS do
		if #found < 8 and string.sub(keyword, 1, #lower) == lower and keyword ~= lower then
			found[#found + 1] = keyword
		end
	end
	if #found > 0 then
		popup = { words = found, chosen = 1, from = from, version = version }
	end
end

-- The chosen word in place of the one before the caret: a change of the version the popup was made
-- for, which the editor carries over anything typed since.
local function accept()
	local chosen = popup.words[popup.chosen]
	local ask = { change = { { from = popup.from, to = caret, insert = chosen .. " " } }, base = version }
	popup = nil
	return { redraw = true, editor = { [SQL] = ask } }
end

-- --- The program ---------------------------------------------------------------------------------

-- The statement the caret is in: from after the `;` before it to the `;` after it.
local function statement()
	local length = layout.editor_length(SQL) or 0
	local text = layout.editor_text(SQL, 1, length + 1) or ""
	local before = 0
	local at = string.find(text, ";", 1, true)
	while at and at < caret do
		before = at
		at = string.find(text, ";", at + 1, true)
	end
	-- `at` is the first `;` at or after the caret, if there is one.
	return string.sub(text, before + 1, (at or #text + 1) - 1)
end

local function run()
	local sql = string.gsub(statement(), "^%s+", "")
	b.call("run", { sql = sql }, function(result, error)
		if error then
			note = "It failed: " .. (error.message or error.code or "?")
		else
			note = result.note or "Done."
		end
		-- An answer is a reply as well: what the language has for the editor goes out with it.
		return syntax:reply(true)
	end)
	note = "Running…"
	return true
end

-- --- The page ------------------------------------------------------------------------------------

local function render(size)
	local editor = b.editor(SQL, terminal.editor(options)):grow(1):bg("raised1"):focused(true)
		:command("run", "Run the statement", "cmd+enter")
	if popup then
		editor = editor
			:command("completion-next", "Next completion", "down")
			:command("completion-previous", "Previous completion", "up")
			:command("completion-accept", "Accept the completion", "enter")
			:command("completion-take", "Take the completion", "tab")
			:command("completion-close", "Close the completions", "escape")
		local list = b.div():id(POPUP):anchor(b.caret_of(SQL), "below", { gap = 2 })
			:bg("raised2"):rounded(4):p(4):border(1, "border")
		for index, word in popup.words do
			local row = b.div():px(6):py(2):child(b.text(word))
			if index == popup.chosen then
				row = row:bg("selection")
			end
			list = list:child(row)
		end
		editor = editor:child(list)
	end
	return b.div():size_full():child(editor):child(b.div():p(6):child(b.text(note):color("dim")))
end

-- What the page does of an event, the colours aside.
local function respond(event)
	local kind = event.type
	if kind == "editor_ready" and event.editor == SQL then
		pending = sample()
		return { editor = { [SQL] = { next_piece() } } }
	elseif kind == "edit" and event.editor == SQL then
		version = event.version
		if event.origin == "load" then
			local piece = next_piece()
			if piece then
				return { editor = { [SQL] = { piece } } }
			end
		end
		return { editor = { [SQL] = { folds() } } }
	elseif kind == "selection" and event.editor == SQL then
		local main = event.ranges[event.main]
		caret = main.head
		composing = event.composing
		if not composing then
			complete()
		end
		return true
	elseif kind == "terminal_look" then
		-- The person changed how the terminal draws its cursor or its selection: the page is made
		-- again, and `render` reads it.
		return true
	elseif kind == "editor_refused" then
		note = "The editor did not do it: " .. event.reason
		popup = nil
		return true
	elseif kind == "command" then
		if event.id == "run" then
			return run()
		elseif popup and event.id == "completion-next" then
			popup.chosen = popup.chosen % #popup.words + 1
			return true
		elseif popup and event.id == "completion-previous" then
			popup.chosen = (popup.chosen - 2) % #popup.words + 1
			return true
		elseif popup and (event.id == "completion-accept" or event.id == "completion-take") then
			return accept()
		elseif event.id == "completion-close" then
			popup = nil
			return true
		end
	end
	return nil
end

-- The language is given every event (an edit and what is in view are its, any other a call to go
-- on in), and what it asks of the editor goes first in the reply.
local function handle(event)
	syntax:handle(event)
	return syntax:reply(respond(event))
end

return { render = render, handle = handle }
