-- text-lua: what a page does with the editor's text interactions, with answers of its own where a
-- language server's would be.
--
-- * the pointer rests on a word (`hover` events, after `hover.delay`): a card stands by the word
--   (`pencere.range_of`), saying what the page knows of it, and goes when the pointer leaves it or
--   the text changes;
-- * ⌘ held over a word (`target` events, `gestures`): the page makes a link of it with a look that is
--   `held` and `over`: the editor draws it and gives the hand while ⌘ is held; a ⌘-press on it is told
--   to the page, which says where it would go;
-- * inlay hints (`widget`, before an argument), hidden while ⌃⌥ are held (the group's `show`), and an
--   error at the end of a line (a widget `side = "end"`);
-- * a code lens: an element of the page in a gap above a line (a zone `side = "above"`).

local b = pencere

local CODE = "code"

local lines = {
	"fn main() {",
	"    let total = add(3, 4);",
	"    println!(\"{}\", total)",
	"}",
	"",
	"fn add(left: i32, right: i32) -> i32 {",
	"    left + right",
	"}",
}
local text = table.concat(lines, "\n")

-- Where each line starts, in characters (the text is ASCII).
local starts = {}
do
	local at = 0
	for n, line in lines do
		starts[n] = at
		at += #line + 1
	end
end

local version = 0
local card = nil -- { from =, to =, word =, notes = }
local link = nil -- { from =, to = }
local status = "Hold ⌘ over a word, rest the pointer on one."

local function layers()
	local add_call = starts[2] + 4 + #"let total = " -- `add`
	local diagnostics = {
		-- The `add` of the call: a named style, so that the rest on it tells of it in its `hits`.
		{ from = add_call, to = add_call + 3, underline = { wavy = "warning" }, id = "warn:unused-result" },
	}
	local hints = {
		{ from = add_call + 4, to = add_call + 4, widget = "left:", side = "before", group = "hints", id = "hint:left" },
		{ from = add_call + 7, to = add_call + 7, widget = "right:", side = "before", group = "hints", id = "hint:right" },
		{ from = starts[3], to = starts[3], widget = { "expected `;`", { " (add one)", color = "quiet" } },
			side = "end", group = "errors", color = "error", id = "error:semicolon" },
	}
	local lens = {
		{ from = starts[6], to = starts[6], zone = "lens", height = 22, side = "above" },
	}
	local links = {}
	if link then
		table.insert(links, { from = link.from, to = link.to, color = "accent", underline = "accent", held = "mod",
			over = true, id = "def:" .. link.from })
	end
	local named = {}
	if card then
		table.insert(named, { from = card.from, to = card.to, anchor = "card" })
	end
	local function asked(layer, decorations)
		return { decorate = layer, version = version, from = 0, to = #text, decorations = decorations }
	end
	return {
		asked("diagnostics", diagnostics),
		asked("hints", hints),
		asked("lens", lens),
		asked("links", links),
		asked("names", named),
	}
end

local function render()
	local root = b.div():size_full():col():family("terminal")
	root:child(
		b.editor(CODE, {
			gutter = false,
			events = { "hover", "target" },
			hover = { delay = 0.25 },
			gestures = { ["mod+press"] = "target" },
			widgets = {
				hints = { show = "unless_held", size = 0.85, radius = 5, pad = 4 },
				errors = { gap = 24 },
			},
		})
			:grow(1)
			:child(
				b.div():id("lens"):zone("lens"):h(22):row():gap(12):items_center():px(6)
					:child(b.text("1 reference"):color("accent"))
					:child(b.text("Run Test"):color("accent"))
			)
	)
	root:child(b.text(status):id("status"):color("dim"):px(6):py(4))
	if card then
		root:child(
			b.div():id("card"):anchor(pencere.range_of(CODE, "card"), "below", { gap = 4 })
				:bg("raised2"):border(1, "border"):rounded(6):px(10):py(6):col():gap(2)
				:child(b.text(card.word):weight(700))
				:child(b.text(card.notes):color("dim"))
		)
	end
	return root
end

local function reply(extra)
	extra = extra or {}
	extra.redraw = true
	extra.editor = { [CODE] = layers() }
	return extra
end

local function handle(event)
	local kind = event.type
	if kind == "editor_ready" and event.editor == CODE then
		return { editor = { [CODE] = { { load = text } } } }
	elseif kind == "edit" and event.editor == CODE then
		version = event.version
		card, link = nil, nil
		return reply()
	elseif kind == "hover" and event.kind == "dwell" then
		local word = pencere.layout.editor_text(CODE, event.from, event.to)
		local notes = #event.hits > 0 and ("warning: " .. event.hits[1].id) or "no diagnostics"
		if event.over == "widget" then
			word, notes = "hint", event.id
		end
		card = { from = event.from, to = event.to, word = word, notes = notes }
		return reply()
	elseif kind == "hover" and event.kind == "leave" then
		card = nil
		return reply()
	elseif kind == "target" and event.kind == "enter" then
		link = { from = event.from, to = event.to }
		return reply()
	elseif kind == "target" and event.kind == "leave" then
		link = nil
		return reply()
	elseif kind == "target" and event.kind == "press" then
		local word = pencere.layout.editor_text(CODE, event.from, event.to)
		status = (event.source == "widget" and "Pressed the hint " or "Go to the definition of ") .. tostring(event.id or word)
		return reply()
	end
	return nil
end

return { render = render, handle = handle }
