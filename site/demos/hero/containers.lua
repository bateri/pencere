--@ size 960 540
-- The interface the hero's light shows on the screen: the containers `docker ps -a` lists in the
-- terminal beside it, as a program would show them in its own pane. The whole pane, no crop.
local ui = require("@pencere/ui")

local u = ui.new()

local CONTAINERS = {
	{ name = "shop-web", image = "node:22-alpine", state = "running", tone = "success", ports = "3000:3000", up = "3 min" },
	{ name = "shop-db", image = "postgres:17", state = "running", tone = "success", ports = "5432", up = "2 h" },
	{ name = "shop-cache", image = "redis:7", state = "running", tone = "success", ports = "6379", up = "2 h" },
	{ name = "shop-worker", image = "node:22-alpine", state = "running", tone = "success", ports = "", up = "2 h" },
	{ name = "shop-mailer", image = "busybox", state = "exited 1", tone = "error", ports = "", up = "12 min ago" },
}

local cpu, memory = {}, {}
for index = 1, 48 do
	table.insert(cpu, 12 + 7 * math.sin(index / 4) + 4 * math.sin(index / 1.6))
	table.insert(memory, 40 + 3 * math.sin(index / 9))
end

local function dot(tone)
	return pencere.div():w(8):h(8):rounded(4):bg(tone):no_shrink()
end

local function sidebar()
	local function entry(title, count, active)
		local row = pencere.div():row():items_center():px(10):py(7):rounded(6)
			:child(pencere.text(title):color(active and "foreground" or "dim"))
			:child(pencere.spacer())
			:child(pencere.text(tostring(count)):color("dim"))
		if active then
			row:bg("selection")
		end
		return row
	end
	return pencere.div():w(150):h_full():no_shrink():bg("raised1"):p(10):gap(2)
		:child(entry("Containers", 5, true))
		:child(entry("Images", 4))
		:child(entry("Volumes", 3))
		:child(entry("Networks", 2))
		:child(pencere.spacer())
		:child(pencere.div():row():items_center():gap(8):px(10):child(dot("success")):child(pencere.text("docker 29.0.1"):color("dim")))
end

local WIDTHS = { 22, 110, 112, 84, 64 }

local function cells(parts, color)
	local row = pencere.div():row():items_center()
	for index, part in parts do
		row:child(pencere.div():w(WIDTHS[index]):no_shrink():child(part))
	end
	return row
end

local function table_rows()
	local list = pencere.div():gap(2)
		:child(cells({
			pencere.text(""),
			pencere.text("NAME"):color("dim"),
			pencere.text("IMAGE"):color("dim"),
			pencere.text("STATE"):color("dim"),
			pencere.text("PORTS"):color("dim"),
		}):px(10):py(6))
	for index, c in CONTAINERS do
		local row = cells({
			dot(c.tone),
			pencere.text(c.name):weight(700):truncate(),
			pencere.text(c.image):color("dim"):truncate(),
			pencere.text(c.state):color(c.tone),
			pencere.text(c.ports):color("dim"),
		}):px(10):py(8):rounded(6)
		if index == 1 then
			row:bg("selection")
		end
		list:child(row)
	end
	return list
end

local LOGS = {
	{ "09:12:45", "INFO", "listening on :3000", "dim" },
	{ "09:12:47", "WARN", "slow query 412ms", "warning" },
	{ "09:12:51", "INFO", "GET /checkout 200 18ms", "dim" },
	{ "09:12:52", "INFO", "POST /api/cart 201 9ms", "dim" },
	{ "09:13:02", "INFO", "GET /products?page=2 200 31ms", "dim" },
}

local function logs()
	local box = pencere.div():p(12):gap(6):bg("raised1"):border(1, "border"):rounded(8)
	for _, line in LOGS do
		box:child(pencere.div():row():gap(12)
			:child(pencere.text(line[1]):color("dim"))
			:child(pencere.div():w(40):no_shrink():child(pencere.text(line[2]):color(line[4])))
			:child(pencere.text(line[3]):truncate()))
	end
	return pencere.div():gap(8)
		:child(pencere.div():row():items_center()
			:child(pencere.text("shop-web logs"):weight(700))
			:child(pencere.spacer())
			:child(pencere.text("following"):color("dim")))
		:child(box)
