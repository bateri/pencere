-- The view of motion-lua: how a change of a page moves instead of jumping, a section for each thing
-- an element's `motion` follows. The page builds itself as it is now, each time, and says nothing of
-- the frames in between: the host moves what the page says moves (an element is followed by its id),
-- and a person who has asked for less motion sees every change as a jump and nothing that loops.
--
-- * a spring that bounces, whose place and width are sent elsewhere while it is on its way;
-- * a panel whose height follows the tab, a card whose shadow and corners follow the pointer;
-- * a header whose padding and the size of its words follow a press;
-- * a picture that turns (`transform`), and what goes round and round: a status dot that fades
--   in and out (`loop`), an arrow that bobs, a spinner (`spin`).

local b = pencere
local ui = b.Ui.new()

-- What the page keeps of what the person did.
local state = { segment = 1, tab = 1, open = false, compact = false }

local SEGMENTS = { "Day", "Week", "Month" }
local SEGMENT_WIDTH = 80
-- How wide the pill is under each of them.
local PILL = { 64, 80, 72 }

local TABS = { "Overview", "Details", "Log" }
local PANEL = { 54, 92, 130 }
local LINES = {
	{ "Nothing is wrong." },
	{ "Four things are running.", "Two of them are waiting." },
	{ "12:01 started", "12:02 waiting for the disk", "12:04 the disk is there", "12:05 running" },
}

local function section(title, ...)
	local row = b.div():col():gap(6):child(b.text(title):color("dim"):text_size(12))
	for _, child in ipairs({ ... }) do
		row:child(child)
	end
	return row
end

-- A button of words: it is named by its id and answers the pointer.
local function button(id, words, selected)
	local over = ui.hover == id
	return b.div():press(id):px(12):py(5):rounded(6)
		:bg(selected and "raised3" or (over and "raised2" or "raised1"))
		:child(b.text(words))
end

local function springs()
	local row = b.div():row()
	for index, name in ipairs(SEGMENTS) do
		row:child(
			b.div():press("segment:" .. index):w(SEGMENT_WIDTH):py(5):items_center()
				:child(b.text(name):color(state.segment == index and "foreground" or "dim"))
		)
	end
	local pill = b.div():row():h(6):child(
		b.div():id("pill-gap"):h(6):w((state.segment - 1) * SEGMENT_WIDTH):no_shrink()
			:motion({ seconds = 0.45, ease = "spring", bounce = 0.35, size = true })
	):child(
		b.div():id("pill"):h(6):w(PILL[state.segment]):rounded(3):bg("accent"):no_shrink()
			:motion({ seconds = 0.45, ease = "spring", bounce = 0.35, size = true })
	)
	return section("A spring that bounces, and keeps its speed where it is sent elsewhere", row, pill)
end

local function panel()
	local tabs = b.div():row():gap(6)
	for index, name in ipairs(TABS) do
		tabs:child(button("tab:" .. index, name, state.tab == index))
	end
	local body = b.div():id("panel"):h(PANEL[state.tab]):px(12):py(8):col():gap(4):bg("raised1"):rounded(6):clip()
		:motion({ seconds = 0.35, ease = "spring", bounce = 0.15, size = true })
	for _, line in ipairs(LINES[state.tab]) do
		body:child(b.text(line):no_shrink())
	end
	return section("A panel that is as high as its tab", tabs, body)
end

local function card()
	local over = ui.hover == "card"
	return section(
		"A card whose corners and shadow follow the pointer",
		b.div():press("card"):w(220):h(54):px(14):items_center():row()
			:bg(over and "raised3" or "raised2")
			:rounded(over and 16 or 4)
			:shadow(0, over and 10 or 1, over and 24 or 3, b.alpha("foreground", 0.3))
			:motion({ seconds = 0.2, color = true, rounded = true, shadow = true })
			:child(b.text(over and "Lifted" or "Resting"))
	)
end

local function header()
	local compact = state.compact
	return section(
		"A header whose padding and words follow a press",
		b.div():press("header"):row():items_center():gap(8)
			:p(compact and 4 or 14)
			:text_size(compact and 13 or 20)
			:bg("raised1"):rounded(6)
			:motion({ seconds = 0.3, padding = true, text_size = true })
			:child(b.text("Header"):weight(600))
			:child(b.text(compact and "compact" or "roomy"):color("dim"))
	)
end

local function turning()
	return section(
		"A picture that turns",
		b.div():press("disclose"):row():items_center():gap(6)
			:child(
				b.image("assets/arrow.svg"):id("arrow"):size(16, 16):tint("foreground")
					:transform({ rotate = state.open and 90 or 0 })
					:motion({ seconds = 0.4, ease = "spring", bounce = 0.3, transform = true })
			)
			:child(b.text(state.open and "Hide the details" or "Show the details"))
	)
end

local function going_round()
	return section(
		"What goes round and round",
		b.div():row():items_center():gap(24)
			:child(
				b.div():row():items_center():gap(6)
					:child(
						b.div():id("status"):size(10, 10):rounded(5):bg("accent")
							:role("status"):label("Live")
							:motion({ loop = { seconds = 1.6, opacity = 0.3 } })
					)
					:child(b.text("Live"))
			)
			:child(
				b.image("assets/arrow.svg"):id("bob"):size(16, 16):tint("foreground")
					:transform({ rotate = 90 })
					:motion({ loop = { seconds = 1.2, dy = 4 } })
			)
			:child(
				b.image("assets/loader.svg"):id("spinner"):size(20, 20):tint("accent")
					:motion({ spin = 1 })
			)
	)
end

local function render()
	return b.div():size_full():col():p(16):gap(18)
		:child(springs())
		:child(panel())
		:child(card())
		:child(header())
		:child(turning())
		:child(going_round())
end

local function released(id)
	local segment = string.match(id, "^segment:(%d)$")
	local tab = string.match(id, "^tab:(%d)$")
	if segment then
		state.segment = tonumber(segment)
	elseif tab then
		state.tab = tonumber(tab)
	elseif id == "header" then
		state.compact = not state.compact
	elseif id == "disclose" then
		state.open = not state.open
	end
end

local function handle(event)
	if event.type == "move" then
		return ui:point(b.layout, event)
	elseif event.type == "leave" then
		return ui:point(b.layout, nil)
	elseif event.type == "press" then
		ui:press(b.layout, event)
	elseif event.type == "release" then
		local id = ui:release(b.layout, event)
		if id then
			released(id)
			return true
		end
	end
	return nil
end

return { render = render, handle = handle }
