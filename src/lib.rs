#![warn(clippy::cargo)]

pub mod format;
pub mod ser;


use crate::ser::{SerError, Serializer};
use serde::Serialize;
use std::io;

/// Serialize the given data structure in lua representation into the IO stream.
///
/// # Errors
///
/// Serialization can fail if `T`'s implementation of `Serialize` decides to
/// fail, or if `T` contains a map with non-string keys.
#[inline]
pub fn to_writer<W, T>(writer: W, value: &T) -> Result<(), SerError>
where
    W: io::Write,
    T: ?Sized + Serialize,
{
    let mut ser = Serializer::new(writer);
    value.serialize(&mut ser)
}

/// Serialize the given data structure as a pretty-printed lua representation into the IO
/// stream.
///
/// # Errors
///
/// Serialization can fail if `T`'s implementation of `Serialize` decides to
/// fail, or if `T` contains a map with non-string keys.
#[inline]
pub fn to_writer_pretty<W, T>(writer: W, value: &T) -> Result<(), SerError>
where
    W: io::Write,
    T: ?Sized + Serialize,
{
    let mut ser = Serializer::pretty(writer);
    value.serialize(&mut ser)
}

/// Serialize the given data structure in lua representation byte vector.
///
/// # Errors
///
/// Serialization can fail if `T`'s implementation of `Serialize` decides to
/// fail, or if `T` contains a map with non-string keys.
#[inline]
pub fn to_vec<T>(value: &T) -> Result<Vec<u8>, SerError>
where
    T: ?Sized + Serialize,
{
    let mut writer = Vec::with_capacity(128);
    to_writer(&mut writer, value)?;
    Ok(writer)
}

/// Serialize the given data structure as a pretty-printed lua representation byte vector.
///
/// # Errors
///
/// Serialization can fail if `T`'s implementation of `Serialize` decides to
/// fail, or if `T` contains a map with non-string keys.
#[inline]
pub fn to_vec_pretty<T>(value: &T) -> Result<Vec<u8>, SerError>
where
    T: ?Sized + Serialize,
{
    let mut writer = Vec::with_capacity(128);
    to_writer_pretty(&mut writer, value)?;
    Ok(writer)
}

/// Serialize the given data structure as a String in lua representation.
///
/// # Errors
///
/// Serialization can fail if `T`'s implementation of `Serialize` decides to
/// fail, or if `T` contains a map with non-string keys.
#[inline]
pub fn to_string<T>(value: &T) -> Result<String, SerError>
where
    T: ?Sized + Serialize,
{
    let vec = to_vec(value)?;
    let string = unsafe {
        // Safety: We do not emit invalid UTF-8.
        String::from_utf8_unchecked(vec)
    };
    Ok(string)
}

/// Serialize the given data structure as a pretty-printed String in lua representation.
///
/// # Errors
///
/// Serialization can fail if `T`'s implementation of `Serialize` decides to
/// fail, or if `T` contains a map with non-string keys.
#[inline]
pub fn to_string_pretty<T>(value: &T) -> Result<String, SerError>
where
    T: ?Sized + Serialize,
{
    let vec = to_vec_pretty(value)?;
    let string = unsafe {
        // Safety: We do not emit invalid UTF-8.
        String::from_utf8_unchecked(vec)
    };
    Ok(string)
}

#[cfg(test)]
mod tests {
    use crate::*;
    use mlua::{Lua, Value};

    #[derive(serde::Serialize)]
    struct Simple {
        id: i64,
        key: Option<&'static str>,
    }

    #[derive(serde::Serialize)]
    struct Nested {
        name: &'static str,
        simple: Simple,
        by_key: indexmap::IndexMap<&'static str, i64>,
        list: Vec<Simple>,
    }

    #[derive(serde::Serialize)]
    struct Renamed {
        plain_name: i64,
        #[serde(rename = "my-field")]
        dashed: i64,
    }

    /// `end` is a valid Rust field name but a Lua keyword.
    #[derive(serde::Serialize)]
    struct Keyword {
        end: i64,
    }

    #[derive(serde::Serialize)]
    struct Empty {}

    #[derive(serde::Serialize)]
    enum Variant {
        Named { x: i64 },
        Renamed {
            #[serde(rename = "x-y")]
            xy: i64,
        },
    }

