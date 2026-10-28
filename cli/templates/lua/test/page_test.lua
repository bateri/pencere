-- Tests of {{name}}. `pencere test` runs every *_test.lua file in this directory: the page of the app
-- is opened under the host with no screen (the kit's own interpreter, laid out and scrolled as a
-- terminal does it, with a clock the test moves), and the test does to it what a person does.

local test = require("pencere.test")
local expect = test.expect

test("the button shows what the program answers", function()
	-- The program is a stand-in made of the calls the page makes: each is a function of its params.
	local page = test.open({
		calls = {
			hello = function(params)
				return { message = "Hello, " .. params.name .. "!" }
			end,
		},
	})
	expect(page):to_say("Press the button.")
	page:get_by_role("button", { name = "Greet" }):click()
	expect(page):to_say("Hello, pencere!")
	expect(page):to_have_called("hello", { name = "pencere" })
end)

test("the clock shows what the program pushes", function()
	local page = test.open({ calls = {} })
	page:push("clock", { now = 3723 })
	expect(page):to_say("01:02:03 UTC")
end)

test("the app's own program answers the page", function()
	-- With no `calls`, the page speaks to the app's own program (src/program/app.lua), which runs in
	-- this process, with its own time.
	local page = test.open()
	page:get_by_role("button", { name = "Greet" }):click()
	expect(page):to_say("Hello, pencere!")
end)
