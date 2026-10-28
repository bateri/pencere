-- @pencere/ui, the context: what every component is drawn in and told through.
--
-- A page is built afresh for every frame, and so is each component in it: a component is a
-- function the page calls in `render` (`u:button(id, props)`), which returns an element. What it
-- shows is what the page gives it each time (`props`); what the hand did to it (the pointer over
-- it, a button held on it, the keyboard on it) is kept by name in the context, as `pencere.Ui` keeps
-- it; and what a person would miss if it were lost (the row chosen, the folders open, a width) is a
-- table the page may own and pass in (`props.state`), which the component changes in place as the
-- person uses it. A component the page does not give that table keeps it in the context by its name,
-- for as long as it is drawn: one that a render does not draw is forgotten, as a long list is.
--
-- What came of the person's doing is said in `handle`, never in `render`: `u:handle(event)` gives the
-- event to the component it is for and returns the reply, the actions that came of it (`{ from =
-- id, kind = "press" }`: what the page does something about) and whether a component took it.
--
--   local u = ui.new()
--   render = function(size, now, reduce_motion)
--       u:begin(size, now, reduce_motion)
--       return u:finish(pencere.div():child(u:button("save", { title = "Save" })))
--   end,
--   handle = function(event)
--       local reply, actions, taken = u:handle(event)
--       for _, act in actions do ... end
--       return reply
--   end,

local theme = require("./theme")
local keys = require("@pencere/keys")

local layout = pencere.layout
local merge = pencere.merge

local core = {}

local Ctx = {}
Ctx.__index = Ctx
core.Ctx = Ctx

-- --- Making one -----------------------------------------------------------------------------------

-- A context for a page: `options.theme` changes tokens of the theme (`theme.luau`),
-- `options.density` is "regular" (the default) or "compact", and `options.pointer` is a
-- `pencere.Ui` the page keeps already (a new one where it gives none).
function core.new(options)
	if options ~= nil and type(options) ~= "table" then
		error("ui.new takes a table of options, not " .. type(options), 2)
	end
	options = options or {}
	if options.pointer ~= nil and getmetatable(options.pointer) ~= pencere.Ui then
		error("ui.new's pointer is a pencere.Ui", 2)
	end
	return setmetatable({
		theme = theme.make(options.theme, options.density, 2),
		pointer = options.pointer or pencere.Ui.new(),
		size = { w = 0, h = 0 },
		now = 0,
		reduce_motion = false,
		-- The components of the last render, by name, and the names of their parts; what the next
		-- event is told through. Made again by each render.
		mounts = {},
		parts = {},
		-- The fields and editors the keyboard goes into as text, by name (`u:field`).
		fields = {},
		-- The layers (dialogs, palettes) of the last render, in order (the last is in front), and
		-- by name; where the keyboard was before each opened.
		layers = {},
		open = {},
		saved = {},
		top = nil,
		front = nil,
		-- What a component keeps by its name where the page gives it no table: { name =, state =,
		-- seen = }.
		memory = {},
		-- What the host holds in each field a component drew, as far as the context knows it: the text
		-- the person made or the page filled in it last.
		held = {},
		-- The commands of the last render (`ui.commands`), by id; and those components ask to be written
		-- on the page's outermost box, for `finish` to write.
		commands = {},
		queued = {},
		-- The menu the page last asked the host for: { id =, entries = }.
		asked = nil,
		-- Where the keyboard is: "page", or the name of a field or an editor that has it.
		holder = "page",
		-- The last of what the person did was a key: the ring of what has the keyboard is drawn.
		keyed = false,
		-- A part a button went down on that follows the pointer until it comes up: { mount =, name =,
		-- key =, from = }.
		capture = nil,
		-- The pointer's shape the context last asked for.
		cursor = nil,
		-- The ids around the element the pointer is over and the one the keyboard is on, as the page
		-- was last laid out: sets, read by `within` and `focus_within`.
		hover_path = {},
		focus_path = {},
		-- What the render asks of the frames: one now, or one at a time.
		animating = false,
		wake = nil,
		building = false,
		stack = {},
	}, Ctx)
end

-- --- Components -----------------------------------------------------------------------------------