end

local function main()
	return pencere.div():w(480):h_full():no_shrink():p(16):gap(14)
		:child(pencere.div():row():items_center():gap(10)
			:child(pencere.text("Containers"):text_size(16):weight(700))
			:child(pencere.text("4 running, 5 in all"):color("dim"))
			:child(pencere.spacer())
			:child(u:button("pull", { title = "Pull" })))
		:child(u:segments("filter", { value = "all", items = {
			{ key = "all", title = "All 5" },
			{ key = "running", title = "Running 4" },
			{ key = "stopped", title = "Stopped 1" },
		} }))
		:child(table_rows())
		:child(pencere.spacer())
		:child(logs())
end

local function card(title, value, note, values, color)
	return pencere.div():grow():p(12):gap(4):border(1, "border"):rounded(8)
		:child(pencere.text(title):color("dim"))
		:child(pencere.text(value):text_size(16))
		:child(pencere.text(note):color("dim"))
		:child(pencere.spark(values, color):peak(60):w_full():h(34))
end

local function field(name, value)
	return pencere.div():row():gap(12)
		:child(pencere.div():w(84):no_shrink():child(pencere.text(name):color("dim")))
		:child(pencere.text(value):truncate())
end

local function detail()
	return pencere.div():w(330):h_full():no_shrink():bg("raised1"):p(16):gap(14)
		:child(pencere.div():row():items_center():gap(10)
			:child(dot("success"))
			:child(pencere.text("shop-web"):text_size(15):weight(700))
			:child(pencere.spacer())
			:child(u:badge("healthy", { tone = "ok" })))
		:child(pencere.div():row():gap(8)
			:child(u:button("stop", { title = "Stop" }))
			:child(u:button("restart", { title = "Restart" }))
			:child(u:button("shell", { title = "Shell" })))
		:child(u:tabs("pane", { active = "overview", closable = false, items = {
			{ key = "overview", title = "Overview" },
			{ key = "logs", title = "Logs" },
			{ key = "env", title = "Environment" },
		} }))
		:child(pencere.div():row():gap(10)
			:child(card("CPU", "12.5%", "of one processor", cpu, "blue"))
			:child(card("Memory", "48.2 MiB", "9.4% of 512 MiB", memory, "cyan")))
		:child(pencere.div():gap(8)
			:child(field("image", "node:22-alpine"))
			:child(field("ports", "0.0.0.0:3000 to 3000/tcp"))
			:child(field("command", "node server.js"))
			:child(field("started", "3 minutes ago")))
		:child(pencere.spacer())
		:child(pencere.div():p(12):gap(6):border(1, "border"):rounded(8)
			:child(pencere.div():row():items_center():gap(8)
				:child(pencere.text("Health"):color("dim"))
				:child(u:badge("healthy", { tone = "ok" })))
			:child(pencere.text("wget localhost:3000/healthz"):color("dim"))
			:child(pencere.text("ok, 3 checks in a row")))
end

return {
	render = function(size, now, reduce_motion)
		u:begin(size, now, reduce_motion)
		local body = pencere.div():row():w(960):h(516):child(sidebar()):child(main()):child(detail())
		local root = pencere.div():col():size_full():bg("background"):color("foreground")
			:child(body)
			:child(u:status("keys", {
				start = { { key = "stop", words = "s stop", priority = 9 }, { key = "restart", words = "r restart", priority = 8 }, { key = "shell", words = "e shell", priority = 7 } },
				finish = { { key = "help", words = "? help", priority = 6 }, { key = "quit", words = "^C quit", priority = 5 } },
			}))
		return u:finish(root)
	end,
	settle = function()
		return u:settle()
	end,
	handle = function(event)
		local reply = u:handle(event)
		return reply
	end,
}
