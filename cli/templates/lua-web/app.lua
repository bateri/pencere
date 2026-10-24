-- {{name}}: the program. It runs in the terminal pane, and web/ is the page it shows there: HTML,
-- JavaScript and CSS, in the terminal's web view. `pencere dev` runs it and runs it again when a
-- file changes (the page reloads without ending the program); `pencere build` makes one file of it.

local pencere = require("pencere")
local time = pencere.time

local outcome = pencere.serve {
	web = "web", -- the files of web/, whose entry is index.html
	title = "{{name}}",

	-- What the page asks for, once: a function that answers, or raises `{ code =, message = }`.
	calls = {
		hello = function(params)
			return { message = "Hello, " .. (params.name or "world") .. "!" }
		end,
	},

	-- What the page listens to as it changes: the function starts feeding `sub`, and says how to stop.
	topics = {
		clock = function(_, sub)
			local timer = time.every(1, function()
				sub:push({ now = time.now() })
			end)
			sub:on_end(function()
				timer:cancel()
			end)
			sub:push({ now = time.now() })
		end,
	},

	-- A terminal that cannot show a view: say it in text.
	fallback = function()
		print("{{name}}: this terminal shows no views, so here is the text.")
		print("Hello, world!")
	end,
}

-- A terminal that draws views itself can be offered a page of the kit's page API first, and this
-- page to the others: `views = { { kind = "lua", page = "page.lua" }, { kind = "web", dir = "web" } }`.

-- After the view: ⌃C ends the app with the status a shell gives it.
pencere.exit(outcome.status)