-- A component: `spec.view(u, id, props, state) -> Element` draws it. The other members of `spec`
-- are optional:
--
--   state(props) -> table     what it keeps that a person would miss, as it starts
--   check(props) -> words     why the props are wrong (raised at the line of the page that drew it)
--   click(u, id, props, state, part, key, event)      a left click on it or a part of it
--   press(u, id, props, state, part, key, event)      any button went down on it. A third value, true, says
--                                                     the part is one that is held and dragged: `drag`
--                                                     and `release` come to the component until it is up
--   drag(u, id, props, state, part, key, at)          the pointer moved with the button held that
--                                                     went down on it (`press` took it): at.x, at.y,
--                                                     at.dx, at.dy from where it went down
--   release(u, id, props, state, part, key, at)       that button came up
--   drop(u, id, props, state, dropped)                what was dragged off it was put down
--   key(u, id, props, state, event, part, key)        a key, while the keyboard is on it or in it
--   text(u, id, props, state, part, key, event)       a field of it changed
--   command(u, id, props, state, part, key, event)    a command it wrote was asked for
--   scrolled(u, id, props, state, part, key, box)     a box of it was scrolled
--   editor(u, id, props, state, event)                an event of the editor `props.editor`
--   away(u, id, props, state, event, hit)             a button went down outside it (a popover closes)
--   dismiss(u, id, props, state)                      escape came to it as the layer in front: what it says
--                                                     of that (`{ kind = "dismiss" }` where it says nothing)
--   action(u, id, props, state, action)               an action of a component inside it
--   hover(u, id, props, state, part, key)             the pointer came over it or a part of it
--   cursor(u, id, props, state, part, key)            the pointer's shape over it
--   settle(u, id, props, state) -> boolean            the page was laid out
--
-- A handler answers `reply, action`: a reply of the kit's (`false` for "taken, no frame") and the
-- action that came of it, or nil for "not mine", and the event goes on to the component around it.
-- An action is `{ kind = ..., ... }`; `from` is the component's name unless it says another.
function core.component(name, spec)
	if type(name) ~= "string" then
		error("ui.component takes a name, then what the component is", 2)
	end
	if type(spec) ~= "table" or type(spec.view) ~= "function" then
		error("a component has a view: ui.component(name, { view = function(u, id, props, state) ... end })", 2)
	end
	return function(u, id, props)
		if getmetatable(u) ~= Ctx then
			error("a component is drawn in a context: u:" .. name .. "(id, props)", 2)
		end
		if type(id) ~= "string" then
			error("a " .. name .. " is named by a string, not " .. type(id), 2)
		end
		if props ~= nil and type(props) ~= "table" then
			error("a " .. name .. "'s props are a table, not " .. type(props), 2)
		end
		props = props or {}
		if spec.check then
			local wrong = spec.check(props)
			if wrong then
				error(wrong, 2)
			end
		end
		if not u.building then
			error("a component is drawn in render, between u:begin and u:finish", 2)
		end
		if u.mounts[id] then
			error("two components on the page are named " .. id, 2)
		end
		return u:_mount(name, spec, id, props)
	end
end

