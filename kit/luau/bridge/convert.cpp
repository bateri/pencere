#include "convert.h"

#include <stdlib.h>
#include <string.h>

#include <unordered_map>
#include <vector>

#include "lualib.h"

namespace bridge {

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
    out.scroll_x = number(L, s, "scroll_x", 0.0f);
    out.scroll_y = number(L, s, "scroll_y", 0.0f);
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

// The commands of an element at `list`: `{ id =, title =, keys = { key =, mods = {...} }, enabled = }`.
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
        get(L, command, "keys");
        if (lua_type(L, -1) == LUA_TTABLE) {
            int keys = lua_gettop(L);
            get(L, keys, "key");
            size_t length = 0;
            const char* name = lua_type(L, -1) == LUA_TSTRING ? lua_tolstring(L, -1, &length) : "";
            out.keys.is_some = true;
            out.keys.val.key = key_named(L, name, length);
            lua_pop(L, 1);
            get(L, keys, "mods");
            if (lua_type(L, -1) == LUA_TTABLE) {
                int mods = lua_gettop(L);
                out.keys.val.mods.shift = flag(L, mods, "shift");
                out.keys.val.mods.control = flag(L, mods, "control");
                out.keys.val.mods.alt = flag(L, mods, "alt");
                out.keys.val.mods.command = flag(L, mods, "command");
            }
            lua_pop(L, 1);
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
    }
}

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
    // The texts the fields are to hold: { [name] = "text", ... }.
    get(L, 1, "fill");
    if (lua_type(L, -1) == LUA_TTABLE) {
        int fill = lua_gettop(L);
        std::vector<pencere_view_types_field_text_t> texts;
        lua_pushnil(L);
        while (lua_next(L, fill) != 0) {
            if (texts.size() >= kMaxNodes) luaL_error(L, "fill names more fields than a page may have");
            if (lua_type(L, -1) != LUA_TSTRING) luaL_error(L, "fill gives each field a string");
            pencere_view_types_field_text_t said = {};
            said.field = id_of(L, lua_gettop(L) - 1);
            size_t length = 0;
            const char* bytes = lua_tolstring(L, -1, &length);
            said.text = host_string(bytes, length);
            texts.push_back(said);
            lua_pop(L, 1);
        }
        if (!texts.empty()) {
            size_t bytes = texts.size() * sizeof(pencere_view_types_field_text_t);
            out.fill.ptr = (pencere_view_types_field_text_t*)xmalloc(bytes);
            out.fill.len = texts.size();
            memcpy(out.fill.ptr, texts.data(), bytes);
        }
    } else if (!lua_isnil(L, -1)) {
        luaL_error(L, "fill is a table of the fields' names and the texts they are to hold");
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
                    get(L, entry_index, "key");
                    if (lua_type(L, -1) == LUA_TSTRING && lua_objlen(L, -1) > 0) {
                        std::string one = repaired(lua_tostring(L, -1), lua_objlen(L, -1));
                        // The first character of the key.
                        entry.val.item.key = {true, first_character(one)};
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
