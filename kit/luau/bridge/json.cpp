#include "json.h"

#include <stdlib.h>
#include <string.h>

#include <string>

#include "lualib.h"

namespace bridge {
namespace {

// Arrays and objects nested this many levels or more, the outermost counting as the first, are no
// JSON message (specification section 4.5). The prelude's writer holds to the same number
// (`MAX_DEPTH` in `prelude/30-json.lua`); keep the two equal.
constexpr int kMaxDepth = 128;

bool is_digit(char c) { return c >= '0' && c <= '9'; }

bool is_hex(char c) { return is_digit(c) || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F'); }

int hex_value(char c) {
    if (is_digit(c)) return c - '0';
    if (c >= 'a' && c <= 'f') return c - 'a' + 10;
    return c - 'A' + 10;
}

// A control character as Lua's `%c` names it in the C locale.
bool is_control(char c) {
    unsigned char byte = (unsigned char)c;
    return byte < 0x20 || byte == 0x7f;
}

void append_utf8(std::string& out, unsigned code) {
    if (code < 0x80) {
        out.push_back((char)code);
    } else if (code < 0x800) {
        out.push_back((char)(0xC0 | (code >> 6)));
        out.push_back((char)(0x80 | (code & 0x3F)));
    } else if (code < 0x10000) {
        out.push_back((char)(0xE0 | (code >> 12)));
        out.push_back((char)(0x80 | ((code >> 6) & 0x3F)));
        out.push_back((char)(0x80 | (code & 0x3F)));
    } else {
        out.push_back((char)(0xF0 | (code >> 18)));
        out.push_back((char)(0x80 | ((code >> 12) & 0x3F)));
        out.push_back((char)(0x80 | ((code >> 6) & 0x3F)));
        out.push_back((char)(0x80 | (code & 0x3F)));
    }
}

// One reading of one text. Places are offsets from 0; the reasons count from 1, as the Lua reader
// before this one did.
struct Reader {
    lua_State* L;
    const char* text;
    size_t size;
    // Stack indices of the arguments the value is made with.
    int null_at;
    int array_at;
    int nulls_at;
    // Why it failed, and where.
    std::string why;

    bool fail(size_t at, const char* reason) {
        why = std::string(reason) + " at " + std::to_string(at + 1);
        return false;
    }

    char at(size_t place) const { return place < size ? text[place] : '\0'; }

    bool has(size_t place, char c) const { return place < size && text[place] == c; }

    size_t skip(size_t place) const {
        while (place < size && (text[place] == ' ' || text[place] == '\t' || text[place] == '\r' || text[place] == '\n'))
            place++;
        return place;
    }

    bool four_hex(size_t place) const {
        if (place + 4 > size) return false;
        for (size_t i = 0; i < 4; i++)
            if (!is_hex(text[place + i])) return false;
        return true;
    }

    unsigned code_at(size_t place) const {
        unsigned code = 0;
        for (size_t i = 0; i < 4; i++) code = code * 16 + (unsigned)hex_value(text[place + i]);
        return code;
    }

    // The string whose opening quote is at `place`; `place` is left after the closing one.
    bool read_string(size_t& place, std::string& out) {
        size_t from = place + 1;
        for (;;) {
            size_t stop = from;
            while (stop < size && text[stop] != '"' && text[stop] != '\\' && !is_control(text[stop])) stop++;
            if (stop >= size) return fail(place, "a string that does not end");
            out.append(text + from, stop - from);
            char character = text[stop];
            if (character == '"') {
                place = stop + 1;
                return true;
            }
            if (character != '\\') return fail(stop, "a control character in a string");
            char escaped = at(stop + 1);
            if (stop + 1 >= size) return fail(stop, "a bad escape");
            switch (escaped) {
                case '"': out.push_back('"'); from = stop + 2; break;
                case '\\': out.push_back('\\'); from = stop + 2; break;
                case '/': out.push_back('/'); from = stop + 2; break;
                case 'b': out.push_back('\b'); from = stop + 2; break;
                case 'f': out.push_back('\f'); from = stop + 2; break;
                case 'n': out.push_back('\n'); from = stop + 2; break;
                case 'r': out.push_back('\r'); from = stop + 2; break;
                case 't': out.push_back('\t'); from = stop + 2; break;
                case 'u': {
                    if (!four_hex(stop + 2)) return fail(stop, "a bad \\u escape");
                    unsigned code = code_at(stop + 2);
                    from = stop + 6;
                    if (code >= 0xD800 && code < 0xDC00) {
                        if (has(from, '\\') && has(from + 1, 'u') && four_hex(from + 2) && code_at(from + 2) >= 0xDC00 &&
                            code_at(from + 2) < 0xE000) {
                            code = 0x10000 + (code - 0xD800) * 0x400 + (code_at(from + 2) - 0xDC00);
                            from += 6;
                        } else {
                            code = 0xFFFD;
                        }
                    } else if (code >= 0xDC00 && code < 0xE000) {
                        code = 0xFFFD;
                    }
                    append_utf8(out, code);
                    break;
                }
                default: return fail(stop, "a bad escape");
            }
        }
    }

    // Notes that `key` of the object at stack index `object` was `null`.
    void note_null(int object, const char* key, size_t length) {
        lua_pushvalue(L, object);
        lua_rawget(L, nulls_at);
        if (lua_isnil(L, -1)) {
            lua_pop(L, 1);
            lua_newtable(L);
            lua_pushvalue(L, object);
            lua_pushvalue(L, -2);
            lua_rawset(L, nulls_at);
        }
        lua_pushlstring(L, key, length);
        lua_pushboolean(L, 1);
        lua_rawset(L, -3);
        lua_pop(L, 1);
    }

    // The value at `place`, pushed; `place` is left after it.
    bool read(size_t& place, int depth) {
        place = skip(place);
        char character = at(place);
        if ((character == '{' || character == '[') && depth >= kMaxDepth) return fail(place, "nested too deep");
        luaL_checkstack(L, 8, "a JSON text nested too deep");
        if (character == '{') {
            lua_newtable(L);
            int object = lua_gettop(L);
            place = skip(place + 1);
            if (has(place, '}')) {
                place++;
                return true;
            }
            for (;;) {
                if (!has(place, '"')) return fail(place, "a key was expected");
                std::string key;
                if (!read_string(place, key)) return false;
                place = skip(place);
                if (!has(place, ':')) return fail(place, "a colon was expected");
                place++;
                if (!read(place, depth + 1)) return false;
                if (lua_rawequal(L, -1, null_at)) {
                    lua_pop(L, 1);
                    note_null(object, key.data(), key.size());
                } else {
                    lua_pushlstring(L, key.data(), key.size());
                    lua_insert(L, -2);
                    lua_rawset(L, object);
                }
                place = skip(place);
                if (has(place, '}')) {
                    place++;
                    return true;
                }
                if (!has(place, ',')) return fail(place, "a comma or a brace was expected");
                place = skip(place + 1);
            }
        }
        if (character == '[') {
            lua_newtable(L);
            int list = lua_gettop(L);
            place = skip(place + 1);
            if (has(place, ']')) {
                // An empty list is written again as one, not as an empty object.
                place++;
                lua_pushvalue(L, array_at);
                lua_setmetatable(L, list);
                return true;
            }
            int count = 0;
            for (;;) {
                if (!read(place, depth + 1)) return false;
                lua_rawseti(L, list, ++count);
                place = skip(place);
                if (has(place, ']')) {
                    place++;
                    return true;
                }
                if (!has(place, ',')) return fail(place, "a comma or a bracket was expected");
                place++;
            }
        }
        if (character == '"') {
            std::string value;
            if (!read_string(place, value)) return false;
            lua_pushlstring(L, value.data(), value.size());
            return true;
        }
        if (place + 4 <= size && memcmp(text + place, "true", 4) == 0) {
            place += 4;
            lua_pushboolean(L, 1);
            return true;
        }
        if (place + 5 <= size && memcmp(text + place, "false", 5) == 0) {
            place += 5;
            lua_pushboolean(L, 0);
            return true;
        }
        if (place + 4 <= size && memcmp(text + place, "null", 4) == 0) {
            place += 4;
            lua_pushvalue(L, null_at);
            return true;
        }
        // `-?%d+%.?%d*[eE]?[+-]?%d*`: what the number looks like; whether it is one is `strtod`'s
        // to say (so "1e" and "1+2" are looked at and refused, "1." and "01" are numbers).
        size_t end = place;
        if (has(end, '-')) end++;
        size_t digits = end;
        while (end < size && is_digit(text[end])) end++;
        if (end == digits) return fail(place, "a value was expected");
        if (has(end, '.')) end++;
        while (end < size && is_digit(text[end])) end++;
        if (has(end, 'e') || has(end, 'E')) end++;
        if (has(end, '+') || has(end, '-')) end++;
        while (end < size && is_digit(text[end])) end++;
        std::string number(text + place, end - place);
        char* stop = nullptr;
        double value = strtod(number.c_str(), &stop);
        if (stop == number.c_str() || *stop != '\0') return fail(place, "a bad number");
        place = end;
        lua_pushnumber(L, value);
        return true;
    }
};

}  // namespace

int json_read(lua_State* L) {
    size_t size = 0;
    const char* text = luaL_checklstring(L, 1, &size);
    luaL_checktype(L, 2, LUA_TTABLE);
    luaL_checktype(L, 3, LUA_TTABLE);
    luaL_checktype(L, 4, LUA_TTABLE);
    lua_settop(L, 4);
    Reader reader = {L, text, size, 2, 3, 4, std::string()};
    size_t place = 0;
    if (!reader.read(place, 1)) {
        lua_settop(L, 4);
        lua_pushnil(L);
        lua_pushlstring(L, reader.why.data(), reader.why.size());
        return 2;
    }
    if (reader.skip(place) < size) {
        lua_settop(L, 4);
        lua_pushnil(L);
        std::string why = "more after the value at " + std::to_string(place + 1);
        lua_pushlstring(L, why.data(), why.size());
        return 2;
    }
    // A text that is `null` alone says nothing.
    if (lua_rawequal(L, -1, 2)) {
        lua_settop(L, 4);
        lua_pushnil(L);
    }
    return 1;
}

}  // namespace bridge
