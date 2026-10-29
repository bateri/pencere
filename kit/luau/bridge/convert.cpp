#include "convert.h"

#include <float.h>
#include <stdlib.h>
#include <string.h>

#include <cctype>
#include <string>

#include <unordered_map>
#include <vector>

#include "lualib.h"

namespace bridge {

// The page's text as the host takes it (defined below, with what makes it well-formed).
view_string_t host_string(const char* bytes, size_t length);

namespace {

// How deep and how large a tree this reads. The application holds a page to its own, smaller
// limits and closes it past them; these only keep a script's mistake (a tree that contains
// itself) from running this out of stack or memory before the application can say so.
constexpr int kMaxDepth = 256;
constexpr size_t kMaxNodes = 1 << 16;
constexpr size_t kMaxList = 4096;

void* xmalloc(size_t n) {
    void* p = malloc(n ? n : 1);
    if (!p) __builtin_trap();
    return p;
}

void* xrealloc(void* old, size_t n) {
    void* p = realloc(old, n ? n : 1);
    if (!p) __builtin_trap();
    return p;
}

// --- The lists a frame names the parts of ----------------------------------------------------

// What a frame says that is not of a fixed size (words, values, tracks) is in three lists, once
// for the whole frame, and an element names the part it means (see `frame` in the interface): a
// script's page is read into these as it is walked, and they are handed to the host as they are.
struct Arenas {
    std::string text;
    std::vector<float> numbers;
    std::vector<pencere_view_types_track_t> tracks;
    std::vector<pencere_view_types_span_t> spans;
    std::vector<pencere_view_types_path_t> paths;
    std::vector<pencere_view_types_segment_t> segments;
    std::vector<pencere_view_types_command_t> commands;
};

// A list for the host: a malloc'ed copy of `items` (the generated `_free` functions release it),
// and nothing at all for an empty one.
template <typename List, typename Item>
void take_list(List& out, const std::vector<Item>& items) {
    if (items.empty()) return;
    size_t bytes = items.size() * sizeof(Item);
    out.ptr = (Item*)xmalloc(bytes);
    out.len = items.size();
    memcpy(out.ptr, items.data(), bytes);
}

// The part of a list that begins at `start` and has `len` items.
pencere_view_types_slice_t part(size_t start, size_t len) {
    pencere_view_types_slice_t out;
    out.start = (uint32_t)start;
    out.len = (uint32_t)len;
    return out;
}

// The words in the list, as a part of it. Valid UTF-8 whatever the script gave (the host would
// refuse a page that was not), and so are the ends of every part.
pencere_view_types_slice_t add_words(Arenas& arenas, const char* bytes, size_t length) {
    std::string clean = repaired(bytes, length);
    size_t start = arenas.text.size();
    arenas.text += clean;
    return part(start, clean.size());
}

// The lists as the host takes them: malloc'ed (the generated `_free` functions release them), and
// nothing at all for an empty one.
view_string_t take_text(const std::string& text) {
    view_string_t out = {nullptr, 0};
    if (!text.empty()) {
        out.ptr = (uint8_t*)xmalloc(text.size());
        out.len = text.size();
        memcpy(out.ptr, text.data(), text.size());
    }
    return out;
}

// --- Names ----------------------------------------------------------------------------------

std::unordered_map<uint64_t, std::string>& names() {
    static std::unordered_map<uint64_t, std::string> map;
    return map;
}

uint64_t fnv(const char* bytes, size_t length) {
    uint64_t hash = 0xcbf29ce484222325ull;
    for (size_t at = 0; at < length; at++) {
        hash ^= (unsigned char)bytes[at];
        hash *= 0x100000001b3ull;
    }
    return hash;
}

// --- Reading a table ------------------------------------------------------------------------

// The value of `name` in the table at the absolute stack index `t`, on the stack.
void get(lua_State* L, int t, const char* name) { lua_rawgetfield(L, t, name); }

float number(lua_State* L, int t, const char* name, float fallback) {
    get(L, t, name);
    float value = fallback;
    int kind = lua_type(L, -1);
    if (kind == LUA_TNUMBER) value = (float)lua_tonumber(L, -1);
    else if (kind != LUA_TNIL) luaL_error(L, "%s must be a number", name);
    lua_pop(L, 1);
    return value;
}

view_option_f32_t optional_number(lua_State* L, int t, const char* name) {
    view_option_f32_t out = {false, 0.0f};
    get(L, t, name);
    int kind = lua_type(L, -1);
    if (kind == LUA_TNUMBER) {
        out.is_some = true;
        out.val = (float)lua_tonumber(L, -1);
    } else if (kind != LUA_TNIL) {
        luaL_error(L, "%s must be a number", name);
    }
    lua_pop(L, 1);
    return out;
}

bool flag(lua_State* L, int t, const char* name) {
    get(L, t, name);
    bool value = lua_toboolean(L, -1) != 0;
    lua_pop(L, 1);
    return value;
}

// The word `name` holds, as its place in `words`; `fallback` when it holds nothing. -1 when it
// is none of the words (the caller says which it was).
// A field that is true, false or not said.
view_option_bool_t optional_flag(lua_State* L, int t, const char* name) {
    view_option_bool_t out = {};
    get(L, t, name);
    if (!lua_isnil(L, -1)) {
        out.is_some = true;
        out.val = lua_toboolean(L, -1) != 0;
    }
    lua_pop(L, 1);
    return out;
}

int word(lua_State* L, int t, const char* name, const char* const* words, int count, int fallback) {
    get(L, t, name);
    int found = fallback;
    int kind = lua_type(L, -1);
    if (kind == LUA_TSTRING) {
        const char* given = lua_tostring(L, -1);
        found = -1;
        for (int at = 0; at < count; at++) {
            if (strcmp(given, words[at]) == 0) {
                found = at;
                break;
            }
        }
        if (found < 0) luaL_error(L, "%s: \"%s\" is not one of the words it takes", name, given);
    } else if (kind != LUA_TNIL) {
        luaL_error(L, "%s must be a word", name);
    }
    lua_pop(L, 1);
    return found;
}

const char* const kDisplay[] = {"flex", "grid", "none"};
const char* const kDirection[] = {"row", "column"};
const char* const kItems[] = {"start", "center", "end", "stretch"};
const char* const kJustify[] = {"start", "center", "end", "between"};
const char* const kOverflow[] = {"visible", "hidden", "scroll"};
const char* const kFamily[] = {"system", "terminal"};
const char* const kAlign[] = {"start", "center", "end"};
const char* const kFit[] = {"contain", "cover"};
const char* const kSide[] = {"below", "above", "right", "left"};

// A word of the tables above is the place of the constant it names, so the places are held to the
// interface here: an enum that is reordered or grows in the middle stops the build and does not
// quietly name another word.
static_assert(PENCERE_VIEW_TYPES_DISPLAY_FLEX == 0 && PENCERE_VIEW_TYPES_DISPLAY_GRID == 1 &&
                  PENCERE_VIEW_TYPES_DISPLAY_NONE == 2,
              "kDisplay is the order of display");
static_assert(PENCERE_VIEW_TYPES_DIRECTION_ROW == 0 && PENCERE_VIEW_TYPES_DIRECTION_COLUMN == 1,
              "kDirection is the order of direction");
static_assert(PENCERE_VIEW_TYPES_ITEMS_START == 0 && PENCERE_VIEW_TYPES_ITEMS_CENTER == 1 &&
                  PENCERE_VIEW_TYPES_ITEMS_END == 2 && PENCERE_VIEW_TYPES_ITEMS_STRETCH == 3,
              "kItems is the order of items");
static_assert(PENCERE_VIEW_TYPES_JUSTIFY_START == 0 && PENCERE_VIEW_TYPES_JUSTIFY_CENTER == 1 &&
                  PENCERE_VIEW_TYPES_JUSTIFY_END == 2 && PENCERE_VIEW_TYPES_JUSTIFY_BETWEEN == 3,
              "kJustify is the order of justify");
static_assert(PENCERE_VIEW_TYPES_OVERFLOW_VISIBLE == 0 && PENCERE_VIEW_TYPES_OVERFLOW_HIDDEN == 1 &&
                  PENCERE_VIEW_TYPES_OVERFLOW_SCROLL == 2,
              "kOverflow is the order of overflow");
static_assert(PENCERE_VIEW_TYPES_FAMILY_SYSTEM == 0 && PENCERE_VIEW_TYPES_FAMILY_TERMINAL == 1,
              "kFamily is the order of family");
static_assert(PENCERE_VIEW_TYPES_ALIGN_START == 0 && PENCERE_VIEW_TYPES_ALIGN_CENTER == 1 &&
                  PENCERE_VIEW_TYPES_ALIGN_END == 2,
              "kAlign is the order of align");
static_assert(PENCERE_VIEW_TYPES_FIT_CONTAIN == 0 && PENCERE_VIEW_TYPES_FIT_COVER == 1,
              "kFit is the order of fit");
static_assert(PENCERE_VIEW_TYPES_SIDE_BELOW == 0 && PENCERE_VIEW_TYPES_SIDE_ABOVE == 1 &&
                  PENCERE_VIEW_TYPES_SIDE_RIGHT == 2 && PENCERE_VIEW_TYPES_SIDE_LEFT == 3,
              "kSide is the order of side");

// --- Colours, lengths, tracks ---------------------------------------------------------------

// The roles that are a name and nothing else, each with the interface's own constant: found by
// name, not by where a constant stands among the others, so that a role added to the interface
// in the middle of its list moves nothing here.
struct NamedRole {
    const char* name;
    uint8_t tag;
};

const NamedRole kRoles[] = {
    {"background", PENCERE_VIEW_TYPES_ROLE_BACKGROUND},
    {"foreground", PENCERE_VIEW_TYPES_ROLE_FOREGROUND},
    {"dim", PENCERE_VIEW_TYPES_ROLE_DIM},
    {"accent", PENCERE_VIEW_TYPES_ROLE_ACCENT},
    {"cursor", PENCERE_VIEW_TYPES_ROLE_CURSOR},
    {"success", PENCERE_VIEW_TYPES_ROLE_SUCCESS},
    {"error", PENCERE_VIEW_TYPES_ROLE_ERROR},
    {"info", PENCERE_VIEW_TYPES_ROLE_INFO},
    {"warning", PENCERE_VIEW_TYPES_ROLE_WARNING},
    {"quiet", PENCERE_VIEW_TYPES_ROLE_QUIET},
    {"border", PENCERE_VIEW_TYPES_ROLE_BORDER},
    {"shadow", PENCERE_VIEW_TYPES_ROLE_SHADOW},
    {"selection", PENCERE_VIEW_TYPES_ROLE_SELECTION},
    // The theme's ANSI palette.
    {"black", PENCERE_VIEW_TYPES_ROLE_BLACK},
    {"red", PENCERE_VIEW_TYPES_ROLE_RED},
    {"green", PENCERE_VIEW_TYPES_ROLE_GREEN},
    {"yellow", PENCERE_VIEW_TYPES_ROLE_YELLOW},
    {"blue", PENCERE_VIEW_TYPES_ROLE_BLUE},
    {"magenta", PENCERE_VIEW_TYPES_ROLE_MAGENTA},
    {"cyan", PENCERE_VIEW_TYPES_ROLE_CYAN},
    {"white", PENCERE_VIEW_TYPES_ROLE_WHITE},
    {"bright-black", PENCERE_VIEW_TYPES_ROLE_BRIGHT_BLACK},
    {"bright-red", PENCERE_VIEW_TYPES_ROLE_BRIGHT_RED},
    {"bright-green", PENCERE_VIEW_TYPES_ROLE_BRIGHT_GREEN},
    {"bright-yellow", PENCERE_VIEW_TYPES_ROLE_BRIGHT_YELLOW},
    {"bright-blue", PENCERE_VIEW_TYPES_ROLE_BRIGHT_BLUE},
    {"bright-magenta", PENCERE_VIEW_TYPES_ROLE_BRIGHT_MAGENTA},
    {"bright-cyan", PENCERE_VIEW_TYPES_ROLE_BRIGHT_CYAN},
    {"bright-white", PENCERE_VIEW_TYPES_ROLE_BRIGHT_WHITE},
};

pencere_view_types_role_t role_of(lua_State* L, const char* given) {
    pencere_view_types_role_t role = {};
    for (const NamedRole& named : kRoles) {
        if (strcmp(given, named.name) == 0) {
            role.tag = named.tag;
            return role;
        }
    }
    if (strncmp(given, "raised", 6) == 0) {
        const char* digits = given + 6;
        int steps = 0;
        bool ok = *digits != 0;
        for (const char* d = digits; *d; d++) {
            if (*d < '0' || *d > '9' || steps > 25) {
                ok = false;
                break;
            }
            steps = steps * 10 + (*d - '0');
        }
        if (ok && steps <= 255) {
            role.tag = PENCERE_VIEW_TYPES_ROLE_RAISED;
            role.val.raised = (uint8_t)steps;
            return role;
        }
    }
    luaL_error(L, "\"%s\" is not the name of a role", given);
}

int hex_digit(char c) {
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'f') return c - 'a' + 10;
    if (c >= 'A' && c <= 'F') return c - 'A' + 10;
    return -1;
}

// A colour value: "#rrggbb" or "#rrggbbaa", any case.
pencere_view_types_rgba_t rgba_of(lua_State* L, const char* given) {
    size_t length = strlen(given);
    bool ok = given[0] == '#' && (length == 7 || length == 9);
    uint8_t channel[4] = {0, 0, 0, 255};
    for (size_t at = 0; ok && at * 2 + 1 < length - 1; at++) {
        int high = hex_digit(given[1 + at * 2]);
        int low = hex_digit(given[2 + at * 2]);
        if (high < 0 || low < 0) ok = false;
        else channel[at] = (uint8_t)(high * 16 + low);
    }
    if (!ok) luaL_error(L, "\"%s\" is not a colour value (\"#rrggbb\" or \"#rrggbbaa\")", given);
    pencere_view_types_rgba_t out = {};
    out.r = channel[0];
    out.g = channel[1];
    out.b = channel[2];
    out.a = channel[3];
    return out;
}

// What a string names: a value if it starts with '#', else a role.
pencere_view_types_source_t source_of(lua_State* L, const char* given) {
    pencere_view_types_source_t source = {};
    if (given[0] == '#') {
        source.tag = PENCERE_VIEW_TYPES_SOURCE_RGBA;
        source.val.rgba.light = rgba_of(L, given);
    } else {
        source.tag = PENCERE_VIEW_TYPES_SOURCE_ROLE;
        source.val.role = role_of(L, given);
    }
    return source;
}

// A value for each brightness of the theme: the table `{ light = "#...", dark = "#..." }`, the dark
// one optional, at the absolute stack index `t`.
pencere_view_types_source_t forms_at(lua_State* L, int t) {
    pencere_view_types_source_t source = {};
    source.tag = PENCERE_VIEW_TYPES_SOURCE_RGBA;
    lua_rawgetfield(L, t, "light");
    if (lua_type(L, -1) != LUA_TSTRING) luaL_error(L, "light must be a colour value (\"#rrggbb\" or \"#rrggbbaa\")");
    source.val.rgba.light = rgba_of(L, lua_tostring(L, -1));
    lua_pop(L, 1);
    lua_rawgetfield(L, t, "dark");
    if (!lua_isnil(L, -1)) {
        if (lua_type(L, -1) != LUA_TSTRING) luaL_error(L, "dark must be a colour value (\"#rrggbb\" or \"#rrggbbaa\")");
        source.val.rgba.dark.is_some = true;
        source.val.rgba.dark.val = rgba_of(L, lua_tostring(L, -1));
    }
    lua_pop(L, 1);
    return source;
}

const char kColourIs[] =
    "a colour is a role's name, a \"#rrggbb\" or \"#rrggbbaa\" value, { light = value, dark = value }, "
    "one of those taken toward another (pencere.mix), or any of those with an opacity: { colour, alpha }";

// What a colour is taken toward (`pencere.mix`): a role's name, a value, or a value for each
// brightness, at the absolute stack index `t`.
pencere_view_types_source_t toward_at(lua_State* L, int t) {
    int kind = lua_type(L, t);
    if (kind == LUA_TSTRING) return source_of(L, lua_tostring(L, t));
    if (kind == LUA_TTABLE) {
        lua_rawgetfield(L, t, "light");
        bool forms = !lua_isnil(L, -1);
        lua_pop(L, 1);
        if (forms) return forms_at(L, t);
    }
    luaL_error(L, "a colour is taken toward a role's name, a value or { light = value, dark = value }");
}

// A colour: a role's name, a value, a value for each brightness, or any of those with an
// opacity: `{colour, alpha}`.
pencere_view_types_color_t color_at(lua_State* L, int index) {
    pencere_view_types_color_t color = {};
    color.alpha = 1.0f;
    int kind = lua_type(L, index);
    if (kind == LUA_TSTRING) {
        color.source = source_of(L, lua_tostring(L, index));
    } else if (kind == LUA_TTABLE) {
        lua_rawgeti(L, index, 1);
        int first = lua_type(L, -1);
        if (first == LUA_TSTRING) {
            color.source = source_of(L, lua_tostring(L, -1));
        } else if (first == LUA_TTABLE) {
            color.source = forms_at(L, lua_gettop(L));
        } else if (first == LUA_TNIL) {
            lua_rawgetfield(L, index, "light");
            bool forms = !lua_isnil(L, -1);
            lua_pop(L, 1);
            if (!forms) luaL_error(L, "%s", kColourIs);
            color.source = forms_at(L, index);
        } else {
            luaL_error(L, "%s", kColourIs);
        }
        lua_pop(L, 1);
        lua_rawgeti(L, index, 2);
        if (lua_type(L, -1) == LUA_TNUMBER) color.alpha = (float)lua_tonumber(L, -1);
        lua_pop(L, 1);
        lua_rawgetfield(L, index, "toward");
        if (!lua_isnil(L, -1)) {
            color.mix.is_some = true;
            color.mix.val.toward = toward_at(L, lua_gettop(L));
            color.mix.val.amount = number(L, index, "amount", 0.0f);
        }
        lua_pop(L, 1);
    } else {
        luaL_error(L, "%s", kColourIs);
    }
    return color;
}

pencere_view_types_option_color_t optional_color(lua_State* L, int t, const char* name) {
    pencere_view_types_option_color_t out = {};
    get(L, t, name);
    if (!lua_isnil(L, -1)) {
        out.is_some = true;
        out.val = color_at(L, lua_gettop(L));
    }
    lua_pop(L, 1);
    return out;
}

pencere_view_types_length_t length_of(lua_State* L, int t, const char* name) {
    pencere_view_types_length_t out = {};
    get(L, t, name);
    int kind = lua_type(L, -1);
    if (kind == LUA_TNUMBER) {
        out.tag = PENCERE_VIEW_TYPES_LENGTH_PX;
        out.val.px = (float)lua_tonumber(L, -1);
    } else if (kind == LUA_TTABLE) {
        int table = lua_gettop(L);
        out.tag = PENCERE_VIEW_TYPES_LENGTH_PERCENT;
        out.val.percent = number(L, table, "percent", 0.0f);
    } else if (kind != LUA_TNIL) {
        luaL_error(L, "%s is a number of points, {percent = n}, or nothing", name);
    }
    lua_pop(L, 1);
    return out;
}

// The tracks at `name`, as a part of the frame's list: numbers are points, `{fr = n}`,
// `{min = n, fr = n}` and "auto".
pencere_view_types_slice_t tracks_of(lua_State* L, int t, const char* name, Arenas& arenas) {
    pencere_view_types_slice_t out = part(0, 0);
    get(L, t, name);
    if (lua_isnil(L, -1)) {
        lua_pop(L, 1);
        return out;
    }
    if (lua_type(L, -1) != LUA_TTABLE) luaL_error(L, "%s is a list of tracks", name);
    int list = lua_gettop(L);
    size_t count = (size_t)lua_objlen(L, list);
    if (count > kMaxList) luaL_error(L, "%s has more tracks than a grid may", name);
    size_t start = arenas.tracks.size();
    for (size_t at = 0; at < count; at++) {
        pencere_view_types_track_t track = {};
        lua_rawgeti(L, list, (int)at + 1);
        int kind = lua_type(L, -1);
        if (kind == LUA_TNUMBER) {
            track.tag = PENCERE_VIEW_TYPES_TRACK_PX;
            track.val.px = (float)lua_tonumber(L, -1);
        } else if (kind == LUA_TTABLE) {
            int table = lua_gettop(L);
            lua_rawgetfield(L, table, "min");
            bool ranged = !lua_isnil(L, -1);
            lua_pop(L, 1);
            if (ranged) {
                track.tag = PENCERE_VIEW_TYPES_TRACK_RANGE;
                track.val.range.min = number(L, table, "min", 0.0f);
                track.val.range.fr = number(L, table, "fr", 1.0f);
            } else {
                track.tag = PENCERE_VIEW_TYPES_TRACK_FR;
                track.val.fr = number(L, table, "fr", 1.0f);
            }
        } else if (kind == LUA_TSTRING && strcmp(lua_tostring(L, -1), "auto") == 0) {
            track.tag = PENCERE_VIEW_TYPES_TRACK_AUTO;
        } else {
            luaL_error(L, "a track is points, {fr = n}, {min = n, fr = n} or \"auto\"");
        }
        lua_pop(L, 1);
        arenas.tracks.push_back(track);
    }
    lua_pop(L, 1);
    return part(start, count);
}

// Four numbers of the list at `name` (top, right, bottom, left, or the corners in order).
void four(lua_State* L, int t, const char* name, float out[4]) {
    out[0] = out[1] = out[2] = out[3] = 0.0f;
    get(L, t, name);
    if (lua_type(L, -1) == LUA_TTABLE) {
        int list = lua_gettop(L);
        for (int at = 0; at < 4; at++) {
            lua_rawgeti(L, list, at + 1);
            if (lua_type(L, -1) == LUA_TNUMBER) out[at] = (float)lua_tonumber(L, -1);
            lua_pop(L, 1);
        }
    } else if (!lua_isnil(L, -1)) {
        luaL_error(L, "%s is four numbers", name);
    }
    lua_pop(L, 1);
}

void read_style(lua_State* L, int s, pencere_view_types_style_t& out, Arenas& arenas) {
    out.display = (uint8_t)word(L, s, "display", kDisplay, 3, 0);
    out.direction = (uint8_t)word(L, s, "direction", kDirection, 2, PENCERE_VIEW_TYPES_DIRECTION_COLUMN);
    out.wrap = flag(L, s, "wrap");
    out.absolute = flag(L, s, "absolute");
    out.overlay = flag(L, s, "overlay");
    // Beside an element: { to = name, side = "below", align = "start", gap = n }.
    get(L, s, "anchor");
    if (lua_type(L, -1) == LUA_TTABLE) {
        int anchor = lua_gettop(L);
        out.anchor.is_some = true;
        get(L, anchor, "to");
        if (lua_isnil(L, -1)) luaL_error(L, "an anchor names the element it is beside: anchor(id, side)");
        out.anchor.val.to = id_of(L, lua_gettop(L));
        lua_pop(L, 1);
        out.anchor.val.side = (uint8_t)word(L, anchor, "side", kSide, 4, 0);
        out.anchor.val.align = (uint8_t)word(L, anchor, "align", kAlign, 3, 0);
        out.anchor.val.gap = number(L, anchor, "gap", 0.0f);
    }
    lua_pop(L, 1);
    get(L, s, "inset");
    if (lua_type(L, -1) == LUA_TTABLE) {
        int inset = lua_gettop(L);
        out.inset.top = optional_number(L, inset, "top");
        out.inset.right = optional_number(L, inset, "right");
        out.inset.bottom = optional_number(L, inset, "bottom");
        out.inset.left = optional_number(L, inset, "left");
    }
    lua_pop(L, 1);
    out.width = length_of(L, s, "width");
    out.height = length_of(L, s, "height");
    out.min_width = length_of(L, s, "min_width");
    out.min_height = length_of(L, s, "min_height");
    out.max_width = length_of(L, s, "max_width");
    out.max_height = length_of(L, s, "max_height");
    float edges[4];
    four(L, s, "padding", edges);
    out.padding = {edges[0], edges[1], edges[2], edges[3]};
    four(L, s, "margin", edges);
    out.margin = {edges[0], edges[1], edges[2], edges[3]};
    out.gap_x = number(L, s, "gap_x", 0.0f);
    out.gap_y = number(L, s, "gap_y", 0.0f);
    out.grow = number(L, s, "grow", 0.0f);
    out.shrink = optional_number(L, s, "shrink");
    int items = word(L, s, "items", kItems, 4, -1);
    if (items >= 0) out.items = {true, (uint8_t)items};
    int justify = word(L, s, "justify", kJustify, 4, -1);
    if (justify >= 0) out.justify = {true, (uint8_t)justify};
    out.columns = tracks_of(L, s, "columns", arenas);
    out.rows = tracks_of(L, s, "rows", arenas);
    float span = number(L, s, "column_span", 0.0f);
    out.column_span = span < 0 ? 0 : (span > 65535 ? 65535 : (uint16_t)span);
    out.overflow = (uint8_t)word(L, s, "overflow", kOverflow, 3, 0);
    out.fill = optional_color(L, s, "fill");
    // A fill from one colour to another: { angle = degrees, from = colour, to = colour }.
    get(L, s, "gradient");
    if (lua_type(L, -1) == LUA_TTABLE) {
        int gradient = lua_gettop(L);
        out.gradient.is_some = true;
        out.gradient.val.angle = number(L, gradient, "angle", 180.0f);
        lua_rawgetfield(L, gradient, "from");
        out.gradient.val.start = color_at(L, lua_gettop(L));
        lua_pop(L, 1);
        lua_rawgetfield(L, gradient, "to");
        out.gradient.val.end = color_at(L, lua_gettop(L));
        lua_pop(L, 1);
    }
    lua_pop(L, 1);
    get(L, s, "border");
    if (lua_type(L, -1) == LUA_TTABLE) {
        int border = lua_gettop(L);
        out.border.is_some = true;
        out.border.val.width = number(L, border, "width", 0.0f);
        lua_rawgetfield(L, border, "color");
        out.border.val.color = color_at(L, lua_gettop(L));
        lua_pop(L, 1);
    }
    lua_pop(L, 1);
    four(L, s, "radii", edges);
    out.radii = {edges[0], edges[1], edges[2], edges[3]};
    get(L, s, "shadow");
    if (lua_type(L, -1) == LUA_TTABLE) {
        int shadow = lua_gettop(L);
        out.shadow.is_some = true;
        out.shadow.val.dx = number(L, shadow, "dx", 0.0f);
        out.shadow.val.dy = number(L, shadow, "dy", 0.0f);
        out.shadow.val.blur = number(L, shadow, "blur", 0.0f);
        lua_rawgetfield(L, shadow, "color");
        out.shadow.val.color = color_at(L, lua_gettop(L));
        lua_pop(L, 1);
    }
    lua_pop(L, 1);
    out.opacity = number(L, s, "opacity", 1.0f);
    get(L, s, "text");
    if (lua_type(L, -1) == LUA_TTABLE) {
        int text = lua_gettop(L);
        int family = word(L, text, "family", kFamily, 2, -1);
        if (family >= 0) out.text.family = {true, (uint8_t)family};
        out.text.size = optional_number(L, text, "size");
        view_option_f32_t weight = optional_number(L, text, "weight");
        if (weight.is_some) {
            out.text.weight.is_some = true;
            out.text.weight.val = weight.val < 0 ? 0 : (weight.val > 65535 ? 65535 : (uint16_t)weight.val);
        }
        out.text.italic = optional_flag(L, text, "italic");
        out.text.underline = optional_flag(L, text, "underline");
        out.text.strike = optional_flag(L, text, "strike");
        out.text.color = optional_color(L, text, "color");
        int align = word(L, text, "align", kAlign, 3, -1);
        if (align >= 0) out.text.align = {true, (uint8_t)align};
    }
    lua_pop(L, 1);
    out.truncate = flag(L, s, "truncate");
}

// --- The tree --------------------------------------------------------------------------------

struct Flat {
    pencere_view_types_frame_t* frame;
    size_t cap;
    Arenas arenas;
};

static_assert(PENCERE_VIEW_TYPES_EASE_LINEAR == 0 && PENCERE_VIEW_TYPES_EASE_OUT == 1 &&
                  PENCERE_VIEW_TYPES_EASE_IN_OUT == 2 && PENCERE_VIEW_TYPES_EASE_SPRING == 3,
              "the eases are in the order of `kEases`");

// Where an element comes in from or goes to: `{ opacity =, dx =, dy =, seconds = }`, absent in
// `t[name]` when it is not a table.
pencere_view_types_option_shift_t optional_shift(lua_State* L, int t, const char* name) {
    pencere_view_types_option_shift_t out = {};
    get(L, t, name);
    if (lua_type(L, -1) == LUA_TTABLE) {
        int shift = lua_gettop(L);
        out.is_some = true;
        out.val.opacity = number(L, shift, "opacity", 0.0f);
        out.val.dx = number(L, shift, "dx", 0.0f);
        out.val.dy = number(L, shift, "dy", 0.0f);
        out.val.seconds = number(L, shift, "seconds", 0.2f);
    }
    lua_pop(L, 1);
    return out;
}

// How an element moves: `{ seconds =, ease =, color =, opacity =, size =, place =, enter =,
// exit = }` at `index`.
pencere_view_types_motion_t motion_at(lua_State* L, int index) {
    static const char* const kEases[] = {"linear", "out", "in-out", "spring"};
    pencere_view_types_motion_t motion = {};
    motion.seconds = number(L, index, "seconds", 0.2f);
    motion.ease = (pencere_view_types_ease_t)word(L, index, "ease", kEases, 4, PENCERE_VIEW_TYPES_EASE_OUT);
    motion.color = flag(L, index, "color");
    motion.opacity = flag(L, index, "opacity");
    motion.size = flag(L, index, "size");
    motion.place = flag(L, index, "place");
    motion.enter = optional_shift(L, index, "enter");
    motion.exit = optional_shift(L, index, "exit");
    motion.spin = optional_number(L, index, "spin");
    return motion;
}

// The words a string at `name` of the table `t` holds, as a part of the frame's text; nothing for
// what is not a string.
pencere_view_types_slice_t words_at(lua_State* L, int t, const char* name, Arenas& arenas) {
    get(L, t, name);
    size_t length = 0;
    const char* bytes = lua_type(L, -1) == LUA_TSTRING ? lua_tolstring(L, -1, &length) : "";
    pencere_view_types_slice_t out = add_words(arenas, bytes, length);
    lua_pop(L, 1);
    return out;
}

// The parts of a text of many looks at `list`: tables of `text` and what each changes.
pencere_view_types_slice_t spans_at(lua_State* L, int list, Arenas& arenas) {
    size_t count = (size_t)lua_objlen(L, list);
    if (count > kMaxList) luaL_error(L, "a text has more parts than it may");
    std::vector<pencere_view_types_span_t> made;
    made.reserve(count);
    for (size_t at = 0; at < count; at++) {
        lua_rawgeti(L, list, (int)at + 1);
        if (lua_type(L, -1) != LUA_TTABLE) luaL_error(L, "a part of a text is made by pencere.span");
        int span = lua_gettop(L);
        pencere_view_types_span_t out = {};
        out.text = words_at(L, span, "text", arenas);
        out.color = optional_color(L, span, "color");
        view_option_f32_t weight = optional_number(L, span, "weight");
        if (weight.is_some) {
            out.weight.is_some = true;
            out.weight.val = weight.val < 0 ? 0 : (weight.val > 65535 ? 65535 : (uint16_t)weight.val);
        }
        out.italic = optional_flag(L, span, "italic");
        out.underline = optional_flag(L, span, "underline");
        out.strike = optional_flag(L, span, "strike");
        int family = word(L, span, "family", kFamily, 2, -1);
        if (family >= 0) out.family = {true, (uint8_t)family};
        out.background = optional_color(L, span, "background");
        get(L, span, "link");
        if (!lua_isnil(L, -1)) {
            out.link.is_some = true;
            out.link.val = id_of(L, lua_gettop(L));
        }
        lua_pop(L, 1);
        made.push_back(out);
        lua_pop(L, 1);
    }
    size_t start = arenas.spans.size();
    arenas.spans.insert(arenas.spans.end(), made.begin(), made.end());
    return part(start, count);
}

// The point at the places `from` and `from + 1` of the step at `step`.
pencere_view_types_point_t point_at(lua_State* L, int step, int from) {
    pencere_view_types_point_t out = {};
    lua_rawgeti(L, step, from);
    out.x = (float)lua_tonumber(L, -1);
    lua_pop(L, 1);
    lua_rawgeti(L, step, from + 1);
    out.y = (float)lua_tonumber(L, -1);
    lua_pop(L, 1);
    return out;
}

// The paths of a canvas at `list`: each `{ segments = { {"move", x, y}, {"line", x, y},
// {"quad", cx, cy, x, y}, {"cubic", ax, ay, bx, by, x, y}, {"close"} }, fill =, stroke = }`.
pencere_view_types_slice_t paths_at(lua_State* L, int list, Arenas& arenas) {
    static const char* const kSteps[] = {"move", "line", "quad", "cubic", "close"};
    size_t count = (size_t)lua_objlen(L, list);
    if (count > kMaxList) luaL_error(L, "a canvas has more paths than it may");
    std::vector<pencere_view_types_path_t> made;
    made.reserve(count);
    for (size_t at = 0; at < count; at++) {
        lua_rawgeti(L, list, (int)at + 1);
        if (lua_type(L, -1) != LUA_TTABLE) luaL_error(L, "a path of a canvas is made by pencere.path");
        int path = lua_gettop(L);
        pencere_view_types_path_t out = {};
        get(L, path, "segments");
        if (lua_type(L, -1) == LUA_TTABLE) {
            int steps = lua_gettop(L);
            size_t many = (size_t)lua_objlen(L, steps);
            if (many > kMaxList * 4) luaL_error(L, "a path has more steps than it may");
            size_t start = arenas.segments.size();
            for (size_t k = 0; k < many; k++) {
                lua_rawgeti(L, steps, (int)k + 1);
                int step = lua_gettop(L);
                if (lua_type(L, step) != LUA_TTABLE) luaL_error(L, "a step of a path is a table");
                lua_rawgeti(L, step, 1);
                const char* name = lua_type(L, -1) == LUA_TSTRING ? lua_tostring(L, -1) : "";
                int kind = -1;
                for (int w = 0; w < 5; w++) {
                    if (strcmp(name, kSteps[w]) == 0) kind = w;
                }
                lua_pop(L, 1);
                pencere_view_types_segment_t segment = {};
                switch (kind) {
                    case 0:
                        segment.tag = PENCERE_VIEW_TYPES_SEGMENT_MOVE_TO;
                        segment.val.move_to = point_at(L, step, 2);
                        break;
                    case 1:
                        segment.tag = PENCERE_VIEW_TYPES_SEGMENT_LINE_TO;
                        segment.val.line_to = point_at(L, step, 2);
                        break;
                    case 2:
                        segment.tag = PENCERE_VIEW_TYPES_SEGMENT_QUAD_TO;
                        segment.val.quad_to.control = point_at(L, step, 2);
                        segment.val.quad_to.to = point_at(L, step, 4);
                        break;
                    case 3:
                        segment.tag = PENCERE_VIEW_TYPES_SEGMENT_CUBIC_TO;
                        segment.val.cubic_to.a = point_at(L, step, 2);
                        segment.val.cubic_to.b = point_at(L, step, 4);
                        segment.val.cubic_to.to = point_at(L, step, 6);
                        break;
                    case 4:
                        segment.tag = PENCERE_VIEW_TYPES_SEGMENT_CLOSE;
                        break;
                    default:
                        luaL_error(L, "a step of a path is \"move\", \"line\", \"quad\", \"cubic\" or \"close\", not \"%s\"", name);
                }
                arenas.segments.push_back(segment);
                lua_pop(L, 1);
            }
            out.segments = part(start, many);
        }
        lua_pop(L, 1);
        out.fill = optional_color(L, path, "fill");
        get(L, path, "stroke");
        if (lua_type(L, -1) == LUA_TTABLE) {
            int stroke = lua_gettop(L);
            out.stroke.is_some = true;
            out.stroke.val.width = number(L, stroke, "width", 1.0f);
            lua_rawgetfield(L, stroke, "color");
            out.stroke.val.color = color_at(L, lua_gettop(L));
            lua_pop(L, 1);
        }
        lua_pop(L, 1);
        made.push_back(out);
        lua_pop(L, 1);
    }
    size_t start = arenas.paths.size();
    arenas.paths.insert(arenas.paths.end(), made.begin(), made.end());
    return part(start, count);
}

// The names of the keys the page reads, in the order of the interface's `key` (the character and
// the function keys apart).
const char* const kKeys[] = {"up",  "down",  "left",   "right", "page_up",   "page_down", "home",
                             "end", "enter", "escape", "tab",   "backspace", "delete"};
static_assert(PENCERE_VIEW_TYPES_KEY_UP == 0 && PENCERE_VIEW_TYPES_KEY_DELETE == 12 &&
                  PENCERE_VIEW_TYPES_KEY_FUNCTION == 13 && PENCERE_VIEW_TYPES_KEY_CHARACTER == 14,
              "kKeys is the order of key");

// The first character of `bytes` (valid UTF-8), as a code point.
uint32_t first_character(const std::string& one) {
    uint32_t cp = (unsigned char)one[0];
    size_t extra = cp >= 0xF0 ? 3 : cp >= 0xE0 ? 2 : cp >= 0xC0 ? 1 : 0;
    cp = extra == 0 ? cp : (cp & (0x3F >> extra));
    for (size_t k = 1; k <= extra && k < one.size(); k++) cp = (cp << 6) | ((unsigned char)one[k] & 0x3F);
    return cp;
}

// A key by its name: one of `kKeys`, "f1" to "f20", or one character.
pencere_view_types_key_t key_named(lua_State* L, const char* name, size_t length) {
    pencere_view_types_key_t key = {};
    for (int at = 0; at < 13; at++) {
        if (strcmp(name, kKeys[at]) == 0) {
            key.tag = (uint8_t)at;
            return key;
        }
    }
    if (name[0] == 'f' && length >= 2 && length <= 3) {
        int number = atoi(name + 1);
        if (number >= 1 && number <= 20) {
            key.tag = PENCERE_VIEW_TYPES_KEY_FUNCTION;
            key.val.function = (uint8_t)number;
            return key;
        }
    }
    std::string one = repaired(name, length);
    // One character, of however many bytes.
    if (!one.empty()) {
        uint32_t cp = first_character(one);
        size_t first = cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
        if (first == one.size()) {
            key.tag = PENCERE_VIEW_TYPES_KEY_CHARACTER;
            key.val.character = cp;
            return key;
        }
    }
    luaL_error(L, "\"%s\" is not a key", name);
}

// Keys that do something, at `keys`: `{ key =, mods = { shift =, control =, alt =, command = } }`,
// as `pencere.keys` reads them.
pencere_view_types_shortcut_t shortcut_at(lua_State* L, int keys) {
    pencere_view_types_shortcut_t out = {};
    get(L, keys, "key");
    size_t length = 0;
    const char* name = lua_type(L, -1) == LUA_TSTRING ? lua_tolstring(L, -1, &length) : "";
    out.key = key_named(L, name, length);
    lua_pop(L, 1);
    get(L, keys, "mods");
    if (lua_type(L, -1) == LUA_TTABLE) {
        int mods = lua_gettop(L);
        out.mods.shift = flag(L, mods, "shift");
        out.mods.control = flag(L, mods, "control");
        out.mods.alt = flag(L, mods, "alt");
        out.mods.command = flag(L, mods, "command");
    }
    lua_pop(L, 1);
    return out;
}

// The keys the words at `words` say (`"ctrl+w"`), read as a command's are, by the prelude's
// `pencere.keys`: one reader of keys, whose errors name what is not one.
pencere_view_types_shortcut_t shortcut_of_words(lua_State* L, int words) {
    lua_getglobal(L, "pencere");
    lua_rawgetfield(L, -1, "keys");
    lua_remove(L, -2);
    lua_pushvalue(L, words);
    lua_call(L, 1, 1);
    pencere_view_types_shortcut_t out = shortcut_at(L, lua_gettop(L));
    lua_pop(L, 1);
    return out;
}

// The keys a menu's entry wrote as `key`, the name it had: read as keys where they are, and where
// they are not, its first character as a page that wrote them before was shown (a page that says
// nothing new is read as it was, and its reply is not failed for words the kit now reads).
pencere_view_types_shortcut_t shortcut_or_character(lua_State* L, int words) {
    lua_getglobal(L, "pencere");
    lua_rawgetfield(L, -1, "keys");
    lua_remove(L, -2);
    lua_pushvalue(L, words);
    if (lua_pcall(L, 1, 1, 0) == 0) {
        int keys = lua_gettop(L);
        get(L, keys, "key");
        size_t length = 0;
        const char* name = lua_type(L, -1) == LUA_TSTRING ? lua_tolstring(L, -1, &length) : "";
        bool known = false;
        for (int at = 0; at < 13 && !known; at++) known = strcmp(name, kKeys[at]) == 0;
        if (!known && name[0] == 'f' && length >= 2 && length <= 3) {
            int number = atoi(name + 1);
            known = number >= 1 && number <= 20;
        }
        if (!known) {
            std::string one = repaired(name, length);
            uint32_t cp = one.empty() ? 0 : first_character(one);
            size_t first = cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
            known = !one.empty() && first == one.size();
        }
        lua_pop(L, 1);
        if (known) {
            pencere_view_types_shortcut_t out = shortcut_at(L, keys);
            lua_pop(L, 1);
            return out;
        }
    }
    lua_pop(L, 1);
    size_t length = 0;
    const char* bytes = lua_tolstring(L, words, &length);
    pencere_view_types_shortcut_t out = {};
    out.key.tag = PENCERE_VIEW_TYPES_KEY_CHARACTER;
    out.key.val.character = first_character(repaired(bytes, length));
    return out;
}

// The commands of an element at `list`: `{ id =, title =, keys = { { key =, mods = {...} }, ... },
// enabled = }`.
pencere_view_types_slice_t commands_at(lua_State* L, int list, Arenas& arenas) {
    size_t count = (size_t)lua_objlen(L, list);
    if (count > kMaxList) luaL_error(L, "an element has more commands than it may");
    std::vector<pencere_view_types_command_t> made;
    made.reserve(count);
    for (size_t at = 0; at < count; at++) {
        lua_rawgeti(L, list, (int)at + 1);
        if (lua_type(L, -1) != LUA_TTABLE) luaL_error(L, "a command is made by :command(id, title, keys)");
        int command = lua_gettop(L);
        pencere_view_types_command_t out = {};
        get(L, command, "id");
        if (lua_isnil(L, -1)) luaL_error(L, "a command is named: :command(id, title, keys)");
        out.id = id_of(L, lua_gettop(L));
        lua_pop(L, 1);
        out.title = words_at(L, command, "title", arenas);
        get(L, command, "enabled");
        out.enabled = lua_isnil(L, -1) ? true : lua_toboolean(L, -1) != 0;
        lua_pop(L, 1);
        // The keys that do it, the first the ones a list shows (the prelude makes the list).
        get(L, command, "keys");
        if (lua_type(L, -1) == LUA_TTABLE) {
            int keys = lua_gettop(L);
            size_t many = (size_t)lua_objlen(L, keys);
            if (many > kMaxList) luaL_error(L, "a command has more keys than it may");
            if (many) {
                out.keys.ptr = (pencere_view_types_shortcut_t*)xmalloc(many * sizeof(pencere_view_types_shortcut_t));
                memset(out.keys.ptr, 0, many * sizeof(pencere_view_types_shortcut_t));
                out.keys.len = many;
            }
            for (size_t k = 0; k < many; k++) {
                lua_rawgeti(L, keys, (int)k + 1);
                if (lua_type(L, -1) != LUA_TTABLE) luaL_error(L, "the keys of a command are written as words: \"cmd+k\"");
                out.keys.ptr[k] = shortcut_at(L, lua_gettop(L));
                lua_pop(L, 1);
            }
        }
        lua_pop(L, 1);
        made.push_back(out);
        lua_pop(L, 1);
    }
    size_t start = arenas.commands.size();
    arenas.commands.insert(arenas.commands.end(), made.begin(), made.end());
    return part(start, count);
}

// What an element is to a screen reader, by its word, and the level of a heading.
const char* const kAccessibleRoles[] = {"button", "text", "link", "checkbox", "switch", "radio",
                                "tab",    "menu_item", "heading", "image", "progress", "slider"};
static_assert(PENCERE_VIEW_TYPES_ACCESSIBLE_ROLE_BUTTON == 0 && PENCERE_VIEW_TYPES_ACCESSIBLE_ROLE_MENU_ITEM == 7 &&
                  PENCERE_VIEW_TYPES_ACCESSIBLE_ROLE_HEADING == 8 && PENCERE_VIEW_TYPES_ACCESSIBLE_ROLE_SLIDER == 11,
              "kAccessibleRoles is the order of accessible-role");

// --- An editor ------------------------------------------------------------------------------

// A colour of an editor's, `name` in the table at `t`: not said, the theme's; `false`, none (it is
// not drawn); anything else a colour.
pencere_view_types_editor_tint_t tint_at(lua_State* L, int t, const char* name) {
    pencere_view_types_editor_tint_t out = {};
    out.tag = PENCERE_VIEW_TYPES_EDITOR_TINT_THEME;
    get(L, t, name);
    int kind = lua_type(L, -1);
    if (kind == LUA_TBOOLEAN && !lua_toboolean(L, -1)) {
        out.tag = PENCERE_VIEW_TYPES_EDITOR_TINT_OFF;
    } else if (kind != LUA_TNIL) {
        out.tag = PENCERE_VIEW_TYPES_EDITOR_TINT_COLOR;
        out.val.color = color_at(L, lua_gettop(L));
    }
    lua_pop(L, 1);
    return out;
}

// A caret's `motion`: "snap", "ease" or "spring" (a terminal's numbers), `{ ease = seconds }` or
// `{ spring = stiffness }`; `false` is none, as "snap".
pencere_view_types_caret_motion_t motion_at(lua_State* L, int t, const char* name) {
    pencere_view_types_caret_motion_t out = {};
    out.tag = PENCERE_VIEW_TYPES_CARET_MOTION_SNAP;
    get(L, t, name);
    int kind = lua_type(L, -1);
    if (kind == LUA_TSTRING) {
        const char* given = lua_tostring(L, -1);
        if (strcmp(given, "snap") == 0) {
        } else if (strcmp(given, "ease") == 0) {
            out.tag = PENCERE_VIEW_TYPES_CARET_MOTION_EASE;
        } else if (strcmp(given, "spring") == 0) {
            out.tag = PENCERE_VIEW_TYPES_CARET_MOTION_SPRING;
        } else {
            luaL_error(L, "%s: \"%s\" is not one of the words it takes: \"snap\", \"ease\", \"spring\"", name, given);
        }
    } else if (kind == LUA_TTABLE) {
        int table = lua_gettop(L);
        view_option_f32_t ease = optional_number(L, table, "ease");
        view_option_f32_t spring = optional_number(L, table, "spring");
        if (ease.is_some == spring.is_some) {
            luaL_error(L, "%s is one of { ease = seconds } and { spring = stiffness }", name);
        }
        if (ease.is_some) {
            out.tag = PENCERE_VIEW_TYPES_CARET_MOTION_EASE;
            out.val.ease = ease;
        } else {
            out.tag = PENCERE_VIEW_TYPES_CARET_MOTION_SPRING;
            out.val.spring = spring;
        }
    } else if (kind != LUA_TNIL && !(kind == LUA_TBOOLEAN && !lua_toboolean(L, -1))) {
        luaL_error(L, "%s is \"snap\", \"ease\", \"spring\", { ease = seconds } or { spring = stiffness }", name);
    }
    lua_pop(L, 1);
    return out;
}

const char* const kCaretShape[] = {"bar", "block", "underline"};
const char* const kCaretHeight[] = {"row", "text"};
const char* const kCaretUnfocused[] = {"hidden", "hollow", "solid"};
const char* const kLineNumbers[] = {"absolute", "relative", "hidden"};
const char* const kLineHighlight[] = {"text", "gutter", "all"};
static_assert(PENCERE_VIEW_TYPES_CARET_SHAPE_BAR == 0 && PENCERE_VIEW_TYPES_CARET_SHAPE_BLOCK == 1 &&
                  PENCERE_VIEW_TYPES_CARET_SHAPE_UNDERLINE == 2,
              "kCaretShape is the order of caret-shape");
static_assert(PENCERE_VIEW_TYPES_CARET_HEIGHT_ROW == 0 && PENCERE_VIEW_TYPES_CARET_HEIGHT_TEXT == 1,
              "kCaretHeight is the order of caret-height");
static_assert(PENCERE_VIEW_TYPES_CARET_UNFOCUSED_HIDDEN == 0 && PENCERE_VIEW_TYPES_CARET_UNFOCUSED_HOLLOW == 1 &&
                  PENCERE_VIEW_TYPES_CARET_UNFOCUSED_SOLID == 2,
              "kCaretUnfocused is the order of caret-unfocused");
static_assert(PENCERE_VIEW_TYPES_LINE_NUMBERS_ABSOLUTE == 0 && PENCERE_VIEW_TYPES_LINE_NUMBERS_RELATIVE == 1 &&
                  PENCERE_VIEW_TYPES_LINE_NUMBERS_HIDDEN == 2,
              "kLineNumbers is the order of line-numbers");
static_assert(PENCERE_VIEW_TYPES_LINE_HIGHLIGHT_TEXT == 0 && PENCERE_VIEW_TYPES_LINE_HIGHLIGHT_GUTTER == 1 &&
                  PENCERE_VIEW_TYPES_LINE_HIGHLIGHT_ALL == 2,
              "kLineHighlight is the order of line-highlight");


// Defined with the strings the host takes, below.
view_string_t string_of(lua_State* L, int t, const char* name);
// Defined with the editor's commands, below.
pencere_view_types_list_gesture_binding_t gestures_at(lua_State* L, int t);
pencere_view_types_list_key_binding_t keymap_at(lua_State* L, int t);
pencere_view_types_option_mods_t held_at(lua_State* L, int t, const char* name);
pencere_view_types_widget_t widget_at(lua_State* L, int t);
pencere_view_types_list_widget_group_t widget_groups_at(lua_State* L, int t);

const char* const kCursorNames[] = {"arrow",           "pointer",     "text", "resize_horizontal",
                                    "resize_vertical", "not_allowed", "grab", "grabbing",
                                    "crosshair"};
const char* const kGutterShapes[] = {"dot", "bar", "above", "below", "image"};
const char* const kRulerLanes[] = {"full", "left", "center", "right"};
static_assert(PENCERE_VIEW_TYPES_GUTTER_SHAPE_DOT == 0 && PENCERE_VIEW_TYPES_GUTTER_SHAPE_BAR == 1 &&
                  PENCERE_VIEW_TYPES_GUTTER_SHAPE_ABOVE == 2 && PENCERE_VIEW_TYPES_GUTTER_SHAPE_BELOW == 3 &&
                  PENCERE_VIEW_TYPES_GUTTER_SHAPE_IMAGE == 4,
              "kGutterShapes is the order of gutter-shape");
static_assert(PENCERE_VIEW_TYPES_RULER_LANE_FULL == 0 && PENCERE_VIEW_TYPES_RULER_LANE_LEFT == 1 &&
                  PENCERE_VIEW_TYPES_RULER_LANE_CENTER == 2 && PENCERE_VIEW_TYPES_RULER_LANE_RIGHT == 3,
              "kRulerLanes is the order of ruler-lane");

// A gutter marker's shape in the table at `t`: `shape = "dot" | "bar" | "above" | "below" | "image"`,
// the picture `image = "assets/x.svg"` (which alone says the shape is an image).
pencere_view_types_gutter_shape_t gutter_shape_at(lua_State* L, int t) {
    pencere_view_types_gutter_shape_t out = {};
    get(L, t, "image");
    bool image = lua_type(L, -1) == LUA_TSTRING;
    lua_pop(L, 1);
    int shape = word(L, t, "shape", kGutterShapes, 5, image ? PENCERE_VIEW_TYPES_GUTTER_SHAPE_IMAGE : 0);
    out.tag = (uint8_t)shape;
    if (shape == PENCERE_VIEW_TYPES_GUTTER_SHAPE_IMAGE) {
        if (!image) luaL_error(L, "a gutter marker that is an image says which: image = \"assets/x.svg\"");
        out.val.image = string_of(L, t, "image");
    }
    return out;
}

// One column of the gutter, at the absolute index `t`: its name ("numbers", "folds", "marks" are
// the editor's), or `{ name =, width =, events = { "press", "menu", "hover" } | false, hover = {
// shape =, image =, color = }, pad = points }`.
pencere_view_types_gutter_column_t gutter_column_at(lua_State* L, int t) {
    pencere_view_types_gutter_column_t out = {};
    if (lua_type(L, t) == LUA_TSTRING) {
        size_t length = 0;
        const char* bytes = lua_tolstring(L, t, &length);
        out.name = host_string(bytes, length);
        return out;
    }
    if (lua_type(L, t) != LUA_TTABLE)
        luaL_error(L, "a gutter column is its name, or a table { name =, width =, events =, hover =, pad = }");
    out.name = string_of(L, t, "name");
    out.width = optional_number(L, t, "width");
    out.pad = optional_number(L, t, "pad");
    get(L, t, "events");
    int events = lua_gettop(L);
    if (lua_type(L, events) == LUA_TTABLE) {
        out.events.is_some = true;
        for (size_t at = 1; at <= (size_t)lua_objlen(L, events); at++) {
            lua_rawgeti(L, events, (int)at);
            const char* said = lua_type(L, -1) == LUA_TSTRING ? lua_tostring(L, -1) : "";
            if (strcmp(said, "press") == 0) out.events.val.press = true;
            else if (strcmp(said, "menu") == 0) out.events.val.menu = true;
            else if (strcmp(said, "hover") == 0) out.events.val.hover = true;
            else luaL_error(L, "a gutter column hears \"press\", \"menu\" and \"hover\", not \"%s\"", said);
            lua_pop(L, 1);
        }
    } else if (lua_type(L, events) == LUA_TBOOLEAN) {
        out.events.is_some = true;
        out.events.val.press = out.events.val.menu = lua_toboolean(L, events) != 0;
    } else if (!lua_isnil(L, events)) {
        luaL_error(L, "the events of a gutter column are a list of words: { \"press\", \"menu\", \"hover\" }");
    }
    lua_pop(L, 1);
    get(L, t, "hover");
    int hover = lua_gettop(L);
    if (lua_type(L, hover) == LUA_TTABLE) {
        out.hover.is_some = true;
        out.hover.val.shape = gutter_shape_at(L, hover);
        pencere_view_types_option_color_t colour = optional_color(L, hover, "color");
        if (!colour.is_some) luaL_error(L, "the hover of a gutter column says its color");
        out.hover.val.color = colour.val;
    } else if (!lua_isnil(L, hover)) {
        luaL_error(L, "the hover of a gutter column is a table: { shape =, color = }");
    }
    lua_pop(L, 1);
    return out;
}

// The string `name` of the table at `t`, a part of the frame's text; none where it is not said.
pencere_view_types_option_slice_t words_at(lua_State* L, int t, const char* name, const char* what,
                                           Arenas& arenas) {
    pencere_view_types_option_slice_t out = {};
    get(L, t, name);
    if (lua_type(L, -1) == LUA_TSTRING) {
        size_t length = 0;
        const char* bytes = lua_tolstring(L, -1, &length);
        out.is_some = true;
        out.val = add_words(arenas, bytes, length);
    } else if (!lua_isnil(L, -1)) {
        luaL_error(L, "%s is a string", what);
    }
    lua_pop(L, 1);
    return out;
}

// `comment = "--"` (a line comment alone) or `comment = { line = "--", block = { "--[[", "]]" } }`,
// the block's tokens a pair or `{ open =, close = }`.
pencere_view_types_comment_tokens_t comment_at(lua_State* L, int t, Arenas& arenas) {
    pencere_view_types_comment_tokens_t out = {};
    get(L, t, "comment");
    int comment = lua_gettop(L);
    if (lua_type(L, comment) == LUA_TSTRING) {
        size_t length = 0;
        const char* bytes = lua_tolstring(L, comment, &length);
        out.line.is_some = true;
        out.line.val = add_words(arenas, bytes, length);
    } else if (lua_type(L, comment) == LUA_TTABLE) {
        out.line = words_at(L, comment, "line", "the line comment of an editor", arenas);
        get(L, comment, "block");
        int block = lua_gettop(L);
        if (lua_type(L, block) == LUA_TTABLE) {
            const char* const kPair = "the block comment of an editor is a pair of strings: { \"/*\", \"*/\" }";
            const char* names[] = {"open", "close"};
            pencere_view_types_slice_t* slots[] = {&out.block.val.open, &out.block.val.close};
            for (int at = 0; at < 2; at++) {
                get(L, block, names[at]);
                if (lua_isnil(L, -1)) {
                    lua_pop(L, 1);
                    lua_rawgeti(L, block, at + 1);
                }
                if (lua_type(L, -1) != LUA_TSTRING) luaL_error(L, "%s", kPair);
                size_t length = 0;
                const char* bytes = lua_tolstring(L, -1, &length);
                *slots[at] = add_words(arenas, bytes, length);
                lua_pop(L, 1);
            }
            out.block.is_some = true;
        } else if (!lua_isnil(L, block)) {
            luaL_error(L, "the block comment of an editor is a pair of strings: { \"/*\", \"*/\" }");
        }
        lua_pop(L, 1);
    } else if (!lua_isnil(L, comment)) {
        luaL_error(L, "the comment of an editor is a string, or a table: { line =, block = { open, close } }");
    }
    lua_pop(L, 1);
    return out;
}

const char* const kFolds[] = {"page", "indent"};
static_assert(PENCERE_VIEW_TYPES_EDITOR_FOLDS_PAGE == 0 && PENCERE_VIEW_TYPES_EDITOR_FOLDS_INDENT == 1,
              "kFolds is the order of editor-folds");
const char* const kWhitespace[] = {"none", "boundary", "selection", "trailing", "all"};
static_assert(PENCERE_VIEW_TYPES_EDITOR_WHITESPACE_NONE == 0 && PENCERE_VIEW_TYPES_EDITOR_WHITESPACE_BOUNDARY == 1 &&
                  PENCERE_VIEW_TYPES_EDITOR_WHITESPACE_SELECTION == 2 &&
                  PENCERE_VIEW_TYPES_EDITOR_WHITESPACE_TRAILING == 3 && PENCERE_VIEW_TYPES_EDITOR_WHITESPACE_ALL == 4,
              "kWhitespace is the order of editor-whitespace");
const char* const kMatches[] = {"none", "selection", "word"};
static_assert(PENCERE_VIEW_TYPES_EDITOR_MATCHES_NONE == 0 && PENCERE_VIEW_TYPES_EDITOR_MATCHES_SELECTION == 1 &&
                  PENCERE_VIEW_TYPES_EDITOR_MATCHES_WORD == 2,
              "kMatches is the order of editor-matches");

// `meta.editor`: { wrap =, tab_width =, hard_tabs =, gutter =, read_only =, keys = "editor" | "page",
// hint = "words", comment = "--" | { line =, block = }, pairs = "()[]{}", indent = { increase =,
// decrease = }, folds = "page" | "indent", line_height =, padding =, scroll_margin =,
// numbers = "absolute" | "relative" | "hidden", current_line = "text" | "gutter" | "all",
// whitespace = "none" | "boundary" | "selection" | "trailing" | "all", indent_guides =,
// matches = "none" | "selection" | "word" (true is "selection"),
// caret = { shape =, height =, width =, color =, text = },
// colors = { current_line =, … } }. What it does not say is the default (a gutter, tabs of four);
// a number the interface takes as an option goes as none, and the host's default stands. Whether
// a rule of the indent is a regular expression is the host's to say: the page fails there.
pencere_view_types_editor_options_t editor_options_at(lua_State* L, int t, Arenas& arenas) {
    pencere_view_types_editor_options_t out = {};
    out.wrap = flag(L, t, "wrap");
    float tab = number(L, t, "tab_width", 4.0f);
    out.tab_width = tab < 1 ? 1 : (tab > 16 ? 16 : (uint8_t)tab);
    out.hard_tabs = flag(L, t, "hard_tabs");
    view_option_bool_t gutter = optional_flag(L, t, "gutter");
    out.gutter = gutter.is_some ? gutter.val : true;
    out.read_only = flag(L, t, "read_only");
    static const char* const kKeysOf[] = {"editor", "page"};
    out.keys = (uint8_t)word(L, t, "keys", kKeysOf, 2, 0);
    out.hint = words_at(L, t, "hint", "the hint of an editor", arenas);
    out.comment = comment_at(L, t, arenas);
    out.pairs = words_at(L, t, "pairs", "the pairs of an editor", arenas);
    get(L, t, "indent");
    int indent = lua_gettop(L);
    if (lua_type(L, indent) == LUA_TTABLE) {
        out.indent.increase = words_at(L, indent, "increase", "the indent.increase of an editor", arenas);
        out.indent.decrease = words_at(L, indent, "decrease", "the indent.decrease of an editor", arenas);
    } else if (!lua_isnil(L, indent)) {
        luaL_error(L, "the indent of an editor is a table: { increase =, decrease = }");
    }
    lua_pop(L, 1);
    out.folds = (uint8_t)word(L, t, "folds", kFolds, 2, 0);
    out.line_height = optional_number(L, t, "line_height");
    out.padding = optional_number(L, t, "padding");
    view_option_f32_t margin = optional_number(L, t, "scroll_margin");
    if (margin.is_some) {
        out.scroll_margin.is_some = true;
        out.scroll_margin.val = margin.val < 0 ? 0 : (margin.val > 255 ? 255 : (uint8_t)margin.val);
    }
    out.numbers = (uint8_t)word(L, t, "numbers", kLineNumbers, 3, 0);
    out.current_line = (uint8_t)word(L, t, "current_line", kLineHighlight, 3, 0);
    out.whitespace = (uint8_t)word(L, t, "whitespace", kWhitespace, 5, 0);
    out.indent_guides = flag(L, t, "indent_guides");
    get(L, t, "matches");
    bool said = lua_type(L, -1) == LUA_TBOOLEAN;
    bool on = said && lua_toboolean(L, -1);
    lua_pop(L, 1);
    out.matches = said ? (uint8_t)(on ? PENCERE_VIEW_TYPES_EDITOR_MATCHES_SELECTION : PENCERE_VIEW_TYPES_EDITOR_MATCHES_NONE)
                       : (uint8_t)word(L, t, "matches", kMatches, 3, 0);
    get(L, t, "gutters");
    int gutters = lua_gettop(L);
    if (lua_type(L, gutters) == LUA_TTABLE) {
        size_t count = (size_t)lua_objlen(L, gutters);
        if (count > 8) luaL_error(L, "an editor has at most 8 gutter columns");
        out.gutters.ptr = count ? (pencere_view_types_gutter_column_t*)xmalloc(count * sizeof(pencere_view_types_gutter_column_t))
                                : nullptr;
        out.gutters.len = count;
        for (size_t at = 0; at < count; at++) {
            lua_rawgeti(L, gutters, (int)at + 1);
            out.gutters.ptr[at] = gutter_column_at(L, lua_gettop(L));
            lua_pop(L, 1);
        }
    } else if (!lua_isnil(L, gutters)) {
        luaL_error(L, "the gutters of an editor are a list: { \"numbers\", { name = \"changes\", width = 6 }, \"folds\" }");
    }
    lua_pop(L, 1);
    get(L, t, "marks");
    int marks = lua_gettop(L);
    if (lua_type(L, marks) == LUA_TTABLE) {
        out.marks.bar = optional_number(L, marks, "bar");
        out.marks.hover = optional_number(L, marks, "hover");
        out.marks.ruler = optional_number(L, marks, "ruler");
    } else if (!lua_isnil(L, marks)) {
        luaL_error(L, "the marks of an editor are a table: { bar =, hover =, ruler = }");
    }
    lua_pop(L, 1);
    // events = { "hover" }: what of its own the editor tells the page; hover = { delay = seconds }.
    get(L, t, "events");
    int told = lua_gettop(L);
    if (lua_type(L, told) == LUA_TTABLE) {
        for (size_t at = 1; at <= (size_t)lua_objlen(L, told); at++) {
            lua_rawgeti(L, told, (int)at);
            const char* said = lua_type(L, -1) == LUA_TSTRING ? lua_tostring(L, -1) : "";
            if (strcmp(said, "hover") == 0) out.events.hover = true;
            else if (strcmp(said, "target") == 0) out.events.target = true;
            else luaL_error(L, "an editor tells of \"hover\" and \"target\", not \"%s\"", said);
            lua_pop(L, 1);
        }
    } else if (!lua_isnil(L, told)) {
        luaL_error(L, "the events of an editor are a list of words: { \"hover\" }");
    }
    lua_pop(L, 1);
    out.widgets = widget_groups_at(L, t);
    out.gestures = gestures_at(L, t);
    out.keymap = keymap_at(L, t);
    out.chord_timeout = optional_number(L, t, "chord_timeout");
    get(L, t, "links");
    int links = lua_gettop(L);
    if (lua_type(L, links) == LUA_TTABLE) {
        out.links.delay = optional_number(L, links, "delay");
    } else if (!lua_isnil(L, links)) {
        luaL_error(L, "the links of an editor are a table: { delay = seconds }");
    }
    lua_pop(L, 1);
    get(L, t, "hover");
    int rest = lua_gettop(L);
    if (lua_type(L, rest) == LUA_TTABLE) {
        out.hover.delay = optional_number(L, rest, "delay");
    } else if (!lua_isnil(L, rest)) {
        luaL_error(L, "the hover of an editor is a table: { delay = seconds }");
    }
    lua_pop(L, 1);
    get(L, t, "selection");
    int selection = lua_gettop(L);
    if (lua_type(L, selection) == LUA_TTABLE) {
        out.selection.radius = optional_number(L, selection, "radius");
    } else if (!lua_isnil(L, selection)) {
        luaL_error(L, "the selection of an editor is a table: { radius = }");
    }
    lua_pop(L, 1);
    out.caret.color.tag = PENCERE_VIEW_TYPES_EDITOR_TINT_THEME;
    out.caret.text.tag = PENCERE_VIEW_TYPES_EDITOR_TINT_THEME;
    get(L, t, "caret");
    int caret = lua_gettop(L);
    if (lua_type(L, caret) == LUA_TTABLE) {
        out.caret.shape = (uint8_t)word(L, caret, "shape", kCaretShape, 3, 0);
        out.caret.height = (uint8_t)word(L, caret, "height", kCaretHeight, 2, 0);
        out.caret.width = optional_number(L, caret, "width");
        out.caret.color = tint_at(L, caret, "color");
        out.caret.text = tint_at(L, caret, "text");
        out.caret.radius = optional_number(L, caret, "radius");
        out.caret.glow = optional_number(L, caret, "glow");
        out.caret.blink = optional_flag(L, caret, "blink");
        out.caret.blink_interval = optional_number(L, caret, "blink_interval");
        out.caret.blink_for = optional_number(L, caret, "blink_for");
        out.caret.unfocused = (uint8_t)word(L, caret, "unfocused", kCaretUnfocused, 3, 0);
        out.caret.motion = motion_at(L, caret, "motion");
    } else if (!lua_isnil(L, caret)) {
        luaL_error(L, "the caret of an editor is a table: { shape =, width =, color =, text =, radius =, glow =, blink =, blink_interval =, blink_for =, unfocused =, motion = }");
    }
    lua_pop(L, 1);
    pencere_view_types_editor_colors_t& colors = out.colors;
    struct {
        const char* name;
        pencere_view_types_editor_tint_t* slot;
    } const kColors[] = {
        {"current_line", &colors.current_line},     {"selection", &colors.selection},
        {"selection_blurred", &colors.selection_blurred}, {"found", &colors.found},
        {"brackets", &colors.brackets},             {"numbers", &colors.numbers},
        {"current_number", &colors.current_number}, {"gutter", &colors.gutter},
        {"folds", &colors.folds},                   {"folded", &colors.folded},
        {"hint", &colors.hint},                     {"whitespace", &colors.whitespace},
        {"indent_guides", &colors.indent_guides},   {"active_indent_guide", &colors.active_indent_guide},
        {"matches", &colors.matches},                {"mark_hover", &colors.mark_hover},
    };
    get(L, t, "colors");
    int table = lua_gettop(L);
    for (const auto& colour : kColors) {
        colour.slot->tag = PENCERE_VIEW_TYPES_EDITOR_TINT_THEME;
        if (lua_type(L, table) == LUA_TTABLE) *colour.slot = tint_at(L, table, colour.name);
    }
    if (lua_type(L, table) != LUA_TTABLE && !lua_isnil(L, table)) {
        luaL_error(L, "the colors of an editor are a table: { current_line =, selection =, … }");
    }
    lua_pop(L, 1);
    return out;
}

void flatten(lua_State* L, int element, Flat& flat, int depth) {
    if (depth > kMaxDepth) luaL_error(L, "the page is nested more than %d boxes deep", kMaxDepth);
    luaL_checkstack(L, 32, "the page is nested too deep");
    if (lua_type(L, element) != LUA_TTABLE) luaL_error(L, "an element of the page is a table");
    auto& nodes = flat.frame->nodes;
    if (nodes.len >= kMaxNodes) luaL_error(L, "the page has more than %d elements", (int)kMaxNodes);
    if (nodes.len == flat.cap) {
        flat.cap = flat.cap ? flat.cap * 2 : 64;
        nodes.ptr = (pencere_view_types_node_t*)xrealloc(nodes.ptr, flat.cap * sizeof(pencere_view_types_node_t));
    }
    size_t at = nodes.len++;
    // Zero is the default of everything but the direction: a box is a column.
    memset(&nodes.ptr[at], 0, sizeof(pencere_view_types_node_t));
    nodes.ptr[at].style.direction = PENCERE_VIEW_TYPES_DIRECTION_COLUMN;
    nodes.ptr[at].style.opacity = 1.0f;

    get(L, element, "kind");
    const char* kind = lua_type(L, -1) == LUA_TSTRING ? lua_tostring(L, -1) : "box";
    bool is_text = strcmp(kind, "text") == 0;
    bool is_spark = strcmp(kind, "spark") == 0;
    bool is_image = strcmp(kind, "image") == 0;
    bool is_rich = strcmp(kind, "rich") == 0;
    bool is_canvas = strcmp(kind, "canvas") == 0;
    if (!is_text && !is_spark && !is_image && !is_rich && !is_canvas && strcmp(kind, "box") != 0)
        luaL_error(L,
                   "an element is a \"box\", a \"text\", a \"rich\" text, a \"spark\", an \"image\" or a "
                   "\"canvas\", not \"%s\"",
                   kind);
    lua_pop(L, 1);

    if (is_rich) {
        get(L, element, "spans");
        if (lua_type(L, -1) != LUA_TTABLE) luaL_error(L, "a text of many looks is made by pencere.rich");
        nodes.ptr[at].content.tag = PENCERE_VIEW_TYPES_CONTENT_RICH;
        nodes.ptr[at].content.val.rich = spans_at(L, lua_gettop(L), flat.arenas);
        lua_pop(L, 1);
    } else if (is_canvas) {
        get(L, element, "paths");
        if (lua_type(L, -1) != LUA_TTABLE) luaL_error(L, "a canvas is made by pencere.canvas");
        nodes.ptr[at].content.tag = PENCERE_VIEW_TYPES_CONTENT_CANVAS;
        nodes.ptr[at].content.val.canvas = paths_at(L, lua_gettop(L), flat.arenas);
        lua_pop(L, 1);
    }

    if (is_text) {
        get(L, element, "text");
        size_t length = 0;
        const char* bytes = lua_type(L, -1) == LUA_TSTRING ? lua_tolstring(L, -1, &length) : "";
        nodes.ptr[at].content.tag = PENCERE_VIEW_TYPES_CONTENT_TEXT;
        nodes.ptr[at].content.val.text = add_words(flat.arenas, bytes, length);
        lua_pop(L, 1);
    } else if (is_spark) {
        pencere_view_types_spark_t spark = {};
        get(L, element, "values");
        if (lua_type(L, -1) == LUA_TTABLE) {
            int list = lua_gettop(L);
            size_t count = (size_t)lua_objlen(L, list);
            if (count > kMaxList * 4) luaL_error(L, "a sparkline has more values than it may");
            size_t start = flat.arenas.numbers.size();
            for (size_t i = 0; i < count; i++) {
                lua_rawgeti(L, list, (int)i + 1);
                flat.arenas.numbers.push_back((float)lua_tonumber(L, -1));
                lua_pop(L, 1);
            }
            spark.values = part(start, count);
        }
        lua_pop(L, 1);
        spark.color.alpha = 1.0f;
        get(L, element, "line");
        if (!lua_isnil(L, -1)) spark.color = color_at(L, lua_gettop(L));
        lua_pop(L, 1);
        spark.peak = optional_number(L, element, "ceiling");
        nodes.ptr[at].content.tag = PENCERE_VIEW_TYPES_CONTENT_SPARK;
        nodes.ptr[at].content.val.spark = spark;
    } else if (is_image) {
        pencere_view_types_image_t image = {};
        // What the picture is lives in a table of its own (see `pencere.image`): its names are
        // the chain's methods' too.
        get(L, element, "picture");
        if (lua_type(L, -1) != LUA_TTABLE) luaL_error(L, "an image is made by pencere.image");
        int picture = lua_gettop(L);
        get(L, picture, "src");
        size_t length = 0;
        const char* bytes = lua_type(L, -1) == LUA_TSTRING ? lua_tolstring(L, -1, &length) : "";
        image.src = add_words(flat.arenas, bytes, length);
        lua_pop(L, 1);
        int fit = word(L, picture, "fit", kFit, 2, 0);
        image.fit = (pencere_view_types_fit_t)fit;
        image.tint = optional_color(L, picture, "tint");
        lua_pop(L, 1);
        nodes.ptr[at].content.tag = PENCERE_VIEW_TYPES_CONTENT_IMAGE;
        nodes.ptr[at].content.val.image = image;
    }

    get(L, element, "style");
    if (lua_type(L, -1) == LUA_TTABLE) {
        pencere_view_types_style_t style = nodes.ptr[at].style;
        read_style(L, lua_gettop(L), style, flat.arenas);
        nodes.ptr[at].style = style;
    }
    lua_pop(L, 1);

    get(L, element, "meta");
    if (lua_type(L, -1) == LUA_TTABLE) {
        int meta = lua_gettop(L);
        get(L, meta, "id");
        if (!lua_isnil(L, -1)) {
            nodes.ptr[at].id.is_some = true;
            nodes.ptr[at].id.val = id_of(L, lua_gettop(L));
        }
        lua_pop(L, 1);
        nodes.ptr[at].pressable = flag(L, meta, "pressable");
        nodes.ptr[at].focusable = flag(L, meta, "focusable");
        static const char* const kFieldKinds[] = {"line", "lines", "secret"};
        int field = word(L, meta, "field", kFieldKinds, 3, -1);
        if (field >= 0) nodes.ptr[at].field = {true, (uint8_t)field};
        get(L, meta, "editor");
        if (lua_type(L, -1) == LUA_TTABLE) {
            nodes.ptr[at].editor.is_some = true;
            nodes.ptr[at].editor.val = editor_options_at(L, lua_gettop(L), flat.arenas);
        } else if (!lua_isnil(L, -1)) {
            luaL_error(L, "an editor's options are a table");
        }
        lua_pop(L, 1);
        get(L, meta, "zone");
        if (!lua_isnil(L, -1)) {
            nodes.ptr[at].zone.is_some = true;
            nodes.ptr[at].zone.val = id_of(L, lua_gettop(L));
        }
        lua_pop(L, 1);
        get(L, meta, "label");
        if (lua_type(L, -1) == LUA_TSTRING) {
            size_t length = 0;
            const char* bytes = lua_tolstring(L, -1, &length);
            nodes.ptr[at].label.is_some = true;
            nodes.ptr[at].label.val = add_words(flat.arenas, bytes, length);
        }
        lua_pop(L, 1);
        get(L, meta, "motion");
        if (lua_type(L, -1) == LUA_TTABLE) {
            nodes.ptr[at].motion.is_some = true;
            nodes.ptr[at].motion.val = motion_at(L, lua_gettop(L));
        }
        lua_pop(L, 1);
        get(L, meta, "tip");
        if (lua_type(L, -1) == LUA_TTABLE) {
            int tip = lua_gettop(L);
            get(L, tip, "words");
            if (lua_type(L, -1) == LUA_TSTRING) {
                size_t length = 0;
                const char* bytes = lua_tolstring(L, -1, &length);
                nodes.ptr[at].tip.is_some = true;
                nodes.ptr[at].tip.val.words = add_words(flat.arenas, bytes, length);
                get(L, tip, "keys");
                if (lua_type(L, -1) == LUA_TSTRING) {
                    bytes = lua_tolstring(L, -1, &length);
                    nodes.ptr[at].tip.val.keys.is_some = true;
                    nodes.ptr[at].tip.val.keys.val = add_words(flat.arenas, bytes, length);
                }
                lua_pop(L, 1);
            }
            lua_pop(L, 1);
        }
        lua_pop(L, 1);
        int role = word(L, meta, "role", kAccessibleRoles, 12, -1);
        if (role >= 0) {
            nodes.ptr[at].role.is_some = true;
            nodes.ptr[at].role.val.tag = (uint8_t)role;
            if (role == PENCERE_VIEW_TYPES_ACCESSIBLE_ROLE_HEADING) {
                float level = number(L, meta, "level", 1.0f);
                nodes.ptr[at].role.val.val.heading = level < 1 ? 1 : (level > 6 ? 6 : (uint8_t)level);
            }
        }
        nodes.ptr[at].state.checked = optional_flag(L, meta, "checked");
        nodes.ptr[at].state.selected = flag(L, meta, "selected");
        nodes.ptr[at].state.expanded = optional_flag(L, meta, "expanded");
        nodes.ptr[at].state.disabled = flag(L, meta, "disabled");
        get(L, meta, "value");
        if (lua_type(L, -1) == LUA_TSTRING) {
            size_t length = 0;
            const char* bytes = lua_tolstring(L, -1, &length);
            nodes.ptr[at].state.value.is_some = true;
            nodes.ptr[at].state.value.val = add_words(flat.arenas, bytes, length);
        }
        lua_pop(L, 1);
        get(L, meta, "commands");
        if (lua_type(L, -1) == LUA_TTABLE) nodes.ptr[at].commands = commands_at(L, lua_gettop(L), flat.arenas);
        lua_pop(L, 1);
        nodes.ptr[at].focused = flag(L, meta, "focused");
        nodes.ptr[at].draggable = flag(L, meta, "draggable");
        nodes.ptr[at].drop_target = flag(L, meta, "drop_target");
    }
    lua_pop(L, 1);

    get(L, element, "kids");
    if (lua_type(L, -1) == LUA_TTABLE) {
        int list = lua_gettop(L);
        int count = lua_objlen(L, list);
        nodes.ptr[at].children = (uint32_t)count;
        for (int child = 1; child <= count; child++) {
            lua_rawgeti(L, list, child);
            flatten(L, lua_gettop(L), flat, depth + 1);
            lua_pop(L, 1);
        }
    }
    lua_pop(L, 1);
}

}  // namespace

