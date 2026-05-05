//! Serde helpers for values that need a JSON representation which JavaScript can
//! decode without losing precision.
//!
//! This crate exposes a single wrapper type, [`Jsone<T>`] which wraps any field or type,
//! and handles the lossless serialization and deserialization of values in Rust. You must
//! pair this with the frontend encoder or decoder to turn the response back to regular JavaScript.
//!
//! **Be aware** if the decoder detects a bigint value, it will may return a JS [`BigInt`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/BigInt) object instead of a `number`
//! so your code must be able to handle this!
//!
//! ## Supported values
//!
//! We support losslessly serializing and deserializing:
//!
//! - special-case floating-point numbers like [`f64::NAN`], [`f64::INFINITY`], [`f64::NEG_INFINITY`]
//! - Rust integer types which are larger than [`Number.MAX_SAFE_INTEGER`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Number/MAX_SAFE_INTEGER) or smaller than [`Number.MIN_SAFE_INTEGER`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Number/MIN_SAFE_INTEGER)
//!
//! We do this by encoding them into a special JSON struct like:
//!  - For special-cases we use the form `{ "$$jsone$remap$$": 1 }` where each number represents a known special case.
//!  - For integer outside safe range: `{ "$$jsone$remap$$": "12345678901234567890" }`
//!
//! The JS decoder can find this object key and replace the whole object with the correct value, making end to end lossless serialization and deserialization possible.
//!
//! # Example
//!
//! ```
//! use jsone::Jsone;
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Debug, Deserialize, PartialEq, Serialize)]
//! struct Payload {
//!     id: Jsone<i32>,
//! }
//!
//! let json = serde_json::to_string(&Payload { id: Jsone(42) }).unwrap();
//! assert_eq!(json, r#"{"id":42}"#);
//!
//! let payload: Payload = serde_json::from_str(&json).unwrap();
//! assert_eq!(payload.id, Jsone(42));
//! ```
#![cfg_attr(docsrs, feature(doc_cfg))]

use serde::de::{Error, IntoDeserializer, MapAccess, SeqAccess, Visitor};
use serde::ser::{
    SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant, SerializeTuple,
    SerializeTupleStruct, SerializeTupleVariant,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer, forward_to_deserialize_any};
use std::fmt;

// This is used to hide the raw string from being printed by Rustdoc inline.
// We do that manually using a revealable section.
const JS_RUNTIME_STR: &str = include_str!("./index.js");

/// The JavaScript runtime for encoding and decoding.
///
/// You can expand and copy the runtime into your project! Otherwise you can use this [`RUNTIME`] constant directly!
///
#[doc = "<details>"]
#[doc = "<summary>Show JavaScript runtime</summary>"]
#[doc = ""]
#[doc = "```ts"]
#[doc = include_str!("./index.js")]
#[doc = "```"]
#[doc = "</details>"]
pub const JS_RUNTIME: &str = JS_RUNTIME_STR;

/// Field name used for the JSON object that marks the remapped value.
/// Changing this would require the frontend to also reflect the new value so would be a majorly breaking change.
const REMAP_KEY: &str = "$$jsone$remap$$";
const MAX_SAFE_INTEGER: i128 = 9_007_199_254_740_991;
const MIN_SAFE_INTEGER: i128 = -MAX_SAFE_INTEGER;

/// Serde wrapper that applies the remap logic.
///
/// Refer to the crate documentation for an explanation.
///
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Copy)]
pub struct Jsone<T>(pub T);

impl<T> Serialize for Jsone<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(JsoneSerializer(serializer))
    }
}

struct JsoneValueSerialize<'a, T: ?Sized>(&'a T);

impl<T> Serialize for JsoneValueSerialize<'_, T>
where
    T: ?Sized + Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(JsoneSerializer(serializer))
    }
}

struct JsoneSerializer<S>(S);

macro_rules! serialize_signed_number {
    ($method:ident, $ty:ty) => {
        fn $method(self, value: $ty) -> Result<Self::Ok, Self::Error> {
            if (MIN_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&(value as i128)) {
                self.0.$method(value)
            } else {
                serialize_remapped_value(self.0, &value.to_string())
            }
        }
    };
}

macro_rules! serialize_unsigned_number {
    ($method:ident, $ty:ty) => {
        fn $method(self, value: $ty) -> Result<Self::Ok, Self::Error> {
            if value as u128 <= MAX_SAFE_INTEGER as u128 {
                self.0.$method(value)
            } else {
                serialize_remapped_value(self.0, &value.to_string())
            }
        }
    };
}

