-- @pencere/ui, the commands of a page, written once: what the host lists where a person looks for what
-- can be done, what the palette offers and what the help says, from one list.
--
--   local cmds = ui.commands({
--       { id = "save", title = "Save", group = "File", keys = "mod+s", when = function() return doc.dirty end },
--       { id = "palette", title = "Command Palette", keys = "mod+shift+p", global = true },
--   })
--   -- in render, after the page is drawn:   cmds:write(u, root)
--   -- in handle:                            act.from == "commands" and act.kind == "command" → act.id
--   u:palette("palette", { items = cmds:items(), ... })
--
-- A command is a thing the page can be told to do (`command(id, title, keys, enabled)`, Section 2.1):
-- the host does it by its keys wherever the keyboard is, and says `{ kind = "command", id }` to the
-- page. While a modal (a dialog, a palette) is in front, a command that is not `global` is written
-- not `enabled`, so that its keys are keys of the page and no other: the person typing in a dialog
-- does not save the file under it. `when` (a function) says whether it can be done now.

local keys = require("@pencere/keys")

local commands = {}

local Commands = {}
Commands.__index = Commands

local function check(list)
	if type(list) ~= "table" then
		error("ui.commands takes a list of { id =, title =, keys = }", 3)
	end
	local seen = {}
	for _, command in list do
		if type(command.id) ~= "string" or type(command.title) ~= "string" then
			error("a command has an id and a title: { id = \"save\", title = \"Save\", keys = \"mod+s\" }", 3)
		end
		if seen[command.id] then
			error("two commands are named " .. command.id, 3)
		end
		seen[command.id] = true
		local given = command.keys
		if type(given) == "string" then
			given = { given }
		end
		for _, words in given or {} do
			local _, why = keys.parse(words)
			if why then
				error("the command " .. command.id .. "'s keys: " .. why, 3)
			end
		end
	end
end

function commands.new(list)
	check(list)
	local by_id = {}
	for _, command in list do
		by_id[command.id] = command
	end
	return setmetatable({ list = list, by_id = by_id }, Commands)
end

-- The command named `id`, or nil.
function Commands:get(id)
	return self.by_id[id]
end

-- Whether a modal is in front of the page, as far as it is known as the page is drawn.
local function modal_open(u)
	if u.top ~= nil then
		return true
	end
	for _, layer in u.layers do
		if layer.modal then
			return true
		end
	end
	return false
end

-- Whether the command can be done now.
function Commands:enabled(u, command)
	if command.when ~= nil and not command.when() then
		return false
	end
	return command.global == true or not modal_open(u)
end

-- Writes the commands on `root`, the page's outermost box, so that the host lists them and does them
-- by their keys whatever has the keyboard; the context is told which there are, to give the page what
-- the host does as an action. After the page is drawn. `root`.
function Commands:write(u, root)
	for _, command in self.list do
		u.commands[command.id] = command
		root:command(command.id, command.title, command.keys, self:enabled(u, command))
	end
	return root
end

-- The commands as the items of a palette: `{ key = id, title, hint = group, keys }`, those that can
-- be done now.
function Commands:items(options)
	options = options or {}
	local items = {}
	for _, command in self.list do
		if command.when == nil or command.when() then
			table.insert(items, { key = command.id, title = command.title, hint = command.group, keys = command.keys })
		end
	end
	return items
end

-- The commands in groups, for a table of the keys: `{ { title = group, items = { { id, title, keys } } } }`,
-- in the order each group first appears.
function Commands:help()
	local groups, at = {}, {}
	for _, command in self.list do
		local name = command.group or ""
		local group = at[name]
		if group == nil then
			group = { title = name, items = {} }
			at[name] = group
			table.insert(groups, group)
		end
		table.insert(group.items, { id = command.id, title = command.title, keys = command.keys })
	end
	return groups
end

return commands
