-- @pencere/languages: the languages an editor reads, as CodeMirror's `@codemirror/language-data`
-- knows them: a list of descriptions (what a language is called and how a file in it is told),
-- always there and cheap, and a language built from one only when a page asks for it.
--
--   local languages = require("@pencere/languages")
--   local Syntax = require("@pencere/syntax")
--
--   local found = languages.detect("src/main.rs")      -- the description of Rust
--   local rust = languages.load(found)                  -- built now, and kept: languages.load("Rust")
--   local syntax = Syntax.new(rust.syntax, "editor")    -- colours the editor
--
--   render = function() return pencere.editor("editor", rust.config) end
--   handle = function(event) syntax:handle(event); return syntax:reply(nil) end
--
-- A description is { name =, aliases =, extensions =, filenames =, patterns =, interpreters =,
-- first_line =, family = }. A language is { name =, description =, syntax =, config = }: `syntax` is
-- a language of `@pencere/syntax` that names its tokens from `languages.styles`, and `config` is what
-- an editor is told of the language (`pencere.editor(id, config)`: its comments, the rules of its
-- indent, folding by the indent, the brackets and quotes that close themselves, and, where the
-- language has one way to indent, `hard_tabs` and `tab_width`).
--
-- The readers are in families (the languages of braces, the scripting ones, markup, data, style
-- sheets, the tools', SQL), each a module of its own required only when a language of it is first
-- loaded, so that a page that requires this module and loads one language pays for that one. A
-- language of markup that holds another (a page of HTML its scripts, Markdown the code in a fence)
-- loads the other through this module when it first meets it; one it does not know is plain text.
--
-- It knows no file that is plain text: `detect` says nil for a file in no language it knows, and what
-- an editor does with one is the page's to say. The looks are the theme's roles: a page's colours
-- follow the theme it wears.

local Syntax = require("@pencere/syntax")

local mix = pencere.mix

local languages = {}

-- --- Looks -------------------------------------------------------------------------------------

-- The looks of code, in the theme's roles: every language names its tokens from this one table (a
-- dotted name falls back to what is before its last dot, "string.special" to "string"), so that a
-- keyword is a keyword in any of them. Code needs about seven kinds told apart; each takes one of
-- the theme's colours as it is, chosen by the theme's author to read on its ground, and the few in
-- between are mixed from two.
local comment = mix("dim", "quiet", 0.3)
local punctuation = mix("foreground", "background", 0.38)
local special = mix("error", "warning", 0.4)

local styles = {
	keyword = { color = "accent" },
	["keyword.modifier"] = { color = "accent", italic = true },
	["variable.special"] = { color = "accent", italic = true },
	string = { color = "success" },
	["string.special"] = { color = special },
	escape = { color = special },
	regexp = { color = special },
	number = { color = "warning" },
	atom = { color = "warning" },
	constant = { color = "warning" },
	type = { color = "info" },
	["function"] = { color = mix("accent", "info", 0.6) },
	property = { color = mix("foreground", "info", 0.22) },
	variable = { color = "foreground" },
	comment = { color = comment, italic = true },
	punctuation = { color = punctuation },
	operator = { color = punctuation },
	meta = { color = punctuation },
	tag = { color = "error" },
	attribute = { color = "warning" },
	-- Prose and the like.
	heading = { color = "accent", weight = 700 },
	emphasis = { italic = true },
	strong = { weight = 700 },
	strike = { strike = true },
	link = { color = "info", underline = "info" },
	url = { color = "info" },
	quote = { color = comment, italic = true },
	code = { color = "success" },
	list = { color = "warning" },
	-- Diffs.
	inserted = { color = "success" },
	deleted = { color = "error" },
	changed = { color = "warning" },
	invalid = { color = "error", underline = { wavy = "error" } },
}
-- Frozen to the bottom: a look's underline and a mixed colour are tables of their own.
for _, look in styles do
	for _, value in look do
		-- A colour mixed from two is a table as well, and one may stand in more than one look.
		if type(value) == "table" and not table.isfrozen(value) then
			table.freeze(value)
		end
	end
	table.freeze(look)
end
languages.styles = table.freeze(styles)

