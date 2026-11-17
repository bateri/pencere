--@ size 520 160
--@ crop demo
local page = require("../_page")

local cpu, net = {}, {}
for index = 1, 60 do
	table.insert(cpu, 30 + 14 * math.sin(index / 5) + 6 * math.sin(index / 1.7))
	table.insert(net, math.max(1, 10 + 9 * math.sin(index / 3.3) * math.sin(index / 9)))
end

local function card(title, value, values, color)
	return pencere.div():w(160):gap(6)
		:child(pencere.div():row():child(pencere.text(title):color("dim")):child(pencere.spacer()):child(pencere.text(value)))
		:child(pencere.spark(values, color):peak(60):w_full():h(36))
end

return page(function()
	return pencere.div():row():gap(20)
		:child(card("CPU", "30%", cpu, "blue"))
		:child(card("Network", "3 MiB/s", net, "cyan"))
end)