fn serialize_remapped_value<S, T>(serializer: S, value: &T) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: ?Sized + Serialize,
{
    let mut map = serializer.serialize_map(Some(1))?;
    map.serialize_entry(REMAP_KEY, value)?;
    map.end()
}

impl<S> Serializer for JsoneSerializer<S>
where
    S: Serializer,
{
    type Ok = S::Ok;
    type Error = S::Error;
    type SerializeSeq = JsoneSerializeSeq<S::SerializeSeq>;
    type SerializeTuple = JsoneSerializeTuple<S::SerializeTuple>;
    type SerializeTupleStruct = JsoneSerializeTupleStruct<S::SerializeTupleStruct>;
    type SerializeTupleVariant = JsoneSerializeTupleVariant<S::SerializeTupleVariant>;
    type SerializeMap = JsoneSerializeMap<S::SerializeMap>;
    type SerializeStruct = JsoneSerializeStruct<S::SerializeStruct>;
    type SerializeStructVariant = JsoneSerializeStructVariant<S::SerializeStructVariant>;

    fn serialize_bool(self, value: bool) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_bool(value)
    }

    serialize_signed_number!(serialize_i8, i8);
    serialize_signed_number!(serialize_i16, i16);
    serialize_signed_number!(serialize_i32, i32);
    serialize_signed_number!(serialize_i64, i64);
    serialize_signed_number!(serialize_i128, i128);
    serialize_unsigned_number!(serialize_u8, u8);
    serialize_unsigned_number!(serialize_u16, u16);
    serialize_unsigned_number!(serialize_u32, u32);
    serialize_unsigned_number!(serialize_u64, u64);
    serialize_unsigned_number!(serialize_u128, u128);

    fn serialize_f32(self, value: f32) -> Result<Self::Ok, Self::Error> {
        self.serialize_f64(value.into())
    }

    fn serialize_f64(self, value: f64) -> Result<Self::Ok, Self::Error> {
        if value.is_nan() {
            serialize_remapped_value(self.0, &1u8)
        } else if value == f64::INFINITY {
            serialize_remapped_value(self.0, &2u8)
        } else if value == f64::NEG_INFINITY {
            serialize_remapped_value(self.0, &3u8)
        } else if value >= MIN_SAFE_INTEGER as f64 && value <= MAX_SAFE_INTEGER as f64 {
            self.0.serialize_f64(value)
        } else {
            serialize_remapped_value(self.0, &value.to_string())
        }
    }

    fn serialize_char(self, value: char) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_char(value)
    }

    fn serialize_str(self, value: &str) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_str(value)
    }

    fn serialize_bytes(self, value: &[u8]) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_bytes(value)
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_none()
    }

    fn serialize_some<T>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.0.serialize_some(&JsoneValueSerialize(value))
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_unit()
    }

    fn serialize_unit_struct(self, name: &'static str) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_unit_struct(name)
    }

    fn serialize_unit_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_unit_variant(name, variant_index, variant)
    }

    fn serialize_newtype_struct<T>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.0
            .serialize_newtype_struct(name, &JsoneValueSerialize(value))
    }

    fn serialize_newtype_variant<T>(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.0
            .serialize_newtype_variant(name, variant_index, variant, &JsoneValueSerialize(value))
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        self.0.serialize_seq(len).map(JsoneSerializeSeq)
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        self.0.serialize_tuple(len).map(JsoneSerializeTuple)
    }

    fn serialize_tuple_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        self.0
            .serialize_tuple_struct(name, len)
            .map(JsoneSerializeTupleStruct)
    }

    fn serialize_tuple_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        self.0
            .serialize_tuple_variant(name, variant_index, variant, len)
            .map(JsoneSerializeTupleVariant)
    }

    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        self.0.serialize_map(len).map(JsoneSerializeMap)
    }

    fn serialize_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        self.0.serialize_struct(name, len).map(JsoneSerializeStruct)
    }

    fn serialize_struct_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        self.0
            .serialize_struct_variant(name, variant_index, variant, len)
            .map(JsoneSerializeStructVariant)
    }

    fn collect_str<T>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + fmt::Display,
    {
        self.0.collect_str(value)
    }
}

struct JsoneSerializeSeq<S>(S);

impl<S> SerializeSeq for JsoneSerializeSeq<S>
where
    S: SerializeSeq,
{
    type Ok = S::Ok;
    type Error = S::Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.0.serialize_element(&JsoneValueSerialize(value))
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.0.end()
    }
}