// --- Strings ---------------------------------------------------------------------------------

std::string repaired(const char* s, size_t n) {
    std::string out;
    out.reserve(n);
    size_t i = 0;
    while (i < n) {
        unsigned char c = (unsigned char)s[i];
        if (c < 0x80) {
            out.push_back((char)c);
            i++;
            continue;
        }
        size_t len = 0;
        uint32_t cp = 0, least = 0;
        if (c >= 0xC2 && c <= 0xDF) {
            len = 2;
            cp = c & 0x1F;
            least = 0x80;
        } else if ((c & 0xF0) == 0xE0) {
            len = 3;
            cp = c & 0x0F;
            least = 0x800;
        } else if (c >= 0xF0 && c <= 0xF4) {
            len = 4;
            cp = c & 0x07;
            least = 0x10000;
        }
        bool ok = len != 0 && i + len <= n;
        for (size_t k = 1; ok && k < len; k++) {
            unsigned char d = (unsigned char)s[i + k];
            if ((d & 0xC0) != 0x80) ok = false;
            cp = (cp << 6) | (d & 0x3F);
        }
        if (ok && (cp < least || cp > 0x10FFFF || (cp >= 0xD800 && cp <= 0xDFFF))) ok = false;
        if (ok) {
            out.append(s + i, len);
            i += len;
        } else {
            out += "\xEF\xBF\xBD";
            i++;
        }
    }
    return out;
}

