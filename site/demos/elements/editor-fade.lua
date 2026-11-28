--@ size 560 160
--@ crop demo
local Syntax = require("@pencere/syntax")
local languages = require("@pencere/languages")
local page = require("../_page")

-- A line that goes past the text thins out at its edge: over 22 points where the page says nothing,
-- here over 48. (A room that is scrolled across thins out at its left too.)
local DOC = [[
local pencere = require("pencere")

local greeting = "Hello, " .. name .. "! This is a line much longer than the room the editor has, so its end thins out."
local short = "Fits."]]

local language = languages.load("Lua")
local syntax = Syntax.new(language.syntax, "code")
local options = table.clone(language.config)
options.read_only = true
options.fade = 48

return page(function()
	return pencere.editor("code", options):w(480):h(120)
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
