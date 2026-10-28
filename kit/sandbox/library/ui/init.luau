-- @pencere/ui: the components of a page — buttons, toggles, badges, banners — written on the page
-- API alone, drawn in the roles of the theme, used with the keyboard as with the pointer, and said
-- to a screen reader.
--
--   local ui = require("@pencere/ui")
--   local u = ui.new()                      -- { theme =, density = "regular" | "compact", pointer = }
--
--   render = function(size, now, reduce_motion)
--       u:begin(size, now, reduce_motion)
--       local root = pencere.div():size_full()
--           :child(u:button("save", { title = "Save", tone = "primary", keys = "cmd+s" }))
--       return u:finish(root)               -- root, animating, wake_at
--   end,
--   settle = function() return u:settle() end,
--   handle = function(event)
--       local reply, actions, taken = u:handle(event)
--       for _, act in actions do            -- { from = "save", kind = "press" }
--           reply = pencere.merge(reply, on_action(act))
--       end
--       if not taken then
--           reply = pencere.merge(reply, on_event(event))
--       end
--       return reply
--   end,
--
-- A component is a method of the context, called in `render` with a name and its props; what the
-- person does with it comes back from `u:handle` as actions. A page writes components of its own the
-- same way (`ui.component`). The context's other methods (`u:hovered`, `u:focused`, `u:part`, …)
-- are in `core.luau`, the tokens of the theme in `theme.luau`.

local core = require("@self/core")
local look = require("@self/look")
local basic = require("@self/basic")
local controls = require("@self/controls")
local layers = require("@self/layers")
local commands = require("@self/commands")

local ui = {}

ui.new = core.new
ui.component = core.component
-- The names of the glyphs a component draws, and that an icon may name ("chevron_right", "close", …).
ui.GLYPHS = look.GLYPHS
-- A toast to add to the page's items, and the items that have not run out.
ui.commands = commands.new
ui.toast = layers.toast
ui.toasts_prune = layers.toasts_prune

-- The components, as methods of the context.
for _, parts in { basic, controls, { dialog = layers.dialog, toasts = layers.toasts } } do
	for name, component in parts do
		core.Ctx[name] = component
	end
end

return ui