view_string_t host_string(const char* bytes, size_t length) {
    std::string clean = repaired(bytes, length);
    view_string_t out;
    out.ptr = (uint8_t*)xmalloc(clean.size());
    out.len = clean.size();
    memcpy(out.ptr, clean.data(), clean.size());
    return out;
}

// --- Names -----------------------------------------------------------------------------------

uint64_t id_of(lua_State* L, int index) {
    int kind = lua_type(L, index);
    if (kind == LUA_TTABLE) {
        // The caret of an editor (`pencere.caret_of`): { caret = the editor's name }, the id the
        // engine's `Id::caret` makes of it.
        lua_rawgetfield(L, index, "caret");
        if (lua_isnil(L, -1)) {
            lua_pop(L, 1);
            // A range of an editor's text the page named (`pencere.range_of`): { anchor = the editor's
            // name, name = the range's }, the id the engine's `Id::anchor` makes of them.
            lua_rawgetfield(L, index, "anchor");
            if (lua_isnil(L, -1)) luaL_error(L, "an element is named by a string or a whole number");
            uint64_t editor = id_of(L, lua_gettop(L));
            lua_pop(L, 1);
            lua_rawgetfield(L, index, "name");
            if (lua_type(L, -1) != LUA_TSTRING) luaL_error(L, "the name of a range is a string");
            size_t length = 0;
            const char* bytes = lua_tolstring(L, -1, &length);
            uint64_t named = fnv(bytes, length);
            lua_pop(L, 1);
            const uint64_t role = 0xa7;
            auto mix = [&](uint64_t of) {
                uint64_t mixed = (of ^ ((role + 1) << 56)) * 0x9e3779b97f4a7c15ull;
                return mixed ^ (mixed >> 29);
            };
            uint64_t of_name = mix(named);
            return mix(editor) ^ ((of_name << 17) | (of_name >> 47));
        }
        uint64_t editor = id_of(L, lua_gettop(L));
        lua_pop(L, 1);
        const uint64_t role = 0xc7;
        uint64_t mixed = (editor ^ ((role + 1) << 56)) * 0x9e3779b97f4a7c15ull;
        return mixed ^ (mixed >> 29);
    }
    if (kind == LUA_TSTRING) {
        size_t length = 0;
        const char* bytes = lua_tolstring(L, index, &length);
        uint64_t id = fnv(bytes, length);
        auto& known = names();
        if (known.size() > 100000) known.clear();
        auto found = known.find(id);
        if (found == known.end()) known.emplace(id, std::string(bytes, length));
        return id;
    }
    if (kind == LUA_TNUMBER) {
        double number = lua_tonumber(L, index);
        if (!(number >= 0 && number < 9007199254740992.0) || number != (double)(uint64_t)number)
            luaL_error(L, "a number that names an element is a whole number from 0 to 2^53");
        return (uint64_t)number;
    }
    luaL_error(L, "an element is named by a string or a whole number");
}

