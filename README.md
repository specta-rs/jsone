# jsone

Zero-loss JSON serialization for Rust values that JavaScript or JSON cannot normally
encode/decode without losing precision.

`jsone` provides serde helpers for values that need a JSON representation which
can be decoded safely on the JavaScript side. The crate exposes a single wrapper
type, `BigInt<T>`, which wraps a field or value and handles lossless Rust-side
serialization and deserialization.

Pair this crate with the frontend encoder or decoder so JavaScript can turn the
encoded JSON representation back into regular JavaScript values.

Checkout the [documentation on crates.io](https://docs.rs/jsone).

This crate was developed around research I did while funded from [Flight Science](https://flightscience.ai) for work on Specta v2!
