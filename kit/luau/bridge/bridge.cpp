// The Lua page: Luau, the page interface (`kit/wit/view.wit`) and a thin layer
// between them.
//
// This file is the interface's side of the page: it implements what a page exports (render,
// settle, handle, message, room), gives the script what it may ask of the application (the
// layout, a way to speak to the program) and says why when the script cannot go on. What a page
// *is* — its elements, the pointer's bookkeeping, the messages with the program — is Lua, in
// `prelude/`, built into this module; a script's own text is what the application hands over
// through `script.source`.
//
// The script runs in Luau's sandbox, with the pure libraries only (`string`, `table`, `math`,
// `utf8` and the base functions): no `io`, `os` or `debug`, no way to load bytecode. It has a
// `require`, and the only thing `require` reaches is the other scripts of the bundle the script
// came in (see "Loading the other scripts of the bundle"). The module itself can reach nothing the
// page interface does not give it; the calls the C library would make to the system are answered
// here (see the end of this file).

#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include <map>
#include <string>
#include <vector>

#include "Luau/Require.h"
#include "convert.h"
#include "lua.h"
#include "luacode.h"
#include "lualib.h"

extern "C" {
#include "view.h"
}

// The preludes, as `kPrelude[i]` (a name and the source), written by the build script.
#include "prelude.inc"