void push_id(lua_State* L, uint64_t id) {
    auto& known = names();
    auto found = known.find(id);
    if (found != known.end()) {
        lua_pushlstring(L, found->second.data(), found->second.size());
    } else if (id < 9007199254740992ull) {
        lua_pushnumber(L, (double)id);
    } else {
        char hex[20];
        snprintf(hex, sizeof hex, "%016llx", (unsigned long long)id);
        lua_pushstring(L, hex);
    }
}

void forget_ids() { names().clear(); }

// --- The tree, the event, the reply -----------------------------------------------------------

int frame_call(lua_State* L) {
    auto* job = (FrameJob*)lua_touserdata(L, 2);
    Flat flat = {job->out, 0, {}};
    flatten(L, 1, flat, 1);
    // The tree is read: the lists the nodes name the parts of go to the host with it.
    job->out->text = take_text(flat.arenas.text);
    if (!flat.arenas.numbers.empty()) {
        size_t bytes = flat.arenas.numbers.size() * sizeof(float);
        job->out->numbers.ptr = (float*)xmalloc(bytes);
        job->out->numbers.len = flat.arenas.numbers.size();
        memcpy(job->out->numbers.ptr, flat.arenas.numbers.data(), bytes);
    }
    if (!flat.arenas.tracks.empty()) {
        size_t bytes = flat.arenas.tracks.size() * sizeof(pencere_view_types_track_t);
        job->out->tracks.ptr = (pencere_view_types_track_t*)xmalloc(bytes);
        job->out->tracks.len = flat.arenas.tracks.size();
        memcpy(job->out->tracks.ptr, flat.arenas.tracks.data(), bytes);
    }
    take_list(job->out->spans, flat.arenas.spans);
    take_list(job->out->paths, flat.arenas.paths);
    take_list(job->out->segments, flat.arenas.segments);
    take_list(job->out->commands, flat.arenas.commands);
    job->out->animating = job->animating;
    job->out->wake_at.is_some = job->has_wake_at;
    job->out->wake_at.val = job->wake_at;
    job->out->wake.is_some = job->has_wake;
    job->out->wake.val = job->wake;
    return 0;
}

