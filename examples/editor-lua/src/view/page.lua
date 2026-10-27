-- The view of editor-lua: an editor of SQL, which the terminal keeps, lays out and draws, and the
-- page makes a language of. It is a page of the kit's page API, and an example of every hook the
-- editor gives a page:
--
-- * colours: the page reads the lines in view (`pencere.layout.editor_text`) and colours their
--   words with a small lexer, a layer of decorations at a time (`decorate = "syntax"`), only the
--   lines it was told are in view (`viewport`) or that changed (`edit`);
-- * folds: each statement of more than one line is a region that folds (a layer of its own);
-- * completions: a popup anchored to the caret (`pencere.caret_of`), whose keys (↑ ↓ ↩ ⇥ Esc) are
--   commands of the page only while it is open, and which puts the word in with a change against
--   the version it was made for (`change`, `base`);
-- * the program: ⌘↩ sends the statement under the caret to the program, and shows what it says;
-- * a large document, loaded in pieces (`load`, `more`).

local b = pencere
local layout = b.layout

local SQL = "sql"
local POPUP = "completions"

-- --- The language ------------------------------------------------------------------------------

local KEYWORDS = {
	"select", "from", "where", "group", "by", "order", "having", "limit", "offset", "insert",
	"into", "values", "update", "set", "delete", "create", "table", "drop", "alter", "join",
	"left", "right", "inner", "outer", "on", "and", "or", "not", "null", "is", "in", "as",
	"distinct", "count", "sum", "avg", "min", "max", "case", "when", "then", "else", "end",
	"begin", "commit", "rollback", "union", "all", "exists", "between", "like",
}
local IS_KEYWORD = {}
for _, word in KEYWORDS do
	IS_KEYWORD[word] = true
end

local LOOKS = {
	keyword = { color = "accent", weight = 700 },
	string = { color = "success" },
	number = { color = "warning" },
	comment = { color = "dim", italic = true },
}

