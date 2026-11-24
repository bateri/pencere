-- The page of the containers app, tested the way a person uses it: under the host with no screen,
-- with the app's own program in this process, and with stand-ins for the calls the page makes.

local test = require("pencere.test")
local expect = test.expect

test("the page lists the containers its own program answers", function()
	local page = test.open({ size = { 900, 500 } })
	expect(page):to_say("Containers")
	expect(page):to_say("nginx:1.27")
	expect(page):to_have_called("list")
end)

test("a stand-in program answers the list, and the page shows only that", function()
	local page = test.open({
		size = { 900, 500 },
		calls = {
			list = function()
				return {
					containers = {
						{ id = "a", name = "alpha", image = "alpine:3", ports = "", state = "running" },
					},
				}
			end,
		},
	})
	expect(page):to_say("alpha")
	expect(page).never:to_say("nginx")
end)

test("a call nobody answers is waited for, and the test answers it", function()
	local page = test.open({ size = { 900, 500 }, calls = {} })
	expect(page).never:to_say("alpha")
	page:answer("list", {
		containers = { { id = "a", name = "alpha", image = "alpine:3", ports = "", state = "running" } },
	})
	expect(page):to_say("alpha")
end)

test("a button is found by its role and its words", function()
	local page = test.open({ size = { 900, 500 } })
	local stop = page:get_by_role("button", { name = "Stop" })
	expect(stop):to_exist()
	expect(stop:count() > 1).never:to_be(nil)
end)

test("the page is read element by element, the ones with no name too", function()
	local page = test.open({ size = { 900, 500 } })
	local looked = page:inspect()
	local lines = {}
	for line in page:inspect_text():gmatch("[^\n]+") do
		lines[#lines + 1] = line
	end
	expect(lines[1]):to_match("^page 900x500 · %d+ elements$")
	expect(lines[2]):to_be("box 0,0 900x500 padding 20 · w 100% · h 100% · gap 10,10")
	-- The first box in it has no name: where it stands and how it was placed are what there is to
	-- tell it by.
	local title = looked.items[2]
	expect(title.name):to_be_nil()
	expect(title.path):to_equal({ 1 })
	expect(title.rect.x):to_be(20)
	expect(title.placing.direction):to_be("row")
	-- What is at a point in the button that stops a container is in the button.
	local stop
	for _, item in ipairs(looked.items) do
		for _, said in ipairs(item.said) do
			if said.role == "button" and said.words == "Stop" then
				stop = stop or item
			end
		end
	end
	expect(stop ~= nil):to_be(true)
	local found = page:inspect_at(stop.rect.x + stop.rect.w / 2, stop.rect.y + stop.rect.h / 2)
	expect(found ~= nil):to_be(true)
	expect(#found.path >= #stop.path):to_be(true)
	for at, place in ipairs(stop.path) do
		expect(found.path[at]):to_be(place)
	end
end)
