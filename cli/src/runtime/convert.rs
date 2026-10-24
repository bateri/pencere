//! JSON and Lua values into one another: what a page sends a call is a Lua table in the app, and
//! what the app answers is JSON on the wire.
//!
//! A table is an array when its keys are 1, 2, 3 … with none missing (and an empty table is an
//! array: `[]` is what a list with nothing in it is for a page); a table that is meant to be an
//! object with no members is made with `json.object()`, which is also what an empty `{}` of a
//! JSON text is read as, so that it comes back out the same.

use mlua::{Lua, Table, Value};
use serde_json::{Map, Number, Value as Json};

/// The name the metatable of an empty object is kept under in the registry.
const OBJECT: &str = "pencere.json.object";

/// How deep a value may be before it is taken for one that contains itself.
const DEPTH: usize = 128;

/// The metatable that marks an empty table as an object.
pub fn object_marker(lua: &Lua) -> mlua::Result<Table> {
    if let Ok(marker) = lua.named_registry_value::<Table>(OBJECT) {
        return Ok(marker);
    }
    let marker = lua.create_table()?;
    marker.set("__metatable", "json.object")?;
    lua.set_named_registry_value(OBJECT, marker.clone())?;
    Ok(marker)
}

/// The JSON of a Lua value.
pub fn to_json(lua: &Lua, value: &Value) -> Result<Json, String> {
    let marker = object_marker(lua).map_err(|e| e.to_string())?;
    convert(value, &marker, 0)
}

fn convert(value: &Value, marker: &Table, depth: usize) -> Result<Json, String> {
    if depth > DEPTH {
        return Err("the value is too deep, or it contains itself".into());
    }
    Ok(match value {
        Value::Nil => Json::Null,
        Value::Boolean(flag) => Json::Bool(*flag),
        Value::Integer(n) => Json::Number(Number::from(*n)),
        Value::Number(n) => number(*n)?,
        Value::String(text) => {
            let text = text
                .to_str()
                .map_err(|_| "a string that is not UTF-8 cannot be sent as JSON".to_owned())?;
            Json::String(text.to_owned())
        }
        Value::Table(table) => {
            let is_object = table
                .metatable()
                .is_some_and(|meta| meta.equals(marker).unwrap_or(false));
            let length = table.raw_len();
            let mut entries = 0usize;
            for pair in table.pairs::<Value, Value>() {
                pair.map_err(|e| e.to_string())?;
                entries += 1;
            }
            if !is_object && entries == length {
                let mut list = Vec::with_capacity(length);
                for index in 1..=length {
                    let item: Value = table.raw_get(index).map_err(|e| e.to_string())?;
                    list.push(convert(&item, marker, depth + 1)?);
                }
                Json::Array(list)
            } else {
                let mut object = Map::new();
                for pair in table.pairs::<Value, Value>() {
                    let (key, item) = pair.map_err(|e| e.to_string())?;
                    let Value::String(key) = key else {
                        return Err(
                            "a table with keys that are not strings, and not a list, cannot be sent as JSON"
                                .into(),
                        );
                    };
                    let key = key
                        .to_str()
                        .map_err(|_| "a key that is not UTF-8 cannot be sent as JSON".to_owned())?
                        .to_owned();
                    object.insert(key, convert(&item, marker, depth + 1)?);
                }
                Json::Object(object)
            }
        }
        other => {
            return Err(format!("a {} cannot be sent as JSON", other.type_name()));
        }
    })
}

fn number(n: f64) -> Result<Json, String> {
    if !n.is_finite() {
        return Err("a number that is not finite cannot be sent as JSON".into());
    }
    // A whole number is written as one (`3`, not `3.0`): the page reads it as the integer it is.
    if n.fract() == 0.0 && n.abs() < 9_007_199_254_740_992.0 {
        return Ok(Json::Number(Number::from(n as i64)));
    }
    Number::from_f64(n)
        .map(Json::Number)
        .ok_or_else(|| "a number that is not finite cannot be sent as JSON".into())
}

/// The Lua value of some JSON.
pub fn from_json(lua: &Lua, value: &Json) -> mlua::Result<Value> {
    Ok(match value {
        Json::Null => Value::Nil,
        Json::Bool(flag) => Value::Boolean(*flag),
        Json::Number(n) => match n.as_i64() {
            Some(whole) => Value::Number(whole as f64),
            None => Value::Number(n.as_f64().unwrap_or(0.0)),
        },
        Json::String(text) => Value::String(lua.create_string(text)?),
        Json::Array(items) => {
            let table = lua.create_table_with_capacity(items.len(), 0)?;
            for (index, item) in items.iter().enumerate() {
                // A `null` is nothing, a hole in the list: the items after it keep their places.
                table.raw_set(index + 1, from_json(lua, item)?)?;
            }
            Value::Table(table)
        }
        Json::Object(members) => {
            let table = lua.create_table_with_capacity(0, members.len())?;
            for (key, item) in members {
                table.raw_set(key.as_str(), from_json(lua, item)?)?;
            }
            if members.is_empty() {
                table.set_metatable(Some(object_marker(lua)?))?;
            }
            Value::Table(table)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn lua_value(lua: &Lua, source: &str) -> Value {
        lua.load(source).eval().expect("a value")
    }

    #[test]
    fn lists_objects_numbers_and_nothing_go_to_json() {
        let lua = Lua::new();
        let value = lua_value(
            &lua,
            r#"return { list = {1, 2.5, 3}, name = "a", nested = { ok = true }, empty = {}, none = nil }"#,
        );
        assert_eq!(
            to_json(&lua, &value).expect("json"),
            json!({ "list": [1, 2.5, 3], "name": "a", "nested": { "ok": true }, "empty": [] })
        );
    }

    #[test]
    fn an_object_with_no_members_stays_one() {
        let lua = Lua::new();
        let value = from_json(&lua, &json!({ "a": {}, "b": [] })).expect("lua");
        assert_eq!(
            to_json(&lua, &value).expect("json"),
            json!({ "a": {}, "b": [] })
        );
    }

    #[test]
    fn what_cannot_be_json_is_told_why() {
        let lua = Lua::new();
        for (source, wanted) in [
            ("return { [1] = 1, [3] = 3 }", "not strings"),
            ("return 0/0", "finite"),
            ("return function() end", "function"),
            ("return \"\\xff\"", "UTF-8"),
            ("local t = {} t.t = t return t", "too deep"),
        ] {
            let value = lua_value(&lua, source);
            let why = to_json(&lua, &value).expect_err(source);
            assert!(why.contains(wanted), "{source}: {why}");
        }
    }
}
