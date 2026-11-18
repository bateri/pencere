--@ size 560 220
--@ crop demo
local Syntax = require("@pencere/syntax")
local languages = require("@pencere/languages")
local page = require("../_page")

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

return page(function()
	return pencere.editor("code", options):w(480):h(196)
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
