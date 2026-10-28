-- @pencere/markdown: Markdown read into a tree of blocks and inlines, and drawn as elements of the
-- page.
--
-- The dialect is CommonMark with GitHub's extensions (GFM): tables, task lists, strikethrough and
-- the links that need no brackets (`www.…`, `https://…`, `ftp://…`, `a@b.c`). HTML in the text, a
-- block of it or a tag in a line, is kept as the text it is and shown so, never run. The reading is
-- CommonMark's own (its reference parser, commonmark.js): blocks a line at a time, then the inlines
-- of each block with the delimiter runs of emphasis and the brackets of links on stacks.
--
--   local markdown = require("@pencere/markdown")
--   local tree = markdown.parse("# Title\n\nSome *words* and a [link](https://example.com).")
--   local element = markdown.render(tree, { id = "notes" })
--   handle = function(event) return markdown.handle(event) end      -- a press on a link opens it
--
-- A text that changes (a preview beside an editor) is a kept document, read again only from the
-- block a change touched and drawn again only where its blocks changed; a long one is drawn as a
-- long list, only the blocks near what is in view made:
--
--   local doc = markdown.document()
--   doc:set(text)                                    -- again with each change of the text
--   render = function() return doc:list("preview", options):size_full() end
--   settle = function() return doc:settle() end
--   handle = function(event) return markdown.handle(event) end
--
-- The tree: every block has the lines of the text it came from (`line`, from 1, and `lines`).
--   document { children }
--   heading { level, children }, paragraph { children }, quote { children },
--   list { ordered, start, tight, children = items }, item { checked = nil | true | false, children },
--   code { language, info, text }, html { text }, rule,
--   table { align = { "left" | "center" | "right" | false, ... }, head = { cell, ... },
--           rows = { { cell, ... }, ... } }   (a cell is a list of inlines)
-- and the inlines:
--   text { text }, emphasis { children }, strong { children }, strike { children }, code { text },
--   link { href, title, children }, image { src, title, alt }, html { text }, softbreak, hardbreak
--
-- It is written on the page API alone: elements, `pencere.layout` (what was pressed, how tall a
-- block came out), `pencere.Rows` and `box:list` for a long document, `pencere.time` to read a
-- large text over several calls.

local Document = require("@self/document")
local render = require("@self/render")

local markdown = {}

-- The tree of `text` (Markdown), read all at once: { type = "document", children = { block, ... } }.
function markdown.parse(text)
	if type(text) ~= "string" then
		error("markdown.parse takes the text of a document, not a " .. type(text), 2)
	end
	return Document.parse(text)
end

-- The element of a document (what `parse` made): a column of its blocks. `options` (each optional):
--   id         the start of the names of its elements (its links are `<id>/link/<n>`): "markdown"
--   headings   the text sizes of the six levels of heading: { 22, 18, 16, 14.5, 13.5, 13 }
--   colors     the colours it draws with, roles of the theme: text (its words; nil, the box's),
--              quiet ("dim": quotes, list markers, HTML, images), link ("accent"), code ("raised2":
--              the ground of code), head ("raised1": a table's head), quote ("border": the bar of a
--              quote), rule ("border": rules and the lines of tables)
--   gap        the room between blocks, points: 12
--   size       the size of its text, points, for what it works out before the layout (a list's
--              markers, a long document's heights); it does not set the size: 13
--   link       function(href) -> reply: what a press on a link answers (`markdown.handle`); by
--              default `{ link = href }` for an address of the web (http, https), nothing else
--   code       function(language, text) -> element: a block of code (`language` the first word
--              of its fence's info, or nil); by default a box of the terminal's font coloured by
--              `@pencere/languages` where it knows the language
--   image      function(src, alt, title) -> element: an image; by default its description, as
--              words of the text (a page shows only the pictures of its bundle)
--   languages  what finds the language of a block of code (`find(word)`, `load(description)`, as
--              `@pencere/languages`): that module by default, where the kit has it
--   limit      the most elements it makes: 10 000; what is past it is left out, and "…" says so
function markdown.render(document, options)
	if type(document) ~= "table" or document.type ~= "document" then
		error("markdown.render takes a document (markdown.parse), then options", 2)
	end
	local o = render.options(options, 2)
	local owner = render.owner(o.id or "markdown", true)
	owner.answer = o.link
	return render.document(document, render.draw(o, owner))
end

-- A document kept for a text that changes: `doc:set(text)`, `doc:tree()`, `doc:render(options)`,
-- `doc:list(id, options)`, `doc:settle()`, `doc:reveal(line)`, `doc:busy()`, `doc:work()`,
-- `doc:reread()`.
function markdown.document()
	return Document.new()
end

-- What a press on a link drawn by this module (`render`, a document's `render` or `list`) answers:
-- the page gives it its events, and it answers a click on a link (the left button pressed and let
-- go on it) with what the draw's `link` option says. Nil for anything else.
local pressed = nil

function markdown.handle(event)
	if type(event) ~= "table" then
		return nil
	end
	local kind = event.type
	if (kind ~= "press" and kind ~= "release") or event.button ~= "left" or type(event.x) ~= "number" then
		return nil
	end
	local id = pencere.layout.hit({ x = event.x, y = event.y })
	if kind == "press" then
		pressed = render.href(id) and id or nil
		return nil
	end
	local was = pressed
	pressed = nil
	if id ~= nil and id == was then
		return render.answer(id)
	end
	return nil
end

-- The address of the link drawn under the name `id`, or nil: for a page that finds what was
-- clicked its own way (`Ui:release`).
function markdown.href(id)
	return (render.href(id))
end

return markdown
