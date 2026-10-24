-- The view of {{name}}: a page of the kit's page API, which the terminal draws in its own font and
-- theme. It talks to app.lua with `pencere.call` (ask once) and `pencere.subscribe` (listen).

local b = pencere
local ui = b.Ui.new()

local BUTTON = "greet"
local greeting = "Press the button."
local seconds = nil

b.subscribe("clock", nil, function(value)
	seconds = math.floor(value.now) % 86400
	return true
end)

local function greet()
	b.call("hello", { name = "pencere" }, function(result, err)
		greeting = err and ("It failed: " .. tostring(err.message or err.code)) or result.message
		return true
	end)
end

local function clock()
	if not seconds then
		return "…"
	end
	return string.format("%02d:%02d:%02d UTC", seconds // 3600, seconds // 60 % 60, seconds % 60)
end

return {
	render = function(size, now, reduce_motion)
		local hovered = ui.hover == BUTTON
		return b.div():size_full():items_center():justify_center():gap(14)
			:child(b.image("assets/icon.svg"):tint("accent"):size(48, 48))
			:child(b.text(greeting):text_size(20))
			:child(b.text(clock()):color("dim"))
			:child(b.div():press(BUTTON):px(14):py(8):rounded(8)
				:bg(hovered and "raised3" or "raised2")
				:child(b.text("Greet")))
	end,

	handle = function(event)
		if event.type == "move" then
			return ui:point(b.layout, event)
		elseif event.type == "leave" then
			return ui:point(b.layout, nil)
		elseif event.type == "press" then
			ui:press(b.layout, event)
		elseif event.type == "release" then
			if ui:release(b.layout, event) == BUTTON then
				greet()
				return true
			end
		end
	end,
}
