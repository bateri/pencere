#include "convert.h"

#include <stdlib.h>
#include <string.h>

#include <unordered_map>

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

// --- Colours, lengths, tracks ---------------------------------------------------------------

pencere_view_types_tone_t tone_of(lua_State* L, const char* given) {
    static const char* const plain[] = {"background", "foreground", "dim",     "accent", "cursor",
                                        "success",    "error",      "info",    "warning"};
    // The theme's ANSI palette, in the interface's order (so that the tags run on from `black`).
    static const char* const ansi[] = {"black",        "red",          "green",        "yellow",
                                       "blue",         "magenta",      "cyan",         "white",
                                       "bright-black", "bright-red",   "bright-green", "bright-yellow",
                                       "bright-blue",  "bright-magenta", "bright-cyan", "bright-white"};
    pencere_view_types_tone_t tone = {};
    for (int at = 0; at < 9; at++) {
        if (strcmp(given, plain[at]) == 0) {
            tone.tag = (uint8_t)at;
            return tone;
        }
    }
    for (int at = 0; at < 16; at++) {
        if (strcmp(given, ansi[at]) == 0) {
            tone.tag = (uint8_t)(PENCERE_VIEW_TYPES_TONE_BLACK + at);
            return tone;
        }
    }
    if (strcmp(given, "quiet") == 0) {
        tone.tag = PENCERE_VIEW_TYPES_TONE_QUIET;
        return tone;
    }
    if (strcmp(given, "line") == 0) {
        tone.tag = PENCERE_VIEW_TYPES_TONE_LINE;
        return tone;
    }
    if (strcmp(given, "shadow") == 0) {
        tone.tag = PENCERE_VIEW_TYPES_TONE_SHADOW;
        return tone;
    }
    if (strcmp(given, "selection") == 0) {
        tone.tag = PENCERE_VIEW_TYPES_TONE_SELECTION;
        return tone;
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
            tone.tag = PENCERE_VIEW_TYPES_TONE_RAISED;
            tone.val.raised = (uint8_t)steps;
            return tone;
        }
    }
    luaL_error(L, "\"%s\" is not the name of a tone", given);
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

