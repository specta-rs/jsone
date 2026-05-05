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
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

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
    T: fmt::Display + Copy + Into<f64>,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let value = self.0.into();
        let mut map = serializer.serialize_map(Some(1))?;

        if value.is_nan() {
            map.serialize_entry(REMAP_KEY, &1)?;
        } else if value == f64::INFINITY {
            map.serialize_entry(REMAP_KEY, &2)?;
        } else if value == f64::NEG_INFINITY {
            map.serialize_entry(REMAP_KEY, &3)?;
        } else {
            map.serialize_entry(REMAP_KEY, &self.0.to_string())?;
        }

        map.end()
    }
}

impl<'de, T> Deserialize<'de> for BigInt<T>
where
    T: FromStr,
    T::Err: fmt::Display,
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
    T: FromStr,
    T::Err: fmt::Display,
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
    T: FromStr,
    T::Err: fmt::Display,
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
    T: FromStr,
    T::Err: fmt::Display,
{
    type Value = BigIntValue<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a bigint string or special numeric code")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: Error,
    {
        T::from_str(value).map(BigIntValue).map_err(E::custom)
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
        match value {
            1 => T::from_str("NaN"),
            2 => T::from_str("inf"),
            3 => T::from_str("-inf"),
            _ => return Err(E::custom("expected special bigint code 1, 2, or 3")),
        }
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
