-- files-lua: an app that moves files with its view. The page shows a picture this program attaches
-- (`picture`), passes on the handle of a file the person gave it, which this program receives into a
-- directory (`receive`), and the handle of a place the person chose in the Save panel, to which this
-- program writes a copy of a file it received (`save`).
--
--   files-lua [--into DIR]   received files go to DIR (by default the directory it is run in)

local pencere = require("pencere")
local files, path = pencere.files, pencere.path

local into = "."
local args = pencere.args
local at = 1
while at <= #args do
	if args[at] == "--into" and args[at + 1] then
		into = args[at + 1]
		at = at + 2
	else
		print("files-lua: no option " .. args[at])
		pencere.exit(64)
	end
end

-- Where each file received is now, by the handle the person gave it with.
local received = {}

pencere.serve({
	calls = {
		-- The picture the page shows: the app's own, attached to the view, drawn by the name.
		picture = function()
			return { name = files.attach_data(pencere.app.read("assets/picture.png")) }
		end,
		-- A file the person gave the page, received under its own name. The name is the person's:
		-- only its last part, with nothing a path could be made of, goes into the path. Where the
		-- page was handed the file's path (this program runs on the person's machine), it passes it
		-- on with the size, and the file is copied from there rather than through the terminal.
		receive = function(params)
			local to = path.join(into, files.safe_name(params.name))
			local size = files.receive(params.handle, to, { path = params.path, size = params.size })
			received[params.handle] = to
			return { size = size }
		end,
		-- A copy of a file received (`of`, its handle), written to the place the person chose.
		save = function(params)
			local from = received[params.of]
			if not from then
				error({ code = "not-found", message = "that file was not received" })
			end
			return { size = files.send(params.handle, from) }
		end,
	},
	fallback = function()
		print("files-lua: this terminal shows no views")
	end,
})
