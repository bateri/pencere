-- @pencere/languages/sql: SQL, one reader for three descriptions: "PostgreSQL" and "SQLite", whose
-- forms it tells apart where the two read the same characters differently, and "SQL", which reads
-- the forms of both, for a file that does not say its database. `@pencere/languages` requires it
-- when one of them is first loaded.
--
-- Where they differ:
--   `[`       a subscript or an array in PostgreSQL (`a[1]`, `ARRAY[1, 2]`), a quoted name in SQLite
--             (`[order]`); SQL reads it as a subscript right after a name or a bracket, else a name
--   `?`       an operator of `jsonb` in PostgreSQL (`?`, `?|`, `?&`), a parameter in SQLite (`?`,
--             `?1`); SQL reads `?|` and `?&` as operators and the rest as parameters
--   `@name`, `$name`   parameters of SQLite (`@` is an operator of PostgreSQL's)
--   `$tag$…$tag$`, `E'…'`, `::`, nested block comments   PostgreSQL's (and SQL's, but its block
--             comments do not nest)
--   `` `name` ``   a quoted name of SQLite's
-- Both: `--` and `/* */` comments, strings in `'` over lines (`''` a quote in one), `X'…'` and
-- `B'…'`, quoted names in `"` (`""` a quote in one), `$1` and `:name` parameters. `#` is no comment
-- (it is an operator of PostgreSQL's).

local common = require("./common")

local set = common.set

local KEYWORDS = set(
	"abort add after all alter always analyze and any array as asc attach autoincrement before begin between "
		.. "by cascade case cast check collate column commit concurrently conflict constraint create cross "
		.. "current_date current_time current_timestamp database deallocate declare default deferrable deferred "
		.. "delete desc detach distinct do drop each else end escape except exclusive execute exists explain "
		.. "extension fetch filter first following for foreign from full function generated glob grant group "
		.. "having if ignore ilike immediate in index indexed initially inner insert instead intersect into is "
		.. "isnull join key language last lateral left like limit local materialized natural no not nothing "
		.. "notnull nulls of offset on only or order outer over partition pragma preceding primary procedure "
		.. "range recursive references regexp reindex release rename replace restrict return returning returns "
		.. "revoke right rollback row rows savepoint schema select sequence set similar some strict table temp "
		.. "temporary then to transaction trigger truncate unbounded union unique update using vacuum values "
		.. "view virtual when where window with within without"
)
local TYPES = set(
	"bigint bigserial bit blob bool boolean box bpchar bytea char character cidr circle date datetime decimal "
		.. "double float float4 float8 inet int int2 int4 int8 integer interval json jsonb line lseg macaddr money "
		.. "numeric oid path point polygon precision real regclass serial serial2 serial4 serial8 smallint "
		.. "smallserial text time timestamp timestamptz timetz tinyint tsquery tsvector uuid varbit varchar "
		.. "varying xml"
)
local ATOMS = set("true false null")

-- What each description reads (see the head of this module).
local DIALECTS = {
	SQL = {
		dollar_quotes = true,
		escape_strings = true,
		casts = true,
		nested = false,
		brackets = "guess",
		backticks = true,
		question = "guess",
		named = true,
	},
	PostgreSQL = {
		dollar_quotes = true,
		escape_strings = true,
		casts = true,
		nested = true,
		brackets = "subscript",
		backticks = false,
		question = "operator",
		named = false,
	},
	SQLite = {
		dollar_quotes = false,
		escape_strings = false,
		casts = false,
		nested = false,
		brackets = "name",
		backticks = true,
		question = "parameter",
		named = true,
	},
}

local IDENT = "^[%a_\128-\255][%w_%$\128-\255]*"
local OPERATOR = "^[%+%-%*/<>=~!@#%%%^&|%?]+"

local function reader(d)
	-- The state: `block` how deep a block comment is, `quote` a string that goes on ("'" or "e" for
	-- one of `E'…'`, whose `\` takes the character after it), `dollar` the tag of a `$tag$` string
	-- that goes on; and, to the line's end, `cast` (a `::` was read: the word after it is a type).
	local function start()
		return {}
	end

	-- A block comment from where the stream is, to its end or the end of the line.
	local function block_comment(stream, state)
		while not stream:eol() do
			if stream:match("%*/") then
				state.block -= 1
				if state.block == 0 then
					state.block = nil
					return "comment"
				end
			elseif d.nested and stream:match("/%*") then
				state.block += 1
			else
				-- On to the next `*` or `/`, where one may close or open.
				stream:next()
				stream.pos = string.find(stream.string, "[%*/]", stream.pos) or #stream.string + 1
			end
		end
		return "comment"
	end

	-- A string in `'` from where the stream is (past its quote): `''` is a quote in it, and `\` takes
	-- the character after it where `escapes`.
	local function string_body(stream, state, escapes)
		while not stream:eol() do
			local c = stream:next()
			if c == "\\" and escapes then
				stream:next()
			elseif c == "'" then
				if not stream:eat("'") then
					state.quote = nil
					return "string"
				end
			end
		end
		state.quote = escapes and "e" or "'"
		return "string"
	end

	local function dollar_body(stream, state)
		local close = "$" .. state.dollar .. "$"
		if stream:skip_to(close) then
			stream.pos += #close
			state.dollar = nil
		else
			stream:skip_to_end()
		end
		return "string"
	end

	-- A quoted name from where the stream is (past its quote) to `close`, which is a `close` in it
	-- where it is twice.
	local function quoted_name(stream, close)
		while not stream:eol() do
			if stream:next() == close and not stream:eat(close) then
				break
			end
		end
		return "property"
	end

	local function read(stream, state)
		if state.block then
			return block_comment(stream, state)
		elseif state.quote then
			return string_body(stream, state, state.quote == "e")
		elseif state.dollar then
			return dollar_body(stream, state)
		end
		if stream:eat_space() then
			return nil
		end
		local cast = state.cast
		state.cast = nil
		if stream:match("%-%-") then
			stream:skip_to_end()
			return "comment"
		end
		if stream:match("/%*") then
			state.block = 1
			return block_comment(stream, state)
		end
		-- Strings, with what may be before their quote: `E'…'` (escapes), `X'…'`, `B'…'`, `N'…'`.
		local prefix = stream:match("[eEbBxXnN]'")
		if prefix then
			local escapes = d.escape_strings and (prefix == "e'" or prefix == "E'")
			return string_body(stream, state, escapes)
		end
		if stream:match("[uU]&'") or stream:eat("'") then
			return string_body(stream, state, false)
		end
		if d.dollar_quotes then
			local tag = stream:match("%$([%a_][%w_]*)%$") or (stream:match("%$%$") and "")
			if tag then
				state.dollar = tag
				return dollar_body(stream, state)
			end
		end
		-- Quoted names.
		if stream:match('[uU]&"') or stream:eat('"') then
			return quoted_name(stream, '"')
		end
		if d.backticks and stream:eat("`") then
			return quoted_name(stream, "`")
		end
		if stream:peek() == "[" then
			local brackets = d.brackets
			if brackets == "guess" then
				local before = stream.pos > 1 and string.sub(stream.string, stream.pos - 1, stream.pos - 1) or ""
				brackets = string.find(before, "[%w_%]%)\128-\255]") and "subscript" or "name"
			end
			if brackets == "name" then
				stream:next()
				stream:skip_to("]")
				stream:eat("]")
				return "property"
			end
			stream:next()
			return "punctuation"
		end
		-- Parameters.
		if stream:match("%$%d+") then
			return "variable.special"
		end
		if stream:match("::") then
			if d.casts then
				state.cast = true
			end
			return "operator"
		end
		if stream:match(":[%a_][%w_]*") then
			return "variable.special"
		end
		if d.named and (stream:match("@[%a_][%w_]*") or stream:match("%$[%a_][%w_]*")) then
			return "variable.special"
		end
		if stream:peek() == "?" then
			local question = d.question
			if question == "guess" then
				question = stream:match("%?[|&]", false) and "operator" or "parameter"
			end
			if question == "parameter" then
				stream:match("%?%d*")
				return "variable.special"
			end
		end
		-- Numbers.
		if
			stream:match("0[xX][%x_]+")
			or stream:match("%d[%d_]*%.?[%d_]*[eE][%+%-]?%d+")
			or stream:match("%d[%d_]*%.?[%d_]*")
			or stream:match("%.%d[%d_]*")
		then
			return "number"
		end
		local word = stream:match(IDENT)
		if word then
			local lower = string.lower(word)
			if cast and not KEYWORDS[lower] then
				return "type"
			elseif ATOMS[lower] then
				return "atom"
			elseif TYPES[lower] then
				return "type"
			elseif KEYWORDS[lower] then
				return "keyword"
			elseif stream:match("%s*%(", false) then
				return "function"
			end
			return nil
		end
		if stream:match("[%(%)%],;%.]") then
			return "punctuation"
		end
		local operator = stream:match(OPERATOR)
		if operator then
			-- A comment after an operator with no space between is a comment still.
			local comment = string.find(operator, "%-%-", 2) or string.find(operator, "/%*", 2)
			if comment then
				stream.pos = stream.start + comment - 1
			end
			return "operator"
		end
		stream:next()
		return nil
	end

	local function token(stream, state)
		local style = read(stream, state)
		if stream:eol() then
			state.cast = nil
		end
		return style
	end

	return { start = start, token = token }
end

local sql = {}

-- The language named `name` of the family ("SQL", "PostgreSQL", "SQLite"): its reader, and what an
-- editor is told of it.
function sql.make(name)
	local dialect = DIALECTS[name]
	return {
		reader = reader(dialect),
		config = {
			comment = { line = "--", block = { "/*", "*/" } },
			indent = { increase = "\\(\\s*$", decrease = "^\\s*\\)" },
			pairs = dialect.backticks and "()[]''\"\"``" or "()[]''\"\"",
			folds = "indent",
		},
	}
end

return sql