// What a string names: a value if it starts with '#', else a tone.
pencere_view_types_source_t source_of(lua_State* L, const char* given) {
    pencere_view_types_source_t source = {};
    if (given[0] == '#') {
        source.tag = PENCERE_VIEW_TYPES_SOURCE_RGBA;
        source.val.rgba.light = rgba_of(L, given);
    } else {
        source.tag = PENCERE_VIEW_TYPES_SOURCE_ROLE;
        source.val.role = tone_of(L, given);
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
    "a colour is a tone's name, a \"#rrggbb\" or \"#rrggbbaa\" value, { light = value, dark = value }, "
    "or one of those with an opacity: { colour, alpha }";

// A colour: a tone's name, a value, a value for each brightness, or any of those with an
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

// The tracks at `name`: numbers are points, `{fr = n}`, `{min = n, fr = n}` and "auto".
pencere_view_types_list_track_t tracks_of(lua_State* L, int t, const char* name) {
    pencere_view_types_list_track_t out = {nullptr, 0};
    get(L, t, name);
    if (lua_isnil(L, -1)) {
        lua_pop(L, 1);
        return out;
    }
    if (lua_type(L, -1) != LUA_TTABLE) luaL_error(L, "%s is a list of tracks", name);
    int list = lua_gettop(L);
    size_t count = (size_t)lua_objlen(L, list);
    if (count > kMaxList) luaL_error(L, "%s has more tracks than a grid may", name);
    if (count) {
        out.ptr = (pencere_view_types_track_t*)xmalloc(count * sizeof(pencere_view_types_track_t));
        out.len = count;
    }
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
        out.ptr[at] = track;
    }
    lua_pop(L, 1);
    return out;
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

void read_style(lua_State* L, int s, pencere_view_types_style_t& out) {
    out.display = (uint8_t)word(L, s, "display", kDisplay, 3, 0);
    out.direction = (uint8_t)word(L, s, "direction", kDirection, 2, PENCERE_VIEW_TYPES_DIRECTION_COLUMN);
    out.wrap = flag(L, s, "wrap");
    out.absolute = flag(L, s, "absolute");
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
    out.columns = tracks_of(L, s, "columns");
    out.rows = tracks_of(L, s, "rows");
    float span = number(L, s, "column_span", 0.0f);
    out.column_span = span < 0 ? 0 : (span > 65535 ? 65535 : (uint16_t)span);
    out.overflow = (uint8_t)word(L, s, "overflow", kOverflow, 3, 0);
    out.scroll_x = number(L, s, "scroll_x", 0.0f);
    out.scroll_y = number(L, s, "scroll_y", 0.0f);
    out.fill = optional_color(L, s, "fill");
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
};

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

    get(L, element, "kind");
    const char* kind = lua_type(L, -1) == LUA_TSTRING ? lua_tostring(L, -1) : "box";
    bool is_text = strcmp(kind, "text") == 0;
    bool is_spark = strcmp(kind, "spark") == 0;
    bool is_image = strcmp(kind, "image") == 0;
    if (!is_text && !is_spark && !is_image && strcmp(kind, "box") != 0)
        luaL_error(L, "an element is a \"box\", a \"text\", a \"spark\" or an \"image\", not \"%s\"", kind);
    lua_pop(L, 1);

    if (is_text) {
        get(L, element, "text");
        size_t length = 0;
        const char* bytes = lua_type(L, -1) == LUA_TSTRING ? lua_tolstring(L, -1, &length) : "";
        nodes.ptr[at].content.tag = PENCERE_VIEW_TYPES_CONTENT_TEXT;
        nodes.ptr[at].content.val.text = host_string(bytes, length);
        lua_pop(L, 1);
    } else if (is_spark) {
        pencere_view_types_spark_t spark = {};
        get(L, element, "values");
        if (lua_type(L, -1) == LUA_TTABLE) {
            int list = lua_gettop(L);
            size_t count = (size_t)lua_objlen(L, list);
            if (count > kMaxList * 4) luaL_error(L, "a sparkline has more values than it may");
            if (count) {
                spark.values.ptr = (float*)xmalloc(count * sizeof(float));
                spark.values.len = count;
            }
            for (size_t i = 0; i < count; i++) {
                lua_rawgeti(L, list, (int)i + 1);
                spark.values.ptr[i] = (float)lua_tonumber(L, -1);
                lua_pop(L, 1);
            }
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
        get(L, element, "src");
        size_t length = 0;
        const char* bytes = lua_type(L, -1) == LUA_TSTRING ? lua_tolstring(L, -1, &length) : "";
        image.src = host_string(bytes, length);
        lua_pop(L, 1);
        int fit = word(L, element, "fit", kFit, 2, 0);
        image.fit = (pencere_view_types_fit_t)fit;
        image.tint = optional_color(L, element, "tint");
        image.opacity = number(L, element, "opacity", 1.0f);
        nodes.ptr[at].content.tag = PENCERE_VIEW_TYPES_CONTENT_IMAGE;
        nodes.ptr[at].content.val.image = image;
    }

    get(L, element, "style");
    if (lua_type(L, -1) == LUA_TTABLE) {
        pencere_view_types_style_t style = nodes.ptr[at].style;
        read_style(L, lua_gettop(L), style);
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
        nodes.ptr[at].text_field = flag(L, meta, "text_field");
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
    Flat flat = {job->out, 0};
    flatten(L, 1, flat, 1);
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
    return button == PENCERE_VIEW_TYPES_BUTTON_RIGHT ? "right" : "left";
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
            set_number(L, "dy", event.val.scroll.dy);
            break;
        case PENCERE_VIEW_TYPES_EVENT_KEY: {
            static const char* const keys[] = {"up",   "down", "left", "right",  "page_up", "page_down",
                                               "home", "end",  "enter", "escape", "tab"};
            set_word(L, "type", "key");
            const auto& key = event.val.key.key;
            if (key.tag == PENCERE_VIEW_TYPES_KEY_CHARACTER) push_character(L, key.val.character);
            else lua_pushstring(L, key.tag < 11 ? keys[key.tag] : "unknown");
            lua_setfield(L, -2, "key");
            set_mods(L, event.val.key.mods);
            break;
        }
        case PENCERE_VIEW_TYPES_EVENT_TEXT:
            set_word(L, "type", "text");
            lua_pushlstring(L, (const char*)event.val.text.ptr, event.val.text.len);
            lua_setfield(L, -2, "text");
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
    static const char* const cursors[] = {"arrow", "pointer", "text"};
    out.cursor = (uint8_t)word(L, 1, "cursor", cursors, 3, 0);
    static const char* const keyboards[] = {"page", "field"};
    int keyboard = word(L, 1, "keyboard", keyboards, 2, -1);
    if (keyboard >= 0) out.keyboard = {true, (uint8_t)keyboard};
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
                    entry.val.item.title = host_string(bytes, length);
                    lua_pop(L, 1);
                    get(L, entry_index, "key");
                    if (lua_type(L, -1) == LUA_TSTRING && lua_objlen(L, -1) > 0) {
                        std::string one = repaired(lua_tostring(L, -1), lua_objlen(L, -1));
                        // The first character of the key.
                        uint32_t cp = (unsigned char)one[0];
                        size_t extra = cp >= 0xF0 ? 3 : cp >= 0xE0 ? 2 : cp >= 0xC0 ? 1 : 0;
                        cp = extra == 0 ? cp : (cp & (0x3F >> extra));
                        for (size_t k = 1; k <= extra && k < one.size(); k++) cp = (cp << 6) | ((unsigned char)one[k] & 0x3F);
                        entry.val.item.key = {true, cp};
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
    }
    lua_pop(L, 1);
    return 0;
}

}  // namespace bridge
