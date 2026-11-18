-- A page of one part, as the site pictures it: the part on the theme's ground, in an element named
-- `demo` that the picture is cut to. `draw(u)` makes the part in a render; `options.on_event(event)`
-- and `options.on_action(action)` answer what the page is told, and `options.width` and
-- `options.height` give the part a size where it has none of its own.
local ui = require("@pencere/ui")

return function(draw, options)
	options = options or {}
	local u = ui.new()
	return {
		render = function(size, now, reduce_motion)
			u:begin(size, now, reduce_motion)
			local demo = pencere.div():id("demo"):p(options.pad or 20):child(draw(u))
			if options.width then
				demo:w(options.width + 2 * (options.pad or 20))
			end
			if options.height then
				demo:h(options.height + 2 * (options.pad or 20))
			end
			local root = pencere.div():size_full():items_start():bg("background"):color("foreground"):child(demo)
			if options.finish then
				options.finish(u, root)
			end
			return u:finish(root)
		end,
		settle = function()
			return u:settle()
		end,
		handle = function(event)
			local reply, actions = u:handle(event)
			if options.on_event then
				reply = pencere.merge(reply, options.on_event(event))
			end
			for _, action in actions do
				if options.on_action then
					reply = pencere.merge(reply, options.on_action(action))
				end
			end
			if options.reply then
				reply = options.reply(reply)
			end
			return reply
		end,
	}
end
