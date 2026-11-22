-- The page of motion-lua, tested the way a person uses it: under the host with no screen, whose
-- clock the test moves (`page:advance`), looking at where things are on the way.

local test = require("pencere.test")
local expect = test.expect

local function open()
	return test.open({ size = { 700, 820 } })
end

local function width_of(page, id)
	return page:get_by_id(id):bounds().w
end

test("a spring goes past where it is sent, comes back and stays", function()
	local page = open()
	expect(width_of(page, "pill-gap")):to_be(8)
	page:get_by_id("segment:3"):click()
	local widest = 0
	for _ = 1, 40 do
		page:advance(0.02)
		widest = math.max(widest, width_of(page, "pill-gap"))
	end
	expect(widest > 164):to_be(true)
	page:idle()
	expect(width_of(page, "pill-gap")):to_be(164)
	expect(width_of(page, "pill")):to_be(72)
end)

test("a spring sent elsewhere in the middle of its way goes on from where it is", function()
	local page = open()
	page:get_by_id("segment:3"):click()
	page:advance(0.1)
	local before = width_of(page, "pill-gap")
	expect(before > 8 and before < 164):to_be(true)
	page:get_by_id("segment:1"):click()
	local after = width_of(page, "pill-gap")
	-- Where it was, not where it began or where it was going.
	expect(math.abs(after - before) <= 2):to_be(true)
	page:idle()
	expect(width_of(page, "pill-gap")):to_be(8)
end)

test("a panel is as high as its tab, and gets there over a moment", function()
	local page = open()
	local first = page:get_by_id("panel"):bounds().h
	page:get_by_id("tab:3"):click()
	page:advance(0.1)
	local on_the_way = page:get_by_id("panel"):bounds().h
	expect(on_the_way > first and on_the_way < 130):to_be(true)
	page:idle()
	expect(page:get_by_id("panel"):bounds().h):to_be(130)
	expect(page):to_say("12:05 running")
end)

test("an answer opens to what it holds, its lines where they will be, and closes the same way", function()
	local page = open()
	local function body() return page:get_by_id("answer-body:2"):bounds() end
	local closed = body()
	expect(closed.h):to_be(0)
	local below = page:get_by_id("answer:3"):bounds().y
	page:get_by_id("answer:2"):click()
	local line = page:get_by_id("answer-line:2:2"):bounds()
	local heights = {}
	for _ = 1, 8 do
		page:advance(0.04)
		local on_the_way = body()
		-- The line is where it will be from the first frame, whatever of it the box shows.
		local moved = page:get_by_id("answer-line:2:2"):bounds()
		expect(moved.y):to_be(line.y)
		expect(moved.h):to_be(line.h)
		-- What is under the box follows its edge.
		expect(math.abs(page:get_by_id("answer:3"):bounds().y - (below + on_the_way.h)) <= 1):to_be(true)
		table.insert(heights, on_the_way.h)
	end
	page:idle()
	local open_h = body().h
	expect(open_h > 30):to_be(true)
	local between = false
	for _, h in ipairs(heights) do
		between = between or (h > 0 and h < open_h)
	end
	expect(between):to_be(true)
	expect(page):to_say("every change is a jump.")
end)

test("an answer that closes goes on being drawn while the box shrinks, and then it is gone", function()
	local page = open()
	page:get_by_id("answer:1"):click()
	page:idle()
	local open_h = page:get_by_id("answer-body:1"):bounds().h
	page:get_by_id("answer:1"):click()
	local last = open_h
	for _ = 1, 6 do
		page:advance(0.04)
		local h = page:get_by_id("answer-body:1"):bounds().h
		expect(h <= last):to_be(true)
		last = h
	end
	expect(last < open_h):to_be(true)
	page:idle()
	expect(page:get_by_id("answer-body:1"):bounds().h):to_be(0)
	expect(page:get_by_id("answer-line:1:1"):exists()):to_be(false)
end)

test("the padding and the words of a header follow a press, and the page is laid out for each frame", function()
	local page = open()
	local roomy = page:get_by_id("header"):bounds()
	page:get_by_id("header"):click()
	page:advance(0.1)
	local on_the_way = page:get_by_id("header"):bounds()
	page:idle()
	local compact = page:get_by_id("header"):bounds()
	expect(compact.h < on_the_way.h and on_the_way.h < roomy.h):to_be(true)
	expect(page):to_say("compact")
end)

test("a picture that is turned keeps its box, and a page that goes round and round is at rest for idle", function()
	local page = open()
	local arrow = page:get_by_id("arrow"):bounds()
	page:get_by_id("disclose"):click()
	page:advance(0.1)
	local turned = page:get_by_id("arrow"):bounds()
	expect(turned.x):to_be(arrow.x)
	expect(turned.w):to_be(arrow.w)
	-- A status dot, a bobbing arrow and a spinner never stop: `idle` does not wait for them.
	page:idle()
	expect(page):to_say("Hide the details")
	expect(page:get_by_role("status"):exists()):to_be(true)
end)

test("what bobs is moved by the clock alone, and the others stay where they are", function()
	local page = open()
	local at = page:get_by_id("bob"):bounds().y
	page:advance(0.3)
	local moved = page:get_by_id("bob"):bounds().y
	expect(moved > at):to_be(true)
	expect(moved - at <= 4):to_be(true)
	-- A spinner turns about its middle: its box stays.
	local spinner = page:get_by_id("spinner"):bounds()
	page:advance(0.2)
	local turned = page:get_by_id("spinner"):bounds()
	expect(turned.x):to_be(spinner.x)
	expect(turned.y):to_be(spinner.y)
end)

test("the page asks of the kit nothing it does not have, so nothing is warned", function()
	local page = open()
	page:get_by_id("segment:2"):click()
	page:advance(0.2)
	expect(#page:logged()):to_be(0)
end)
