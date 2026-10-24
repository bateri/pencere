-- containers-lua: a made-up Docker host, as an app of one Lua file (src/program/) and a page
-- (src/view/page.lua, which `serve` shows because it is told no other).
--
--   containers-lua            the view, with containers that are made up
--   containers-lua --docker   the view, with the containers `docker` lists on this machine
--   containers-lua --text     the list as text, whatever the terminal is
--
-- The list is a call the page makes once; the readings are a topic the page listens to, which this
-- program feeds once a second (`time.every`) and, with --docker, from the output of `docker stats`
-- as it comes (`process.spawn`). In a terminal that shows no views the list is printed instead,
-- and ⌃C ends the app with the status a shell gives it.

local pencere = require("pencere")
local time, process, json = pencere.time, pencere.process, pencere.json

local docker, text = false, false
for _, argument in ipairs(pencere.args) do
	if argument == "--docker" then
		docker = true
	elseif argument == "--text" then
		text = true
	else
		print("containers-lua: no option " .. argument)
		pencere.exit(64)
	end
end

-- ---- the containers -----------------------------------------------------------------------------

-- Made up: a name, an image, the ports, what it uses when it runs.
local made_up = {
	{ id = "web", name = "web", image = "nginx:1.27", ports = "8080->80", state = "running", cpu = 3.0, memory = 48 },
	{ id = "api", name = "api", image = "ghcr.io/acme/api:2.4", ports = "3000->3000", state = "running", cpu = 11.0, memory = 212 },
	{ id = "db", name = "db", image = "postgres:16", ports = "5432", state = "running", cpu = 6.0, memory = 390 },
	{ id = "cache", name = "cache", image = "redis:7", ports = "6379", state = "running", cpu = 1.5, memory = 22 },
	{ id = "worker", name = "worker", image = "ghcr.io/acme/worker:2.4", ports = "", state = "exited", cpu = 0, memory = 0 },
	{ id = "mail", name = "mail", image = "mailhog/mailhog", ports = "8025", state = "exited", cpu = 0, memory = 0 },
}

-- What the page is told of a container.
local function described(container)
	return {
		id = container.id,
		name = container.name,
		image = container.image,
		ports = container.ports,
		state = container.state,
	}
end

-- The containers `docker ps -a` lists, as the same records.
local function from_docker()
	local result = process.run { "docker", "ps", "-a", "--format", "{{json .}}" }
	if not result.ok then
		error({ code = "docker", message = result.stderr ~= "" and result.stderr or "docker ps failed" })
	end
	local list = {}
	for line in string.gmatch(result.stdout, "[^\n]+") do
		local c = json.decode(line)
		list[#list + 1] = {
			id = c.ID,
			name = c.Names,
			image = c.Image,
			ports = c.Ports or "",
			state = c.State,
		}
	end
	return list
end

local function list_containers()
	if docker then
		return from_docker()
	end
	local list = {}
	for index, container in ipairs(made_up) do
		list[index] = described(container)
	end
	return list
end

local function find(id)
	for _, container in ipairs(made_up) do
		if container.id == id then
			return container
		end
	end
end

-- ---- the text ---------------------------------------------------------------------------------

local function show_text()
	local ok, list = pcall(list_containers)
	if not ok then
		print("containers-lua: " .. tostring(list.message or list))
		return
	end
	print(string.format("%-12s %-26s %-10s %s", "NAME", "IMAGE", "STATE", "PORTS"))
	for _, c in ipairs(list) do
		print(string.format("%-12s %-26s %-10s %s", c.name, c.image, c.state, c.ports))
	end
end

if text then
	show_text()
	pencere.exit(0)
end

-- ---- the view ---------------------------------------------------------------------------------

local function calls()
	return {
		-- The list, once.
		list = function()
			return { containers = list_containers() }
		end,

		-- Stops or starts a container, and says what became of it.
		toggle = function(params)
			local id = params and params.id
			if type(id) ~= "string" then
				error({ code = "bad-params", message = "which container?" })
			end
			if docker then
				local container
				for _, c in ipairs(from_docker()) do
					if c.id == id or c.name == id then
						container = c
					end
				end
				if not container then
					error({ code = "not-found", message = "there is no container " .. id })
				end
				local verb = container.state == "running" and "stop" or "start"
				local done = process.run { "docker", verb, container.id }
				if not done.ok then
					error({ code = "docker", message = done.stderr })
				end
				return { id = id, state = verb == "stop" and "exited" or "running" }
			end
			local container = find(id)
			if not container then
				error({ code = "not-found", message = "there is no container " .. id })
			end
			-- A stop takes a moment; the view does not wait for it to ask for anything else.
			time.sleep(0.4)
			if container.state == "running" then
				container.state, container.cpu, container.memory = "exited", 0, 0
			else
				container.state, container.cpu, container.memory = "running", 2.0, 30
			end
			return { id = id, state = container.state }
		end,
	}
end

-- A reading for each container that runs: how much of the processor it uses, and its memory in
-- MiB. Made up, they wander around what each container uses, the way real ones do.
local function made_up_readings()
	local readings = {}
	local now = time.monotonic()
	for index, container in ipairs(made_up) do
		if container.state == "running" then
			local wave = math.sin(now * 0.7 + index * 1.9) * 0.5 + math.sin(now * 2.3 + index) * 0.5
			readings[#readings + 1] = {
				name = container.name,
				cpu = math.max(0.1, container.cpu * (1 + wave * 0.5)),
				memory = container.memory * (1 + wave * 0.03),
			}
		end
	end
	return readings
end

local function topics()
	return {
		stats = function(_, subscription)
			if docker then
				-- `docker stats` goes on until it is told to stop, a record a line.
				local child = process.spawn {
					argv = { "docker", "stats", "--format", "{{json .}}" },
					on_stdout = function(line)
						local record = string.match(line, "{.*}")
						if not record then
							return
						end
						local c = json.decode(record)
						subscription:push({
							readings = {
								{
									name = c.Name,
									cpu = tonumber((string.gsub(c.CPUPerc, "%%", ""))) or 0,
									memory = tonumber(string.match(c.MemUsage, "[%d%.]+")) or 0,
								},
							},
						})
					end,
				}
				subscription:on_end(function()
					child:kill()
				end)
				return
			end
			subscription:push({ readings = made_up_readings() })
			local timer = time.every(1, function()
				subscription:push({ readings = made_up_readings() })
			end)
			subscription:on_end(function()
				timer:cancel()
			end)
		end,
	}
end

local outcome = pencere.serve {
	title = "Containers",
	icon = "assets/icon.png",
	calls = calls(),
	topics = topics(),
	fallback = show_text,
}

-- Said on the screen the person was given back (a view that ended in an error said why already).
if outcome.shown then
	print("containers-lua: the view ended (" .. tostring(outcome.ended) .. ")")
end

-- The status a shell gives ⌃C, or 0 when the view was closed.
pencere.exit(outcome.status)