struct JsoneSerializeTuple<S>(S);

impl<S> SerializeTuple for JsoneSerializeTuple<S>
where
    S: SerializeTuple,
{
    type Ok = S::Ok;
    type Error = S::Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.0.serialize_element(&JsoneValueSerialize(value))
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.0.end()
    }
}

struct JsoneSerializeTupleStruct<S>(S);

impl<S> SerializeTupleStruct for JsoneSerializeTupleStruct<S>
where
    S: SerializeTupleStruct,
{
    type Ok = S::Ok;
    type Error = S::Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.0.serialize_field(&JsoneValueSerialize(value))
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.0.end()
    }
}

struct JsoneSerializeTupleVariant<S>(S);

impl<S> SerializeTupleVariant for JsoneSerializeTupleVariant<S>
where
    S: SerializeTupleVariant,
{
    type Ok = S::Ok;
    type Error = S::Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.0.serialize_field(&JsoneValueSerialize(value))
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.0.end()
    }
}

struct JsoneSerializeMap<S>(S);

impl<S> SerializeMap for JsoneSerializeMap<S>
where
    S: SerializeMap,
{
    type Ok = S::Ok;
    type Error = S::Error;

    fn serialize_key<T>(&mut self, key: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.0.serialize_key(key)
    }

    fn serialize_value<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.0.serialize_value(&JsoneValueSerialize(value))
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.0.end()
    }
}

struct JsoneSerializeStruct<S>(S);

impl<S> SerializeStruct for JsoneSerializeStruct<S>
where
    S: SerializeStruct,
{
    type Ok = S::Ok;
    type Error = S::Error;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.0.serialize_field(key, &JsoneValueSerialize(value))
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.0.end()
    }
}

struct JsoneSerializeStructVariant<S>(S);

impl<S> SerializeStructVariant for JsoneSerializeStructVariant<S>
where
    S: SerializeStructVariant,
{
    type Ok = S::Ok;
    type Error = S::Error;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.0.serialize_field(key, &JsoneValueSerialize(value))
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.0.end()
    }
}

impl<'de, T> Deserialize<'de> for Jsone<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;

        T::deserialize(JsoneJsonValueDeserializer(unwrap_remapped_values(value)))
            .map(Jsone)
            .map_err(D::Error::custom)
    }
}

fn unwrap_remapped_values(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(unwrap_remapped_values).collect())
        }
        serde_json::Value::Object(mut values) => {
            if values.len() == 1 && values.contains_key(REMAP_KEY) {
                let value = values.remove(REMAP_KEY).expect("remap key exists");

                return match value {
                    serde_json::Value::Number(number)
                        if matches!(number.as_u64(), Some(1 | 2 | 3)) =>
                    {
                        serde_json::Value::Number(number)
                    }
                    serde_json::Value::Number(number) => {
                        serde_json::json!({ REMAP_KEY: number })
                    }
                    value => unwrap_remapped_values(value),
                };
            }

            serde_json::Value::Object(
                values
                    .into_iter()
                    .map(|(key, value)| (key, unwrap_remapped_values(value)))
                    .collect(),
            )
        }
        value => value,
    }
}

struct JsoneJsonValueDeserializer(serde_json::Value);

macro_rules! deserialize_json_number_from_string {
    ($method:ident, $visit:ident, $ty:ty) => {
        fn $method<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            match self.0 {
                serde_json::Value::String(value) => {
                    visitor.$visit(value.parse::<$ty>().map_err(Self::Error::custom)?)
                }
                value => value.into_deserializer().$method(visitor),
            }
        }
    };
}