namespace {

lua_State* L = nullptr;
// The state is made when the page is first asked for something. The application asks for
// `settle` once as the module starts (before any page, so that it is a call that only starts
// the script): a script that cannot start fails there, under the time a module has to start in,
// and the first page is not charged for it.
bool started = false;
// The script cannot go on: the application has been told, and is closing the page.
bool broken = false;

constexpr size_t kMessageLimit = 2048;

void fail(const std::string& why) {
    broken = true;
    std::string text = bridge::repaired(why.data(), why.size() > kMessageLimit ? kMessageLimit : why.size());
    view_string_t message;
    message.ptr = (uint8_t*)text.data();
    message.len = text.size();
    pencere_view_diagnostics_fail(&message);
}

// The error on top of the stack as words, popped.
std::string reason(lua_State* state) {
    std::string out;
    if (lua_type(state, -1) == LUA_TSTRING) {
        out = lua_tostring(state, -1);
    } else {
        size_t length = 0;
        const char* shown = luaL_tolstring(state, -1, &length);
        out.assign(shown, length);
        lua_pop(state, 1);
    }
    lua_pop(state, 1);
    return out;
}

// Compiles `source` as the chunk `name` and leaves it on the stack of `state`; the error
// otherwise (the line is in it).
bool load(lua_State* state, const char* name, const char* source, size_t length, std::string& error) {
    lua_CompileOptions options = {};
    options.optimizationLevel = 1;
    options.debugLevel = 1;
    size_t size = 0;
    char* bytecode = luau_compile(source, length, &options, &size);
    int status = luau_load(state, name, bytecode, size, 0);
    free(bytecode);
    if (status != 0) {
        error = reason(state);
        return false;
    }
    return true;
}

// --- What the script may ask of the application -------------------------------------------------

int l_send(lua_State* state) {
    size_t length = 0;
    const char* bytes = luaL_checklstring(state, 1, &length);
    std::string text = bridge::repaired(bytes, length);
    view_string_t message;
    message.ptr = (uint8_t*)text.data();
    message.len = text.size();
    lua_pushboolean(state, pencere_view_program_send(&message) == PENCERE_VIEW_PROGRAM_DELIVERY_ACCEPTED);
    return 1;
}

void push_rect(lua_State* state, const pencere_view_types_rect_t& rect) {
    lua_createtable(state, 0, 4);
    lua_pushnumber(state, rect.x);
    lua_setfield(state, -2, "x");
    lua_pushnumber(state, rect.y);
    lua_setfield(state, -2, "y");
    lua_pushnumber(state, rect.w);
    lua_setfield(state, -2, "w");
    lua_pushnumber(state, rect.h);
    lua_setfield(state, -2, "h");
}

void push_ids(lua_State* state, view_list_id_t& ids) {
    lua_createtable(state, (int)ids.len, 0);
    for (size_t at = 0; at < ids.len; at++) {
        bridge::push_id(state, ids.ptr[at]);
        lua_rawseti(state, -2, (int)at + 1);
    }
    view_list_id_free(&ids);
}

int l_hit(lua_State* state) {
    pencere_view_types_point_t at = {(float)luaL_checknumber(state, 1), (float)luaL_checknumber(state, 2)};
    uint64_t id = 0;
    if (pencere_view_layout_hit(&at, &id)) bridge::push_id(state, id);
    else lua_pushnil(state);
    return 1;
}

int l_ids_at(lua_State* state) {
    pencere_view_types_point_t at = {(float)luaL_checknumber(state, 1), (float)luaL_checknumber(state, 2)};
    view_list_id_t ids = {nullptr, 0};
    pencere_view_layout_ids_at(&at, &ids);
    push_ids(state, ids);
    return 1;
}

int l_bounds(lua_State* state) {
    pencere_view_types_rect_t rect;
    if (pencere_view_layout_bounds(bridge::id_of(state, 1), &rect)) push_rect(state, rect);
    else lua_pushnil(state);
    return 1;
}

int l_scroll_extent(lua_State* state) {
    pencere_view_types_extent_t extent;
    if (pencere_view_layout_scroll_extent(bridge::id_of(state, 1), &extent)) {
        // Down as `content` and `view`, across as `content_w` and `view_w`.
        lua_createtable(state, 0, 6);
        lua_pushnumber(state, extent.content.h);
        lua_setfield(state, -2, "content");
        lua_pushnumber(state, extent.view.h);
        lua_setfield(state, -2, "view");
        lua_pushnumber(state, extent.content.w);
        lua_setfield(state, -2, "content_w");
        lua_pushnumber(state, extent.view.w);
        lua_setfield(state, -2, "view_w");
        // How far it is scrolled: `offset` down, `offset_x` across.
        lua_pushnumber(state, extent.offset.y);
        lua_setfield(state, -2, "offset");
        lua_pushnumber(state, extent.offset.x);
        lua_setfield(state, -2, "offset_x");
    } else {
        lua_pushnil(state);
    }
    return 1;
}

// scroll_to(id, x, y, animate): x and y nil stay where they are.
int l_scroll_to(lua_State* state) {
    uint64_t id = bridge::id_of(state, 1);
    float x = 0.0f, y = 0.0f;
    bool has_x = lua_type(state, 2) == LUA_TNUMBER, has_y = lua_type(state, 3) == LUA_TNUMBER;
    if (has_x) x = (float)lua_tonumber(state, 2);
    if (has_y) y = (float)lua_tonumber(state, 3);
    pencere_view_layout_scroll_to(id, has_x ? &x : nullptr, has_y ? &y : nullptr, lua_toboolean(state, 4) != 0);
    return 0;
}

int l_focus_order(lua_State* state) {
    view_list_id_t ids = {nullptr, 0};
    pencere_view_layout_focus_order(&ids);
    push_ids(state, ids);
    return 1;
}

int l_path_of(lua_State* state) {
    view_list_id_t ids = {nullptr, 0};
    pencere_view_layout_path_of(bridge::id_of(state, 1), &ids);
    push_ids(state, ids);
    return 1;
}

int l_drag_source(lua_State* state) {
    pencere_view_types_point_t at = {(float)luaL_checknumber(state, 1), (float)luaL_checknumber(state, 2)};
    uint64_t id = 0;
    if (pencere_view_layout_drag_source(&at, &id)) bridge::push_id(state, id);
    else lua_pushnil(state);
    return 1;
}

int l_drop_target(lua_State* state) {
    pencere_view_types_point_t at = {(float)luaL_checknumber(state, 1), (float)luaL_checknumber(state, 2)};
    uint64_t id = 0;
    if (pencere_view_layout_drop_target(&at, &id)) bridge::push_id(state, id);
    else lua_pushnil(state);
    return 1;
}

// --- The text of the page's editors ------------------------------------------------------------
//
// What the application posted of each editor's document: read at once, of the version the last
// `edit` event the page was told says. Places and lines are whole numbers from 0.

uint32_t checked_place(lua_State* state, int index, const char* what) {
    double at = luaL_checknumber(state, index);
    if (!(at >= 0 && at <= 4294967295.0) || at != (double)(uint32_t)at)
        luaL_error(state, "%s is a whole number from 0", what);
    return (uint32_t)at;
}

int l_editor_version(lua_State* state) {
    uint64_t version = 0;
    if (pencere_view_layout_editor_version(bridge::id_of(state, 1), &version)) lua_pushnumber(state, (double)version);
    else lua_pushnil(state);
    return 1;
}

int l_editor_length(lua_State* state) {
    uint32_t length = 0;
    if (pencere_view_layout_editor_length(bridge::id_of(state, 1), &length)) lua_pushnumber(state, length);
    else lua_pushnil(state);
    return 1;
}

int l_editor_lines(lua_State* state) {
    uint32_t lines = 0;
    if (pencere_view_layout_editor_lines(bridge::id_of(state, 1), &lines)) lua_pushnumber(state, lines);
    else lua_pushnil(state);
    return 1;
}

int l_editor_text(lua_State* state) {
    uint64_t editor = bridge::id_of(state, 1);
    uint32_t from = checked_place(state, 2, "from");
    uint32_t to = checked_place(state, 3, "to");
    view_string_t text = {nullptr, 0};
    if (pencere_view_layout_editor_text(editor, from, to, &text)) {
        lua_pushlstring(state, (const char*)text.ptr, text.len);
        view_string_free(&text);
    } else {
        lua_pushnil(state);
    }
    return 1;
}

int l_line_start(lua_State* state) {
    uint64_t editor = bridge::id_of(state, 1);
    uint32_t at = 0;
    if (pencere_view_layout_line_start(editor, checked_place(state, 2, "a line"), &at)) lua_pushnumber(state, at);
    else lua_pushnil(state);
    return 1;
}

int l_line_of(lua_State* state) {
    uint64_t editor = bridge::id_of(state, 1);
    uint32_t line = 0;
    if (pencere_view_layout_line_of(editor, checked_place(state, 2, "a place"), &line)) lua_pushnumber(state, line);
    else lua_pushnil(state);
    return 1;
}

// --- Loading the other scripts of the bundle -------------------------------------------------------
//
// `require` is Luau's own require-by-string (the `Luau.Require` library), pointed at the bundle the
// script came in instead of at a file system: a path is `./` or `../` from the requiring script (an
// `init` script stands for its directory, as Luau has it), and what it names is a script of the
// bundle, `name.luau`, `name.lua` or `name/init.luau` or `name/init.lua`. Everything about the
// bundle is the application's to say (`script.has-module`, `script.module`): the module has no
// file system to look in, and a path that leaves the bundle's root, an absolute path, an alias
// (there is no configuration file to define one) or a file that is not a script is an error that
// names the path. Each script runs once, in a thread of its own with globals of its own like the
// entry, and what it returns is kept: the next `require` of it is that value.

// The path of the script the page is, in the bundle: what a path is relative to, and what the
// entry's errors name.
std::string entry_path;
// What the application said about the scripts of the bundle, by path: asked once for each.
std::map<std::string, bool> known;
// The scripts whose first run has not ended, the outermost first (the entry, while it starts).
std::vector<std::string> running;
// Where a `require` is now: the components of a module's path, from the root of the bundle.
std::vector<std::string> place;
// The error that is on its way up came out of a script that was running, not out of the
// `require` that is raising it: it has its own file and line already.
bool carried = false;

bool ends_with(const std::string& text, const char* suffix) {
    size_t length = strlen(suffix);
    return text.size() >= length && text.compare(text.size() - length, length, suffix) == 0;
}

constexpr const char* kSuffixes[] = {".luau", ".lua"};
constexpr const char* kInitSuffixes[] = {"/init.luau", "/init.lua"};

// The path of a script without what makes it a file: `a/b.luau` is the module `a/b`, and
// `a/init.lua` is the module `a`, its directory.
std::string module_of(std::string file) {
    for (const char* suffix : kInitSuffixes) {
        if (ends_with(file, suffix)) return file.substr(0, file.size() - strlen(suffix));
    }
    for (const char* suffix : kSuffixes) {
        if (ends_with(file, suffix)) return file.substr(0, file.size() - strlen(suffix));
    }
    return file;
}

std::vector<std::string> split(const std::string& path) {
    std::vector<std::string> parts;
    size_t start = 0;
    while (start <= path.size()) {
        size_t end = path.find('/', start);
        if (end == std::string::npos) end = path.size();
        if (end > start) parts.push_back(path.substr(start, end - start));
        start = end + 1;
    }
    return parts;
}

std::string joined(const std::vector<std::string>& parts) {
    std::string out;
    for (const std::string& part : parts) {
        if (!out.empty()) out += '/';
        out += part;
    }
    return out;
}

// Whether the bundle has a script at `path` (a path the manifest lists).
bool has_script(const std::string& path) {
    auto seen = known.find(path);
    if (seen != known.end()) return seen->second;
    // A name is a string a script made: the host reads valid UTF-8 only.
    std::string text = bridge::repaired(path.data(), path.size());
    view_string_t name;
    name.ptr = (uint8_t*)text.data();
    name.len = text.size();
    bool has = pencere_view_script_has_module(&name);
    known.emplace(path, has);
    return has;
}

// The bytes of the script at `path`, or why not.
bool script_text(const std::string& path, std::string& bytes, std::string& why) {
    std::string text = bridge::repaired(path.data(), path.size());
    view_string_t name;
    name.ptr = (uint8_t*)text.data();
    name.len = text.size();
    view_list_u8_t given = {nullptr, 0};
    view_string_t reason_given = {nullptr, 0};
    if (pencere_view_script_module(&name, &given, &reason_given)) {
        bytes.assign((const char*)given.ptr, given.len);
        view_list_u8_free(&given);
        return true;
    }
    why.assign((const char*)reason_given.ptr, reason_given.len);
    view_string_free(&reason_given);
    return false;
}

// The scripts a module path can be: `a/b` is `a/b.luau`, `a/b.lua`, `a/b/init.luau` or
// `a/b/init.lua`, and is a module when exactly one of them is there. (`init` is no module by
// a file of its own name: `a/init` is the directory `a/init/`'s.)
struct Candidates {
    int count;
    std::string file;
};

Candidates candidates(const std::vector<std::string>& parts) {
    Candidates found = {0, ""};
    if (parts.empty()) return found;
    std::string base = joined(parts);
    auto look = [&](const std::string& file) {
        if (!has_script(file)) return;
        found.count++;
        found.file = file;
    };
    if (parts.back() != "init") {
        for (const char* suffix : kSuffixes) look(base + suffix);
    }
    for (const char* suffix : kInitSuffixes) look(base + suffix);
    return found;
}

luarequire_WriteResult write_text(const std::string& text, char* buffer, size_t size, size_t* written) {
    size_t needed = text.size() + 1;
    *written = needed;
    if (size < needed) return WRITE_BUFFER_TOO_SMALL;
    memcpy(buffer, text.c_str(), needed);
    return WRITE_SUCCESS;
}

// Only what this module loaded can require: a chunk named `@path` is a script of the bundle.
bool require_allowed(lua_State*, void*, const char* chunkname) { return chunkname[0] == '@'; }

luarequire_NavigateResult require_reset(lua_State*, void*, const char* chunkname) {
    place = split(module_of(chunkname + 1));
    return NAVIGATE_SUCCESS;
}

// There are no aliases: no configuration file to define one.
luarequire_NavigateResult require_jump(lua_State*, void*, const char*) { return NAVIGATE_NOT_FOUND; }

luarequire_ConfigStatus require_config_status(lua_State*, void*) { return CONFIG_ABSENT; }

luarequire_WriteResult require_alias(lua_State*, void*, const char*, char*, size_t, size_t*) { return WRITE_FAILURE; }

// The bundle's root has no parent: a path that leaves it ends here, and is an error that says so.
luarequire_NavigateResult require_parent(lua_State*, void*) {
    if (place.empty()) return NAVIGATE_NOT_FOUND;
    place.pop_back();
    return NAVIGATE_SUCCESS;
}

// Any name can be a directory, whether the bundle has one or not: whether it is a module is
// asked when the path ends (`require_present`).
luarequire_NavigateResult require_child(lua_State*, void*, const char* name) {
    if (strcmp(name, ".config") == 0) return NAVIGATE_NOT_FOUND;
    place.push_back(name);
    return candidates(place).count > 1 ? NAVIGATE_AMBIGUOUS : NAVIGATE_SUCCESS;
}

bool require_present(lua_State* state, void*) {
    if (candidates(place).count == 1) return true;
    // The path that was asked for is the one argument of the `require` that is running.
    std::string wanted = lua_isstring(state, 1) ? lua_tostring(state, 1) : "?";
    std::string base = joined(place);
    std::string message = "error requiring module \"" + wanted + "\": ";
    if (place.empty()) {
        message += "that is the root of the bundle, not a script";
    } else {
        message += "the bundle has no script at \"" + base + "\" (it looked for " + base + ".luau, " + base +
                   ".lua, " + base + "/init.luau and " + base + "/init.lua)";
    }
    lua_pushlstring(state, message.data(), message.size());
    lua_error(state);
}

luarequire_WriteResult require_chunkname(lua_State*, void*, char* buffer, size_t size, size_t* written) {
    return write_text("@" + candidates(place).file, buffer, size, written);
}

luarequire_WriteResult require_loadname(lua_State*, void*, char* buffer, size_t size, size_t* written) {
    return write_text(candidates(place).file, buffer, size, written);
}

// Two paths that name the same script are one entry in the cache.
luarequire_WriteResult require_cache_key(lua_State*, void*, char* buffer, size_t size, size_t* written) {
    return write_text(candidates(place).file, buffer, size, written);
}

// Why the script at `file` cannot be loaded now; empty when it can.
std::string refusal(const std::string& file) {
    for (size_t at = 0; at < running.size(); at++) {
        if (running[at] != file) continue;
        std::string chain;
        for (size_t next = at; next < running.size(); next++) chain += running[next] + " -> ";
        return "require cycle: " + chain + file;
    }
    // The entry has run: it is the page, and not a module another script can have.
    if (file == entry_path) return "\"" + file + "\" is the script of the page, and is not a module to require";
    return "";
}

// Runs the script at `file` in a thread of its own, once, and leaves what it returns.
int require_load(lua_State* state, void*, const char*, const char* chunkname, const char* loadname) {
    // The thread comes from the main state, so that it does not inherit the requirer's globals.
    lua_State* main = lua_mainthread(state);
    lua_State* thread = lua_newthread(main);
    lua_xmove(main, state, 1);
    luaL_sandboxthread(thread);

    std::string file = loadname;
    std::string error = refusal(file);
    bool ran = false;
    int status = LUA_OK;
    if (error.empty()) {
        std::string source;
        if (script_text(file, source, error)) {
            if (load(thread, chunkname, source.data(), source.size(), error)) {
                ran = true;
                running.push_back(file);
                status = lua_resume(thread, state, 0);
                running.pop_back();
            } else {
                // A syntax error says its file and its line.
                carried = true;
            }
        }
    }
    if (!ran) {
        lua_pushlstring(state, error.data(), error.size());
        lua_error(state);
    }
    if (status == LUA_YIELD) {
        error = file + ": a script that is required cannot wait";
        lua_pushlstring(state, error.data(), error.size());
        lua_error(state);
    }
    if (status != LUA_OK) {
        // It has its file and line: the error goes up as it is.
        carried = true;
        lua_xmove(thread, state, 1);
        lua_error(state);
    }
    if (lua_gettop(thread) != 1) {
        error = file + ": a script that is required returns exactly one value";
        lua_pushlstring(state, error.data(), error.size());
        lua_error(state);
    }
    lua_xmove(thread, state, 1);
    lua_remove(state, -2);
    return 1;
}

void require_configuration(luarequire_Configuration* config) {
    config->is_require_allowed = require_allowed;
    config->reset = require_reset;
    config->jump_to_alias = require_jump;
    config->to_parent = require_parent;
    config->to_child = require_child;
    config->is_module_present = require_present;
    config->get_chunkname = require_chunkname;
    config->get_loadname = require_loadname;
    config->get_cache_key = require_cache_key;
    config->get_config_status = require_config_status;
    config->get_alias = require_alias;
    config->load = require_load;
}

// `require` as a script sees it: the library's, with the place of the call added to what it says
// where the error is its own (a path that is wrong), and not a script's that was running.
int l_require(lua_State* state) {
    lua_settop(state, 1);
    lua_pushvalue(state, lua_upvalueindex(1));
    lua_insert(state, 1);
    carried = false;
    if (lua_pcall(state, 1, 1, 0) != LUA_OK) {
        if (!carried && lua_type(state, -1) == LUA_TSTRING) {
            luaL_where(state, 1);
            lua_insert(state, -2);
            lua_concat(state, 2);
        }
        lua_error(state);
    }
    return 1;
}

// --- Starting ------------------------------------------------------------------------------------

void open_libraries(lua_State* state) {
    luaopen_base(state);
    lua_pop(state, 1);
    luaopen_table(state);
    lua_pop(state, 1);
    luaopen_string(state);
    lua_pop(state, 1);
    luaopen_math(state);
    lua_pop(state, 1);
    luaopen_utf8(state);
    lua_pop(state, 1);
    // The page has nowhere to print to, and nothing of the system to ask: what the base
    // library has of either goes.
    static const char* const gone[] = {"print", "collectgarbage", "gcinfo", "newproxy", "getfenv", "setfenv"};
    for (const char* name : gone) {
        lua_pushnil(state);
        lua_setglobal(state, name);
    }
}

void host_functions(lua_State* state) {
    static const luaL_Reg functions[] = {
        {"send", l_send},
        {"hit", l_hit},
        {"ids_at", l_ids_at},
        {"bounds", l_bounds},
        {"scroll_extent", l_scroll_extent},
        {"scroll_to", l_scroll_to},
        {"focus_order", l_focus_order},
        {"path_of", l_path_of},
        {"drag_source", l_drag_source},
        {"drop_target", l_drop_target},
        {"editor_version", l_editor_version},
        {"editor_length", l_editor_length},
        {"editor_lines", l_editor_lines},
        {"editor_text", l_editor_text},
        {"line_start", l_line_start},
        {"line_of", l_line_of},
        {nullptr, nullptr},
    };
    lua_newtable(state);
    luaL_register(state, nullptr, functions);
    lua_setglobal(state, "pencere");
}

// Makes the state: the libraries, the prelude, the sandbox, then the script. False (and the
// application told why) when any of it fails.
bool start() {
    started = true;
    L = luaL_newstate();
    if (!L) {
        fail("the interpreter could not be started");
        return false;
    }
    open_libraries(L);
    host_functions(L);
    luarequire_pushrequire(L, require_configuration, nullptr);
    lua_pushcclosurek(L, l_require, "require", 1, nullptr);
    lua_setglobal(L, "require");
    for (const auto& chunk : kPrelude) {
        std::string error;
        std::string name = std::string("=") + chunk.name;
        if (!load(L, name.c_str(), chunk.source, chunk.length, error) || lua_pcall(L, 0, 0, 0) != 0) {
            if (error.empty()) error = reason(L);
            fail(error);
            return false;
        }
    }
    luaL_sandbox(L);
    // The script gets a thread of its own, with globals of its own over the shared, read-only
    // ones: what it assigns is its own.
    lua_State* thread = lua_newthread(L);
    lua_ref(L, -1);
    lua_pop(L, 1);
    luaL_sandboxthread(thread);

    view_string_t path = {nullptr, 0};
    pencere_view_script_path(&path);
    entry_path.assign((const char*)path.ptr, path.len);
    view_string_free(&path);
    view_list_u8_t source = {nullptr, 0};
    pencere_view_script_source(&source);
    std::string error;
    // Named by its path in the bundle, as every script is: that is what an error says.
    std::string chunkname = "@" + entry_path;
    bool loaded = load(thread, chunkname.c_str(), (const char*)source.ptr, source.len, error);
    view_list_u8_free(&source);
    if (!loaded) {
        fail(error);
        return false;
    }
    running.push_back(entry_path);
    int status = lua_pcall(thread, 0, 1, 0);
    running.pop_back();
    if (status != 0) {
        fail(reason(thread));
        return false;
    }
    // The prelude's `start` takes what the script returned: the page.
    lua_getglobal(L, "pencere");
    lua_getfield(L, -1, "start");
    lua_xmove(thread, L, 1);
    if (lua_pcall(L, 1, 0, 0) != 0) {
        fail(reason(L));
        return false;
    }
    lua_pop(L, 1);
    return true;
}

bool ready() {
    if (broken) return false;
    if (started) return true;
    return start();
}

// Calls `pencere.<name>` with the `arguments` the caller has pushed after it, protected. The
// results are left on the stack; false when it failed (and the application has been told).
bool entry(const char* name, int arguments, int results) {
    // The function goes below its arguments.
    lua_getglobal(L, "pencere");
    lua_getfield(L, -1, name);
    lua_remove(L, -2);
    lua_insert(L, -(arguments + 1));
    if (lua_pcall(L, arguments, results, 0) != 0) {
        fail(reason(L));
        return false;
    }
    return true;
}

// The value on top of the stack as a reply, popped; the default where it is wrong.
void answer(pencere_view_types_reply_t* out) {
    bridge::ReplyJob job = {out};
    lua_pushcfunction(L, bridge::reply_call, "reply");
    lua_insert(L, -2);
    lua_pushlightuserdata(L, &job);
    if (lua_pcall(L, 2, 0, 0) != 0) {
        pencere_view_types_reply_free(out);
        memset(out, 0, sizeof *out);
        fail(reason(L));
    }
}

}  // namespace

