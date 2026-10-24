-- {{name}}: the program. It runs in the terminal pane and shows the view of src/view/ there:
-- `serve` told no view shows what that directory has, here index.html, the entry of a web page
-- (HTML, JavaScript and CSS, in the terminal's web view). `pencere dev` runs it and runs it again
-- when a file changes (the page reloads without ending the program); `pencere build` makes one
-- file of it, dist/{{name}}.

local pencere = require("pencere")
local time = pencere.time

local outcome = pencere.serve {
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
-- page to the others: put a page.lua beside index.html in src/view/, and `serve` offers both.

-- After the view: ⌃C ends the app with the status a shell gives it.
pencere.exit(outcome.status)
