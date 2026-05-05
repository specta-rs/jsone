// This MUST match the value in Rust.
const REMAP_KEY = "$$jsone$remap$$";
const hasOwn = Object.prototype.hasOwnProperty;

/**
 * Encodes values JSON cannot carry losslessly.
 *
 * Bigint values, unsafe numbers, and special floating point values are encoded
 * with `{ "$$jsone$remap$$": ... }` so the resulting string is valid JSON and
 * can be deserialized by this package's Rust `Jsone<T>` wrapper.
 *
 * Object and array containers are mutated in place. If you need to keep the
 * original value unchanged, clone first, for example with `structuredClone`.
 *
 * ```js
 * const encoded = encode(structuredClone(value));
 * // or
 * const json = JSON.stringify(value, (_key, value) => encode(value));
 * ```
 *
 * If the root value itself needs remapping, a new remapped wrapper is returned
 * because primitives cannot be reassigned in place.
 *
 * @param {unknown} value
 * @returns {unknown}
 */
export function encode(value) {
  return encodeValue(value);
}

/**
 * @param {unknown} value
 * @param {WeakSet<object>} [seen]
 * @returns {unknown}
 */
function encodeValue(value, seen) {
  if (typeof value === "bigint") return { [REMAP_KEY]: value.toString() };
  else if (typeof value === "number") {
    if (Number.isNaN(value)) return { [REMAP_KEY]: 1 };
    else if (value === Number.POSITIVE_INFINITY) return { [REMAP_KEY]: 2 };
    else if (value === Number.NEGATIVE_INFINITY) return { [REMAP_KEY]: 3 };
    else if (
      Number.isInteger(value) &&
      (value < Number.MIN_SAFE_INTEGER || value > Number.MAX_SAFE_INTEGER)
    )
      return { [REMAP_KEY]: value.toString() };
  }

  if (typeof value !== "object" || value === null) return value;

  seen ??= new WeakSet();
  if (seen.has(value)) return value;

  seen.add(value);

  if (Array.isArray(value)) {
    for (let index = 0; index < value.length; index += 1)
      value[index] = encodeValue(value[index], seen);

    return value;
  }

  for (const key in value)
    if (hasOwn.call(value, key)) value[key] = encodeValue(value[key], seen);

  return value;
}

/**
 * Decodes remapped wrappers to native JavaScript values.
 *
 * Use directly on a value, or as a primitive inside `JSON.parse`'s reviver
 * callback.
 *
 * Object and array containers are mutated in place. If you need to keep the
 * original value unchanged, clone first, for example with `structuredClone`.
 *
 * ```js
 * const value = decode(encoded);
 * // or
 * const value = JSON.parse(json, (_key, value) => decode(value));
 * ```
 *
 * @param {unknown} value
 * @returns {unknown}
 */
export function decode(value) {
  return decodeValue(value);
}

/**
 * @param {unknown} value
 * @param {WeakSet<object>} [seen]
 * @returns {unknown}
 */
function decodeValue(value, seen) {
  if (typeof value !== "object" || value === null) return value;

  let isRemappedWrapper = false;

  for (const key in value) {
    if (!hasOwn.call(value, key)) continue;
    if (key !== REMAP_KEY) {
      isRemappedWrapper = false;
      break;
    }

    isRemappedWrapper = true;
  }

  if (isRemappedWrapper) {
    const remapped = value[REMAP_KEY];

    if (typeof remapped === "string") return BigInt(remapped);
    if (remapped === 1) return Number.NaN;
    if (remapped === 2) return Number.POSITIVE_INFINITY;
    if (remapped === 3) return Number.NEGATIVE_INFINITY;

    throw new TypeError(
      `jsone decode error: found unknown remap key ${remapped}.`,
    );
  }

  seen ??= new WeakSet();
  if (seen.has(value)) return value;

  seen.add(value);

  if (Array.isArray(value)) {
    for (let index = 0; index < value.length; index += 1)
      value[index] = decodeValue(value[index], seen);

    return value;
  }

  for (const key in value)
    if (hasOwn.call(value, key)) value[key] = decodeValue(value[key], seen);

  return value;
}
