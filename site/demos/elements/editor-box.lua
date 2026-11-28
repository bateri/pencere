--@ size 560 260
--@ crop demo
local Syntax = require("@pencere/syntax")
local languages = require("@pencere/languages")
local page = require("../_page")

-- An editor in a box with a border and a padding draws and cuts inside the border, and its text starts
-- past the box's padding and then its own (`padding`, 4 by default): 2 + 14 + 4 points from the edge.
-- The box's padding goes with the text when it is scrolled.
local DOC = [[
local pencere = require("pencere")

pencere.serve {
  calls = {
    hello = function(params)
      return { message = "Hello, " .. params.name .. "!" }
    end,
  },
}]]

local language = languages.load("Lua")
local syntax = Syntax.new(language.syntax, "code")
local options = table.clone(language.config)
options.read_only = true
options.gutter = false

return page(function()
	return pencere.editor("code", options):w(500):h(236):bg("raised1"):border(2, "border"):p(14)
end, {
	on_event = function(event)
		syntax:handle(event)
		if event.type == "editor_ready" and event.editor == "code" then
			return { editor = { code = { load = DOC } } }
		end
	end,
	reply = function(reply)
		return syntax:reply(reply)
	end,
})
