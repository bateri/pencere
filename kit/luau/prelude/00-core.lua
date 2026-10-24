-- The page API a script is written against, part 1: what the host gives (kept out of the
-- script's reach once it is wrapped) and the small things the rest is made of.
--
-- A script is Luau. What it can use besides the language is the base functions, `string`,
-- `table`, `math` and `utf8`, and `pencere`, the table this prelude fills:
--
--   elements   pencere.div(), pencere.text(), pencere.spark(), pencere.image(), pencere.spacer()
--   colours    a role of the theme ("accent", "red", "bright-red", "selection", "raised2", ...), a
--              value ("#rrggbb", "#rrggbbaa", or { light = "#...", dark = "#..." } for a dark
--              theme), and any of them with an opacity: pencere.alpha("accent", 0.5)
--   pages      the script returns { render = ..., handle = ..., settle = ..., message = ..., room = ... }
--   memory     pencere.Ui, pencere.Fades, pencere.Rows (what a page remembers between frames)
--   layout     pencere.layout (what is where on the page as it was last laid out)
--   program    pencere.send, pencere.call, pencere.subscribe, pencere.cancel, pencere.json
--
-- Lengths are points. An element is a plain table; the builders are methods that return it.

pencere.version = 1

-- A number as the engine holds it, in single precision. The engine measures and draws in
-- single precision, and a page that adds up highlights or sizes in doubles can come to a
-- value that is not the engine's (a level of 1e-17 where the engine has 0).
function pencere.f32(value)
	return (string.unpack("<f", string.pack("<f", value)))
end

-- The host's functions, taken here and given a shape of Lua's below (`pencere.layout`,
-- `pencere.send`); the table of the raw ones stays at `pencere.host` for a page that wants them.
local host = {
	send = pencere.send,
	hit = pencere.hit,
	ids_at = pencere.ids_at,
	bounds = pencere.bounds,
	scroll_extent = pencere.scroll_extent,
	focus_order = pencere.focus_order,
	path_of = pencere.path_of,
	text_field = pencere.text_field,
}
pencere.send, pencere.hit, pencere.ids_at, pencere.bounds = nil, nil, nil, nil
pencere.scroll_extent, pencere.focus_order, pencere.path_of, pencere.text_field = nil, nil, nil, nil
pencere.host = host

-- A colour with an opacity. Everywhere a colour is taken, a role's name, a "#rrggbb" value and a
-- { light =, dark = } table do too; `colour` is any of the three.
function pencere.alpha(colour, alpha)
	return { colour, alpha }
end

-- The ground raised by `steps` levels ("raised1" is a card on the pane).
function pencere.raised(steps)
	return "raised" .. steps
end

-- A grid track: points are a number, the rest are these.
function pencere.fr(share)
	return { fr = share }
end

function pencere.range(least, share)
	return { min = least, fr = share }
end

-- What the host answers about the page as it laid it out last (which may be a frame behind
-- the tree handed out since). Points are `{ x =, y = }`.
local layout = {}
pencere.layout = layout

function layout.hit(at)
	return host.hit(at.x, at.y)
end

function layout.ids_at(at)
	return host.ids_at(at.x, at.y)
end

function layout.bounds(id)
	return host.bounds(id)
end

-- How far the scrolling box named `id` reaches: { content =, view = }, with `max_offset`.
function layout.scroll_extent(id)
	local extent = host.scroll_extent(id)
	if extent then
		extent.max_offset = math.max(extent.content - extent.view, 0)
	end
	return extent
end

function layout.focus_order()
	return host.focus_order()
end

function layout.path_of(id)
	return host.path_of(id)
end

function layout.text_field()
	return host.text_field()
end
