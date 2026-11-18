-- @pencere/charts: charts drawn on a canvas, made in two halves: what a chart is (its model: the
-- points of its lines, its rules, the moment under the pointer, in the points of its box) and how it
-- is painted (a painter: the paths of a canvas made of the model). The kit's painter draws smooth
-- lines and fills in the theme's roles; a page, or a kit of its own, may give another (dots, a
-- hatch) and keep the chart's model, its scale and its pointer as they are.
--
--   local charts = require("@pencere/charts")
--   charts.area({
--       id = "cpu", w = 600, h = 120,
--       series = { { values = cpu, role = "accent" } },
--       ceiling = 100,
--       marker = charts.back_at({ w = 600, count = 60 }, pointer_x),
--   })
--
-- What the pointer is over is the page's to say (`marker`): `charts.back_at` turns a point of the
-- box into the sample there.

local charts = {}

-- A line of `count` samples is that many steps of a path, and a path takes 4096.
local MOST_SAMPLES = 4000

local function is_list(value)
	return type(value) == "table" and (#value > 0 or next(value) == nil)
end

local function is_colour(value)
	return type(value) == "string" or type(value) == "table"
end

local function number(value)
	return type(value) == "number" and value == value and value ~= math.huge and value ~= -math.huge
end

-- The newest `count` of `values`.
local function tail(values, count)
	local n = #values
	local out = {}
	for index = math.max(1, n - count + 1), n do
		out[#out + 1] = values[index]
	end
	return out
end

-- What `spec` says, checked, with what it leaves out filled in: `count`, `ceiling` and `rules`. It
-- is called by a function of the module that the page called, so an error for what the page gave is
-- raised two levels up, at the page's line.
local function resolve(spec)
	local level = 3
	if type(spec) ~= "table" then
		error("a chart is a table of what it is, not " .. type(spec), level)
	end
	if not (number(spec.w) and spec.w > 0 and number(spec.h) and spec.h > 0) then
		error("a chart says its size, w and h, points above 0", level)
	end
	if not is_list(spec.series) then
		error("a chart's series is a list of { values =, role = }", level)
	end
	local longest = 0
	for index, series in spec.series do
		if type(series) ~= "table" or not is_list(series.values) then
			error("series " .. index .. " of a chart has no list of values", level)
		end
		if not is_colour(series.role) then
			error("series " .. index .. " of a chart has no colour (role)", level)
		end
		for at, value in series.values do
			if not number(value) then
				error("value " .. at .. " of series " .. index .. " of a chart is not a number", level)
			end
		end
		longest = math.max(longest, #series.values)
	end
	local count = spec.count or math.max(longest, 2)
	if not (number(count) and count == math.floor(count) and count >= 2 and count <= MOST_SAMPLES) then
		error("a chart's count is a whole number of samples from 2 to " .. MOST_SAMPLES, level)
	end
	local ceiling = spec.ceiling
	if ceiling == nil then
		ceiling = 0
		for _, series in spec.series do
			for _, value in tail(series.values, count) do
				ceiling = math.max(ceiling, value)
			end
		end
		-- A chart of nothing but zeros still has a scale.
		if ceiling <= 0 then
			ceiling = 1
		end
	elseif not number(ceiling) then
		error("a chart's ceiling is a number", level)
	elseif ceiling <= 0 then
		-- A scale made of what was seen (a quiet machine's largest value) can be 0: drawn as a chart
		-- of nothing but zeros is.
		ceiling = 1
	end
	local rules = spec.rules or { 0.5 }
	if not is_list(rules) then
		error("a chart's rules are a list of fractions of its height", level)
	end
	for _, fraction in rules do
		if not (number(fraction) and fraction >= 0 and fraction <= 1) then
			error("a chart's rule is a fraction of its height, from 0 to 1", level)
		end
	end
	local marker = spec.marker
	if marker ~= nil and not (number(marker) and marker == math.floor(marker)) then
		error("a chart's marker is a whole number of samples back from the newest", level)
	end
	local painter = spec.painter
	if painter ~= nil and not (type(painter) == "table" and type(painter.area) == "function" and type(painter.marker) == "function") then
		error("a chart's painter has area(model) and marker(model)", level)
	end
	if spec.id ~= nil and type(spec.id) ~= "string" then
		error("a chart's id is a string", level)
	end
	return {
		w = spec.w,
		h = spec.h,
		series = spec.series,
		count = count,
		ceiling = ceiling,
		rules = rules,
		marker = marker,
		painter = painter or charts.smooth,
		id = spec.id,
	}
end

-- The model of a resolved chart, with its marker where it has one.
local function model_of(chart)
	local w, h = chart.w, chart.h
	local step = w / (chart.count - 1)
	local inner = h - 2
	local function y_of(value)
		return 1 + inner - math.min(math.max(value / chart.ceiling, 0), 1) * inner
	end
	local rules = {}
	for index, fraction in chart.rules do
		-- On the middle of a pixel, so that a rule a point wide is not two half lit.
		rules[index] = math.floor(h * fraction) + 0.5
	end
	local model = { w = w, h = h, rules = rules, series = {} }
	for _, series in chart.series do
		local values = tail(series.values, chart.count)
		local n = #values
		if n >= 2 then
			local x0 = w - (n - 1) * step
			local points = {}
			for index, value in values do
				points[index] = { x0 + (index - 1) * step, y_of(value) }
			end
			model.series[#model.series + 1] = { role = series.role, points = points }
		end
	end
	local back = chart.marker
	if back ~= nil and back >= 0 and back <= chart.count - 1 then
		local x = w - back * step
		local points = {}
		for _, series in chart.series do
			local values = tail(series.values, chart.count)
			local value = values[#values - back]
			if value ~= nil then
				points[#points + 1] = { x, y_of(value), series.role }
			end
		end
		model.marker = { x = x, points = points }
	end
	return model
end

-- What a chart is, the model a painter paints (`spec` as `charts.area` takes it):
--
--   { w =, h =,                                           -- the box, points
--     rules = { y, … },                                   -- quiet rules across it
--     series = { { role =, points = { { x, y }, … } } },  -- each line, x rising
--     marker = { x =, points = { { x, y, role }, … } } }  -- the moment under the pointer, or nil
function charts.model(spec)
	return model_of(resolve(spec))
end

-- The sample under the point `x` of a chart's box (0 at its left edge), counted back from the newest
-- as `marker` takes it, or nil where `x` is outside the box. `spec` needs `w` and `count`.
function charts.back_at(spec, x)
	if type(spec) ~= "table" or not (number(spec.w) and spec.w > 0) then
		error("back_at takes a chart's w and count", 2)
	end
	local count = spec.count
	if not (number(count) and count >= 2) then
		error("back_at takes a chart's w and count", 2)
	end
	if not number(x) or x < 0 or x > spec.w then
		return nil
	end
	local step = spec.w / (count - 1)
	return math.min(math.max(math.floor((spec.w - x) / step + 0.5), 0), count - 1)
end

-- The kit's painter: lines along the series, the area under each filled faintly in the line's
-- colour, rules and the marker in `ink`. Each look is an option, its default the kit's:
--
--   line = 1.5     how wide a line is, points
--   fill = 0.2     how opaque the area under a line is
--   ink = "foreground"   the colour of the rules and the marker
--   rule = 0.07    how opaque a rule is
--   marker = 0.45  how opaque the marker's line is
--   dot = 4        the side of the marker's dot on each line, points
--
-- Make it once (`local painter = charts.painter({ … })`), not in `render`: a chart's drawing is kept
-- for the painter it was drawn with.
function charts.painter(options)
	if options ~= nil and type(options) ~= "table" then
		error("a painter's options are a table", 2)
	end
	options = options or {}
	local line = options.line or 1.5
	local fill = options.fill or 0.2
	local ink = options.ink or "foreground"
	local rule = options.rule or 0.07
	local marker = options.marker or 0.45
	local dot = options.dot or 4
	for name, value in { line = line, fill = fill, rule = rule, marker = marker, dot = dot } do
		if not (number(value) and value >= 0) then
			error("a painter's " .. name .. " is a number from 0", 2)
		end
	end
	if not is_colour(ink) then
		error("a painter's ink is a colour", 2)
	end
	local painter = {}
	function painter.area(model)
		local paths = {}
		for _, y in model.rules do
			paths[#paths + 1] = pencere.path():move_to(0, y):line_to(model.w, y):stroke(1, pencere.alpha(ink, rule))
		end
		for _, series in model.series do
			local outline = pencere.path()
			local area = pencere.path()
			for index, point in series.points do
				local x, y = point[1], point[2]
				if index == 1 then
					area:move_to(x, model.h):line_to(x, y)
					outline:move_to(x, y)
				else
					area:line_to(x, y)
					outline:line_to(x, y)
				end
			end
			area:line_to(model.w, model.h):close():fill(pencere.alpha(series.role, fill))
			outline:stroke(line, series.role)
			paths[#paths + 1] = area
			paths[#paths + 1] = outline
		end
		return paths
	end
	function painter.marker(model)
		local at = model.marker
		local paths = { pencere.path():move_to(at.x, 0):line_to(at.x, model.h):stroke(1, pencere.alpha(ink, marker)) }
		for _, point in at.points do
			paths[#paths + 1] = pencere.path():rect(point[1] - dot / 2, point[2] - dot / 2, dot, dot):fill(point[3])
		end
		return paths
	end
	return painter
end

charts.smooth = charts.painter()

-- A colour as words, for the key a drawing is kept by.
local function colour_key(colour)
	if type(colour) ~= "table" then
		return tostring(colour)
	end
	local parts = {}
	for name, value in colour do
		parts[#parts + 1] = tostring(name) .. "=" .. colour_key(value)
	end
	table.sort(parts)
	return "{" .. table.concat(parts, ",") .. "}"
end

-- The drawing of each chart named by an id, the key of what it was drawn of and its paths, for the
-- charts of this frame and of the one before: a drawing can be a large part of the page's memory,
-- and a chart the page stopped drawing (a panel it closed) lets go of its own a frame later. A frame
-- is seen to begin when a chart it already drew is asked for again, so the page says nothing of it.
-- At most `MOST_KEPT` charts are kept for a frame: a page that names a chart anew each time is not
-- made to keep every one.
local MOST_KEPT = 64
local kept_now = {}
local kept_before = {}
local kept_count = 0

-- The drawing kept for `id`, carried into this frame, or nil.
local function kept_drawing(id)
	if kept_now[id] ~= nil or kept_count >= MOST_KEPT then
		-- Asked for again: a new frame.
		kept_before, kept_now = kept_now, {}
		kept_count = 0
	end
	local drawing = kept_before[id]
	kept_before[id] = nil
	return drawing
end

local function key_of(chart)
	local parts = {
		tostring(chart.painter.area),
		chart.w,
		chart.h,
		chart.count,
		chart.ceiling,
		table.concat(chart.rules, ","),
	}
	for _, series in chart.series do
		parts[#parts + 1] = colour_key(series.role)
		parts[#parts + 1] = table.concat(tail(series.values, chart.count), ",")
	end
	return table.concat(parts, ":")
end

-- An area chart: a line for each series (its values oldest first, the newest at the right edge; one
-- shorter than `count` starts further right) and the area under it, between 0 and `ceiling`, `w` by
-- `h` points. `spec`:
--
--   series = { { values =, role = }, … }   each line and its colour
--   w =, h =       the size, points (a canvas has none of its own)
--   count          samples across the box (the longest series', and 2 at least, where not given)
--   ceiling        the top of the scale (the largest value shown, where not given; 1 for 0 or less)
--   rules          fractions of the height a quiet rule is drawn at ({ 0.5 } where not given)
--   marker         the sample under the pointer, counted back from the newest (`charts.back_at`)
--   painter        { area = function(model), marker = function(model) } (`charts.smooth`)
--   id             a name the chart is kept by: its drawing is made again only when what it shows
--                  changes, not when only the pointer moves
--   label          what a screen reader says of it
function charts.area(spec)
	local chart = resolve(spec)
	local model = model_of(chart)
	local marker = model.marker
	model.marker = nil
	local paths
	if chart.id ~= nil then
		local key = key_of(chart)
		local before = kept_drawing(chart.id)
		if before ~= nil and before.key == key then
			paths = before.paths
		else
			paths = chart.painter.area(model)
		end
		kept_now[chart.id] = { key = key, paths = paths }
		kept_count += 1
	else
		paths = chart.painter.area(model)
	end
	if marker ~= nil then
		model.marker = marker
		local all = table.clone(paths)
		for _, path in chart.painter.marker(model) do
			all[#all + 1] = path
		end
		paths = all
	end
	local box = pencere.div():w(chart.w):h(chart.h):no_shrink():clip():child(pencere.canvas(paths):size(chart.w, chart.h))
	if spec.label ~= nil then
		box:label(tostring(spec.label))
	end
	return box
end

return charts
