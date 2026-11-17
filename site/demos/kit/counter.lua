--@ size 420 160
--@ click add
--@ click add
--@ crop demo
local ui = require("@pencere/ui")

local u = ui.new()
local count = 0

return {
	render = function(size, now, reduce_motion)
		u:begin(size, now, reduce_motion)
		local root = pencere.div():size_full():items_start():bg("background"):color("foreground"):child(
			pencere.div():id("demo"):p(20):gap(12)
				:child(pencere.text("Pressed " .. count .. " times"):text_size(16):weight(700))
				:child(pencere.div():row():gap(8)
					:child(u:button("add", { title = "Press", tone = "primary", keys = "cmd+enter" }))
					:child(u:button("reset", { title = "Reset", disabled = count == 0 })))
		)
		return u:finish(root)
	end,
	settle = function()
		return u:settle()
	end,
	handle = function(event)
		local reply, actions = u:handle(event)
		for _, action in actions do
			if action.from == "add" then
				count += 1
			elseif action.from == "reset" then
				count = 0
			end
			reply = pencere.merge(reply, true)
		end
		return reply
	end,
}