-- The look `styles` gives a token named `name`, as an editor coloured by a language shows it: the
-- looks of its names (a name is several, apart: "list atom"), each found by itself or by what is
-- before its last dot, the later over the earlier. Nil for a name no style says: for a page that
-- colours code that is not in an editor (a block of code in a note) with `syntax:tokens`.
function languages.look(name)
	if type(name) ~= "string" then
		error("look takes the name of a token (a string), not a " .. type(name), 2)
	end
	local merged = nil
	for part in string.gmatch(name, "%S+") do
		local found = styles[part]
		while found == nil do
			part = string.match(part, "^(.*)%.[^%.]*$")
			if part == nil then
				break
			end
			found = styles[part]
		end
		if found then
			merged = merged or {}
			for key, value in found do
				merged[key] = value
			end
		end
	end
	return merged
end

-- --- Descriptions ------------------------------------------------------------------------------

-- What each language is called and how a file in it is told, by family:
--   name          what it is called
--   aliases       its other names (any case): a fence of Markdown says one, `find` takes one
--   extensions    what a file's name ends in after its last dot (any case)
--   filenames     whole names of files in it (any case): "Makefile", ".bashrc"
--   patterns      Lua patterns a file's whole name (in lower case) is in it by: "^dockerfile[%.%-]"
--   interpreters  the programs a script's first line (`#!`) runs it with, without their version
--   first_line    a Lua pattern the first line of a file in it matches: "^%s*<%?xml"
--   family        the family of readers it is built with (a module of its own)
local DESCRIPTIONS = {
	-- The languages of braces.
	{
		name = "JavaScript",
		aliases = { "js", "javascript", "node", "ecmascript", "mjs", "cjs" },
		extensions = { "js", "mjs", "cjs" },
		interpreters = { "node", "nodejs", "deno", "bun" },
		family = "clike",
	},
	{ name = "JSX", aliases = { "jsx" }, extensions = { "jsx" }, family = "clike" },
	{
		name = "TypeScript",
		aliases = { "ts", "typescript", "mts", "cts" },
		extensions = { "ts", "mts", "cts" },
		interpreters = { "ts-node", "tsx" },
		family = "clike",
	},
	{ name = "TSX", aliases = { "tsx" }, extensions = { "tsx" }, family = "clike" },
	{ name = "Rust", aliases = { "rs", "rust" }, extensions = { "rs" }, family = "clike" },
	{ name = "Go", aliases = { "go", "golang" }, extensions = { "go" }, family = "clike" },
	{ name = "C", aliases = { "c", "h" }, extensions = { "c", "h" }, family = "clike" },
	{
		name = "C++",
		aliases = { "cpp", "c++", "cxx", "cc", "hpp" },
		extensions = { "cc", "cpp", "cxx", "c++", "hpp", "hh", "hxx", "h++", "ino" },
		family = "clike",
	},
	{ name = "C#", aliases = { "cs", "csharp", "c#" }, extensions = { "cs", "csx" }, family = "clike" },
	{ name = "Java", aliases = { "java" }, extensions = { "java" }, family = "clike" },
	{ name = "Kotlin", aliases = { "kt", "kotlin", "kts" }, extensions = { "kt", "kts" }, family = "clike" },
	{ name = "Swift", aliases = { "swift" }, extensions = { "swift" }, family = "clike" },
	{
		name = "PHP",
		aliases = { "php" },
		extensions = { "php", "phtml" },
		interpreters = { "php" },
		first_line = "^<%?php",
		family = "clike",
	},
	-- The scripting languages.
	{
		name = "Python",
		aliases = { "py", "python", "python3", "py3" },
		extensions = { "py", "pyi", "pyw" },
		filenames = { "SConstruct", "SConscript" },
		interpreters = { "python", "pypy" },
		family = "scripting",
	},
	{
		name = "Ruby",
		aliases = { "rb", "ruby" },
		extensions = { "rb", "rake", "gemspec", "ru" },
		filenames = { "Gemfile", "Rakefile", "Guardfile", "Podfile", "Vagrantfile" },
		interpreters = { "ruby", "jruby" },
		family = "scripting",
	},
	{
		name = "Shell",
		aliases = { "sh", "bash", "zsh", "shell", "shellscript", "console" },
		extensions = { "sh", "bash", "zsh", "ksh", "fish", "command" },
		filenames = {
			".bashrc",
			".bash_profile",
			".bash_aliases",
			".bash_logout",
			".zshrc",
			".zprofile",
			".zshenv",
			".zlogin",
			".zlogout",
			".profile",
			".envrc",
			"PKGBUILD",
		},
		interpreters = { "sh", "bash", "zsh", "ksh", "dash", "ash", "mksh", "fish" },
		family = "scripting",
	},
	{
		name = "Lua",
		aliases = { "lua", "luau" },
		extensions = { "lua", "luau", "rockspec" },
		interpreters = { "lua", "luajit", "luau" },
		family = "scripting",
	},
	-- Markup.
	{
		name = "HTML",
		aliases = { "html", "htm", "xhtml" },
		extensions = { "html", "htm", "xhtml", "shtml", "vue", "svelte" },
		first_line = "^%s*<![dD][oO][cC][tT][yY][pP][eE]%s+[hH][tT][mM][lL]",
		family = "markup",
	},
	{
		name = "XML",
		aliases = { "xml", "svg", "xsd", "xsl", "plist" },
		extensions = {
			"xml",
			"svg",
			"xsd",
			"xsl",
			"xslt",
			"plist",
			"csproj",
			"fsproj",
			"vcxproj",
			"props",
			"targets",
			"xaml",
			"rss",
			"atom",
			"wsdl",
			"storyboard",
			"xib",
		},
		first_line = "^%s*<%?xml",
		family = "markup",
	},
	{
		name = "Markdown",
		aliases = { "md", "markdown" },
		extensions = { "md", "markdown", "mdx", "mkd", "mdown" },
		family = "markup",
	},
	-- Data.
	{
		name = "JSON",
		aliases = { "json", "jsonc", "json5", "jsonl" },
		extensions = { "json", "jsonc", "json5", "jsonl", "geojson", "webmanifest", "har" },
		filenames = { ".babelrc", ".eslintrc", ".prettierrc", ".jshintrc", "composer.lock", "flake.lock" },
		family = "data",
	},
	{
		name = "YAML",
		aliases = { "yaml", "yml" },
		extensions = { "yaml", "yml" },
		filenames = { ".clang-format", ".clangd" },
		family = "data",
	},
	{
		name = "TOML",
		aliases = { "toml" },
		extensions = { "toml" },
		filenames = { "Cargo.lock", "Pipfile", "poetry.lock" },
		family = "data",
	},
	{
		name = "INI",
		aliases = { "ini", "properties", "env", "dotenv", "conf", "cfg", "editorconfig", "gitconfig" },
		extensions = { "ini", "cfg", "conf", "properties", "env", "editorconfig", "desktop", "service", "npmrc" },
		filenames = {
			".editorconfig",
			".gitconfig",
			".gitmodules",
			".npmrc",
			".env",
			".flake8",
			"setup.cfg",
			"tox.ini",
			".pylintrc",
		},
		patterns = { "^%.env%." },
		family = "data",
	},
	-- Style sheets.
	{ name = "CSS", aliases = { "css" }, extensions = { "css", "pcss", "postcss" }, family = "sheets" },
	{ name = "SCSS", aliases = { "scss", "sass" }, extensions = { "scss", "sass" }, family = "sheets" },
	{ name = "Less", aliases = { "less" }, extensions = { "less" }, family = "sheets" },
	-- The tools'.
	{
		name = "Dockerfile",
		aliases = { "dockerfile", "docker", "containerfile" },
		extensions = { "dockerfile", "containerfile" },
		filenames = { "Dockerfile", "Containerfile" },
		patterns = { "^dockerfile[%.%-]", "^containerfile[%.%-]" },
		family = "tools",
	},
	{
		name = "Makefile",
		aliases = { "make", "makefile", "mk" },
		extensions = { "mk", "mak", "make" },
		filenames = { "Makefile", "GNUmakefile", "BSDmakefile" },
		interpreters = { "make" },
		family = "tools",
	},
	{
		name = "Diff",
		aliases = { "diff", "patch" },
		extensions = { "diff", "patch", "rej" },
		first_line = "^diff %-",
		family = "tools",
	},
	-- SQL: what most databases read (the forms of PostgreSQL's and SQLite's both), and the two whose
	-- forms are told apart where they read the same characters differently.
	{ name = "SQL", aliases = { "sql" }, extensions = { "sql", "ddl", "dml" }, family = "sql" },
	{
		name = "PostgreSQL",
		aliases = { "postgresql", "postgres", "pgsql", "psql", "plpgsql" },
		extensions = { "pgsql", "psql" },
		family = "sql",
	},
	{ name = "SQLite", aliases = { "sqlite", "sqlite3" }, family = "sql" },
}