impl<'de> Deserializer<'de> for JsoneJsonValueDeserializer {
    type Error = serde_json::Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.0 {
            serde_json::Value::Null => visitor.visit_unit(),
            serde_json::Value::Bool(value) => visitor.visit_bool(value),
            serde_json::Value::Number(value) => serde_json::Value::Number(value)
                .into_deserializer()
                .deserialize_any(visitor),
            serde_json::Value::String(value) => visitor.visit_string(value),
            serde_json::Value::Array(values) => visitor.visit_seq(JsoneJsonSeqAccess {
                values: values.into_iter(),
            }),
            serde_json::Value::Object(values) => visitor.visit_map(JsoneJsonMapAccess {
                values: values.into_iter(),
                next_value: None,
            }),
        }
    }

    deserialize_json_number_from_string!(deserialize_i8, visit_i8, i8);
    deserialize_json_number_from_string!(deserialize_i16, visit_i16, i16);
    deserialize_json_number_from_string!(deserialize_i32, visit_i32, i32);
    deserialize_json_number_from_string!(deserialize_i64, visit_i64, i64);
    deserialize_json_number_from_string!(deserialize_i128, visit_i128, i128);
    deserialize_json_number_from_string!(deserialize_u8, visit_u8, u8);
    deserialize_json_number_from_string!(deserialize_u16, visit_u16, u16);
    deserialize_json_number_from_string!(deserialize_u32, visit_u32, u32);
    deserialize_json_number_from_string!(deserialize_u64, visit_u64, u64);
    deserialize_json_number_from_string!(deserialize_u128, visit_u128, u128);
    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.0 {
            serde_json::Value::String(value) => {
                visitor.visit_f32(value.parse().map_err(Self::Error::custom)?)
            }
            serde_json::Value::Number(value) if value.as_u64() == Some(1) => {
                visitor.visit_f32(f32::NAN)
            }
            serde_json::Value::Number(value) if value.as_u64() == Some(2) => {
                visitor.visit_f32(f32::INFINITY)
            }
            serde_json::Value::Number(value) if value.as_u64() == Some(3) => {
                visitor.visit_f32(f32::NEG_INFINITY)
            }
            value => value.into_deserializer().deserialize_f32(visitor),
        }
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.0 {
            serde_json::Value::String(value) => {
                visitor.visit_f64(value.parse().map_err(Self::Error::custom)?)
            }
            serde_json::Value::Number(value) if value.as_u64() == Some(1) => {
                visitor.visit_f64(f64::NAN)
            }
            serde_json::Value::Number(value) if value.as_u64() == Some(2) => {
                visitor.visit_f64(f64::INFINITY)
            }
            serde_json::Value::Number(value) if value.as_u64() == Some(3) => {
                visitor.visit_f64(f64::NEG_INFINITY)
            }
            value => value.into_deserializer().deserialize_f64(visitor),
        }
    }

    forward_to_deserialize_any! {
        bool char str string bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

struct JsoneJsonSeqAccess {
    values: std::vec::IntoIter<serde_json::Value>,
}

impl<'de> SeqAccess<'de> for JsoneJsonSeqAccess {
    type Error = serde_json::Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: serde::de::DeserializeSeed<'de>,
    {
        self.values
            .next()
            .map(|value| seed.deserialize(JsoneJsonValueDeserializer(value)))
            .transpose()
    }
}

struct JsoneJsonMapAccess {
    values: serde_json::map::IntoIter,
    next_value: Option<serde_json::Value>,
}

impl<'de> MapAccess<'de> for JsoneJsonMapAccess {
    type Error = serde_json::Error;

    fn next_key_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: serde::de::DeserializeSeed<'de>,
    {
        match self.values.next() {
            Some((key, value)) => {
                self.next_value = Some(value);
                seed.deserialize(key.into_deserializer()).map(Some)
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<T>(&mut self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: serde::de::DeserializeSeed<'de>,
    {
        let value = self
            .next_value
            .take()
            .ok_or_else(|| Self::Error::custom("value is missing for map key"))?;

        seed.deserialize(JsoneJsonValueDeserializer(value))
    }
}

#[cfg(test)]
mod tests {
    use super::{Jsone, REMAP_KEY};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Deserialize, PartialEq, Serialize)]
    struct Payload {
        value: Jsone<i32>,
    }

    #[derive(Debug, Deserialize, PartialEq, Serialize)]
    struct NestedPayload {
        value: Jsone<Nested>,
    }

    #[derive(Debug, Deserialize, PartialEq, Serialize)]
    struct Nested {
        id: u64,
        label: String,
        child: NestedChild,
    }

    #[derive(Debug, Deserialize, PartialEq, Serialize)]
    struct NestedChild {
        count: i32,
        active: bool,
    }

    #[test]
    fn serializes_safe_integer_as_number() {
        let json = serde_json::to_string(&Payload { value: Jsone(123) }).unwrap();

        assert_eq!(json, r#"{"value":123}"#);
        assert_eq!(
            serde_json::to_string(&Jsone(9_007_199_254_740_991i64)).unwrap(),
            "9007199254740991"
        );
        assert_eq!(
            serde_json::to_string(&Jsone(-9_007_199_254_740_991i64)).unwrap(),
            "-9007199254740991"
        );
    }

    #[test]
    fn serializes_unsafe_integer_as_string_wrapped_by_remap_key() {
        assert_eq!(
            serde_json::to_string(&Jsone(9_007_199_254_740_992i64)).unwrap(),
            r#"{"$$jsone$remap$$":"9007199254740992"}"#
        );
        assert_eq!(
            serde_json::to_string(&Jsone(-9_007_199_254_740_992i64)).unwrap(),
            r#"{"$$jsone$remap$$":"-9007199254740992"}"#
        );
    }

    #[test]
    fn deserializes_value_from_string_wrapped_by_remap_key() {
        let payload: Payload =
            serde_json::from_str(r#"{"value":{"$$jsone$remap$$":"123"}}"#).unwrap();

        assert_eq!(payload, Payload { value: Jsone(123) });
    }

    #[test]
    fn serializes_nested_object_number_fields_at_their_original_locations() {
        let json = serde_json::to_string(&NestedPayload {
            value: Jsone(Nested {
                id: 123,
                label: "unchanged".to_owned(),
                child: NestedChild {
                    count: -5,
                    active: true,
                },
            }),
        })
        .unwrap();

        assert_eq!(
            json,
            r#"{"value":{"id":123,"label":"unchanged","child":{"count":-5,"active":true}}}"#
        );
    }

    #[test]
    fn serializes_nested_array_number_fields_at_their_original_locations() {
        let json = serde_json::to_string(&Jsone(vec![
            Nested {
                id: 123,
                label: "first".to_owned(),
                child: NestedChild {
                    count: -5,
                    active: true,
                },
            },
            Nested {
                id: 456,
                label: "second".to_owned(),
                child: NestedChild {
                    count: 7,
                    active: false,
                },
            },
        ]))
        .unwrap();

        assert_eq!(
            json,
            r#"[{"id":123,"label":"first","child":{"count":-5,"active":true}},{"id":456,"label":"second","child":{"count":7,"active":false}}]"#
        );
    }

    #[test]
    fn deserializes_nested_object_number_fields_from_their_original_locations() {
        let payload: NestedPayload = serde_json::from_str(
            r#"{"value":{"id":{"$$jsone$remap$$":"123"},"label":"unchanged","child":{"count":{"$$jsone$remap$$":"-5"},"active":true}}}"#,
        )
        .unwrap();

        assert_eq!(
            payload,
            NestedPayload {
                value: Jsone(Nested {
                    id: 123,
                    label: "unchanged".to_owned(),
                    child: NestedChild {
                        count: -5,
                        active: true,
                    },
                })
            }
        );
    }

    #[test]
    fn serializes_special_float_values_as_numeric_codes() {
        assert_eq!(
            serde_json::to_string(&Jsone(f64::NAN)).unwrap(),
            format!(r#"{{"{REMAP_KEY}":1}}"#)
        );
        assert_eq!(
            serde_json::to_string(&Jsone(f64::INFINITY)).unwrap(),
            format!(r#"{{"{REMAP_KEY}":2}}"#)
        );
        assert_eq!(
            serde_json::to_string(&Jsone(f64::NEG_INFINITY)).unwrap(),
            format!(r#"{{"{REMAP_KEY}":3}}"#)
        );
    }

    #[test]
    fn deserializes_special_float_numeric_codes() {
        assert!(
            serde_json::from_str::<Jsone<f64>>(&format!(r#"{{"{REMAP_KEY}":1}}"#))
                .unwrap()
                .0
                .is_nan()
        );
        assert_eq!(
            serde_json::from_str::<Jsone<f64>>(&format!(r#"{{"{REMAP_KEY}":2}}"#)).unwrap(),
            Jsone(f64::INFINITY)
        );
        assert_eq!(
            serde_json::from_str::<Jsone<f64>>(&format!(r#"{{"{REMAP_KEY}":3}}"#)).unwrap(),
            Jsone(f64::NEG_INFINITY)
        );
    }

    #[test]
    fn rejects_unknown_fields() {
        let error = serde_json::from_str::<Jsone<i32>>(r#"{"unknown":"123"}"#).unwrap_err();

        assert!(error.to_string().contains("invalid type"));
    }

    #[test]
    fn rejects_missing_remap_key() {
        let error = serde_json::from_str::<Jsone<i32>>("{}").unwrap_err();

        assert!(error.to_string().contains("invalid type"));
    }

    #[test]
    fn rejects_invalid_special_numeric_code() {
        let error =
            serde_json::from_str::<Jsone<f64>>(&format!(r#"{{"{REMAP_KEY}":4}}"#)).unwrap_err();

        assert!(error.to_string().contains("invalid type"));
    }
}
