// The reader of JSON that the page API's `pencere.json.decode` is, and with it the text of every
// message the page's program sends.

#pragma once

#include "lua.h"

namespace bridge {

// `read(text, null, array, nulls)`: the value `text` says, or `nil` and why, as a Lua function.
//
// Objects and lists are tables; a list with nothing in it has the metatable `array`, so that it
// is written again as a list. `null` is the value `null` in a list, and an absent key in an
// object, which is recorded in `nulls[object][key] = true` (`nulls` is the weak table the prelude
// keeps for `json.has` and `json.is_null`); a text that is `null` alone is nil. The text came from
// a program the person may not know: it is read with a limit on how deep it goes, and nothing in
// it is run. Where it does not say a value, the reason names the place (`... at 12`, counting from
// 1), and anything but a string or a number for `text` is an error of the caller's.
int json_read(lua_State* L);

}  // namespace bridge