-- The families, each required when a language of it is first loaded (a literal path each, so that
-- what reads Luau finds them).
local FAMILIES = {
	clike = function()
		return require("@self/clike")
	end,
	scripting = function()
		return require("@self/scripting")
	end,
	markup = function()
		return require("@self/markup")
	end,
	data = function()
		return require("@self/data")
	end,
	sheets = function()
		return require("@self/sheets")
	end,
	tools = function()
		return require("@self/tools")
	end,
	sql = function()
		return require("@self/sql")
	end,
}

-- The descriptions by what finds them, and every description (to tell one of this module's from
-- another table).
local by_name = {}
local by_extension = {}
local by_filename = {}
local by_interpreter = {}
local by_pattern = {}
local with_first_line = {}
local known = {}

local function frozen(list)
	return list and table.freeze(table.clone(list)) or table.freeze({})
end

for _, spec in DESCRIPTIONS do
	local description = table.freeze({
		name = spec.name,
		aliases = frozen(spec.aliases),
		extensions = frozen(spec.extensions),
		filenames = frozen(spec.filenames),
		patterns = frozen(spec.patterns),
		interpreters = frozen(spec.interpreters),
		first_line = spec.first_line,
		family = spec.family,
	})
	known[description] = true
	by_name[string.lower(description.name)] = description
	for _, alias in description.aliases do
		local key = string.lower(alias)
		by_name[key] = by_name[key] or description
	end
	for _, extension in description.extensions do
		local key = string.lower(extension)
		by_extension[key] = by_extension[key] or description
	end
	for _, filename in description.filenames do
		local key = string.lower(filename)
		by_filename[key] = by_filename[key] or description
	end
	for _, interpreter in description.interpreters do
		by_interpreter[interpreter] = by_interpreter[interpreter] or description
	end
	for _, pattern in description.patterns do
		by_pattern[#by_pattern + 1] = { pattern, description }
	end
	if description.first_line then
		with_first_line[#with_first_line + 1] = description
	end
end

-- Every description, by name (any case): a new list each time, for the page to keep or sort.
function languages.list()
	local list = {}
	for description in known do
		list[#list + 1] = description
	end
	table.sort(list, function(a, b)
		local x, y = string.lower(a.name), string.lower(b.name)
		if x ~= y then
			return x < y
		end
		return a.name < b.name
	end)
	return list
end

-- The description named `name`, or one of its aliases, in any case; nil for none.
function languages.find(name)
	if type(name) ~= "string" then
		error("find takes the name of a language (a string), not a " .. type(name), 2)
	end
	return by_name[string.lower(name)]
end

-- The program a first line of `#!` runs the file with: the name of the program (the one `env` is
-- given, past its options and settings), in lower case.
local function interpreter_of(line)
	local command = string.match(line, "^#!%s*(.*)$")
	if command == nil then
		return nil
	end
	local words = string.split(command, " ")
	local program = nil
	local through_env = false
	for _, word in words do
		if word ~= "" then
			local name = string.match(word, "([^/]+)$") or word
			if program == nil then
				program = name
				through_env = name == "env"
				if not through_env then
					break
				end
			elseif string.sub(word, 1, 1) ~= "-" and not string.find(word, "=", 1, true) then
				program = name
				break
			end
		end
	end
	if program == nil or (through_env and program == "env") then
		return nil
	end
	return string.lower(program)
end

-- The description a file's first line says: the program of a `#!` line (`python3.12` is
-- `python`'s), then the descriptions' own patterns.
local function by_first_line(line)
	local program = interpreter_of(line)
	if program then
		local found = by_interpreter[program] or by_interpreter[(string.gsub(program, "[%d%.]+$", ""))]
		if found then
			return found
		end
	end
	for _, description in with_first_line do
		if string.find(line, description.first_line) then
			return description
		end
	end
	return nil
end

-- The description of the file at `path` (its whole name is what is read: `"src/main.rs"`, or nil
-- for a document with no name), whose first line is `first_line` (or nil): by its whole name
-- (`filenames`), the descriptions' `patterns`, its extension, then its first line (the program a
-- `#!` line runs it with, `first_line`). Nil where none says it: plain text, as far as this module
-- knows, and what an editor does with it is the page's to say.
function languages.detect(path, first_line)
	if path ~= nil and type(path) ~= "string" then
		error("detect takes the path of a file (a string, or nil), not a " .. type(path), 2)
	end
	if first_line ~= nil and type(first_line) ~= "string" then
		error("detect takes the first line of the file (a string, or nil), not a " .. type(first_line), 2)
	end
	if path then
		local name = string.lower(string.match(path, "([^/\\]*)$") or path)
		local found = by_filename[name]
		if found then
			return found
		end
		for _, rule in by_pattern do
			if string.find(name, rule[1]) then
				return rule[2]
			end
		end
		local extension = string.match(name, "^.+%.([^%.]+)$")
		found = extension and by_extension[extension]
		if found then
			return found
		end
	end
	if first_line then
		return by_first_line(first_line)
	end
	return nil
end

-- --- Languages ---------------------------------------------------------------------------------

-- The languages built, by description, and their readers (`{ start, token, ... }`), by description.
local built = {}
local readers = {}

-- A copy of `value`, tables all the way down, frozen: what a page is given and must not change in
-- place, as every page's load of the language is the same table.
local function freeze_all(value)
	if type(value) ~= "table" then
		return value
	end
	local copy = {}
	for key, item in value do
		copy[key] = freeze_all(item)
	end
	return table.freeze(copy)
end

-- What a family is given to build a language with: the reader of another language, by any of its
-- names, loaded when first asked for (a page of HTML its scripts, Markdown a fence's code); nil for
-- one this module does not know.
local context = {}

-- The description `which` says: a name (or alias), or a description of this module's.
local function described(which, level)
	local kind = type(which)
	if kind == "string" then
		return by_name[string.lower(which)]
	elseif kind == "table" then
		if not known[which] then
			error("a language is said by its name or by its description (languages.find), not another table", level)
		end
		return which
	end
	error("a language is said by its name (a string) or its description, not a " .. kind, level)
end

-- The language `which` says (a name or alias in any case, or a description), built the first time
-- it is asked for and the same table after; nil for a name this module does not know. It is
-- { name =, description =, syntax =, config = }, frozen: a page that adds its own options to the
-- editor's puts them in a copy (`table.clone(language.config)`).
function languages.load(which)
	local description = described(which, 3)
	if description == nil then
		return nil
	end
	local language = built[description]
	if language then
		return language
	end
	local family = FAMILIES[description.family]()
	local made = family.make(description.name, context)
	readers[description] = made.reader
	language = table.freeze({
		name = description.name,
		description = description,
		syntax = Syntax.define({
			name = description.name,
			start = made.reader.start,
			token = made.reader.token,
			blank = made.reader.blank,
			copy = made.reader.copy,
			equal = made.reader.equal,
			styles = styles,
		}),
		config = freeze_all(made.config),
	})
	built[description] = language
	return language
end

-- The language `which` says if it has been built (`load`), else nil: what a page that does not want
-- to build one in a call it is short of time in looks at first.
function languages.loaded(which)
	local description = described(which, 3)
	return description and built[description] or nil
end

function context.reader(name)
	local description = by_name[string.lower(name)]
	if description == nil then
		return nil
	end
	if readers[description] == nil then
		languages.load(description)
	end
	return readers[description]
end

return table.freeze(languages)