// --- What a page exports -------------------------------------------------------------------------

void exports_pencere_view_page_render(exports_pencere_view_page_size_t* size, double now, bool reduce_motion,
                                     exports_pencere_view_page_frame_t* ret) {
    memset(ret, 0, sizeof *ret);
    if (!ready()) return;
    lua_pushnumber(L, size->w);
    lua_pushnumber(L, size->h);
    lua_pushnumber(L, now);
    lua_pushboolean(L, reduce_motion);
    if (!entry("render", 4, 4)) return;
    // The prelude hands back the element, whether it moves, when it wants to be asked again and
    // in how many seconds it wants to be woken (numbers or nil: it checked).
    bool has_wake = lua_type(L, -1) == LUA_TNUMBER;
    double wake = has_wake ? lua_tonumber(L, -1) : 0.0;
    bool has_wake_at = lua_type(L, -2) == LUA_TNUMBER;
    double wake_at = has_wake_at ? lua_tonumber(L, -2) : 0.0;
    bridge::FrameJob job = {ret, lua_toboolean(L, -3) != 0, has_wake_at, wake_at, has_wake, wake};
    lua_pop(L, 3);
    lua_pushcfunction(L, bridge::frame_call, "frame");
    lua_insert(L, -2);
    lua_pushlightuserdata(L, &job);
    if (lua_pcall(L, 2, 0, 0) != 0) {
        exports_pencere_view_page_frame_free(ret);
        memset(ret, 0, sizeof *ret);
        fail(reason(L));
    }
}

