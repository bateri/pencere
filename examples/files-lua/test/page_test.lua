-- The page of files-lua, tested the way a person uses it: under the host with no screen, with
-- stand-ins for the calls the page makes (the program's own `pencere.files` needs a terminal that
-- holds files), the person's files played by the test.

local pencere = require("pencere")
local test = require("pencere.test")
local expect = test.expect

local picture = pencere.fs.read("src/assets/picture.png")
local name = "sha256:" .. pencere.hash.sha256(picture)

-- The stand-ins of the program, and what they were asked, in order.
local function program(asked)
	return {
		picture = function()
			return { name = name }
		end,
		receive = function(params)
			asked[#asked + 1] = "receive " .. params.name
			return { size = 5 }
		end,
		save = function(params)
			asked[#asked + 1] = "save " .. params.handle
			return { size = 5 }
		end,
	}
end

local function open(asked)
	return test.open({ size = { 700, 420 }, attach = { picture }, calls = program(asked) })
end

test("the page draws the picture its program attached", function()
	local page = open({})
	expect(page):to_say("Files")
	expect(page):to_have_called("picture")
	page:shot("attached")
end)

test("files dragged over the page show where they go, and a dropped file is received", function()
	local asked = {}
	local page = open(asked)
	page:get_by_id("drop"):drag_files_over(2)
	expect(page):to_say("Drop 2 Files to Upload")
	page:drag_files_away()
	expect(page):to_say("Drop a file here, or paste one")
	page:get_by_id("drop"):give_files({ { name = "notes.txt", data = "hello" } })
	expect(page):to_say("notes.txt")
	expect(page):to_say("Received")
	expect(asked):to_equal({ "receive notes.txt" })
end)

test("Upload Files asks for the Open panel, and Save a Copy for the Save panel", function()
	local asked = {}
	local page = open(asked)
	page:get_by_text("Upload Files"):click()
	expect(page:panel()):to_equal({ kind = "pick", multiple = true })
	local handles = page:choose_files({ { name = "photo.png", data = picture } })
	expect(page):to_say("photo.png")
	page:get_by_text("Save a Copy"):click()
	expect(page:panel()):to_equal({ kind = "save", name = "photo.png", type = "image/png" })
	local place = page:choose_place("photo copy.png")
	expect(page):to_say("Saved a copy as photo copy.png")
	expect(asked):to_equal({ "receive photo.png", "save " .. place })
	expect(#handles):to_equal(1)
	page:shot("saved")
end)
