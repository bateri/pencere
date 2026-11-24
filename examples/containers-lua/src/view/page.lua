-- The view of containers-lua: the list of containers, a sparkline of each one's processor, and a
-- button that stops or starts it. It is a page of the kit's page API, which the terminal draws in
-- its own font and theme: the colours below are roles of the theme (success, error, accent, dim),
-- and the icons are SVGs of the app that are painted in a role, so they follow the theme too.
--
-- It asks its program for the list (`list`, through the kit's query module, which keeps what the
-- program answered), listens to the readings it feeds (`stats`), and asks it to stop or start a
-- container (`toggle`). Up and down choose a container, Return stops or starts it.

-- (This example is made with the kit it runs on, so it does not ask `pencere.has("library.query")`
-- first, as a page that an older kit may run does.)
local query = require("@pencere/query")

local b = pencere
local ui = b.Ui.new()
local q = query.new()

-- By container name: the last reading, and the line of the last thirty.
local readings = {}
-- What a stop or a start that failed said.
local failed = nil
local chosen = 1
local busy = {}

local function say(error)
	return error.message or error.code or "it failed"
end

-- The containers the program listed, as the render last read them: what a handler that is not a
-- render looks at (`peek` asks nothing).
local function listed()
	local list = q:peek("list")
	return list and list.data and list.data.containers or {}
end

b.subscribe("stats", nil, function(pushed)
	for _, reading in ipairs(pushed.readings or {}) do
		local entry = readings[reading.name]
		if not entry then
			entry = { history = {} }
			readings[reading.name] = entry
		end
		entry.cpu, entry.memory = reading.cpu, reading.memory
		local history = entry.history
		history[#history + 1] = reading.cpu
		if #history > 30 then
			table.remove(history, 1)
		end
	end
	return true
end)

local function toggle(container)
	if busy[container.id] then
		return
	end
	busy[container.id] = true
	b.call("toggle", { id = container.id }, function(result, error)
		busy[container.id] = nil
		if error then
			failed = say(error)
		else
			-- The record the query module holds is the page's to change where the program says it
			-- did: asking for the whole list again would be a call for one word.
			container.state = result.state
			if result.state ~= "running" then
				readings[container.name] = nil
			end
			failed = nil
		end
		return true
	end)
end

local function row(container, index)
	local running = container.state == "running"
	local reading = readings[container.name]
	local id = "row " .. container.id
	local act = "act " .. container.id
	local button_ground = ui.hover == act and "raised3" or "raised2"
	return b.div():id(id):row():items_center():gap(12):px(12):py(8):rounded(8)
		:bg(index == chosen and "selection" or (ui.hover == id and "raised1" or nil))
		:child(b.image("assets/box.svg"):tint(running and "success" or "dim"):size(20, 20))
		:child(b.div():w(130):child(b.text(container.name):weight(600):truncate()))
		:child(b.div():grow(1):child(b.text(container.image):color("dim"):truncate()))
		:child(b.div():w(90):child(b.text(running and "running" or container.state)
			:color(running and "success" or "error")))
		:child(b.div():w(64):child(b.text(reading and string.format("%.1f%%", reading.cpu) or "–")
			:color(reading and "foreground" or "dim")))
		:child(b.div():w(80):h(20):child(reading and b.spark(reading.history, "accent"):size_full() or b.div()))
		:child(b.div():press(act):row():items_center():gap(6):px(10):py(5):rounded(6):bg(button_ground)
			:child(b.image(running and "assets/stop.svg" or "assets/play.svg"):tint(running and "error" or "success"):size(12, 12))
			:child(b.text(busy[container.id] and "…" or (running and "Stop" or "Start"))))
end

return {
	render = function(size, now, reduce_motion)
		local list = q:get("list")
		local containers = list.data and list.data.containers or {}
		chosen = math.min(chosen, math.max(#containers, 1))
		local note = failed
		if note == nil then
			if list.status == "loading" then
				note = "Loading…"
			elseif list.status == "failed" then
				note = "The list failed: " .. say(list.error)
			elseif #containers == 0 then
				note = "There are no containers."
			else
				note = ""
			end
		end
		local page = b.div():size_full():p(20):gap(10)
			:child(b.div():row():items_center():gap(10)
				:child(b.image("assets/icon.png"):size(28, 28))
				:child(b.text("Containers"):text_size(22):weight(600)))
		for index, container in ipairs(containers) do
			page:child(row(container, index))
		end
		return page
			:child(b.text(note):color("dim"))
			:child(b.spacer())
			:child(b.text("↑ ↓ choose · ⏎ stop or start · ⌃C leave"):color("quiet"):text_size(12))
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
			local id = released and string.match(released, "^act (.+)$")
			for index, container in ipairs(listed()) do
				if container.id == id then
					chosen = index
					toggle(container)
					return true
				end
			end
		elseif event.type == "key" then
			if event.key == "up" then
				chosen = math.max(1, chosen - 1)
				return true
			elseif event.key == "down" then
				chosen = math.min(#listed(), chosen + 1)
				return true
			elseif event.key == "enter" and listed()[chosen] then
				toggle(listed()[chosen])
				return true
			end
		end
	end,
}
