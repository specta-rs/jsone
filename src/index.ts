// This MUST match the value in Rust.
const REMAP_KEY = "$$jsone$remap$$";

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
 * ```ts
 * const encoded = encode(structuredClone(value));
 * // or
 * const json = JSON.stringify(value, (_key, value) => encode(value));
 * ```
 *
 * If the root value itself needs remapping, a new remapped wrapper is returned
 * because primitives cannot be reassigned in place.
 */
export function encode(value: unknown, seen = new WeakSet<object>()): unknown {
  if (typeof value === "bigint") return { [REMAP_KEY]: value.toString() };
  else if (typeof value === "number") {
    if (Number.isNaN(value)) return { [REMAP_KEY]: 1 };
    else if (value === Number.POSITIVE_INFINITY) return { [REMAP_KEY]: 2 };
    else if (value === Number.NEGATIVE_INFINITY) return { [REMAP_KEY]: 3 };
    else if (value < Number.MIN_SAFE_INTEGER || value > Number.MAX_SAFE_INTEGER)
      return { [REMAP_KEY]: value.toString() };
  }

  if (typeof value !== "object" || value === null) return value;
  if (seen.has(value)) return value;

  seen.add(value);

  if (Array.isArray(value)) {
    for (let index = 0; index < value.length; index += 1)
      value[index] = encode(value[index], seen);

    return value;
  }

  const record = value as Record<string, unknown>;

  for (const key in record)
    if (Object.prototype.hasOwnProperty.call(record, key))
      record[key] = encode(record[key], seen);

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
 * ```ts
 * const value = decode(encoded);
 * // or
 * const value = JSON.parse(json, (_key, value) => decode(value));
 * ```
 */
export function decode(value: unknown, seen = new WeakSet<object>()): unknown {
  if (typeof value !== "object" || value === null) return value;

  if (Object.keys(value).length === 1 && REMAP_KEY in value) {
    const remapped = (value as Record<typeof REMAP_KEY, unknown>)[REMAP_KEY];

    if (typeof remapped === "string") return BigInt(remapped);
    if (remapped === 1) return Number.NaN;
    if (remapped === 2) return Number.POSITIVE_INFINITY;
    if (remapped === 3) return Number.NEGATIVE_INFINITY;

    throw new TypeError(`unknown remapped numeric code: ${remapped}`);
  }

  if (seen.has(value)) return value;

  seen.add(value);

  if (Array.isArray(value)) {
    for (let index = 0; index < value.length; index += 1)
      value[index] = decode(value[index], seen);

    return value;
  }

  const record = value as Record<string, unknown>;

  for (const key in record)
    if (Object.prototype.hasOwnProperty.call(record, key))
      record[key] = decode(record[key], seen);

  return value;
}
