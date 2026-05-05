//! Serde helpers for values that need a JSON representation which JavaScript can
//! decode without losing precision.
//!
//! This crate exposes a single wrapper type, [`BigInt<T>`] which wraps any field or type,
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
//! use jsone::BigInt;
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Debug, Deserialize, PartialEq, Serialize)]
//! struct Payload {
//!     id: BigInt<i32>,
//! }
//!
//! let json = serde_json::to_string(&Payload { id: BigInt(42) }).unwrap();
//! assert_eq!(json, r#"{"id":{"$$jsone$remap$$":"42"}}"#);
//!
//! let payload: Payload = serde_json::from_str(&json).unwrap();
//! assert_eq!(payload.id, BigInt(42));
//! ```
#![cfg_attr(docsrs, feature(doc_cfg))]

use serde::de::{Error, MapAccess, Visitor};
use serde::ser::{
    SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant, SerializeTuple,
    SerializeTupleStruct, SerializeTupleVariant,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer, forward_to_deserialize_any};
use std::fmt;

/// The JavaScript runtime for encoding and decoding.
///
/// You should copy this into your project!
pub const RUNTIME: &str = include_str!("./index.ts");

/// Field name used for the JSON object that marks the remapped value.
/// Changing this would require the frontend to also reflect the new value so would be a majorly breaking change.
const REMAP_KEY: &str = "$$jsone$remap$$";

/// Serde wrapper that applies the remap logic.
///
/// Refer to the crate documentation for an explanation.
///
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Copy)]
pub struct BigInt<T>(pub T);

impl<T> Serialize for BigInt<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry(REMAP_KEY, &BigIntValueSerialize(&self.0))?;
        map.end()
    }
}

struct BigIntValueSerialize<'a, T: ?Sized>(&'a T);

impl<T> Serialize for BigIntValueSerialize<'_, T>
where
    T: ?Sized + Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(BigIntSerializer(serializer))
    }
}

struct BigIntSerializer<S>(S);

macro_rules! serialize_number_as_string {
    ($method:ident, $ty:ty) => {
        fn $method(self, value: $ty) -> Result<Self::Ok, Self::Error> {
            self.0.serialize_str(&value.to_string())
        }
    };
}

impl<S> Serializer for BigIntSerializer<S>
where
    S: Serializer,
{
    type Ok = S::Ok;
    type Error = S::Error;
    type SerializeSeq = BigIntSerializeSeq<S::SerializeSeq>;
    type SerializeTuple = BigIntSerializeTuple<S::SerializeTuple>;
    type SerializeTupleStruct = BigIntSerializeTupleStruct<S::SerializeTupleStruct>;
    type SerializeTupleVariant = BigIntSerializeTupleVariant<S::SerializeTupleVariant>;
    type SerializeMap = BigIntSerializeMap<S::SerializeMap>;
    type SerializeStruct = BigIntSerializeStruct<S::SerializeStruct>;
    type SerializeStructVariant = BigIntSerializeStructVariant<S::SerializeStructVariant>;

    fn serialize_bool(self, value: bool) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_bool(value)
    }

    serialize_number_as_string!(serialize_i8, i8);
    serialize_number_as_string!(serialize_i16, i16);
    serialize_number_as_string!(serialize_i32, i32);
    serialize_number_as_string!(serialize_i64, i64);
    serialize_number_as_string!(serialize_i128, i128);
    serialize_number_as_string!(serialize_u8, u8);
    serialize_number_as_string!(serialize_u16, u16);
    serialize_number_as_string!(serialize_u32, u32);
    serialize_number_as_string!(serialize_u64, u64);
    serialize_number_as_string!(serialize_u128, u128);

    fn serialize_f32(self, value: f32) -> Result<Self::Ok, Self::Error> {
        self.serialize_f64(value.into())
    }

    fn serialize_f64(self, value: f64) -> Result<Self::Ok, Self::Error> {
        if value.is_nan() {
            self.0.serialize_u8(1)
        } else if value == f64::INFINITY {
            self.0.serialize_u8(2)
        } else if value == f64::NEG_INFINITY {
            self.0.serialize_u8(3)
        } else {
            self.0.serialize_str(&value.to_string())
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
        self.0.serialize_some(&BigIntValueSerialize(value))
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
            .serialize_newtype_struct(name, &BigIntValueSerialize(value))
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
            .serialize_newtype_variant(name, variant_index, variant, &BigIntValueSerialize(value))
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        self.0.serialize_seq(len).map(BigIntSerializeSeq)
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        self.0.serialize_tuple(len).map(BigIntSerializeTuple)
    }

    fn serialize_tuple_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        self.0
            .serialize_tuple_struct(name, len)
            .map(BigIntSerializeTupleStruct)
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
            .map(BigIntSerializeTupleVariant)
    }

    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        self.0.serialize_map(len).map(BigIntSerializeMap)
    }

    fn serialize_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        self.0
            .serialize_struct(name, len)
            .map(BigIntSerializeStruct)
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
            .map(BigIntSerializeStructVariant)
    }

    fn collect_str<T>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + fmt::Display,
    {
        self.0.collect_str(value)
    }
}

