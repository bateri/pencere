-- A Markdown document kept for a text that changes (what a page shows beside an editor): read again
-- from the first block a change touched, until the blocks are those it had before; drawn again only
-- where its blocks changed; and as long a document as any drawn a view at a time.
--
-- The document is the list of its blocks of its own (`entries`: a block at the top, with where it
-- began and how many lines it took). A block at the top begins where every block before it was
-- closed, so a reading may start again there with nothing open: a change is read from the start of
-- the block before the first line it changed where the line may go on with that block (the line
-- right after it, a paragraph's lazy line or a heading's underline; a list or code indented after
-- blank lines), else from the line after that block, until, in the lines it did not change, a block
-- begins at a line where one began before. (A table whose header was the last line of a paragraph
-- began inside that paragraph: it is read again with it.) The blocks after that are the old ones, moved by the lines the change
-- added or took. A change inside a fenced block of code, a list or a quote so reads that block
-- again, all of it, and no more.
--
-- The work of a call is bounded (`step`, bytes of text read): a large text is read over the calls
-- after, by a timer of its own (`pencere.time.after(0, …)`), each of which asks for a frame, and
-- `busy()` says so. Until it is done the document is what is read so far.

local blocks = require("./blocks")
local inlines = require("./inlines")
local render = require("./render")

local byte, find, sub = string.byte, string.find, string.sub

local Document = {}
Document.__index = Document

-- How much one call reads, bytes of text, unless the page says: well within what a call has
-- (250 ms) on a slow machine, with the page's own work beside it.
local STEP = 20000
-- What a line costs besides its bytes, and a block besides its text.
local LINE_COST, BLOCK_COST = 24, 32

local made_count = 0

function Document.new()
	made_count += 1
	return setmetatable({
		-- The text's lines, and the blocks of the document's own read from them, in order.
		lines = {},
		entries = {},
		-- How many entries, from the first, have their inlines read: those are the document.
		inlined = 0,
		-- The definitions of links, by label.
		refs = {},
		-- What is left to read of the last change (nil: nothing).
		job = nil,
		timer = nil,
		step = STEP,
		-- The start of the names of its elements where the page gives none.
		prefix = "markdown" .. made_count,
		serial = 0,
		-- How many lines and blocks the last change was read in.
		read_lines = 0,
		read_blocks = 0,
		-- The document as last made, and the entries of its blocks.
		document = nil,
		shown = nil,
		dirty = true,
		-- What it was last drawn with, the owner of its links, where its blocks stand in a long list.
		o = nil,
		owner = nil,
		tops = nil,
		width = nil,
		made = nil,
		frame = nil,
	}, Document)
end

