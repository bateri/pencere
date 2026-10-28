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