namespace {

void set_number(lua_State* L, const char* name, double value) {
    lua_pushnumber(L, value);
    lua_setfield(L, -2, name);
}

void set_word(lua_State* L, const char* name, const char* value) {
    lua_pushstring(L, value);
    lua_setfield(L, -2, name);
}

void set_mods(lua_State* L, const pencere_view_types_mods_t& mods) {
    lua_createtable(L, 0, 4);
    lua_pushboolean(L, mods.shift);
    lua_setfield(L, -2, "shift");
    lua_pushboolean(L, mods.control);
    lua_setfield(L, -2, "control");
    lua_pushboolean(L, mods.alt);
    lua_setfield(L, -2, "alt");
    lua_pushboolean(L, mods.command);
    lua_setfield(L, -2, "command");
    lua_setfield(L, -2, "mods");
}

const char* button_name(pencere_view_types_button_t button) {
    switch (button) {
        case PENCERE_VIEW_TYPES_BUTTON_RIGHT:
            return "right";
        case PENCERE_VIEW_TYPES_BUTTON_MIDDLE:
            return "middle";
        default:
            return "left";
    }
}

void push_character(lua_State* L, uint32_t cp);

// The name of a key the page reads, as the script is told it.
void push_key(lua_State* L, const pencere_view_types_key_t& key) {
    if (key.tag == PENCERE_VIEW_TYPES_KEY_CHARACTER) {
        push_character(L, key.val.character);
    } else if (key.tag == PENCERE_VIEW_TYPES_KEY_FUNCTION) {
        char name[8];
        snprintf(name, sizeof name, "f%u", (unsigned)key.val.function);
        lua_pushstring(L, name);
    } else {
        lua_pushstring(L, key.tag < 13 ? kKeys[key.tag] : "unknown");
    }
}

void push_character(lua_State* L, uint32_t cp) {
    char out[5] = {0};
    if (cp < 0x80) {
        out[0] = (char)cp;
    } else if (cp < 0x800) {
        out[0] = (char)(0xC0 | (cp >> 6));
        out[1] = (char)(0x80 | (cp & 0x3F));
    } else if (cp < 0x10000) {
        out[0] = (char)(0xE0 | (cp >> 12));
        out[1] = (char)(0x80 | ((cp >> 6) & 0x3F));
        out[2] = (char)(0x80 | (cp & 0x3F));
    } else {
        out[0] = (char)(0xF0 | (cp >> 18));
        out[1] = (char)(0x80 | ((cp >> 12) & 0x3F));
        out[2] = (char)(0x80 | ((cp >> 6) & 0x3F));
        out[3] = (char)(0x80 | (cp & 0x3F));
    }
    lua_pushstring(L, out);
}

}  // namespace

namespace {

// A place of a document as the script is told it: a whole number of characters from 0.
void set_place(lua_State* L, const char* name, uint32_t at) { set_number(L, name, (double)at); }

const char* const kOrigins[] = {"person", "compose", "page", "undo", "redo", "load"};
const char* const kRefusals[] = {"reversed",       "out-of-document",      "overlapping",   "stale-version",
                                 "future-version", "too-large",            "too-many-decorations",
                                 "outside-range",  "bad-pattern",          "no-editor"};

// The ranges of a selection, as a list of { anchor =, head = }.
void push_ranges(lua_State* L, const pencere_view_types_list_text_range_t& ranges) {
    lua_createtable(L, (int)ranges.len, 0);
    for (size_t at = 0; at < ranges.len; at++) {
        lua_createtable(L, 0, 2);
        set_place(L, "anchor", ranges.ptr[at].anchor);
        set_place(L, "head", ranges.ptr[at].head);
        lua_rawseti(L, -2, (int)at + 1);
    }
}

}  // namespace

