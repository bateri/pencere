-- The editor page: an editor the terminal keeps, driven as a person drives it.

local test = require("pencere.test")
local expect = test.expect

test("typing into the editor changes its document", function()
	local page = test.open({ calls = {}, size = { 900, 500 } })
	local before = page:editor("sql")
	expect(before):to_be_truthy()
	page:focus_editor("sql")
	page:caret_to("sql", 0)
	page:type("SELECT 1; ")
	expect(page:editor_text("sql")):to_match("^SELECT 1; ")
	page:press("cmd+a")
	page:press("backspace")
	expect(page:editor_text("sql")):to_be("")
	page:edit("undo")
	expect(page:editor_text("sql")):to_match("SELECT 1")
end)