bool exports_pencere_view_page_settle(void) {
    if (!ready()) return false;
    if (!entry("settle", 0, 1)) return false;
    bool changed = lua_toboolean(L, -1) != 0;
    lua_pop(L, 1);
    return changed;
}

void exports_pencere_view_page_handle(exports_pencere_view_page_event_t* event,
                                     exports_pencere_view_page_reply_t* ret) {
    memset(ret, 0, sizeof *ret);
    if (ready()) {
        bridge::push_event(L, *event);
        if (entry("handle", 1, 1)) answer(ret);
    }
    exports_pencere_view_page_event_free(event);
}

void exports_pencere_view_page_message(view_string_t* message, exports_pencere_view_page_reply_t* ret) {
    memset(ret, 0, sizeof *ret);
    if (ready()) {
        lua_pushlstring(L, (const char*)message->ptr, message->len);
        if (entry("message", 1, 1)) answer(ret);
    }
    view_string_free(message);
}

void exports_pencere_view_page_room(exports_pencere_view_page_reply_t* ret) {
    memset(ret, 0, sizeof *ret);
    if (!ready()) return;
    if (entry("room", 0, 1)) answer(ret);
}

// --- What the system is not asked --------------------------------------------------------------
//
// The C library asks the system for a few things (the clock, the environment, a place to write,
// a way to exit). The module has no system to ask, so the questions are answered here, where
// they stop: nothing is written anywhere, there is no environment, the clock reads zero (the
// page is told the time as an argument of `render`), and an exit is a trap. Defining the names
// the library asks by is what keeps them from becoming imports of the component.