-- The lines of `text`: a line break is `\n` or `\r\n`, and a last one makes no line of its own.
local function lines_of(text)
	local lines = {}
	local at, length = 1, #text
	while at <= length do
		local stop = find(text, "\n", at, true)
		local line = sub(text, at, (stop or length + 1) - 1)
		if byte(line, -1) == 13 then
			line = sub(line, 1, -2)
		end
		lines[#lines + 1] = line
		if stop == nil then
			break
		end
		at = stop + 1
	end
	return lines
end

-- --- Entries --------------------------------------------------------------------------------------

-- Forgets what was drawn of an entry (and the links it named).
local function forget(self, entry)
	if entry.element and self.owner then
		render.forget(self.owner, entry.links)
	end
	entry.element, entry.row, entry.links = nil, nil, nil
end

-- The definitions of `entries`, the first of a label standing.
local function refs_of(entries)
	local refs = {}
	for _, entry in ipairs(entries) do
		if entry.defs then
			for _, def in ipairs(entry.defs) do
				if refs[def.label] == nil then
					refs[def.label] = def
				end
			end
		end
	end
	return refs
end

local function same_def(a, b)
	return a == b or (a and b and a.href == b.href and a.title == b.title) or false
end

-- Moves an entry and its blocks `delta` lines.
local function shift_node(node, delta)
	node.line += delta
	local kids = node.children
	if kids and node.type ~= "paragraph" and node.type ~= "heading" then
		for _, kid in ipairs(kids) do
			shift_node(kid, delta)
		end
	end
end

local function shift(entry, delta)
	entry.line += delta
	if entry.node then
		shift_node(entry.node, delta)
	end
	if entry.defs then
		for _, def in ipairs(entry.defs) do
			def.line += delta
		end
	end
end

local function add_entry(self, entry)
	self.serial += 1
	entry.serial = self.serial
	if entry.defs then
		local refs = self.refs
		for _, def in ipairs(entry.defs) do
			if refs[def.label] == nil then
				refs[def.label] = def
			end
		end
	end
	local entries = self.entries
	entries[#entries + 1] = entry
	self.read_blocks += 1
end

-- Reads the inlines of an entry's texts with the definitions known; what it costs.
local function read_inlines(self, entry)
	local texts = entry.texts
	local cost = BLOCK_COST
	if texts and #texts > 0 then
		local lookups = {}
		local refs = self.refs
		for _, text in ipairs(texts) do
			text[1][text[2]] = inlines.parse(text[3], refs, lookups)
			cost += #text[3]
		end
		-- The texts are kept where they looked a label up: a definition that changes reads them again.
		if next(lookups) then
			entry.lookups = lookups
		else
			entry.lookups, entry.texts = nil, nil
		end
	end
	entry.read = true
	forget(self, entry)
	self.dirty = true
	return cost
end

-- --- Reading ------------------------------------------------------------------------------------

-- The tail of the old blocks comes back from the entry `from` of it on.
local function splice(self, job, from)
	local tail = job.tail.entries
	for k = 1, from - 1 do
		forget(self, tail[k])
	end
	local delta = job.delta
	local entries = self.entries
	for k = from, #tail do
		local entry = tail[k]
		if delta ~= 0 then
			shift(entry, delta)
		end
		entries[#entries + 1] = entry
	end
	job.tail = nil
	job.phase = "refs"
end

-- Takes the blocks the reader closed into the document; where line `number` began a block in the
-- lines the change left as they were, and a block began there before, the old blocks come back.
local function take(self, job, number)
	local reader = job.reader
	local closed = reader.closed
	local tail = job.tail
	local back = nil
	if tail and number and number >= job.tail_from then
		local index = tail.index[number - job.delta]
		if index then
			local open = blocks.open(reader)
			local began = open ~= nil and open.line == number
			for k = job.closed + 1, #closed do
				if closed[k].line == number then
					began = true
				end
			end
			if began then
				back = index
			end
		end
	end
	for k = job.closed + 1, #closed do
		local entry = closed[k]
		if not (back and entry.line == number) then
			add_entry(self, entry)
		end
	end
	job.closed = #closed
	if back then
		splice(self, job, back)
	end
end

-- Reads a line of the change (or ends the reading where there are none left); what it costs.
local function read_line(self, job)
	local lines = self.lines
	local number = job.at
	if number > #lines then
		blocks.finish(job.reader)
		take(self, job, nil)
		if job.tail then
			for _, entry in ipairs(job.tail.entries) do
				forget(self, entry)
			end
			job.tail = nil
		end
		job.phase = "refs"
		return LINE_COST
	end
	local line = lines[number]
	blocks.read(job.reader, line)
	job.at = number + 1
	self.read_lines += 1
	take(self, job, number)
	return #line + LINE_COST
end

-- Does what one call may of what is left: the inlines of the blocks read, the lines of the change,
-- then the blocks whose links a changed definition names, read again. `budget` is bytes (the
-- document's `step` where nil).
function Document:work(budget)
	budget = budget or self.step
	local entries = self.entries
	while budget > 0 do
		if self.inlined < #entries then
			local entry = entries[self.inlined + 1]
			if not entry.read then
				budget -= read_inlines(self, entry)
			end
			self.inlined += 1
			self.dirty = true
		else
			local job = self.job
			if job == nil then
				break
			elseif job.phase == "blocks" then
				budget -= read_line(self, job)
			elseif job.phase == "refs" then
				-- All is read: the definitions as they are now, and the blocks that looked up a label
				-- that now means something else.
				local fresh = refs_of(entries)
				local queue = {}
				for _, entry in ipairs(entries) do
					local lookups = entry.lookups
					if lookups then
						for label, seen in pairs(lookups) do
							if not same_def(fresh[label] or false, seen) then
								queue[#queue + 1] = entry
								break
							end
						end
					end
				end
				self.refs = fresh
				job.queue, job.next, job.phase = queue, 1, "links"
				budget -= #entries + LINE_COST
			else
				local entry = job.queue[job.next]
				if entry == nil then
					self.job = nil
				else
					job.next += 1
					budget -= read_inlines(self, entry)
				end
			end
		end
	end
end

-- Whether there is more to read of the last change. It goes on by itself, a call a frame.
function Document:busy()
	return self.job ~= nil
end

-- Goes on with what is left by itself while there is any: a timer that wakes the page at the next
-- frame, does a call's work and asks for a frame for what it read.
local function go_on(self)
	if self.timer ~= nil or self.job == nil then
		return
	end
	self.timer = pencere.time.after(0, function()
		self.timer = nil
		self:work()
		go_on(self)
		return true
	end)
end

-- Begins reading `text` in place of the text it had: the change is read from the block before the
-- first line it changed.
local function begin(self, text)
	local new = lines_of(text)
	local old = self.lines
	local shorter = math.min(#old, #new)
	local first = 1
	while first <= shorter and old[first] == new[first] do
		first += 1
	end
	if first > #old and first > #new then
		if self.job == nil then
			self.read_lines, self.read_blocks = 0, 0
		end
		return false
	end
	local same = 0
	while same < shorter - first + 1 and old[#old - same] == new[#new - same] do
		same += 1
	end
	local entries = self.entries
	-- Where the old text was read to its end, its blocks after the change may come back.
	local complete = self.job == nil or self.job.phase ~= "blocks"
	-- The block before the first line changed is read again where the line may go on with it (it
	-- is the line after it, or it takes lines after blank ones); else the reading starts after it.
	local before = #entries
	while before >= 1 and entries[before].line >= first do
		before -= 1
	end
	local kept, start = 0, 1
	if before >= 1 then
		local entry = entries[before]
		local last = entry.line + entry.lines - 1
		if last < first - 1 and not entry.joins then
			kept, start = before, last + 1
		else
			-- A block that began on a line read as the block before it is read with that block.
			while before > 1 and entries[before].attached do
				before -= 1
			end
			kept, start = before - 1, entries[before].line
		end
	end
	local tail = nil
	if complete and same > 0 then
		local old_from = #old - same + 1
		tail = { entries = {}, index = {} }
		for index = kept + 1, #entries do
			local entry = entries[index]
			if entry.line >= old_from then
				tail.entries[#tail.entries + 1] = entry
				tail.index[entry.line] = #tail.entries
			end
		end
	end
	for index = #entries, kept + 1, -1 do
		local entry = entries[index]
		if not (tail and tail.index[entry.line] and tail.entries[tail.index[entry.line]] == entry) then
			forget(self, entry)
		end
		entries[index] = nil
	end
	if self.job and self.job.tail then
		for _, entry in ipairs(self.job.tail.entries) do
			forget(self, entry)
		end
	end
	self.inlined = math.min(self.inlined, #entries)
	self.lines = new
	self.refs = refs_of(entries)
	self.job = {
		phase = "blocks",
		reader = blocks.new(start),
		at = start,
		closed = 0,
		tail = tail,
		delta = #new - #old,
		tail_from = #new - same + 1,
	}
	self.read_lines, self.read_blocks = 0, 0
	self.dirty = true
	return true
end

-- The document's text is now `text`: what changed is read again, as much as a call may at once,
-- the rest in the frames after (`busy`).
function Document:set(text)
	if type(text) ~= "string" then
		error("set takes the text of the document, not a " .. type(text), 2)
	end
	if begin(self, text) then
		self:work()
	end
	go_on(self)
end

-- How many lines and blocks the last change was read in, so far.
function Document:reread()
	return self.read_lines, self.read_blocks
end

-- The document as far as it is read: { type = "document", children = { block, ... } }.
function Document:tree()
	if self.dirty or self.document == nil then
		local children, shown = {}, {}
		local entries = self.entries
		for index = 1, self.inlined do
			local entry = entries[index]
			if entry.node then
				children[#children + 1] = entry.node
				shown[#shown + 1] = entry
			end
		end
		self.document = { type = "document", children = children }
		self.shown = shown
		self.dirty = false
		self.tops = nil
	end
	return self.document
end

-- The text read all at once, as a document: what `markdown.parse` is.
function Document.parse(text)
	local self = Document.new()
	begin(self, text)
	self:work(math.huge)
	return self:tree()
end

-- --- Drawing ------------------------------------------------------------------------------------

-- Readies a draw with options `o`: what was drawn with other options is forgotten.
local function prepare(self, o)
	o.id = o.id or self.prefix
	if not render.same(self.o, o) then
		for _, entry in ipairs(self.entries) do
			forget(self, entry)
			entry.height, entry.estimate = nil, nil
		end
		if self.job and self.job.tail then
			for _, entry in ipairs(self.job.tail.entries) do
				forget(self, entry)
				entry.height, entry.estimate = nil, nil
			end
		end
		if self.o == nil or self.o.id ~= o.id then
			self.owner = render.owner(o.id, true)
		end
		self.o = o
		self.tops = nil
	end
	self.owner.answer = o.link
end

-- Asks for one more frame: code was left plain for want of what a draw may colour.
local function ask_frame(self)
	if self.frame == nil then
		self.frame = pencere.time.after(0, function()
			self.frame = nil
			return true
		end)
	end
end

-- The element of an entry: the one drawn before, where its block is as it was.
local function element_of(self, entry, draw)
	if entry.element and not entry.provisional then
		draw.count += entry.count
		draw.bytes += entry.bytes
		return entry.element
	end
	forget(self, entry)
	local count, bytes, provisional = draw.count, draw.bytes, draw.provisional
	draw.links, draw.provisional = {}, false
	local element = render.block(entry.node, draw)
	entry.provisional = draw.provisional or render.full(draw)
	entry.element, entry.links = element, draw.links
	entry.count, entry.bytes = draw.count - count, draw.bytes - bytes
	draw.links, draw.provisional = nil, provisional or entry.provisional
	return element
end

-- The column of the document's blocks, drawn with `options` (`markdown.render`'s): a block that is
-- as it was is the element it was.
function Document:render(options)
	local o = render.options(options, 2)
	prepare(self, o)
	self:tree()
	local draw = render.draw(o, self.owner)
	local column = pencere.div():col():gap(o.gap)
	draw.count += 1
	for _, entry in ipairs(self.shown) do
		if render.full(draw) then
			column:child(render.more(draw))
			break
		end
		column:child(element_of(self, entry, draw))
	end
	if draw.provisional then
		ask_frame(self)
	end
	return column
end

-- Where each block stands in a long list, from what the layout said of it or else what it is
-- likely to be: a table kept, changed in place.
local function tops_of(self)
	if self.tops then
		return self.tops
	end
	local tops = self.tops_table or {}
	self.tops_table = tops
	local o, width = self.o, self.width
	local sum = 0
	tops[1] = 0
	local shown = self.shown
	for index, entry in ipairs(shown) do
		local height = entry.height_width == width and entry.height or nil
		if height == nil then
			if entry.estimate == nil or entry.estimate_width ~= width then
				entry.estimate = render.estimate(entry.node, width, o) + o.gap
				entry.estimate_width = width
			end
			height = entry.estimate
		end
		sum += height
		tops[index + 1] = sum
	end
	for index = #tops, #shown + 2, -1 do
		tops[index] = nil
	end
	self.tops = tops
	return tops
end

local function row_name(self, entry)
	return self.o.id .. "/block/" .. entry.serial
end

-- A box named `id` that scrolls, holding the document's blocks as a long list (`box:list`): only
-- those near what is in view are made. The page sizes and places it with the builders after
-- (`doc:list("preview", options):size_full()`), and gives `settle()` the layout (its `settle` returns
-- it), which says how tall the blocks made are.
function Document:list(id, options)
	if type(id) ~= "string" then
		error("list takes the name of the box of a long document (a string), then the options", 2)
	end
	local box = pencere.div():id(id):scroll()
	local o = render.options(options, 2)
	prepare(self, o)
	self:tree()
	self.list_id = id
	local extent = pencere.layout.scroll_extent(id)
	local width = extent and extent.view_w > 0 and extent.view_w or 600
	if self.width ~= width then
		self.width = width
		self.tops = nil
	end
	local rows = pencere.Rows.from_tops(tops_of(self))
	local draw = render.draw(o, self.owner)
	local made = {}
	self.made = made
	local shown = self.shown
	box:list(rows, function(index)
		local entry = shown[index]
		if render.full(draw) then
			return pencere.div():h(rows:height(index))
		end
		local element = element_of(self, entry, draw)
		if entry.row == nil or entry.row_of ~= element then
			entry.row = pencere.div():id(row_name(self, entry)):pb(o.gap):child(element)
			entry.row_of = element
		end
		draw.count += 1
		made[#made + 1] = entry
		return entry.row
	end)
	if draw.provisional then
		ask_frame(self)
	end
	return box
end

-- The index (in what is shown) of the block that holds line `line` of the text, or of the first
-- after it.
local function index_of(self, line)
	local shown = self.shown
	local low, high = 1, #shown
	while low < high do
		local middle = (low + high) // 2
		local entry = shown[middle]
		if entry.line + entry.lines - 1 < line then
			low = middle + 1
		else
			high = middle
		end
	end
	return math.max(math.min(low, #shown), 1)
end

-- Where the block `entry` starts in the long list, held to how far the list scrolls; nil where it
-- is not shown.
local function top_of(self, entry)
	local shown = self.shown
	local tops = tops_of(self)
	for index = 1, #shown do
		if shown[index] == entry then
			local extent = pencere.layout.scroll_extent(self.list_id)
			local top = tops[index]
			return extent and math.min(top, extent.max_offset) or top
		end
	end
	return nil
end

-- The page was laid out: the blocks the long list made are as tall as the layout says. True where
-- one was not as tall as it stood, for the page to be drawn again (its `settle` returns it). A block
-- the list was scrolled to (`reveal`) is scrolled to again where it no longer stands where it was
-- thought to.
function Document:settle()
	local made = self.made
	if made == nil or self.o == nil then
		return false
	end
	local changed = false
	for _, entry in ipairs(made) do
		local bounds = entry.row and pencere.layout.bounds(row_name(self, entry))
		if bounds then
			if entry.height_width ~= self.width or entry.height == nil or math.abs(entry.height - bounds.h) > 0.5 then
				entry.height, entry.height_width = bounds.h, self.width
				changed = true
			end
		end
	end
	if changed then
		self.tops = nil
	end
	local target = self.target
	if target and self.list_id ~= nil then
		self:tree()
		local top = top_of(self, target.entry)
		local offset = pencere.layout.scroll_offset(self.list_id)
		if top == nil or target.tries <= 0 or math.abs(top - offset) <= 0.5 then
			self.target = nil
		else
			target.tries -= 1
			pencere.layout.scroll_to(self.list_id, { y = top })
			changed = true
		end
	end
	return changed
end

-- Scrolls the long list (`list`) to the block that holds line `line` of the text (from 1), as a
-- page beside an editor follows its caret: at once to where the block is thought to be, and again
-- once the blocks around it are laid out and it is known where it is. False before the document is
-- listed.
function Document:reveal(line)
	if type(line) ~= "number" then
		error("reveal takes the number of a line of the text, not a " .. type(line), 2)
	end
	if self.list_id == nil or self.o == nil then
		return false
	end
	self:tree()
	if #self.shown == 0 then
		return false
	end
	local entry = self.shown[index_of(self, line)]
	-- A few times at the most: each lays out more of the blocks around it.
	self.target = { entry = entry, tries = 4 }
	pencere.layout.scroll_to(self.list_id, { y = top_of(self, entry) or 0 })
	return true
end

return Document