struct BigIntSerializeSeq<S>(S);

impl<S> SerializeSeq for BigIntSerializeSeq<S>
where
    S: SerializeSeq,
{
    type Ok = S::Ok;
    type Error = S::Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.0.serialize_element(&BigIntValueSerialize(value))
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.0.end()
    }
}

struct BigIntSerializeTuple<S>(S);

impl<S> SerializeTuple for BigIntSerializeTuple<S>
where
    S: SerializeTuple,
{
    type Ok = S::Ok;
    type Error = S::Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.0.serialize_element(&BigIntValueSerialize(value))
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.0.end()
    }
}

struct BigIntSerializeTupleStruct<S>(S);

impl<S> SerializeTupleStruct for BigIntSerializeTupleStruct<S>
where
    S: SerializeTupleStruct,
{
    type Ok = S::Ok;
    type Error = S::Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.0.serialize_field(&BigIntValueSerialize(value))
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.0.end()
    }
}

struct BigIntSerializeTupleVariant<S>(S);

impl<S> SerializeTupleVariant for BigIntSerializeTupleVariant<S>
where
    S: SerializeTupleVariant,
{
    type Ok = S::Ok;
    type Error = S::Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.0.serialize_field(&BigIntValueSerialize(value))
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.0.end()
    }
}

struct BigIntSerializeMap<S>(S);

impl<S> SerializeMap for BigIntSerializeMap<S>
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
        self.0.serialize_value(&BigIntValueSerialize(value))
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.0.end()
    }
}

struct BigIntSerializeStruct<S>(S);

impl<S> SerializeStruct for BigIntSerializeStruct<S>
where
    S: SerializeStruct,
{
    type Ok = S::Ok;
    type Error = S::Error;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.0.serialize_field(key, &BigIntValueSerialize(value))
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.0.end()
    }
}

struct BigIntSerializeStructVariant<S>(S);

impl<S> SerializeStructVariant for BigIntSerializeStructVariant<S>
where
    S: SerializeStructVariant,
{
    type Ok = S::Ok;
    type Error = S::Error;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.0.serialize_field(key, &BigIntValueSerialize(value))
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.0.end()
    }
}

impl<'de, T> Deserialize<'de> for BigInt<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(BigIntVisitor(std::marker::PhantomData))
    }
}

struct BigIntVisitor<T>(std::marker::PhantomData<T>);

impl<'de, T> Visitor<'de> for BigIntVisitor<T>
where
    T: Deserialize<'de>,
{
    type Value = BigInt<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a map containing a $bigint field")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut value = None;

        while let Some(key) = map.next_key::<&str>()? {
            if key == REMAP_KEY {
                if value.is_some() {
                    return Err(A::Error::duplicate_field(REMAP_KEY));
                }

                value = Some(map.next_value::<BigIntValue<T>>()?.0);
            } else {
                return Err(A::Error::unknown_field(key, &[REMAP_KEY]));
            }
        }

        value
            .map(BigInt)
            .ok_or_else(|| A::Error::missing_field(REMAP_KEY))
    }
}

struct BigIntValue<T>(T);

impl<'de, T> Deserialize<'de> for BigIntValue<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(BigIntValueVisitor(std::marker::PhantomData))
    }
}

struct BigIntValueVisitor<T>(std::marker::PhantomData<T>);