-- The words of a line and what each is: { from =, to =, kind = } in characters from 0 of the line.
local function lex(line)
	local tokens = {}
	local chars = {}
	for _, code in utf8.codes(line) do
		chars[#chars + 1] = utf8.char(code)
	end
	local at = 1
	while at <= #chars do
		local c = chars[at]
		if c == "-" and chars[at + 1] == "-" then
			tokens[#tokens + 1] = { from = at - 1, to = #chars, kind = "comment" }
			break
		elseif c == "'" then
			local close = at + 1
			while close <= #chars and chars[close] ~= "'" do
				close += 1
			end
			tokens[#tokens + 1] = { from = at - 1, to = math.min(close, #chars), kind = "string" }
			at = close + 1
		elseif string.match(c, "%d") then
			local finish = at
			while finish < #chars and string.match(chars[finish + 1], "[%d%.]") do
				finish += 1
			end
			tokens[#tokens + 1] = { from = at - 1, to = finish, kind = "number" }
			at = finish + 1
		elseif string.match(c, "[%a_]") then
			local finish = at
			while finish < #chars and string.match(chars[finish + 1], "[%w_]") do
				finish += 1
			end
			local word = string.lower(table.concat(chars, "", at, finish))
			if IS_KEYWORD[word] then
				tokens[#tokens + 1] = { from = at - 1, to = finish, kind = "keyword" }
			end
			at = finish + 1
		else
			at += 1
		end
	end
	return tokens
end

-- --- The document ------------------------------------------------------------------------------

-- What the page knows of the editor: the version it was last told, and where the main caret is.
local version = 0
local caret = 0
local composing = false
-- The lines in view, as the editor said.
local first_line, last_line = 0, 0
-- What is to be loaded still, in pieces, and the size of a piece.
local PIECE = 8 * 1024
local pending = nil
local loaded = false
local note = "⌘↩ runs the statement under the caret."

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

-- The decorations of the lines `from` to `to` (from 0): the colours, as one layer, for the range
-- those lines take.
local function colour(from, to)
	local lines = layout.editor_lines(SQL)
	if not lines then
		return nil
	end
	to = math.min(to, lines - 1)
	if from > to then
		return nil
	end
	local start = layout.line_start(SQL, from)
	local finish = (to + 1 < lines) and (layout.line_start(SQL, to + 1) - 1) or layout.editor_length(SQL)
	local decorations = {}
	for line = from, to do
		local base = layout.line_start(SQL, line)
		local after = (line + 1 < lines) and (layout.line_start(SQL, line + 1) - 1) or layout.editor_length(SQL)
		local text = layout.editor_text(SQL, base, after) or ""
		for _, token in lex(text) do
			local look = LOOKS[token.kind]
			decorations[#decorations + 1] = {
				from = base + token.from,
				to = base + token.to,
				color = look.color,
				weight = look.weight,
				italic = look.italic,
			}
		end
	end
	return {
		decorate = "syntax",
		version = layout.editor_version(SQL),
		from = start,
		to = finish,
		decorations = decorations,
	}
end

-- The statements of more than one line, as regions that fold.
local function folds()
	local length = layout.editor_length(SQL)
	if not length then
		return nil
	end
	local regions = {}
	local text = layout.editor_text(SQL, 0, length) or ""
	local start = 0
	local at = 0
	local lines = 0
	for _, code in utf8.codes(text) do
		local c = utf8.char(code)
		if c == "\n" then
			lines += 1
		end
		at += 1
		if c == ";" then
			if lines > 0 then
				regions[#regions + 1] = { from = start, to = at, fold = true }
			end
			start, lines = at, 0
		end
	end
	return { decorate = "folds", version = layout.editor_version(SQL), from = 0, to = length, decorations = regions }
end

-- --- Completions -------------------------------------------------------------------------------

-- The popup: the words that start as the word before the caret does, the one chosen, and where
-- that word starts.
local popup = nil

-- The word before the caret: where it starts, and its letters.
local function word_before()
	local start = math.max(0, caret - 32)
	local text = layout.editor_text(SQL, start, caret) or ""
	local word = string.match(text, "([%a_][%w_]*)$")
	if not word then
		return nil
	end
	return caret - utf8.len(word), word
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
	local text = layout.editor_text(SQL, 0, length) or ""
	local chars = {}
	for _, code in utf8.codes(text) do
		chars[#chars + 1] = utf8.char(code)
	end
	local from, to = caret, caret
	while from > 0 and chars[from] ~= ";" do
		from -= 1
	end
	while to < #chars and chars[to + 1] ~= ";" do
		to += 1
	end
	return table.concat(chars, "", from + 1, to)
end

local function run()
	local sql = string.gsub(statement(), "^%s+", "")
	b.call("run", { sql = sql }, function(result, error)
		if error then
			note = "It failed: " .. (error.message or error.code or "?")
		else
			note = result.note or "Done."
		end
		return true
	end)
	note = "Running…"
	return true
end

-- --- The page ------------------------------------------------------------------------------------

local function render(size)
	local editor = b.editor(SQL, {
		comment = "--",
		pairs = "()''",
		hint = "Write SQL…",
	}):grow(1):bg("raised1"):focused(true)
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

local function handle(event)
	local kind = event.type
	if kind == "viewport" and event.editor == SQL then
		first_line, last_line = event.first, event.last
		local asks = {}
		if not loaded then
			loaded = true
			pending = sample()
			asks[#asks + 1] = next_piece()
		else
			asks[#asks + 1] = colour(first_line, last_line)
		end
		return { editor = { [SQL] = asks } }
	elseif kind == "edit" and event.editor == SQL then
		version = event.version
		local asks = {}
		if event.origin == "load" then
			local piece = next_piece()
			if piece then
				asks[#asks + 1] = piece
			else
				asks[#asks + 1] = colour(first_line, last_line)
				asks[#asks + 1] = folds()
			end
		else
			-- The lines that changed, coloured again (with those in view: a new line moves them).
			local line = layout.line_of(SQL, event.changes[1].from) or first_line
			asks[#asks + 1] = colour(math.min(line, first_line), math.max(line + 1, last_line))
			asks[#asks + 1] = folds()
		end
		return { editor = { [SQL] = asks } }
	elseif kind == "selection" and event.editor == SQL then
		local main = event.ranges[event.main]
		caret = main.head
		composing = event.composing
		if not composing then
			complete()
		end
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

return { render = render, handle = handle }
