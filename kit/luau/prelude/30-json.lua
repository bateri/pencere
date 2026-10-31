-- The page API, part 6: JSON, the text the page and its program write to each other in.
--
-- `json.encode(value)` writes a value: a table with the keys 1..n is a list, any other is an
-- object (keys in order, so that the same value is always the same text); an empty table is an
-- object, unless it is `setmetatable({}, json.array)`. `json.decode(text)` reads one and gives
-- the value, or nil and why: objects and lists are tables (an empty list one marked
-- `json.array`, so that it is written again as a list), `null` is `json.null` in a list and an
-- absent key in an object. The text came from a program the person may not know: it is read
-- with a limit on how deep it goes, and nothing in it is run.
--
-- A member that is `null` holds nothing, as one that is not there does, so that `if v.git then`
-- reads both alike; where the two mean different things (`git: null`, the repository is gone;
-- no `git`, nothing changed), `json.has(object, key)` says whether the object read had the key,
-- `null` too, and `json.is_null(object, key)` whether it had it as `null`. They hold for every
-- object read, those of the program's messages too, while the page has set nothing at that key;
-- such an object is written again with its `null`s.
--
-- Reading is the bridge's (`kit/luau/bridge/json.cpp`), whose result is the one described above;
-- this part keeps what the two share: `json.null`, `json.array` and the record of the `null`s.

local internal = ...

local json = {}
pencere.json = json
local refuse = internal.refuse

json.null = setmetatable({}, { __tostring = function() return "null" end })
json.array = {}

-- The keys each object read had as `null`, by the object: weak, so that an object the page lets go
-- of is forgotten with them, and nothing is put in the object itself.
local nulls = setmetatable({}, { __mode = "k" })

-- Whether `object` has `key` as `null`, as it was read: a key the page has since set holds what it
-- set.
function json.is_null(object, key)
	if type(object) ~= "table" or rawget(object, key) ~= nil then
		return false
	end
	local held = nulls[object]
	return held ~= nil and held[key] == true
end

-- Whether `object` has `key`: holding something, or `null` as it was read.
function json.has(object, key)
	return type(object) == "table" and (rawget(object, key) ~= nil or json.is_null(object, key))
end

-- Arrays and objects nested this many levels or more, the outermost counting as the first, are
-- no JSON message (specification section 4.5): neither read nor written. The reader is the
-- bridge's (`kMaxDepth` in `bridge/json.cpp`), which holds to the same number: keep the two
-- equal.
local MAX_DEPTH = 128

local escapes = {
	['"'] = '\\"',
	["\\"] = "\\\\",
	["\b"] = "\\b",
	["\f"] = "\\f",
	["\n"] = "\\n",
	["\r"] = "\\r",
	["\t"] = "\\t",
}

local function escape(character)
	return escapes[character] or string.format("\\u%04x", string.byte(character))
end

local function is_list(value)
	if getmetatable(value) == json.array then
		return true
	end
	local count = #value
	if count == 0 then
		return false
	end
	for key in pairs(value) do
		if type(key) ~= "number" or key < 1 or key > count or key ~= math.floor(key) then
			return false
		end
	end
	return true
end

local function write(value, out, depth)
	local kind = type(value)
	if kind == "string" then
		out[#out + 1] = '"' .. string.gsub(value, '[%c"\\]', escape) .. '"'
	elseif kind == "number" then
		if value ~= value or value == math.huge or value == -math.huge then
			out[#out + 1] = "null"
		elseif value == math.floor(value) and math.abs(value) < 2 ^ 53 then
			out[#out + 1] = string.format("%d", value)
		else
			out[#out + 1] = string.format("%.17g", value)
		end
	elseif kind == "boolean" then
		out[#out + 1] = value and "true" or "false"
	elseif value == nil or value == json.null then
		out[#out + 1] = "null"
	elseif kind == "table" then
		if depth >= MAX_DEPTH then
			refuse("json.encode: the value is nested too deep")
		end
		if is_list(value) then
			out[#out + 1] = "["
			for index = 1, #value do
				if index > 1 then
					out[#out + 1] = ","
				end
				write(value[index], out, depth + 1)
			end
			out[#out + 1] = "]"
		else
			local keys = {}
			for key in pairs(value) do
				keys[#keys + 1] = tostring(key)
			end
			-- The members read as `null`, which hold nothing, are written as they were read.
			local held = nulls[value]
			if held then
				for key in pairs(held) do
					if rawget(value, key) == nil then
						keys[#keys + 1] = key
					end
				end
			end
			table.sort(keys)
			out[#out + 1] = "{"
			local first = true
			for _, key in ipairs(keys) do
				local member = value[key]
				if member == nil then
					member = value[tonumber(key)]
				end
				if member == nil and held and held[key] then
					member = json.null
				end
				if member ~= nil then
					if not first then
						out[#out + 1] = ","
					end
					first = false
					out[#out + 1] = '"' .. string.gsub(key, '[%c"\\]', escape) .. '":'
					write(member, out, depth + 1)
				end
			end
			out[#out + 1] = "}"
		end
	else
		refuse("json.encode: a " .. kind .. " cannot be written")
	end
end

function json.encode(value)
	local out = {}
	write(value, out, 1)
	return table.concat(out)
end

-- --- Reading -------------------------------------------------------------------------------------

local read = internal.host.json_read

-- The value `text` says, or nil and why (`... at 12`, the place counted from 1); nil alone for a
-- text that is `null`. The bridge makes the tables, with `json.null`, `json.array` and the record of
-- the members that were `null`.
function json.decode(text)
	local kind = type(text)
	if kind ~= "string" and kind ~= "number" then
		refuse("json.decode takes the text to read, not a " .. kind)
	end
	return read(text, json.null, json.array, nulls)
end