void push_event(lua_State* L, const pencere_view_types_event_t& event) {
    lua_createtable(L, 0, 6);
    switch (event.tag) {
        case PENCERE_VIEW_TYPES_EVENT_POINTER_MOVE:
            set_word(L, "type", "move");
            set_number(L, "x", event.val.pointer_move.x);
            set_number(L, "y", event.val.pointer_move.y);
            break;
        case PENCERE_VIEW_TYPES_EVENT_POINTER_LEAVE:
            set_word(L, "type", "leave");
            break;
        case PENCERE_VIEW_TYPES_EVENT_TERMINAL_LOOK:
            set_word(L, "type", "terminal_look");
            break;
        case PENCERE_VIEW_TYPES_EVENT_PRESS:
            set_word(L, "type", "press");
            set_number(L, "x", event.val.press.at.x);
            set_number(L, "y", event.val.press.at.y);
            set_word(L, "button", button_name(event.val.press.button));
            set_number(L, "count", (double)event.val.press.count);
            set_mods(L, event.val.press.mods);
            break;
        case PENCERE_VIEW_TYPES_EVENT_RELEASE:
            set_word(L, "type", "release");
            set_number(L, "x", event.val.release.at.x);
            set_number(L, "y", event.val.release.at.y);
            set_word(L, "button", button_name(event.val.release.button));
            break;
        case PENCERE_VIEW_TYPES_EVENT_SCROLL:
            set_word(L, "type", "scroll");
            set_number(L, "x", event.val.scroll.at.x);
            set_number(L, "y", event.val.scroll.at.y);
            set_number(L, "dx", event.val.scroll.dx);
            set_number(L, "dy", event.val.scroll.dy);
            break;
        case PENCERE_VIEW_TYPES_EVENT_KEY:
            set_word(L, "type", "key");
            push_key(L, event.val.key.key);
            lua_setfield(L, -2, "key");
            set_mods(L, event.val.key.mods);
            break;
        case PENCERE_VIEW_TYPES_EVENT_KEY_UP:
            set_word(L, "type", "key_up");
            push_key(L, event.val.key_up.key);
            lua_setfield(L, -2, "key");
            set_mods(L, event.val.key_up.mods);
            break;
        case PENCERE_VIEW_TYPES_EVENT_MODS:
            set_word(L, "type", "mods");
            set_mods(L, event.val.mods);
            break;
        case PENCERE_VIEW_TYPES_EVENT_COMMAND:
            set_word(L, "type", "command");
            push_id(L, event.val.command);
            lua_setfield(L, -2, "id");
            break;
        case PENCERE_VIEW_TYPES_EVENT_FILES: {
            set_word(L, "type", "files");
            set_number(L, "x", event.val.files.at.x);
            set_number(L, "y", event.val.files.at.y);
            const auto& paths = event.val.files.paths;
            lua_createtable(L, (int)paths.len, 0);
            for (size_t at = 0; at < paths.len; at++) {
                lua_pushlstring(L, (const char*)paths.ptr[at].ptr, paths.ptr[at].len);
                lua_rawseti(L, -2, (int)at + 1);
            }
            lua_setfield(L, -2, "paths");
            break;
        }
        case PENCERE_VIEW_TYPES_EVENT_TEXT:
            set_word(L, "type", "text");
            push_id(L, event.val.text.field);
            lua_setfield(L, -2, "field");
            lua_pushlstring(L, (const char*)event.val.text.text.ptr, event.val.text.text.len);
            lua_setfield(L, -2, "text");
            break;
        case PENCERE_VIEW_TYPES_EVENT_FOCUS:
            set_word(L, "type", "focus");
            if (event.val.focus.is_some) {
                push_id(L, event.val.focus.val);
                lua_setfield(L, -2, "field");
            }
            break;
        case PENCERE_VIEW_TYPES_EVENT_MENU_PICK:
            set_word(L, "type", "menu");
            // The script counts from 1, as a table does.
            set_number(L, "index", (double)event.val.menu_pick + 1);
            break;
        case PENCERE_VIEW_TYPES_EVENT_SCROLLED: {
            // { boxes = { { id =, x =, y = }, ... } }: each box and how far it is scrolled.
            set_word(L, "type", "scrolled");
            const auto& boxes = event.val.scrolled;
            lua_createtable(L, (int)boxes.len, 0);
            for (size_t at = 0; at < boxes.len; at++) {
                lua_createtable(L, 0, 3);
                push_id(L, boxes.ptr[at].id);
                lua_setfield(L, -2, "id");
                set_number(L, "x", boxes.ptr[at].offset.x);
                set_number(L, "y", boxes.ptr[at].offset.y);
                lua_rawseti(L, -2, (int)at + 1);
            }
            lua_setfield(L, -2, "boxes");
            break;
        }
        case PENCERE_VIEW_TYPES_EVENT_EDITOR_POINTER: {
            // The pointer's own event, as it is told when it is not over an editor, with the
            // `place` of the document under it: { editor =, at = }.
            const auto& pointer = event.val.editor_pointer;
            const auto& input = pointer.input;
            if (input.tag == PENCERE_VIEW_TYPES_POINTER_INPUT_PRESS) {
                set_word(L, "type", "press");
                set_number(L, "x", input.val.press.at.x);
                set_number(L, "y", input.val.press.at.y);
                set_word(L, "button", button_name(input.val.press.button));
                set_number(L, "count", (double)input.val.press.count);
                set_mods(L, input.val.press.mods);
            } else if (input.tag == PENCERE_VIEW_TYPES_POINTER_INPUT_RELEASE) {
                set_word(L, "type", "release");
                set_number(L, "x", input.val.release.at.x);
                set_number(L, "y", input.val.release.at.y);
                set_word(L, "button", button_name(input.val.release.button));
            } else {
                set_word(L, "type", "move");
                set_number(L, "x", input.val.move.x);
                set_number(L, "y", input.val.move.y);
            }
            lua_createtable(L, 0, 2);
            push_id(L, pointer.place.editor);
            lua_setfield(L, -2, "editor");
            set_place(L, "at", pointer.place.at);
            lua_setfield(L, -2, "place");
            break;
        }
        case PENCERE_VIEW_TYPES_EVENT_EDIT: {
            // { editor =, version =, origin =, tag =, changes = { { from =, to =, insert = }, ... } }
            const auto& edit = event.val.edit;
            set_word(L, "type", "edit");
            push_id(L, edit.editor);
            lua_setfield(L, -2, "editor");
            set_number(L, "version", (double)edit.version);
            set_word(L, "origin", edit.origin < 6 ? kOrigins[edit.origin] : "person");
            if (edit.tag.is_some) {
                lua_pushlstring(L, (const char*)edit.tag.val.ptr, edit.tag.val.len);
                lua_setfield(L, -2, "tag");
            }
            lua_createtable(L, (int)edit.changes.len, 0);
            for (size_t at = 0; at < edit.changes.len; at++) {
                const auto& change = edit.changes.ptr[at];
                lua_createtable(L, 0, 3);
                set_place(L, "from", change.from);
                set_place(L, "to", change.to);
                lua_pushlstring(L, (const char*)change.insert.ptr, change.insert.len);
                lua_setfield(L, -2, "insert");
                lua_rawseti(L, -2, (int)at + 1);
            }
            lua_setfield(L, -2, "changes");
            break;
        }
        case PENCERE_VIEW_TYPES_EVENT_SELECTION: {
            // { editor =, version =, ranges = { { anchor =, head = }, ... }, main = (from 1), composing = }
            const auto& selection = event.val.selection;
            set_word(L, "type", "selection");
            push_id(L, selection.editor);
            lua_setfield(L, -2, "editor");
            set_number(L, "version", (double)selection.version);
            push_ranges(L, selection.ranges);
            lua_setfield(L, -2, "ranges");
            set_number(L, "main", (double)selection.main + 1);
            lua_pushboolean(L, selection.composing);
            lua_setfield(L, -2, "composing");
            break;
        }
        case PENCERE_VIEW_TYPES_EVENT_VIEWPORT:
            set_word(L, "type", "viewport");
            push_id(L, event.val.viewport.editor);
            lua_setfield(L, -2, "editor");
            set_number(L, "first", (double)event.val.viewport.first);
            set_number(L, "last", (double)event.val.viewport.last);
            break;
        case PENCERE_VIEW_TYPES_EVENT_EDITOR_REFUSED:
            set_word(L, "type", "editor_refused");
            push_id(L, event.val.editor_refused.editor);
            lua_setfield(L, -2, "editor");
            set_word(L, "reason",
                     event.val.editor_refused.reason < 10 ? kRefusals[event.val.editor_refused.reason] : "unknown");
            break;
        case PENCERE_VIEW_TYPES_EVENT_FOUND: {
            // { editor =, version =, count =, current = (from 1, nil for none), more = }
            const auto& found = event.val.found;
            set_word(L, "type", "found");
            push_id(L, found.editor);
            lua_setfield(L, -2, "editor");
            set_number(L, "version", (double)found.version);
            set_number(L, "count", (double)found.count);
            if (found.current.is_some) set_number(L, "current", (double)found.current.val);
            lua_pushboolean(L, found.more);
            lua_setfield(L, -2, "more");
            break;
        }
        case PENCERE_VIEW_TYPES_EVENT_WAKE:
            // { now = }: the time the page asked to be woken at has come (the prelude runs its
            // timers, and tells the page only if it asked itself).
            set_word(L, "type", "wake");
            set_number(L, "now", event.val.wake);
            break;
        case PENCERE_VIEW_TYPES_EVENT_EDITOR_GUTTER: {
            // { editor =, column =, line = (from 0), kind = "press" | "enter" | "leave", button =,
            // count =, mods…, marker = { id =, layer =, from =, to = } (none where the cell is
            // empty), cell = { x =, y =, w =, h = } }
            const auto& gutter = event.val.editor_gutter;
            static const char* const kKinds[] = {"press", "enter", "leave"};
            set_word(L, "type", "gutter");
            push_id(L, gutter.editor);
            lua_setfield(L, -2, "editor");
            lua_pushlstring(L, (const char*)gutter.column.ptr, gutter.column.len);
            lua_setfield(L, -2, "column");
            set_number(L, "line", (double)gutter.line);
            set_word(L, "kind", gutter.kind < 3 ? kKinds[gutter.kind] : "press");
            set_word(L, "button", button_name(gutter.button));
            set_number(L, "count", (double)gutter.count);
            set_mods(L, gutter.mods);
            if (gutter.marker.is_some) {
                lua_createtable(L, 0, 4);
                if (gutter.marker.val.id.is_some) {
                    lua_pushlstring(L, (const char*)gutter.marker.val.id.val.ptr, gutter.marker.val.id.val.len);
                    lua_setfield(L, -2, "id");
                }
                lua_pushlstring(L, (const char*)gutter.marker.val.layer.ptr, gutter.marker.val.layer.len);
                lua_setfield(L, -2, "layer");
                set_place(L, "from", gutter.marker.val.from);
                set_place(L, "to", gutter.marker.val.to);
                lua_setfield(L, -2, "marker");
            }
            lua_createtable(L, 0, 4);
            set_number(L, "x", gutter.cell.x);
            set_number(L, "y", gutter.cell.y);
            set_number(L, "w", gutter.cell.w);
            set_number(L, "h", gutter.cell.h);
            lua_setfield(L, -2, "cell");
            break;
        }
        case PENCERE_VIEW_TYPES_EVENT_EDITOR_HOVER: {
            // { editor =, kind = "dwell" | "leave", at =, from =, to =, line = (from 0), hits = { { layer =,
            // id =, from =, to = }, … }, word = { x =, y =, w =, h = }, mods…, version = }
            const auto& hover = event.val.editor_hover;
            set_word(L, "type", "hover");
            push_id(L, hover.editor);
            lua_setfield(L, -2, "editor");
            set_word(L, "kind", hover.kind == PENCERE_VIEW_TYPES_HOVER_KIND_LEAVE ? "leave" : "dwell");
            set_word(L, "over", hover.over == PENCERE_VIEW_TYPES_HOVER_OVER_WIDGET ? "widget" : "text");
            if (hover.id.is_some) {
                lua_pushlstring(L, (const char*)hover.id.val.ptr, hover.id.val.len);
                lua_setfield(L, -2, "id");
            }
            set_place(L, "at", hover.at);
            set_place(L, "from", hover.from);
            set_place(L, "to", hover.to);
            set_number(L, "line", (double)hover.line);
            lua_createtable(L, (int)hover.hits.len, 0);
            for (size_t at = 0; at < hover.hits.len; at++) {
                const auto& hit = hover.hits.ptr[at];
                lua_createtable(L, 0, 4);
                lua_pushlstring(L, (const char*)hit.layer.ptr, hit.layer.len);
                lua_setfield(L, -2, "layer");
                lua_pushlstring(L, (const char*)hit.id.ptr, hit.id.len);
                lua_setfield(L, -2, "id");
                set_place(L, "from", hit.from);
                set_place(L, "to", hit.to);
                lua_rawseti(L, -2, (int)at + 1);
            }
            lua_setfield(L, -2, "hits");
            lua_createtable(L, 0, 4);
            set_number(L, "x", hover.word.x);
            set_number(L, "y", hover.word.y);
            set_number(L, "w", hover.word.w);
            set_number(L, "h", hover.word.h);
            lua_setfield(L, -2, "word");
            set_mods(L, hover.mods);
            set_number(L, "version", (double)hover.version);
            break;
        }
        case PENCERE_VIEW_TYPES_EVENT_EDITOR_TARGET: {
            // { editor =, kind = "enter" | "leave" | "press", source = "word" | "link", at =, from =, to =,
            // id = (of a link), word = { x =, y =, w =, h = }, mods…, button =, count =, version = }
            const auto& target = event.val.editor_target;
            static const char* const kKinds[] = {"enter", "leave", "press"};
            set_word(L, "type", "target");
            push_id(L, target.editor);
            lua_setfield(L, -2, "editor");
            set_word(L, "kind", target.kind < 3 ? kKinds[target.kind] : "press");
            set_word(L, "source",
                     target.source == PENCERE_VIEW_TYPES_TARGET_SOURCE_LINK     ? "link"
                     : target.source == PENCERE_VIEW_TYPES_TARGET_SOURCE_WIDGET ? "widget"
                                                                                : "word");
            set_place(L, "at", target.at);
            set_place(L, "from", target.from);
            set_place(L, "to", target.to);
            if (target.link.is_some) {
                lua_pushlstring(L, (const char*)target.link.val.ptr, target.link.val.len);
                lua_setfield(L, -2, "id");
            }
            lua_createtable(L, 0, 4);
            set_number(L, "x", target.word.x);
            set_number(L, "y", target.word.y);
            set_number(L, "w", target.word.w);
            set_number(L, "h", target.word.h);
            lua_setfield(L, -2, "word");
            set_mods(L, target.mods);
            set_word(L, "button", button_name(target.button));
            set_number(L, "count", (double)target.count);
            set_number(L, "version", (double)target.version);
            break;
        }
        case PENCERE_VIEW_TYPES_EVENT_KEYMAP_PENDING: {
            // { editor =, keys = { "cmd+k", … } }: the chord so far; none when it is not waited on.
            const auto& pending = event.val.keymap_pending;
            set_word(L, "type", "keymap_pending");
            push_id(L, pending.editor);
            lua_setfield(L, -2, "editor");
            lua_createtable(L, (int)pending.keys.len, 0);
            for (size_t at = 0; at < pending.keys.len; at++) {
                lua_pushlstring(L, (const char*)pending.keys.ptr[at].ptr, pending.keys.ptr[at].len);
                lua_rawseti(L, -2, (int)at + 1);
            }
            lua_setfield(L, -2, "keys");
            break;
        }
        case PENCERE_VIEW_TYPES_EVENT_EDITOR_READY:
            // { editor =, version = }: the host made the editor; its text is posted.
            set_word(L, "type", "editor_ready");
            push_id(L, event.val.editor_ready.editor);
            lua_setfield(L, -2, "editor");
            set_number(L, "version", (double)event.val.editor_ready.version);
            break;
    }
}

