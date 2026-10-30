-- The view of files-lua: the picture its program attached, a place to drop files on, and the files
-- received, each with a button that saves a copy of it. It is a page of the kit's page API at `lua`
-- 2.1: the person's files come as `files` events, each with a handle the page passes on to its
-- program and draws an image by, and the Open and the Save panels are asked for in a reply to a
-- press. The page never sees a file's bytes.

local b = pencere
local ui = b.Ui.new()

local picture = nil
-- How many files are dragged over the page, while they are.
local over = nil
-- The files received, in the order they came: { handle =, name =, size =, type =, state = }.
local received = {}
-- The file whose copy the Save panel is for.
local saving_for = nil
local note = ""

local function say(error)
	return error.message or error.code or "it failed"
end

b.call("picture", nil, function(result, error)
	if error then
		note = "The picture failed: " .. say(error)
	else
		picture = result.name
	end
	return true
end)

local function size_of(bytes)
	if bytes >= 1024 * 1024 then
		return string.format("%.1f MB", bytes / (1024 * 1024))
	elseif bytes >= 1024 then
		return string.format("%.1f KB", bytes / 1024)
	end
	return bytes .. " bytes"
end

local function receive(file)
	local entry = { handle = file.handle, name = file.name, size = file.size, type = file.type, state = "Receiving" }
	received[#received + 1] = entry
	-- The path is there only where the program reads the same files as the person: it copies the
	-- file from there.
	b.call("receive", { handle = file.handle, name = file.name, path = file.path, size = file.size }, function(result, error)
		if error then
			entry.state = "Failed: " .. say(error)
		else
			entry.state = "Received"
			entry.size = result.size
		end
		return true
	end)
end

local function row(entry)
	local save = "save " .. entry.handle
	local is_image = string.match(entry.type or "", "^image/") ~= nil
	local item = b.div():id("file " .. entry.handle):row():items_center():gap(12):px(12):py(8):rounded(8)
		:bg(ui.hover == "file " .. entry.handle and "raised1" or nil)
	if is_image then
		item:child(b.image("file:" .. entry.handle):size(32, 32))
	end
	item:child(b.div():grow(1):child(b.text(entry.name):weight(600):truncate()))
		:child(b.div():w(90):child(b.text(size_of(entry.size)):color("dim")))
		:child(b.div():w(110):child(b.text(entry.state):truncate()
			:color(entry.state == "Received" and "success" or "dim")))
	if entry.state == "Received" then
		item:child(b.div():press(save):px(10):py(5):rounded(6):bg(ui.hover == save and "raised3" or "raised2")
			:child(b.text("Save a Copy")))
	end
	return item
end

return {
	render = function()
		local page = b.div():size_full():p(20):gap(12)
			:child(b.div():row():items_center():gap(12)
				:child(picture and b.image(picture):size(32, 32) or b.div():size(32, 32))
				:child(b.div():gap(2)
					:child(b.text("Files"):text_size(22):weight(600))
					:child(b.text("The picture is attached by the program."):color("dim"))))
			:child(b.div():id("drop"):h(90):rounded(10):border(1, over and "accent" or "border")
				:bg(over and "raised2" or "raised1"):items_center():justify_center():gap(8)
				:child(b.text(over and (over == 1 and "Drop to Upload" or ("Drop " .. over .. " Files to Upload"))
					or "Drop a file here, or paste one"):color(over and "accent" or "dim"))
				:child(b.div():press("choose"):px(10):py(5):rounded(6):bg(ui.hover == "choose" and "raised3" or "raised2")
					:child(b.text("Upload Files"))))
		for _, entry in ipairs(received) do
			page:child(row(entry))
		end
		return page:child(b.text(note):color("dim"))
	end,

	handle = function(event)
		if event.type == "move" then
			return ui:point(b.layout, event)
		elseif event.type == "leave" then
			return ui:point(b.layout, nil)
		elseif event.type == "press" then
			ui:press(b.layout, event)
		elseif event.type == "release" then
			local released = ui:release(b.layout, event)
			if released == "choose" then
				return { pick = { multiple = true } }
			end
			local handle = released and string.match(released, "^save (.+)$")
			for _, entry in ipairs(received) do
				if entry.handle == handle then
					saving_for = entry
					return { save = { name = entry.name, type = entry.type } }
				end
			end
		elseif event.type == "files_over" then
			over = event.count
			return true
		elseif event.type == "files_left" then
			over = nil
			return true
		elseif event.type == "files" then
			over = nil
			for _, file in ipairs(event.files) do
				if file.kind == "folder" then
					note = "Folders can't be uploaded yet."
				else
					receive(file)
				end
			end
			return true
		elseif event.type == "saving" then
			local entry = saving_for
			saving_for = nil
			if event.handle and entry then
				note = "Saving " .. event.name
				b.call("save", { handle = event.handle, of = entry.handle }, function(_, error)
					note = error and ("The copy failed: " .. say(error)) or ("Saved a copy as " .. event.name)
					return true
				end)
			end
			return true
		end
	end,
}
