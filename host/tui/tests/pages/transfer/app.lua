-- The app of the tests of files: it receives the file the person gave its page, into a directory
-- (`upload`), and writes a file of its own to the place the person chose (`download`).
--
--   transfer --from FILE --into DIR

local pencere = require("pencere")
local files, path = pencere.files, pencere.path

local from, into = nil, "."
local args = pencere.args
local at = 1
while at <= #args do
	if args[at] == "--from" and args[at + 1] then
		from = args[at + 1]
		at = at + 2
	elseif args[at] == "--into" and args[at + 1] then
		into = args[at + 1]
		at = at + 2
	else
		print("transfer: no option " .. args[at])
		pencere.exit(64)
	end
end

pencere.serve({
	page = "page.lua",
	calls = {
		-- A file the person gave: received by its handle, never by the path the page was handed.
		upload = function(params)
			local to = path.join(into, files.safe_name(params.name))
			return { size = files.receive(params.handle, to) }
		end,
		-- The place the person chose: the app's file is written to it.
		download = function(params)
			return { size = files.send(params.handle, from) }
		end,
	},
	fallback = function()
		print("transfer: this terminal shows no views")
	end,
})