impl<'de, T> Visitor<'de> for BigIntValueVisitor<T>
where
    T: Deserialize<'de>,
{
    type Value = BigIntValue<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a bigint string or special numeric code")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: Error,
    {
        T::deserialize(BigIntValueDeserializer::String(value))
            .map(BigIntValue)
            .map_err(E::custom)
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: Error,
    {
        self.visit_str(&value)
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: Error,
    {
        T::deserialize(BigIntValueDeserializer::Code(value))
            .map(BigIntValue)
            .map_err(E::custom)
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: Error,
    {
        u64::try_from(value)
            .map_err(E::custom)
            .and_then(|value| self.visit_u64(value))
    }
}

enum BigIntValueDeserializer<'a> {
    String(&'a str),
    Code(u64),
}

impl<'de> Deserializer<'de> for BigIntValueDeserializer<'_> {
    type Error = serde::de::value::Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Self::String(value) => visitor.visit_str(value),
            Self::Code(1) => visitor.visit_f64(f64::NAN),
            Self::Code(2) => visitor.visit_f64(f64::INFINITY),
            Self::Code(3) => visitor.visit_f64(f64::NEG_INFINITY),
            Self::Code(_) => Err(Self::Error::custom(
                "expected special bigint code 1, 2, or 3",
            )),
        }
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i8(parse_string(self)?)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i16(parse_string(self)?)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i32(parse_string(self)?)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i64(parse_string(self)?)
    }

    fn deserialize_i128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i128(parse_string(self)?)
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u8(parse_string(self)?)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u16(parse_string(self)?)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u32(parse_string(self)?)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u64(parse_string(self)?)
    }

    fn deserialize_u128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u128(parse_string(self)?)
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Self::String(value) => visitor.visit_f32(value.parse().map_err(Self::Error::custom)?),
            Self::Code(1) => visitor.visit_f32(f32::NAN),
            Self::Code(2) => visitor.visit_f32(f32::INFINITY),
            Self::Code(3) => visitor.visit_f32(f32::NEG_INFINITY),
            Self::Code(_) => Err(Self::Error::custom(
                "expected special bigint code 1, 2, or 3",
            )),
        }
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Self::String(value) => visitor.visit_f64(value.parse().map_err(Self::Error::custom)?),
            Self::Code(1) => visitor.visit_f64(f64::NAN),
            Self::Code(2) => visitor.visit_f64(f64::INFINITY),
            Self::Code(3) => visitor.visit_f64(f64::NEG_INFINITY),
            Self::Code(_) => Err(Self::Error::custom(
                "expected special bigint code 1, 2, or 3",
            )),
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Self::String(value) => visitor.visit_str(value),
            Self::Code(_) => self.deserialize_any(visitor),
        }
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Self::String(value) => visitor.visit_string(value.to_owned()),
            Self::Code(_) => self.deserialize_any(visitor),
        }
    }

    forward_to_deserialize_any! {
        bool char bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

fn parse_string<T>(deserializer: BigIntValueDeserializer<'_>) -> Result<T, serde::de::value::Error>
where
    T: std::str::FromStr,
    T::Err: fmt::Display,
{
    match deserializer {
        BigIntValueDeserializer::String(value) => {
            value.parse().map_err(serde::de::value::Error::custom)
        }
        BigIntValueDeserializer::Code(_) => Err(serde::de::value::Error::custom(
            "expected bigint string, found special bigint code",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{BigInt, REMAP_KEY};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Deserialize, PartialEq, Serialize)]
    struct Payload {
        value: BigInt<i32>,
    }

    #[test]
    fn serializes_value_as_string_wrapped_by_bigint_key() {
        let json = serde_json::to_string(&Payload { value: BigInt(123) }).unwrap();

        assert_eq!(json, r#"{"value":{"$$jsone$remap$$":"123"}}"#);
    }

    #[test]
    fn deserializes_value_from_string_wrapped_by_bigint_key() {
        let payload: Payload =
            serde_json::from_str(r#"{"value":{"$$jsone$remap$$":"123"}}"#).unwrap();

        assert_eq!(payload, Payload { value: BigInt(123) });
    }

    #[test]
    fn serializes_special_float_values_as_numeric_codes() {
        assert_eq!(
            serde_json::to_string(&BigInt(f64::NAN)).unwrap(),
            format!(r#"{{"{REMAP_KEY}":1}}"#)
        );
        assert_eq!(
            serde_json::to_string(&BigInt(f64::INFINITY)).unwrap(),
            format!(r#"{{"{REMAP_KEY}":2}}"#)
        );
        assert_eq!(
            serde_json::to_string(&BigInt(f64::NEG_INFINITY)).unwrap(),
            format!(r#"{{"{REMAP_KEY}":3}}"#)
        );
    }

    #[test]
    fn deserializes_special_float_numeric_codes() {
        assert!(
            serde_json::from_str::<BigInt<f64>>(&format!(r#"{{"{REMAP_KEY}":1}}"#))
                .unwrap()
                .0
                .is_nan()
        );
        assert_eq!(
            serde_json::from_str::<BigInt<f64>>(&format!(r#"{{"{REMAP_KEY}":2}}"#)).unwrap(),
            BigInt(f64::INFINITY)
        );
        assert_eq!(
            serde_json::from_str::<BigInt<f64>>(&format!(r#"{{"{REMAP_KEY}":3}}"#)).unwrap(),
            BigInt(f64::NEG_INFINITY)
        );
    }

    #[test]
    fn rejects_unknown_fields() {
        let error = serde_json::from_str::<BigInt<i32>>(r#"{"unknown":"123"}"#).unwrap_err();

        assert!(error.to_string().contains("unknown field `unknown`"));
    }

    #[test]
    fn rejects_missing_bigint_key() {
        let error = serde_json::from_str::<BigInt<i32>>("{}").unwrap_err();

        assert!(error.to_string().contains("missing field"));
    }

    #[test]
    fn rejects_invalid_special_numeric_code() {
        let error =
            serde_json::from_str::<BigInt<f64>>(&format!(r#"{{"{REMAP_KEY}":4}}"#)).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("expected special bigint code 1, 2, or 3")
        );
    }
}