extern "C" {

clock_t clock(void) { return 0; }

uint16_t __imported_wasi_snapshot_preview1_environ_sizes_get(uint32_t* count, uint32_t* bytes) {
    *count = 0;
    *bytes = 0;
    return 0;
}

uint16_t __imported_wasi_snapshot_preview1_environ_get(uint8_t** environ, uint8_t* buffer) {
    (void)environ;
    (void)buffer;
    return 0;
}

uint16_t __imported_wasi_snapshot_preview1_clock_time_get(uint32_t clock, uint64_t precision, uint64_t* time) {
    (void)clock;
    (void)precision;
    *time = 0;
    return 0;
}

// 8 is "bad file descriptor": there is no file to close, to seek in or to write to.
uint16_t __imported_wasi_snapshot_preview1_fd_close(uint32_t fd) {
    (void)fd;
    return 8;
}

uint16_t __imported_wasi_snapshot_preview1_fd_fdstat_get(uint32_t fd, void* stat) {
    (void)fd;
    (void)stat;
    return 8;
}

uint16_t __imported_wasi_snapshot_preview1_fd_seek(uint32_t fd, int64_t offset, uint8_t whence, uint64_t* position) {
    (void)fd;
    (void)offset;
    (void)whence;
    (void)position;
    return 8;
}

uint16_t __imported_wasi_snapshot_preview1_fd_write(uint32_t fd, const void* vectors, uint32_t count,
                                                    uint32_t* written) {
    (void)fd;
    (void)vectors;
    (void)count;
    *written = 0;
    return 8;
}

_Noreturn void __imported_wasi_snapshot_preview1_proc_exit(uint32_t code) {
    (void)code;
    __builtin_trap();
}

}  // extern "C"