function Ctx:_mount(name, spec, id, props)
	local state = props.state
	if spec.state then
		if state == nil then
			local kept = self.memory[id]
			if kept == nil or kept.name ~= name then
				kept = { name = name, state = spec.state(props) }
				self.memory[id] = kept
			end
			kept.seen = true
			state = kept.state
		else
			-- The page's table, with what it does not say yet as the component starts it.
			for key, value in spec.state(props) do
				if state[key] == nil then
					state[key] = value
				end
			end
		end
	end
	local mount = {
		id = id,
		name = name,
		spec = spec,
		props = props,
		state = state,
		-- The component whose view drew this one, if one did.
		parent = self.stack[#self.stack],
	}
	self.mounts[id] = mount
	table.insert(self.stack, id)
	local element = spec.view(self, id, props, state)
	table.remove(self.stack)
	if getmetatable(element) ~= pencere.Element then
		error("the view of " .. name .. " " .. id .. " gave no element", 0)
	end
	-- The component's element carries its name, so that the elements inside it find it around them
	-- (`pencere.layout.path_of`).
	if element.meta.id == nil then
		element:id(id)
	elseif element.meta.id ~= id then
		error("the element of " .. name .. " " .. id .. " is named " .. tostring(element.meta.id), 0)
	end
	return element
end

-- The name of the part `part` of the component `id` (with `key`, one of many: a row, a tab): what
-- the component names an element inside it by, and how what is done to that element finds the
-- component. Parts are `id/part` and `id/part/key`.
function Ctx:part(id, part, key)
	local name
	if key == nil then
		name = id .. "/" .. part
	else
		name = id .. "/" .. part .. "/" .. tostring(key)
	end
	self.parts[name] = { mount = id, part = part, key = key }
	return name
end

-- The name `u:part(id, part, key)` gives, with nothing made of it (a component that is asked after
-- the render, in `settle`, names its parts as it did in `view`).
function Ctx:name(id, part, key)
	if key == nil then
		return id .. "/" .. part
	end
	return id .. "/" .. part .. "/" .. tostring(key)
end

-- What the part named `name` is: { mount = the component's name, part =, key = }, or nil.
function Ctx:part_of(name)
	return self.parts[name]
end

-- The component whose element or part is named `name`, with the part and its key (nil for the
-- component's own element).
function Ctx:_owner(name)
	if name == nil then
		return nil
	end
	local mount = self.mounts[name]
	if mount then
		return mount, nil, nil
	end
	local part = self.parts[name]
	if part then
		mount = self.mounts[part.mount]
		if mount then
			return mount, part.part, part.key
		end
	end
	return nil
end

-- The component around `mount`: the one whose view drew it, else the nearest around its element.
function Ctx:_parent(mount)
	if mount.parent then
		return self.mounts[mount.parent]
	end
	local path = layout.path_of(mount.id)
	for index = 2, #path do
		local around = self.mounts[path[index]]
		if around then
			return around
		end
	end
	return nil
end

-- --- What a component reads as it draws ------------------------------------------------------------

-- The ids around `name` as the page was last laid out (it among them), as a set.
local function around(name)
	local set = {}
	if name ~= nil then
		for _, id in layout.path_of(name) do
			set[id] = true
		end
	end
	return set
end

-- The pointer is over the element `id` (one that answers it).
function Ctx:hovered(id)
	return self.pointer.hover == id
end

-- The pointer is over `id` or over something inside it: a tab, or its button that closes it.
function Ctx:within(id)
	return self.pointer.hover == id or self.hover_path[id] == true
end

-- A button went down on `id` and is held there.
function Ctx:pressed(id)
	return self.pointer.pressed == id and self.pointer.hover == id
end

-- The keyboard is on `id`: what the page marks `focused()`, so that its commands can be done.
function Ctx:focused(id)
	return self:target() == id
end

-- The keyboard is on `id` or on something inside it.
function Ctx:focus_within(id)
	return self:target() == id or self.focus_path[id] == true
end

-- The keyboard is on `id`, and a key brought it there: its ring is drawn.
function Ctx:focus_visible(id)
	return self.keyed and self:target() == id
end

-- Where the keyboard is: "page", or the name of the field or editor that has it.
function Ctx:keyboard()
	return self.holder
end

-- What a key goes to: the field or editor that has the keyboard, else the element it is on.
function Ctx:target()
	if self.holder ~= "page" then
		return self.holder
	end
	return self.pointer.focus
end

-- The name of the layer in front that holds the keyboard (a dialog, a palette), or nil (`u:modal()`
-- with no name is this; with one it draws a modal, `layers.luau`).
function Ctx:front_modal()
	return self.top
end

-- The element `id` is a field or an editor: the keyboard goes into it as text, and a reply sends it
-- there (`keyboard = { field = id }`). A component says so of its fields as it draws them; a page
-- says it of its own that the context moves the keyboard to.
function Ctx:field(id)
	self.fields[id] = true
	return id
end

-- The text the page wants in the field `name`, which the context sees to: where the host is not known
-- to hold it, the reply that fills it is asked of the next wake (`select`: what is selected in it, as
-- a reply's `fill` says it). A field is empty when it is made, and a person's typing is told back
-- (`text`), so the page that keeps the text in a table need only say it every render.
function Ctx:field_text(name, wanted, select)
	if wanted == nil or self.held[name] == wanted then
		return
	end
	if self.held[name] == nil and wanted == "" then
		self.held[name] = wanted
		return
	end
	self.held[name] = wanted
	pencere.time.after(0, function()
		if self.held[name] ~= wanted then
			return nil
		end
		return { fill = { [name] = { text = wanted, select = select } } }
	end)
end

-- `element` says `words` while the pointer rests on it, with the keys that do the same as the
-- platform writes them (`@pencere/keys`). Nothing, for no words.
function Ctx:tip(element, words, shortcut)
	if words ~= nil then
		element:tip(words, shortcut and keys.label(shortcut) or nil)
	end
	return element
end

-- A command a component of the page asks for while it is drawn, written on the page's outermost box
-- (the box `finish` is given) so that it is done whatever has the keyboard: the keys of a list of
-- completions, which the editor under it would take. `id` is a part's name (`u:part`), and the
-- command comes back to the component as `command`.
function Ctx:root_command(id, title, shortcut)
	table.insert(self.queued, { id = id, title = title, keys = shortcut })
end

-- The render wants a frame at once (something moves), or one at the time `at` at the latest.
function Ctx:animate()
	self.animating = true
end

function Ctx:wake_at(at)
	if self.wake == nil or at < self.wake then
		self.wake = at
	end
end

-- The layer `id` (a dialog, a palette) is drawn in this render, in front of those drawn before it:
-- while it is in front, the keyboard steps among what is inside it alone, and escape closes it.
-- When it opens the keyboard goes into it (to `options.initial`, else the first thing in it that
-- takes it), and when it closes back to where it was. A layer that is not `modal` (a popover) leaves
-- the keyboard where it is and the page open to it; escape still closes it.
function Ctx:layer(id, options)
	options = options or {}
	table.insert(self.layers, { id = id, initial = options.initial, modal = options.modal ~= false })
end

-- --- Moving the keyboard ---------------------------------------------------------------------------

-- How many frames what the keyboard is sent to (a layer that opened, a bar the page just made) is
-- waited for to be laid out, before the keyboard is left where it is.
local ENTER_TRIES = 4


-- Puts the keyboard on `id`: into it, where it is a field or an editor (`u:field`), else on it as
-- the page's. The reply that asks the host for it.
function Ctx:focus(id, tries)
	if id ~= nil and not self.fields[id] and self.fields[id .. "/field"] then
		-- A component that is a field (an input) is named by its own name.
		id = id .. "/field"
	end
	if id == nil then
		self.pointer.focus = nil
		self.holder = "page"
		return { redraw = true, keyboard = "page" }
	end
	if not self.fields[id] and layout.bounds(id) == nil and (tries or 0) < ENTER_TRIES then
		-- Nothing of that name is laid out yet (a reply that opens a find bar and sends the keyboard
		-- into it): the keyboard goes there at the next frame, when the page has it.
		pencere.time.after(0, function()
			return self:focus(id, (tries or 0) + 1)
		end)
		return true
	end
	self.pointer:focus_on(id)
	if self.fields[id] then
		self.holder = id
		return { redraw = true, keyboard = { field = id } }
	end
	self.holder = "page"
	return { redraw = true, keyboard = "page" }
end

-- Whether `id` is inside the layer `layer`, as the page was last laid out.
local function inside(id, layer)
	for _, around_id in layout.path_of(id) do
		if around_id == layer then
			return true
		end
	end
	return false
end

-- What takes the keyboard, in the order of the page: inside the layer in front where there is one.
function Ctx:_order()
	local order = layout.focus_order()
	local top = self.top
	if top == nil then
		return order
	end
	local held = {}
	for _, id in order do
		if inside(id, top) then
			table.insert(held, id)
		end
	end
	return held
end

-- Moves the keyboard to the next that takes it (the one before, `back`), round from the last to
-- the first. The reply, or nil where nothing takes it.
function Ctx:_step(back)
	local order = self:_order()
	local count = #order
	if count == 0 then
		return nil
	end
	local now = self:target()
	local at = nil
	for index, id in order do
		if id == now then
			at = index
			break
		end
	end
	local next_at
	if at == nil then
		next_at = back and count or 1
	elseif back then
		next_at = (at - 2) % count + 1
	else
		next_at = at % count + 1
	end
	return self:focus(order[next_at])
end


-- A layer opened: the keyboard into it. The layout it is looked for in may still be the one before
-- it was drawn (a wake can come before the frame that drew it is laid out): then it is looked for
-- again at the next wake.
function Ctx:_enter(id, tries)
	local layer = self.open[id]
	if layer == nil then
		return nil
	end
	if layout.bounds(id) == nil then
		tries = (tries or 0) + 1
		if tries < ENTER_TRIES then
			pencere.time.after(0, function()
				return self:_enter(id, tries)
			end)
		end
		return nil
	end
	local initial = layer.initial
	if initial == nil then
		for _, candidate in self:_order() do
			if inside(candidate, id) then
				initial = candidate
				break
			end
		end
	end
	if initial == nil then
		return nil
	end
	return self:focus(initial)
end

-- A layer closed: the keyboard back where it was before it opened.
function Ctx:_leave(saved)
	if saved == nil then
		return nil
	end
	self.pointer.focus = saved.focus
	self.holder = saved.keyboard
	if saved.keyboard ~= "page" then
		return { redraw = true, keyboard = { field = saved.keyboard } }
	end
	return { redraw = true, keyboard = "page" }
end

-- --- The render -------------------------------------------------------------------------------------

-- A render begins: `size`, `now` and `reduce_motion` as the page was given them.
function Ctx:begin(size, now, reduce_motion)
	self.size, self.now, self.reduce_motion = size, now, reduce_motion and true or false
	self.mounts, self.parts, self.fields, self.layers, self.stack = {}, {}, {}, {}, {}
	self.commands = {}
	self.queued = {}
	self.animating, self.wake = false, nil
	for _, kept in self.memory do
		kept.seen = false
	end
	self.hover_path = around(self.pointer.hover)
	self.focus_path = around(self:target())
	self.building = true
end

-- The render made `root`: what a component no longer drawn kept is forgotten, and a layer that came
-- or went moves the keyboard. `root`, whether the page wants the next frame at once, and the time
-- of one it wants later: what `render` returns.
function Ctx:finish(root)
	self.building = false
	for _, command in self.queued do
		root:command(command.id, command.title, command.keys, true)
	end
	for id, kept in self.memory do
		if not kept.seen then
			self.memory[id] = nil
		end
	end
	for name in self.held do
		if not self.fields[name] then
			self.held[name] = nil
		end
	end
	local opened = {}
	local last_modal = nil
	for _, layer in self.layers do
		if layer.modal then
			opened[layer.id] = layer
			last_modal = layer
		end
	end
	-- A render has no reply: the keyboard is moved by the kit's timer, at the next wake (the
	-- frame after this one), with what it returns.
	for id in opened do
		if self.open[id] == nil then
			self.saved[id] = { focus = self.pointer.focus, keyboard = self.holder }
			pencere.time.after(0, function()
				return self:_enter(id)
			end)
		end
	end
	for id in self.open do
		if opened[id] == nil then
			local saved = self.saved[id]
			self.saved[id] = nil
			pencere.time.after(0, function()
				return self:_leave(saved)
			end)
		end
	end
	self.open = opened
	self.top = last_modal and last_modal.id or nil
	local last = self.layers[#self.layers]
	self.front = last and last.id or nil
	return root, self.animating, self.wake
end

-- The page was laid out: what a component brings back within the layout (a tab scrolled into
-- view). Whether something changed, and the page is to be drawn again.
function Ctx:settle()
	local again = false
	for _, mount in self.mounts do
		if mount.spec.settle and mount.spec.settle(self, mount.id, mount.props, mount.state) then
			again = true
		end
	end
	return again
end

-- --- Events ----------------------------------------------------------------------------------------

-- What one event came to: the reply, the actions for the page, whether a component took it.
local function outcome()
	return { reply = nil, actions = {}, taken = false }
end

local function call(u, mount, handler, ...)
	local fn = mount.spec[handler]
	if fn == nil then
		return nil
	end
	return fn(u, mount.id, mount.props, mount.state, ...)
end

-- `action` came of `mount`: each component around it may take it and say another of its own
-- (`action`); what none takes is the page's.
function Ctx:_emit(out, mount, action)
	if action == nil then
		return
	end
	if type(action) ~= "table" or type(action.kind) ~= "string" then
		error("an action of " .. mount.name .. " " .. mount.id .. " is a table with a kind", 0)
	end
	action.from = action.from or mount.id
	local at = mount
	while true do
		local parent = self:_parent(at)
		if parent == nil then
			break
		end
		local reply, instead = call(self, parent, "action", action)
		if reply ~= nil then
			out.reply = merge(out.reply, reply)
			if instead == nil then
				return
			end
			instead.from = instead.from or parent.id
			action = instead
		end
		at = parent
	end
	table.insert(out.actions, action)
end

-- Gives `mount` what `handler` is told, and what it does not take to the components around it.
-- Whether one took it.
function Ctx:_bubble(out, mount, handler, ...)
	local at = mount
	while at do
		local reply, action = call(self, at, handler, ...)
		if reply ~= nil then
			out.reply = merge(out.reply, reply)
			out.taken = true
			self:_emit(out, at, action)
			return true
		end
		at = self:_parent(at)
	end
	return false
end

-- Tells the owner of `name` (its own element or a part) what `handler` is told. Whether it took it.
function Ctx:_tell(out, name, handler, ...)
	local mount, part, key = self:_owner(name)
	if mount == nil then
		return false
	end
	local reply, action = call(self, mount, handler, part, key, ...)
	if reply == nil then
		return false
	end
	out.reply = merge(out.reply, reply)
	out.taken = true
	self:_emit(out, mount, action)
	return true
end

-- The pointer's shape over what it is over now, as the component that owns it says.
function Ctx:_cursor()
	local capture = self.capture
	if capture then
		local mount = self.mounts[capture.mount]
		return mount and call(self, mount, "cursor", capture.part, capture.key) or nil
	end
	local mount, part, key = self:_owner(self.pointer.hover)
	if mount then
		return call(self, mount, "cursor", part, key)
	end
	return nil
end

function Ctx:_pointer_shape(out)
	local shape = self:_cursor()
	if shape ~= self.cursor then
		self.cursor = shape
		out.reply = merge(out.reply, { cursor = shape or "arrow" })
	end
end

local EDITOR_EVENTS = {
	edit = true,
	selection = true,
	viewport = true,
	editor_refused = true,
	found = true,
	editor_ready = true,
}

-- Gives the event to the component it is for: the reply, the actions that came of it for the page,
-- and whether a component took it (an event no component took is the page's to do something with).
function Ctx:handle(event)
	local out = outcome()
	local kind = event.type
	if kind == "move" or kind == "leave" then
		local at = kind == "move" and { x = event.x, y = event.y } or nil
		if self.pointer:point(layout, at) then
			out.reply = merge(out.reply, true)
			local mount, part, key = self:_owner(self.pointer.hover)
			if mount then
				local reply, action = call(self, mount, "hover", part, key)
				if reply ~= nil then
					out.reply = merge(out.reply, reply)
					self:_emit(out, mount, action)
				end
			end
		end
		local capture = self.capture
		if capture and at then
			local mount = self.mounts[capture.mount]
			if mount then
				local reply, action = call(self, mount, "drag", capture.part, capture.key, {
					x = at.x,
					y = at.y,
					dx = at.x - capture.from.x,
					dy = at.y - capture.from.y,
				})
				out.reply = merge(out.reply, reply)
				out.taken = true
				self:_emit(out, mount, action)
			end
		end
		self:_pointer_shape(out)
	elseif kind == "press" then
		self.keyed = false
		if event.place then
			self.holder = event.place.editor
		elseif event.button == "left" and self.holder ~= "page" then
			-- A press outside every editor and field takes the keyboard back to the page.
			self.holder = "page"
		end
		local at = { x = event.x, y = event.y }
		local hit = self.pointer:press(layout, at)
		out.reply = merge(out.reply, true)
		-- A component that a press outside it closes (a popover) is told of it.
		for _, away in self.mounts do
			if away.spec.away and not (hit ~= nil and inside(hit, away.id)) then
				local reply, action = call(self, away, "away", event, hit)
				if reply ~= nil then
					out.reply = merge(out.reply, reply)
					self:_emit(out, away, action)
				end
			end
		end
		local mount, part, key = self:_owner(hit)
		if mount then
			local reply, action, grip = call(self, mount, "press", part, key, event)
			if reply ~= nil then
				out.reply = merge(out.reply, reply)
				out.taken = true
				self:_emit(out, mount, action)
				if event.button == "left" and grip and mount.spec.drag then
					self.capture = { mount = mount.id, part = part, key = key, from = at }
				end
			end
		end
		self:_pointer_shape(out)
	elseif kind == "release" then
		local at = { x = event.x, y = event.y }
		local capture = self.capture
		self.capture = nil
		if capture then
			local mount = self.mounts[capture.mount]
			if mount then
				local reply, action = call(self, mount, "release", capture.part, capture.key, {
					x = at.x,
					y = at.y,
					dx = at.x - capture.from.x,
					dy = at.y - capture.from.y,
				})
				out.reply = merge(out.reply, reply)
				out.taken = true
				self:_emit(out, mount, action)
			end
		end
		local clicked = self.pointer:release(layout, at)
		out.reply = merge(out.reply, true)
		local dropped = self.pointer:take_drop()
		if dropped then
			local mount = self:_owner(dropped.source)
			if mount then
				local reply, action = call(self, mount, "drop", dropped)
				if reply ~= nil then
					out.reply = merge(out.reply, reply)
					out.taken = true
					self:_emit(out, mount, action)
				end
			end
		elseif clicked ~= nil and event.button == "left" and not capture then
			self:_tell(out, clicked, "click", event)
		end
		self:_pointer_shape(out)
	elseif kind == "key" then
		self.keyed = true
		if event.key == "escape" and self.pointer:cancel_drag() then
			out.taken = true
			out.reply = merge(out.reply, true)
			return out.reply, out.actions, out.taken
		end
		local mount, part, key = self:_owner(self:target())
		if mount == nil and self.top then
			mount = self.mounts[self.top]
		end
		local taken = false
		if mount then
			local reply, action = call(self, mount, "key", event, part, key)
			if reply ~= nil then
				out.reply = merge(out.reply, reply)
				out.taken = true
				self:_emit(out, mount, action)
				taken = true
			else
				local parent = self:_parent(mount)
				taken = parent ~= nil and self:_bubble(out, parent, "key", event)
			end
		end
		if not taken then
			local mods = event.mods or {}
			local plain = not (mods.command or mods.control or mods.alt)
			if event.key == "tab" and plain then
				local reply = self:_step(mods.shift)
				if reply then
					out.reply = merge(out.reply, reply)
					out.taken = true
				end
			elseif event.key == "escape" and self.front and self.mounts[self.front] then
				local front = self.mounts[self.front]
				out.taken = true
				out.reply = merge(out.reply, true)
				if front.spec.dismiss then
					local reply, action = call(self, front, "dismiss")
					out.reply = merge(out.reply, reply)
					self:_emit(out, front, action)
				else
					self:_emit(out, front, { kind = "dismiss" })
				end
			end
		end
	elseif kind == "text" then
		self.held[event.field] = event.text
		self:_tell(out, event.field, "text", event)
	elseif kind == "focus" then
		-- The person clicked into a field, or the keyboard went back to the page of itself.
		self.keyed = false
		if event.field ~= nil then
			self.holder = event.field
			self.pointer.focus = event.field
		else
			self.holder = "page"
		end
		out.reply = merge(out.reply, true)
	elseif kind == "command" then
		if not self:_tell(out, event.id, "command", event) and self.commands[event.id] then
			out.taken = true
			out.reply = merge(out.reply, true)
			table.insert(out.actions, { from = "commands", kind = "command", id = event.id })
		end
	elseif kind == "menu" then
		local asked = self.asked
		self.asked = nil
		local entry = asked and asked.keys[event.index]
		if entry ~= nil then
			out.taken = true
			out.reply = merge(out.reply, true)
			table.insert(out.actions, { from = asked.id, kind = "menu", entry = entry })
		end
	elseif kind == "scrolled" then
		for _, box in event.boxes do
			local mount, part, key = self:_owner(box.id)
			if mount then
				local reply, action = call(self, mount, "scrolled", part, key, box)
				out.reply = merge(out.reply, reply)
				self:_emit(out, mount, action)
			end
		end
	elseif EDITOR_EVENTS[kind] then
		for _, mount in self.mounts do
			if mount.props.editor == event.editor and mount.spec.editor then
				local reply, action = call(self, mount, "editor", event)
				if reply ~= nil then
					out.reply = merge(out.reply, reply)
					out.taken = true
					self:_emit(out, mount, action)
				end
			end
		end
	end
	return out.reply, out.actions, out.taken
end

return core