    fn nested() -> Nested {
        Nested {
            name: "n",
            simple: Simple { id: 1, key: None },
            by_key: [("doremi", 1), ("1abc", 2), ("end", 3)].into_iter().collect(),
            list: vec![Simple { id: 2, key: Some("k") }],
        }
    }

    /// Load `return <value>` in Lua and read the table back.
    fn eval(lua: &Lua, lua_source: &str) -> mlua::Table {
        lua.load(format!("return {lua_source}")).eval().unwrap()
    }

    fn invalid_field(result: Result<String, ser::SerError>) -> (&'static str, &'static str) {
        match result {
            Err(ser::SerError::InvalidFieldName { struct_name, field }) => (struct_name, field),
            other => panic!("expected InvalidFieldName, got {other:?}"),
        }
    }

    #[test]
    fn name_keys() {
        assert!(ser::is_name_key("plain_name"));
        assert!(ser::is_name_key("_x1"));
        for key in ["", "my-field", "1st", "a b", "名前", "until", "goto", "end", "nil"] {
            assert!(!ser::is_name_key(key), "{key:?}");
        }
    }

    #[test]
    fn struct_fields_use_simple_keys() {
        assert_eq!(to_string(&Simple { id: 1, key: Some("a") }).unwrap(), r#"{id=1,key="a"}"#);
        assert_eq!(
            to_string_pretty(&Simple { id: 1, key: None }).unwrap(),
            "{\n  id = 1,\n  key = nil\n}"
        );
    }

    #[test]
    fn invalid_field_names_are_errors() {
        let renamed = Renamed { plain_name: 1, dashed: 2 };
        assert_eq!(invalid_field(to_string(&renamed)), ("Renamed", "my-field"));
        assert_eq!(invalid_field(to_string_pretty(&renamed)), ("Renamed", "my-field"));
        assert_eq!(invalid_field(to_string(&Keyword { end: 1 })), ("Keyword", "end"));
        assert_eq!(invalid_field(to_string(&Variant::Renamed { xy: 1 })), ("Variant", "x-y"));

        let message = to_string(&renamed).unwrap_err().to_string();
        assert!(message.contains(r#"field "my-field" of struct Renamed"#), "{message}");
    }

    #[test]
    fn maps_use_brackets_and_structs_simple_keys() {
        let expected = r#"{
  name = "n",
  simple = {
    id = 1,
    key = nil
  },
  by_key = {
    ["doremi"] = 1,
    ["1abc"] = 2,
    ["end"] = 3
  },
  list = {
    {
      id = 2,
      key = "k"
    }
  }
}"#;
        assert_eq!(to_string_pretty(&nested()).unwrap(), expected);
    }

    #[test]
    fn empty_struct_and_struct_variant() {
        assert_eq!(to_string(&Empty {}).unwrap(), "{}");
        assert_eq!(to_string_pretty(&Empty {}).unwrap(), "{}");
        assert_eq!(to_string(&Variant::Named { x: 1 }).unwrap(), r#"{["Named"]={x=1}}"#);
    }

    #[test]
    fn output_loads_in_lua() {
        let lua = Lua::new();
        for source in [to_string(&nested()).unwrap(), to_string_pretty(&nested()).unwrap()] {
            let table = eval(&lua, &source);
            assert_eq!(table.get::<String>("name").unwrap(), "n");
            let simple: mlua::Table = table.get("simple").unwrap();
            assert_eq!(simple.get::<i64>("id").unwrap(), 1);
            let by_key: mlua::Table = table.get("by_key").unwrap();
            assert_eq!(by_key.get::<i64>("doremi").unwrap(), 1);
            assert_eq!(by_key.get::<i64>("1abc").unwrap(), 2);
            assert_eq!(by_key.get::<i64>("end").unwrap(), 3);
            let list: mlua::Table = table.get("list").unwrap();
            let first: mlua::Table = list.get(1).unwrap();
            assert_eq!(first.get::<String>("key").unwrap(), "k");
        }
    }

    #[test]
    fn it_woks() {
        let file = std::fs::read("test_example.lua").unwrap();

        let lua = Lua::new();
        lua.load(&file).exec().unwrap();

        let table: Value = lua.globals().get("ALIEN").unwrap();
        to_writer_pretty(io::stdout(), &table).unwrap();
    }
}
