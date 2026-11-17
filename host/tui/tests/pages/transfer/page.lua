-- The page of the tests of files. F4 asks for the Open panel, F5 for the Save panel, F6 for a menu
-- whose pick puts the page's log on the terminal's pasteboard (the person's own word). What the person
-- gives is passed on to the app (`upload`), and what the person chooses to save in (`download`); how
-- each ended is in the log. F7 passes the app the handle of the last file given as a place to save in,
-- F8 the handle of the last place chosen as a file to read: neither is what it is asked to be. F9 asks
-- for the Save panel with a name that climbs out of the folder the person types.

local b = pencere

local log = {}
local given, placed = nil, nil

local function say(text)
	log[#log + 1] = text
end

local function ended(what)
	return function(result, error)
		if error then
			say(what .. " error " .. tostring(error.code) .. " " .. tostring(error.message))
		else
			say(what .. " " .. tostring(result.size))
		end
		return true
	end
end

return {
	render = function()
		return b.div():size_full():p(8):child(b.text("transfer"))
	end,

	handle = function(event)
		if event.type == "key" then
			if event.key == "f4" then
				return { pick = { multiple = false } }
			elseif event.key == "f5" then
				return { save = { name = "out.bin" } }
			elseif event.key == "f6" then
				return { menu = { at = { x = 80, y = 54 }, entries = { { title = "report", tag = "report" } } } }
			elseif event.key == "f9" then
				-- A name that would put the file somewhere else than where the person chose.
				return { save = { name = "../escaped.bin" } }
			elseif event.key == "f7" and given then
				b.call("download", { handle = given }, ended("download-of-a-given-file"))
			elseif event.key == "f8" and placed then
				b.call("upload", { handle = placed, name = "never.bin" }, ended("upload-of-a-place"))
			end
		elseif event.type == "menu" then
			return { copy = table.concat(log, ";") }
		elseif event.type == "files" then
			local file = event.files[1]
			if file and file.handle then
				given = file.handle
				b.call("upload", { handle = file.handle, name = file.name }, ended("upload"))
			else
				say("pick turned away")
			end
			return true
		elseif event.type == "saving" then
			if event.handle then
				placed = event.handle
				b.call("download", { handle = event.handle }, ended("download"))
			else
				say("save turned away")
			end
			return true
		end
	end,
}