namespace {

// A place of a document the script gives: a whole number of characters from 0.
uint32_t place_of(lua_State* L, int t, const char* name) {
    get(L, t, name);
    if (lua_type(L, -1) != LUA_TNUMBER) luaL_error(L, "%s is a place: a whole number of characters from 0", name);
    double at = lua_tonumber(L, -1);
    lua_pop(L, 1);
    if (!(at >= 0 && at <= 4294967295.0) || at != (double)(uint32_t)at)
        luaL_error(L, "%s is a place: a whole number of characters from 0", name);
    return (uint32_t)at;
}

// A version of a document the script gives (it was told it).
uint64_t version_of(lua_State* L, int t, const char* name) {
    get(L, t, name);
    if (lua_type(L, -1) != LUA_TNUMBER) luaL_error(L, "%s is the version of the document it is of", name);
    double version = lua_tonumber(L, -1);
    lua_pop(L, 1);
    if (!(version >= 0 && version < 9007199254740992.0)) luaL_error(L, "%s is a version: a whole number", name);
    return (uint64_t)version;
}

view_string_t string_of(lua_State* L, int t, const char* name) {
    get(L, t, name);
    if (lua_type(L, -1) != LUA_TSTRING) luaL_error(L, "%s is a string", name);
    size_t length = 0;
    const char* bytes = lua_tolstring(L, -1, &length);
    view_string_t out = host_string(bytes, length);
    lua_pop(L, 1);
    return out;
}

// The length of the list at the absolute index `t`, held to what a reply may say.
size_t list_length(lua_State* L, int t, const char* what) {
    size_t count = (size_t)lua_objlen(L, t);
    if (count > kMaxNodes) luaL_error(L, "%s has more items than a reply may", what);
    return count;
}

const char* const kMovements[] = {"left",      "right",     "word_left",  "word_right",     "up",
                                  "down",      "page_up",   "page_down",  "line_start",     "line_end",
                                  "document_start", "document_end", "bracket"};
const char* const kCommands[] = {"move",          "insert",          "newline",           "delete_backward",
                                 "delete_forward", "delete_word_backward", "delete_word_forward", "indent",
                                 "outdent",       "move_lines_up",   "move_lines_down",   "duplicate_lines",
                                 "toggle_comment", "select_all",     "add_caret_above",   "add_caret_below",
                                 "select_next",   "single_selection", "undo",             "redo",
                                 "fold",          "unfold",          "toggle_fold",       "unfold_all",
                                 "find",          "find_next",       "find_previous",     "stop_finding",
                                 "replace",       "replace_all",     "select_found",      "select_occurrences",
                                 "toggle_block_comment", "fold_all", "delete_lines",   "select_line",
                                 "duplicate_lines_up", "join_lines", "insert_blank_line", "split_line",
                                 "transpose_chars", "delete_to_line_start", "delete_to_line_end"};
static_assert(PENCERE_VIEW_TYPES_EDITOR_COMMAND_TOGGLE_BLOCK_COMMENT == 32 &&
                  PENCERE_VIEW_TYPES_EDITOR_COMMAND_DELETE_TO_LINE_END == 42 && PENCERE_VIEW_TYPES_MOVEMENT_BRACKET == 12,
              "the words of an editor's commands are the places of the interface's");

// `{ command = "undo" }`, `{ command = "move", movement = "word_left", extend = true }`,
// `{ command = "insert", text = "x" }`, `{ command = "find", pattern =, regex =, case =, word = }`,
// `{ command = "replace", text = "x" }`, `{ command = "replace_all", text = "x" }`.
pencere_view_types_editor_command_t command_at(lua_State* L, int t) {
    pencere_view_types_editor_command_t out = {};
    out.tag = (uint8_t)word(L, t, "command", kCommands, 43, -1);
    switch (out.tag) {
        case PENCERE_VIEW_TYPES_EDITOR_COMMAND_MOVE:
            out.val.move.movement = (uint8_t)word(L, t, "movement", kMovements, 13, 0);
            out.val.move.extend = flag(L, t, "extend");
            break;
        case PENCERE_VIEW_TYPES_EDITOR_COMMAND_INSERT:
            out.val.insert = string_of(L, t, "text");
            break;
        case PENCERE_VIEW_TYPES_EDITOR_COMMAND_FIND: {
            out.val.find.pattern = string_of(L, t, "pattern");
            out.val.find.regex = flag(L, t, "regex");
            view_option_bool_t matching = optional_flag(L, t, "case");
            out.val.find.matching_case = matching.is_some ? matching.val : true;
            out.val.find.word = flag(L, t, "word");
            break;
        }
        case PENCERE_VIEW_TYPES_EDITOR_COMMAND_REPLACE:
            out.val.replace = string_of(L, t, "text");
            break;
        case PENCERE_VIEW_TYPES_EDITOR_COMMAND_REPLACE_ALL:
            out.val.replace_all = string_of(L, t, "text");
            break;
        default:
            break;
    }
    return out;
}

namespace {

std::string lower_of(const std::string& text) {
    std::string out = text;
    for (auto& c : out) c = (char)tolower((unsigned char)c);
    return out;
}

// The word `mod` is the host's main modifier: the command key on a Mac, control elsewhere.
const char* main_modifier() {
    return pencere_view_system_keyboard() == PENCERE_VIEW_SYSTEM_KEY_CONVENTION_MAC ? "cmd" : "ctrl";
}

// A modifier written as a word, put in `mods`; whether it was one.
bool modifier_word(const std::string& word, pencere_view_types_mods_t& mods) {
    std::string w = lower_of(word);
    if (w == "mod") w = main_modifier();
    if (w == "cmd" || w == "command" || w == "super" || w == "meta") mods.command = true;
    else if (w == "ctrl" || w == "control") mods.control = true;
    else if (w == "alt" || w == "option" || w == "opt") mods.alt = true;
    else if (w == "shift") mods.shift = true;
    else return false;
    return true;
}

std::vector<std::string> split(const std::string& text, char by) {
    std::vector<std::string> out;
    size_t from = 0;
    while (true) {
        size_t at = text.find(by, from);
        out.push_back(text.substr(from, at == std::string::npos ? std::string::npos : at - from));
        if (at == std::string::npos) break;
        from = at + 1;
    }
    return out;
}

}  // namespace

// `held = "command"`, `"mod"`, `"control+alt"`: the modifiers a look is drawn while held.
pencere_view_types_option_mods_t held_at(lua_State* L, int t, const char* name) {
    pencere_view_types_option_mods_t out = {};
    get(L, t, name);
    if (lua_type(L, -1) == LUA_TSTRING) {
        out.is_some = true;
        for (const auto& word : split(lua_tostring(L, -1), '+'))
            if (!modifier_word(word, out.val))
                luaL_error(L, "%s is modifiers joined by +: command, control, alt, shift or mod, not \"%s\"", name, word.c_str());
    } else if (!lua_isnil(L, -1)) {
        luaL_error(L, "%s is modifiers written as words: \"command\", \"mod\", \"control+alt\"", name);
    }
    lua_pop(L, 1);
    return out;
}

namespace {

view_option_string_t option_string(lua_State* L, int t, const char* name, const char* what) {
    view_option_string_t out = {};
    get(L, t, name);
    if (lua_type(L, -1) == LUA_TSTRING) {
        size_t length = 0;
        const char* bytes = lua_tolstring(L, -1, &length);
        out = {true, host_string(bytes, length)};
    } else if (!lua_isnil(L, -1)) {
        luaL_error(L, "%s is a string", what);
    }
    lua_pop(L, 1);
    return out;
}

}  // namespace

// A decoration's `widget = "words"` or `widget = { "words", { "words", color =, id = }, … }`, with `side =
// "before" | "after" | "end"`, `group =`, `id =`, `label =`, `cursor =`, `anchor =`, `color =`,
// `background =`, `pad_left =`, `pad_right =`, `margin_left =`, `margin_right =`: text that is not
// the document's.
pencere_view_types_widget_t widget_at(lua_State* L, int t) {
    pencere_view_types_widget_t out = {};
    static const char* const kSides[] = {"before", "after", "end"};
    out.side = (uint8_t)word(L, t, "side", kSides, 3, PENCERE_VIEW_TYPES_WIDGET_SIDE_AFTER);
    out.group = option_string(L, t, "group", "the group of a widget");
    out.id = option_string(L, t, "id", "the id of a widget");
    out.label = option_string(L, t, "label", "the label of a widget");
    out.anchor = option_string(L, t, "anchor", "the anchor of a widget");
    int shape = word(L, t, "cursor", kCursorNames, 9, -1);
    if (shape >= 0) out.cursor = {true, (uint8_t)shape};
    out.color = optional_color(L, t, "color");
    out.background = optional_color(L, t, "background");
    out.pad_left = optional_number(L, t, "pad_left");
    out.pad_right = optional_number(L, t, "pad_right");
    out.margin_left = optional_number(L, t, "margin_left");
    out.margin_right = optional_number(L, t, "margin_right");
    get(L, t, "widget");
    int widget = lua_gettop(L);
    std::vector<pencere_view_types_widget_part_t> parts;
    if (lua_type(L, widget) == LUA_TSTRING) {
        size_t length = 0;
        const char* bytes = lua_tolstring(L, widget, &length);
        pencere_view_types_widget_part_t part = {};
        part.text = host_string(bytes, length);
        parts.push_back(part);
    } else if (lua_type(L, widget) == LUA_TTABLE) {
        size_t count = (size_t)lua_objlen(L, widget);
        if (count > 8) luaL_error(L, "a widget has at most 8 parts");
        for (size_t at = 1; at <= count; at++) {
            lua_rawgeti(L, widget, (int)at);
            pencere_view_types_widget_part_t part = {};
            int item = lua_gettop(L);
            if (lua_type(L, item) == LUA_TSTRING) {
                size_t length = 0;
                const char* bytes = lua_tolstring(L, item, &length);
                part.text = host_string(bytes, length);
            } else if (lua_type(L, item) == LUA_TTABLE) {
                lua_rawgeti(L, item, 1);
                if (lua_type(L, -1) != LUA_TSTRING) luaL_error(L, "a part of a widget is its words, then what it is drawn in: { \"words\", color = }");
                size_t length = 0;
                const char* bytes = lua_tolstring(L, -1, &length);
                part.text = host_string(bytes, length);
                lua_pop(L, 1);
                part.color = optional_color(L, item, "color");
                part.background = optional_color(L, item, "background");
                view_option_f32_t weight = optional_number(L, item, "weight");
                if (weight.is_some) {
                    part.weight.is_some = true;
                    part.weight.val = weight.val < 100 ? 100 : (weight.val > 900 ? 900 : (uint16_t)weight.val);
                }
                part.italic = optional_flag(L, item, "italic");
                part.id = option_string(L, item, "id", "the id of a part of a widget");
            } else {
                luaL_error(L, "a part of a widget is words or { \"words\", color = }");
            }
            lua_pop(L, 1);
            parts.push_back(part);
        }
    } else {
        luaL_error(L, "a widget is words, or a list of parts: { from =, to =, widget = \"words\" }");
    }
    lua_pop(L, 1);
    out.parts.len = parts.size();
    out.parts.ptr = parts.empty() ? nullptr
                                  : (pencere_view_types_widget_part_t*)xmalloc(parts.size() * sizeof(pencere_view_types_widget_part_t));
    for (size_t at = 0; at < parts.size(); at++) out.parts.ptr[at] = parts[at];
    return out;
}

// `widgets = { hints = { show = "unless_held", hold = "control+alt", size = 0.9, color =, background =,
// radius = 3, pad = 3, gap = 16, margin = 3 } }`: how a group of widgets look and when they show.
pencere_view_types_list_widget_group_t widget_groups_at(lua_State* L, int t) {
    pencere_view_types_list_widget_group_t out = {};
    get(L, t, "widgets");
    int table = lua_gettop(L);
    if (lua_type(L, table) == LUA_TTABLE) {
        static const char* const kShows[] = {"on", "off", "unless_held", "when_held"};
        std::vector<pencere_view_types_widget_group_t> found;
        lua_pushnil(L);
        while (lua_next(L, table) != 0) {
            if (lua_type(L, -2) != LUA_TSTRING || lua_type(L, -1) != LUA_TTABLE)
                luaL_error(L, "the widgets of an editor are groups by name: { hints = { show = \"on\" } }");
            int group = lua_gettop(L);
            pencere_view_types_widget_group_t binding = {};
            size_t length = 0;
            const char* bytes = lua_tolstring(L, -2, &length);
            binding.name = host_string(bytes, length);
            binding.show = (uint8_t)word(L, group, "show", kShows, 4, 0);
            binding.hold = held_at(L, group, "hold");
            binding.size = optional_number(L, group, "size");
            binding.color = optional_color(L, group, "color");
            binding.background = optional_color(L, group, "background");
            binding.radius = optional_number(L, group, "radius");
            binding.pad = optional_number(L, group, "pad");
            binding.gap = optional_number(L, group, "gap");
            binding.margin = optional_number(L, group, "margin");
            found.push_back(binding);
            if (found.size() > 256) luaL_error(L, "an editor has at most 256 groups of widgets");
            lua_pop(L, 1);
        }
        out.len = found.size();
        out.ptr = found.empty() ? nullptr
                                : (pencere_view_types_widget_group_t*)xmalloc(found.size() * sizeof(pencere_view_types_widget_group_t));
        for (size_t at = 0; at < found.size(); at++) out.ptr[at] = found[at];
    } else if (!lua_isnil(L, table)) {
        luaL_error(L, "the widgets of an editor are a table of groups: { hints = { show = \"on\" } }");
    }
    lua_pop(L, 1);
    return out;
}

const char* const kPressings[] = {"place", "extend", "add_caret", "column", "word", "line", "target", "nothing"};

// `gestures = { ["mod+press"] = "target", ["alt+shift+press"] = "column", ["press\xC3\x972"] = "word", … }`:
// a press by its modifiers, button (`middle+press`) and count (`press×2`, `pressx2`); what it does, or
// false (nothing).
pencere_view_types_list_gesture_binding_t gestures_at(lua_State* L, int t) {
    pencere_view_types_list_gesture_binding_t out = {};
    get(L, t, "gestures");
    int table = lua_gettop(L);
    if (lua_type(L, table) == LUA_TTABLE) {
        std::vector<pencere_view_types_gesture_binding_t> found;
        lua_pushnil(L);
        while (lua_next(L, table) != 0) {
            if (lua_type(L, -2) != LUA_TSTRING) luaL_error(L, "the gestures of an editor are named by the press: [\"mod+press\"] = \"target\"");
            std::string key = lua_tostring(L, -2);
            pencere_view_types_gesture_binding_t binding = {};
            binding.count = 1;
            binding.button = PENCERE_VIEW_TYPES_BUTTON_LEFT;
            auto words = split(key, '+');
            for (size_t at = 0; at < words.size(); at++) {
                std::string word = lower_of(words[at]);
                if (at + 1 < words.size()) {
                    if (word == "left") binding.button = PENCERE_VIEW_TYPES_BUTTON_LEFT;
                    else if (word == "right") binding.button = PENCERE_VIEW_TYPES_BUTTON_RIGHT;
                    else if (word == "middle") binding.button = PENCERE_VIEW_TYPES_BUTTON_MIDDLE;
                    else if (!modifier_word(word, binding.mods))
                        luaL_error(L, "the gesture \"%s\": \"%s\" is no modifier or button", key.c_str(), words[at].c_str());
                } else {
                    if (word.rfind("press", 0) != 0)
                        luaL_error(L, "the gesture \"%s\" ends in \"press\" (or \"press\xC3\x97" "2\")", key.c_str());
                    std::string rest = word.substr(5);
                    if (rest.rfind("\xC3\x97", 0) == 0) rest = rest.substr(2);
                    else if (rest.rfind("x", 0) == 0) rest = rest.substr(1);
                    if (!rest.empty()) {
                        binding.count = (uint32_t)atoi(rest.c_str());
                        if (binding.count < 1 || binding.count > 3)
                            luaL_error(L, "the gesture \"%s\" counts 1 to 3 presses", key.c_str());
                    }
                }
            }
            int value = lua_gettop(L);
            if (lua_type(L, value) == LUA_TBOOLEAN && !lua_toboolean(L, value)) {
                binding.pressing = PENCERE_VIEW_TYPES_PRESSING_NOTHING;
            } else if (lua_type(L, value) == LUA_TSTRING) {
                int found_at = -1;
                for (int at = 0; at < 8; at++)
                    if (strcmp(lua_tostring(L, value), kPressings[at]) == 0) found_at = at;
                if (found_at < 0)
                    luaL_error(L, "the gesture \"%s\" does \"%s\": place, extend, add_caret, column, word, line, target or false", key.c_str(), lua_tostring(L, value));
                binding.pressing = (uint8_t)found_at;
            } else {
                luaL_error(L, "the gesture \"%s\" is a word (or false)", key.c_str());
            }
            found.push_back(binding);
            if (found.size() > 256) luaL_error(L, "an editor has at most 256 gestures");
            lua_pop(L, 1);
        }
        out.len = found.size();
        out.ptr = found.empty() ? nullptr
                                : (pencere_view_types_gesture_binding_t*)xmalloc(found.size() * sizeof(pencere_view_types_gesture_binding_t));
        for (size_t at = 0; at < found.size(); at++) out.ptr[at] = found[at];
    } else if (!lua_isnil(L, table)) {
        luaL_error(L, "the gestures of an editor are a table: { [\"mod+press\"] = \"target\" }");
    }
    lua_pop(L, 1);
    return out;
}

// `keymap = { ["mod+d"] = "select_next", ["mod+/"] = false, ["mod+k mod+c"] = "toggle_comment",
// ["ctrl+j"] = { command = "move", movement = "down" } }`: keys (a chord is keys apart by a space) and
// what they do: the word of an editor command that needs none (`"undo"`), `"copy"`, `"cut"`, `"paste"`,
// `"tab"`, `"escape"`, `"to_line_start"`, `"to_line_end"`, the table of a command, or false (nothing of
// the editor's).
pencere_view_types_list_key_binding_t keymap_at(lua_State* L, int t) {
    pencere_view_types_list_key_binding_t out = {};
    get(L, t, "keymap");
    int table = lua_gettop(L);
    if (lua_type(L, table) == LUA_TTABLE) {
        std::vector<pencere_view_types_key_binding_t> found;
        lua_pushnil(L);
        while (lua_next(L, table) != 0) {
            if (lua_type(L, -2) != LUA_TSTRING) luaL_error(L, "the keymap of an editor is named by the keys: [\"mod+d\"] = \"select_next\"");
            std::string key = lua_tostring(L, -2);
            pencere_view_types_key_binding_t binding = {};
            std::vector<std::string> steps;
            size_t from = 0;
            while (from < key.size()) {
                size_t end = key.find(' ', from);
                if (end == std::string::npos) end = key.size();
                if (end > from) steps.push_back(key.substr(from, end - from));
                from = end + 1;
            }
            if (steps.empty() || steps.size() > 4) luaL_error(L, "a chord of keys has one to four keys, apart by spaces: \"%s\"", key.c_str());
            binding.keys.len = steps.size();
            binding.keys.ptr = (view_string_t*)xmalloc(steps.size() * sizeof(view_string_t));
            for (size_t at = 0; at < steps.size(); at++) {
                // The `mod` of a step is the host's main modifier.
                std::string step = steps[at], written;
                size_t pos = 0;
                while (true) {
                    size_t plus = step.find('+', pos);
                    if (plus == std::string::npos || plus + 1 == step.size()) break;
                    std::string token = step.substr(pos, plus - pos);
                    written += (lower_of(token) == "mod" ? std::string(main_modifier()) : token) + "+";
                    pos = plus + 1;
                }
                written += step.substr(pos);
                binding.keys.ptr[at] = host_string(written.data(), written.size());
            }
            int value = lua_gettop(L);
            if (lua_type(L, value) == LUA_TBOOLEAN && !lua_toboolean(L, value)) {
                // Nothing of the editor's.
            } else if (lua_type(L, value) == LUA_TSTRING) {
                std::string name = lua_tostring(L, value);
                binding.action.is_some = true;
                auto& action = binding.action.val;
                if (name == "copy") action.tag = PENCERE_VIEW_TYPES_KEY_ACTION_COPY;
                else if (name == "cut") action.tag = PENCERE_VIEW_TYPES_KEY_ACTION_CUT;
                else if (name == "paste") action.tag = PENCERE_VIEW_TYPES_KEY_ACTION_PASTE;
                else if (name == "tab") action.tag = PENCERE_VIEW_TYPES_KEY_ACTION_TAB;
                else if (name == "escape") action.tag = PENCERE_VIEW_TYPES_KEY_ACTION_ESCAPE;
                else if (name == "to_line_start") action.tag = PENCERE_VIEW_TYPES_KEY_ACTION_TO_LINE_START;
                else if (name == "to_line_end") action.tag = PENCERE_VIEW_TYPES_KEY_ACTION_TO_LINE_END;
                else {
                    int found_at = -1;
                    for (int at = 0; at < 43; at++)
                        if (name == kCommands[at]) found_at = at;
                    if (found_at < 0) luaL_error(L, "the keys \"%s\" do \"%s\", which is no command of an editor", key.c_str(), name.c_str());
                    if (found_at == PENCERE_VIEW_TYPES_EDITOR_COMMAND_MOVE || found_at == PENCERE_VIEW_TYPES_EDITOR_COMMAND_INSERT ||
                        found_at == PENCERE_VIEW_TYPES_EDITOR_COMMAND_FIND || found_at == PENCERE_VIEW_TYPES_EDITOR_COMMAND_REPLACE ||
                        found_at == PENCERE_VIEW_TYPES_EDITOR_COMMAND_REPLACE_ALL)
                        luaL_error(L, "the keys \"%s\" do \"%s\", which needs more than its name: { command = \"%s\", … }", key.c_str(), name.c_str(), name.c_str());
                    action.tag = PENCERE_VIEW_TYPES_KEY_ACTION_COMMAND;
                    action.val.command.tag = (uint8_t)found_at;
                }
            } else if (lua_type(L, value) == LUA_TTABLE) {
                binding.action.is_some = true;
                binding.action.val.tag = PENCERE_VIEW_TYPES_KEY_ACTION_COMMAND;
                binding.action.val.val.command = command_at(L, value);
            } else {
                luaL_error(L, "the keys \"%s\" do a word, a table or false", key.c_str());
            }
            found.push_back(binding);
            if (found.size() > 256) luaL_error(L, "an editor has at most 256 keymap entries");
            lua_pop(L, 1);
        }
        out.len = found.size();
        out.ptr = found.empty() ? nullptr
                                : (pencere_view_types_key_binding_t*)xmalloc(found.size() * sizeof(pencere_view_types_key_binding_t));
        for (size_t at = 0; at < found.size(); at++) out.ptr[at] = found[at];
    } else if (!lua_isnil(L, table)) {
        luaL_error(L, "the keymap of an editor is a table: { [\"mod+d\"] = \"select_next\" }");
    }
    lua_pop(L, 1);
    return out;
}

// A decoration: `{ from =, to =, fold = true }`, `{ from =, to =, line = colour }`,
// `{ from =, to =, gutter = colour }`, or a look: `{ from =, to =, color =, background =, weight =,
// italic =, strike =, underline = colour | { wavy = colour } }`.
pencere_view_types_decoration_t decoration_at(lua_State* L, int t) {
    pencere_view_types_decoration_t out = {};
    out.from = place_of(L, t, "from");
    out.to = place_of(L, t, "to");
    if (flag(L, t, "fold")) {
        out.mark.tag = PENCERE_VIEW_TYPES_MARK_FOLD;
        return out;
    }
    pencere_view_types_option_color_t line = optional_color(L, t, "line");
    if (line.is_some) {
        out.mark.tag = PENCERE_VIEW_TYPES_MARK_LINE;
        out.mark.val.line = line.val;
        return out;
    }
    pencere_view_types_option_color_t gutter = optional_color(L, t, "gutter");
    if (gutter.is_some) {
        out.mark.tag = PENCERE_VIEW_TYPES_MARK_GUTTER;
        auto& mark = out.mark.val.gutter;
        mark.color = gutter.val;
        get(L, t, "column");
        if (lua_type(L, -1) == LUA_TSTRING) {
            size_t length = 0;
            const char* bytes = lua_tolstring(L, -1, &length);
            mark.column = host_string(bytes, length);
        } else if (!lua_isnil(L, -1)) {
            luaL_error(L, "the column of a gutter marker is its name");
        }
        lua_pop(L, 1);
        mark.shape = gutter_shape_at(L, t);
        get(L, t, "id");
        if (lua_type(L, -1) == LUA_TSTRING) {
            size_t length = 0;
            const char* bytes = lua_tolstring(L, -1, &length);
            mark.id = {true, host_string(bytes, length)};
        } else if (!lua_isnil(L, -1)) {
            luaL_error(L, "the id of a gutter marker is a string");
        }
        lua_pop(L, 1);
        get(L, t, "label");
        if (lua_type(L, -1) == LUA_TSTRING) {
            size_t length = 0;
            const char* bytes = lua_tolstring(L, -1, &length);
            mark.label = {true, host_string(bytes, length)};
        } else if (!lua_isnil(L, -1)) {
            luaL_error(L, "the label of a gutter marker is a string");
        }
        lua_pop(L, 1);
        int cursor = word(L, t, "cursor", kCursorNames, 9, -1);
        if (cursor >= 0) mark.cursor = {true, (uint8_t)cursor};
        mark.hover = optional_color(L, t, "hover");
        mark.pressed = optional_color(L, t, "pressed");
        return out;
    }
    pencere_view_types_option_color_t ruler = optional_color(L, t, "ruler");
    if (ruler.is_some) {
        out.mark.tag = PENCERE_VIEW_TYPES_MARK_RULER;
        out.mark.val.ruler.color = ruler.val;
        out.mark.val.ruler.lane = (uint8_t)word(L, t, "lane", kRulerLanes, 4, 0);
        return out;
    }
    get(L, t, "widget");
    if (!lua_isnil(L, -1)) {
        lua_pop(L, 1);
        out.mark.tag = PENCERE_VIEW_TYPES_MARK_WIDGET;
        out.mark.val.widget = widget_at(L, t);
        return out;
    }
    lua_pop(L, 1);
    get(L, t, "anchor");
    if (lua_type(L, -1) == LUA_TSTRING) {
        size_t length = 0;
        const char* bytes = lua_tolstring(L, -1, &length);
        out.mark.tag = PENCERE_VIEW_TYPES_MARK_ANCHOR;
        out.mark.val.anchor = host_string(bytes, length);
        lua_pop(L, 1);
        return out;
    } else if (!lua_isnil(L, -1)) {
        luaL_error(L, "the name of a range is a string: { from =, to =, anchor = \"name\" }");
    }
    lua_pop(L, 1);
    get(L, t, "zone");
    if (!lua_isnil(L, -1)) {
        out.mark.tag = PENCERE_VIEW_TYPES_MARK_ZONE;
        out.mark.val.zone.id = id_of(L, lua_gettop(L));
        lua_pop(L, 1);
        out.mark.val.zone.height = number(L, t, "height", 0.0f);
        static const char* const kZoneSides[] = {"below", "above"};
        out.mark.val.zone.side = (uint8_t)word(L, t, "side", kZoneSides, 2, 0);
        return out;
    }
    lua_pop(L, 1);
    out.mark.tag = PENCERE_VIEW_TYPES_MARK_STYLE;
    auto& look = out.mark.val.style;
    look.color = optional_color(L, t, "color");
    look.background = optional_color(L, t, "background");
    view_option_f32_t weight = optional_number(L, t, "weight");
    if (weight.is_some) {
        look.weight.is_some = true;
        look.weight.val = weight.val < 100 ? 100 : (weight.val > 900 ? 900 : (uint16_t)weight.val);
    }
    look.italic = optional_flag(L, t, "italic");
    look.strike = flag(L, t, "strike");
    look.held = held_at(L, t, "held");
    look.over = flag(L, t, "over");
    int shape = word(L, t, "cursor", kCursorNames, 9, -1);
    if (shape >= 0) look.cursor = {true, (uint8_t)shape};
    get(L, t, "id");
    if (lua_type(L, -1) == LUA_TSTRING) {
        size_t length = 0;
        const char* bytes = lua_tolstring(L, -1, &length);
        look.id = {true, host_string(bytes, length)};
    } else if (!lua_isnil(L, -1)) {
        luaL_error(L, "the id of a style is a string");
    }
    lua_pop(L, 1);
    // A colour is a straight line; { wavy = colour } is a wavy one.
    get(L, t, "underline");
    int underline = lua_gettop(L);
    if (lua_type(L, underline) == LUA_TTABLE) {
        lua_rawgetfield(L, underline, "wavy");
        if (!lua_isnil(L, -1)) {
            look.underline.is_some = true;
            look.underline.val.tag = PENCERE_VIEW_TYPES_UNDERLINE_WAVY;
            look.underline.val.val.wavy = color_at(L, lua_gettop(L));
        }
        lua_pop(L, 1);
    }
    if (!look.underline.is_some && !lua_isnil(L, underline)) {
        look.underline.is_some = true;
        look.underline.val.tag = PENCERE_VIEW_TYPES_UNDERLINE_STRAIGHT;
        look.underline.val.val.straight = color_at(L, underline);
    }
    lua_pop(L, 1);
    return out;
}

// One ask of an editor's, at the absolute index `t`: what it is is the key it has.
pencere_view_types_editor_ask_kind_t ask_at(lua_State* L, int t) {
    pencere_view_types_editor_ask_kind_t out = {};
    get(L, t, "load");
    bool load = !lua_isnil(L, -1);
    lua_pop(L, 1);
    get(L, t, "change");
    bool change = !lua_isnil(L, -1);
    lua_pop(L, 1);
    get(L, t, "select");
    bool select = !lua_isnil(L, -1);
    lua_pop(L, 1);
    get(L, t, "reveal");
    bool reveal = !lua_isnil(L, -1);
    lua_pop(L, 1);
    get(L, t, "decorate");
    bool decorate = !lua_isnil(L, -1);
    lua_pop(L, 1);
    get(L, t, "command");
    bool command = !lua_isnil(L, -1);
    lua_pop(L, 1);
    if (load) {
        out.tag = PENCERE_VIEW_TYPES_EDITOR_ASK_KIND_LOAD;
        out.val.load.text = string_of(L, t, "load");
        out.val.load.more = flag(L, t, "more");
    } else if (change) {
        // { change = { { from =, to =, insert = }, ... }, base = version, history = false, tag = "x" }
        out.tag = PENCERE_VIEW_TYPES_EDITOR_ASK_KIND_CHANGE;
        out.val.change.base = version_of(L, t, "base");
        view_option_bool_t history = optional_flag(L, t, "history");
        out.val.change.history = history.is_some ? history.val : true;
        get(L, t, "tag");
        if (lua_type(L, -1) == LUA_TSTRING) {
            size_t length = 0;
            const char* bytes = lua_tolstring(L, -1, &length);
            out.val.change.tag = {true, host_string(bytes, length)};
        } else if (!lua_isnil(L, -1)) {
            luaL_error(L, "the tag of a change is a string: the page's name for it");
        }
        lua_pop(L, 1);
        get(L, t, "change");
        int list = lua_gettop(L);
        if (lua_type(L, list) != LUA_TTABLE) luaL_error(L, "change is a list of { from =, to =, insert = }");
        std::vector<pencere_view_types_change_t> changes;
        size_t count = list_length(L, list, "change");
        for (size_t at = 0; at < count; at++) {
            lua_rawgeti(L, list, (int)at + 1);
            int item = lua_gettop(L);
            if (lua_type(L, item) != LUA_TTABLE) luaL_error(L, "a change is { from =, to =, insert = }");
            pencere_view_types_change_t one = {};
            one.from = place_of(L, item, "from");
            one.to = place_of(L, item, "to");
            get(L, item, "insert");
            size_t length = 0;
            const char* bytes = lua_type(L, -1) == LUA_TSTRING ? lua_tolstring(L, -1, &length) : "";
            one.insert = host_string(bytes, length);
            lua_pop(L, 2);
            changes.push_back(one);
        }
        lua_pop(L, 1);
        take_list(out.val.change.changes, changes);
    } else if (select) {
        // { select = { { anchor =, head = }, ... }, version =, main = (from 1) }
        out.tag = PENCERE_VIEW_TYPES_EDITOR_ASK_KIND_SELECT;
        out.val.select.version = version_of(L, t, "version");
        float main = number(L, t, "main", 1.0f);
        out.val.select.main = main < 1 ? 0 : (uint32_t)(main - 1);
        get(L, t, "select");
        int list = lua_gettop(L);
        if (lua_type(L, list) != LUA_TTABLE) luaL_error(L, "select is a list of { anchor =, head = }");
        std::vector<pencere_view_types_text_range_t> ranges;
        size_t count = list_length(L, list, "select");
        for (size_t at = 0; at < count; at++) {
            lua_rawgeti(L, list, (int)at + 1);
            int item = lua_gettop(L);
            if (lua_type(L, item) != LUA_TTABLE) luaL_error(L, "a range is { anchor =, head = }");
            pencere_view_types_text_range_t range = {};
            range.anchor = place_of(L, item, "anchor");
            range.head = place_of(L, item, "head");
            lua_pop(L, 1);
            ranges.push_back(range);
        }
        lua_pop(L, 1);
        take_list(out.val.select.ranges, ranges);
    } else if (reveal) {
        // { reveal = place, version =, align = "nearest" | "start" | "center" | "end" }
        static const char* const kAligns[] = {"nearest", "start", "center", "end"};
        static_assert(PENCERE_VIEW_TYPES_REVEAL_ALIGN_NEAREST == 0 && PENCERE_VIEW_TYPES_REVEAL_ALIGN_END == 3,
                      "kAligns is the order of reveal-align");
        out.tag = PENCERE_VIEW_TYPES_EDITOR_ASK_KIND_REVEAL;
        out.val.reveal.at = place_of(L, t, "reveal");
        out.val.reveal.version = version_of(L, t, "version");
        out.val.reveal.align = (uint8_t)word(L, t, "align", kAligns, 4, 0);
    } else if (decorate) {
        // { decorate = "layer", version =, from =, to =, decorations = { ... } }
        out.tag = PENCERE_VIEW_TYPES_EDITOR_ASK_KIND_DECORATE;
        out.val.decorate.layer = string_of(L, t, "decorate");
        out.val.decorate.version = version_of(L, t, "version");
        out.val.decorate.from = place_of(L, t, "from");
        out.val.decorate.to = place_of(L, t, "to");
        get(L, t, "decorations");
        int list = lua_gettop(L);
        std::vector<pencere_view_types_decoration_t> items;
        if (lua_type(L, list) == LUA_TTABLE) {
            size_t count = list_length(L, list, "decorations");
            for (size_t at = 0; at < count; at++) {
                lua_rawgeti(L, list, (int)at + 1);
                if (lua_type(L, -1) != LUA_TTABLE) luaL_error(L, "a decoration is { from =, to =, ... }");
                items.push_back(decoration_at(L, lua_gettop(L)));
                lua_pop(L, 1);
            }
        } else if (!lua_isnil(L, list)) {
            luaL_error(L, "decorations is a list");
        }
        lua_pop(L, 1);
        take_list(out.val.decorate.decorations, items);
    } else if (command) {
        out.tag = PENCERE_VIEW_TYPES_EDITOR_ASK_KIND_COMMAND;
        out.val.command = command_at(L, t);
    } else {
        luaL_error(L, "an ask of an editor is a load, a change, a select, a reveal, a decorate or a command");
    }
    return out;
}

// `editor = { [name] = ask or { ask, ... }, ... }`.
void read_editor_asks(lua_State* L, int t, pencere_view_types_list_editor_ask_t& out) {
    std::vector<pencere_view_types_editor_ask_t> asks;
    lua_pushnil(L);
    while (lua_next(L, t) != 0) {
        int value = lua_gettop(L);
        if (lua_type(L, value) != LUA_TTABLE) luaL_error(L, "editor gives each editor an ask, or a list of them");
        uint64_t editor = id_of(L, value - 1);
        lua_rawgeti(L, value, 1);
        bool list = lua_type(L, -1) == LUA_TTABLE;
        lua_pop(L, 1);
        size_t count = list ? list_length(L, value, "an editor's asks") : 1;
        for (size_t at = 0; at < count; at++) {
            if (asks.size() >= kMaxNodes) luaL_error(L, "editor asks more than a reply may");
            int ask = value;
            if (list) {
                lua_rawgeti(L, value, (int)at + 1);
                ask = lua_gettop(L);
                if (lua_type(L, ask) != LUA_TTABLE) luaL_error(L, "an ask of an editor is a table");
            }
            pencere_view_types_editor_ask_t one = {};
            one.editor = editor;
            one.ask = ask_at(L, ask);
            asks.push_back(one);
            if (list) lua_pop(L, 1);
        }
        lua_pop(L, 1);
    }
    take_list(out, asks);
}

}  // namespace

int reply_call(lua_State* L) {
    auto* job = (ReplyJob*)lua_touserdata(L, 2);
    pencere_view_types_reply_t& out = *job->out;
    memset(&out, 0, sizeof out);
    out.cursor = PENCERE_VIEW_TYPES_CURSOR_ARROW;
    int kind = lua_type(L, 1);
    if (kind == LUA_TNIL) return 0;
    if (kind == LUA_TBOOLEAN) {
        out.redraw = lua_toboolean(L, 1) != 0;
        return 0;
    }
    if (kind != LUA_TTABLE) luaL_error(L, "a reply is nothing, true, false or a table");
    out.redraw = flag(L, 1, "redraw");
    out.close = flag(L, 1, "close");
    static const char* const cursors[] = {"arrow",           "pointer",     "text", "resize_horizontal",
                                          "resize_vertical", "not_allowed", "grab", "grabbing",
                                          "crosshair"};
    out.cursor = (uint8_t)word(L, 1, "cursor", cursors, 9, 0);
    // Where the keyboard goes: "page", or { field = name }.
    get(L, 1, "keyboard");
    if (lua_type(L, -1) == LUA_TSTRING && strcmp(lua_tostring(L, -1), "page") == 0) {
        out.keyboard.is_some = true;
        out.keyboard.val.tag = PENCERE_VIEW_TYPES_KEYBOARD_PAGE;
    } else if (lua_type(L, -1) == LUA_TTABLE) {
        int keyboard = lua_gettop(L);
        get(L, keyboard, "field");
        if (lua_isnil(L, -1)) luaL_error(L, "keyboard is \"page\" or { field = the field's name }");
        out.keyboard.is_some = true;
        out.keyboard.val.tag = PENCERE_VIEW_TYPES_KEYBOARD_FIELD;
        out.keyboard.val.val.field = id_of(L, lua_gettop(L));
        lua_pop(L, 1);
    } else if (!lua_isnil(L, -1)) {
        luaL_error(L, "keyboard is \"page\" or { field = the field's name }");
    }
    lua_pop(L, 1);
    // What the fields are to hold: { [name] = "text", or { text =, select = { from =, to = } or true },
    // ... }.
    get(L, 1, "fill");
    if (lua_type(L, -1) == LUA_TTABLE) {
        int fill = lua_gettop(L);
        std::vector<pencere_view_types_field_fill_t> fills;
        lua_pushnil(L);
        while (lua_next(L, fill) != 0) {
            if (fills.size() >= kMaxNodes) luaL_error(L, "fill names more fields than a page may have");
            pencere_view_types_field_fill_t said = {};
            said.field = id_of(L, lua_gettop(L) - 1);
            int given = lua_gettop(L);
            size_t length = 0;
            if (lua_type(L, given) == LUA_TSTRING) {
                const char* bytes = lua_tolstring(L, given, &length);
                said.text = {true, host_string(bytes, length)};
            } else if (lua_type(L, given) == LUA_TTABLE) {
                get(L, given, "text");
                if (lua_type(L, -1) == LUA_TSTRING) {
                    const char* bytes = lua_tolstring(L, -1, &length);
                    said.text = {true, host_string(bytes, length)};
                } else if (!lua_isnil(L, -1)) {
                    luaL_error(L, "fill: a field's text is a string");
                }
                lua_pop(L, 1);
                // What is selected: { from =, to = } (characters from 0, either way round), or
                // true for all of it.
                get(L, given, "select");
                if (lua_type(L, -1) == LUA_TBOOLEAN && lua_toboolean(L, -1)) {
                    said.select = {true, {0, 4294967295u}};
                } else if (lua_type(L, -1) == LUA_TTABLE) {
                    int range = lua_gettop(L);
                    said.select.is_some = true;
                    said.select.val.from = place_of(L, range, "from");
                    said.select.val.to = place_of(L, range, "to");
                } else if (!lua_isnil(L, -1)) {
                    luaL_error(L, "fill: what to select in a field is { from =, to = }, or true for all of it");
                }
                lua_pop(L, 1);
                if (!said.text.is_some && !said.select.is_some)
                    luaL_error(L, "fill gives a field a text, what to select in it, or both");
            } else {
                luaL_error(L, "fill gives each field a string, or { text =, select = }");
            }
            fills.push_back(said);
            lua_pop(L, 1);
        }
        if (!fills.empty()) {
            size_t bytes = fills.size() * sizeof(pencere_view_types_field_fill_t);
            out.fill.ptr = (pencere_view_types_field_fill_t*)xmalloc(bytes);
            out.fill.len = fills.size();
            memcpy(out.fill.ptr, fills.data(), bytes);
        }
    } else if (!lua_isnil(L, -1)) {
        luaL_error(L, "fill is a table of the fields' names and the texts they are to hold");
    }
    lua_pop(L, 1);
    // What the page asks of its editors: { [name] = ask, or { ask, ... } }.
    get(L, 1, "editor");
    if (lua_type(L, -1) == LUA_TTABLE) {
        read_editor_asks(L, lua_gettop(L), out.editor);
    } else if (!lua_isnil(L, -1)) {
        luaL_error(L, "editor is a table of the editors' names and what is asked of each");
    }
    lua_pop(L, 1);
    get(L, 1, "copy");
    if (lua_type(L, -1) == LUA_TSTRING) {
        size_t length = 0;
        const char* bytes = lua_tolstring(L, -1, &length);
        out.copy.is_some = true;
        out.copy.val = host_string(bytes, length);
    }
    lua_pop(L, 1);
    // A link to open in the person's browser: the host holds it to the rule of a web page's
    // `link` and drops one it may not open, so only what is not an address at all is an error.
    get(L, 1, "link");
    if (lua_type(L, -1) == LUA_TSTRING) {
        size_t length = 0;
        const char* bytes = lua_tolstring(L, -1, &length);
        out.link.is_some = true;
        out.link.val = host_string(bytes, length);
    } else if (!lua_isnil(L, -1)) {
        luaL_error(L, "link is a string: the http or https address to open");
    }
    lua_pop(L, 1);
    // When to wake the page, seconds from now. The prelude puts it (the earliest its timers and
    // the page ask); what is not a number of seconds is still an error of the script's.
    get(L, 1, "wake");
    if (lua_type(L, -1) == LUA_TNUMBER) {
        double seconds = lua_tonumber(L, -1);
        if (!(seconds >= 0 && seconds <= DBL_MAX)) luaL_error(L, "wake is a number of seconds, 0 or more");
        out.wake.is_some = true;
        out.wake.val = seconds;
    } else if (!lua_isnil(L, -1)) {
        luaL_error(L, "wake is a number of seconds, 0 or more");
    }
    lua_pop(L, 1);
    get(L, 1, "menu");
    if (lua_type(L, -1) == LUA_TTABLE) {
        int menu = lua_gettop(L);
        out.menu.is_some = true;
        get(L, menu, "at");
        if (lua_type(L, -1) == LUA_TTABLE) {
            out.menu.val.at.x = number(L, lua_gettop(L), "x", 0.0f);
            out.menu.val.at.y = number(L, lua_gettop(L), "y", 0.0f);
        }
        lua_pop(L, 1);
        // The titles of the entries are one string, as the words of a frame are.
        Arenas titles;
        get(L, menu, "entries");
        if (lua_type(L, -1) == LUA_TTABLE) {
            int list = lua_gettop(L);
            size_t count = (size_t)lua_objlen(L, list);
            if (count > kMaxList) luaL_error(L, "a menu has more entries than it may");
            if (count) {
                out.menu.val.entries.ptr =
                    (pencere_view_types_menu_entry_t*)xmalloc(count * sizeof(pencere_view_types_menu_entry_t));
                memset(out.menu.val.entries.ptr, 0, count * sizeof(pencere_view_types_menu_entry_t));
                out.menu.val.entries.len = count;
            }
            for (size_t at = 0; at < count; at++) {
                auto& entry = out.menu.val.entries.ptr[at];
                lua_rawgeti(L, list, (int)at + 1);
                int entry_index = lua_gettop(L);
                if (lua_type(L, entry_index) == LUA_TSTRING) {
                    // "-" is the line between entries.
                    entry.tag = PENCERE_VIEW_TYPES_MENU_ENTRY_SEPARATOR;
                } else if (lua_type(L, entry_index) == LUA_TTABLE) {
                    entry.tag = PENCERE_VIEW_TYPES_MENU_ENTRY_ITEM;
                    get(L, entry_index, "title");
                    size_t length = 0;
                    const char* bytes = lua_type(L, -1) == LUA_TSTRING ? lua_tolstring(L, -1, &length) : "";
                    entry.val.item.title = add_words(titles, bytes, length);
                    lua_pop(L, 1);
                    // The keys that do the same, written as a command's: `keys`, or `key` as a
                    // page wrote it before (one character is keys too).
                    get(L, entry_index, "keys");
                    bool legacy = lua_isnil(L, -1);
                    if (legacy) {
                        lua_pop(L, 1);
                        get(L, entry_index, "key");
                    }
                    if (lua_type(L, -1) == LUA_TSTRING && lua_objlen(L, -1) > 0) {
                        int words = lua_gettop(L);
                        entry.val.item.keys = {true, legacy ? shortcut_or_character(L, words)
                                                            : shortcut_of_words(L, words)};
                    } else if (!lua_isnil(L, -1) && !(lua_type(L, -1) == LUA_TSTRING)) {
                        luaL_error(L, "the keys of a menu's entry are written as words: \"ctrl+w\"");
                    }
                    lua_pop(L, 1);
                    get(L, entry_index, "enabled");
                    entry.val.item.enabled = lua_isnil(L, -1) ? true : lua_toboolean(L, -1) != 0;
                    lua_pop(L, 1);
                } else {
                    luaL_error(L, "an entry of a menu is a table, or \"-\" for a line");
                }
                lua_pop(L, 1);
            }
        }
        lua_pop(L, 1);
        out.menu.val.titles = take_text(titles.text);
    }
    lua_pop(L, 1);
    return 0;
}

}  // namespace bridge
