/** Field name used by the Rust serde wrapper and JavaScript remapper. */
export const REMAP_KEY = "$$jsone$remap$$";
export const MAX_SAFE_INTEGER = Number.MAX_SAFE_INTEGER;
export const MIN_SAFE_INTEGER = Number.MIN_SAFE_INTEGER;

/** Wire-format object used to represent values that JSON cannot carry losslessly. */
export type RemappedValue = {
  [REMAP_KEY]: string | 1 | 2 | 3;
};

/** Wire-format object used to represent a JavaScript bigint in JSON. */
export type RemappedBigInt = RemappedValue;

/**
 * Serializes a value with bigint support.
 *
 * Bigint values, unsafe numbers, and special floating point values are encoded
 * with `{ "$$jsone$remap$$": ... }` so the resulting string is valid JSON and
 * can be deserialized by this package's Rust `Jsone<T>` wrapper.
 */
export function stringify(value: unknown, space?: string | number): string {
  return JSON.stringify(
    value,
    (_key, value) => remapValue(value),
    space,
  );
}

/**
 * Remaps values in an existing object graph without using JSON.parse or
 * cloning the graph. Object and array containers are mutated in place by
 * reassigning bigint, unsafe number, and special floating point properties or
 * elements to their remapped wrapper objects, which avoids the extra allocation
 * cost of stringify/parse-style copying.
 *
 * If the root value itself needs remapping, a new remapped wrapper is returned
 * because primitives cannot be reassigned in place.
 */
export function remapValuesInPlace<T>(value: T): T | RemappedValue {
  return remapValuesInPlaceInner(value, new WeakSet<object>()) as
    | T
    | RemappedValue;
}

/**
 * @deprecated Use `remapValuesInPlace`, which also remaps unsafe numbers and
 * special floating point values.
 */
export function remapBigIntsInPlace<T>(value: T): T | RemappedBigInt {
  return remapValuesInPlace(value);
}

/**
 * Parses JSON produced by `stringify` and restores remapped wrappers to native
 * JavaScript values.
 */
export function parse<T = unknown>(text: string): T {
  return JSON.parse(text, (_key, value) => {
    if (isRemappedValue(value)) {
      return restoreValue(value);
    }

    return value;
  }) as T;
}

function remapValue(value: unknown): unknown {
  if (typeof value === "bigint") {
    return remapBigInt(value);
  }

  if (typeof value === "number") {
    return remapNumber(value);
  }

  return value;
}

function remapBigInt(value: bigint): RemappedValue {
  return { [REMAP_KEY]: value.toString() };
}

function remapNumber(value: number): number | RemappedValue {
  if (Number.isNaN(value)) {
    return { [REMAP_KEY]: 1 };
  }

  if (value === Number.POSITIVE_INFINITY) {
    return { [REMAP_KEY]: 2 };
  }

  if (value === Number.NEGATIVE_INFINITY) {
    return { [REMAP_KEY]: 3 };
  }

  if (value < MIN_SAFE_INTEGER || value > MAX_SAFE_INTEGER) {
    return { [REMAP_KEY]: value.toString() };
  }

  return value;
}

function restoreValue(value: RemappedValue): bigint | number {
  const remapped = value[REMAP_KEY];

  if (typeof remapped === "string") {
    return BigInt(remapped);
  }

  if (remapped === 1) {
    return Number.NaN;
  }

  if (remapped === 2) {
    return Number.POSITIVE_INFINITY;
  }

  if (remapped === 3) {
    return Number.NEGATIVE_INFINITY;
  }

  throw new TypeError(`unknown remapped numeric code: ${remapped}`);
}

function isRemappedValue(value: unknown): value is RemappedValue {
  return (
    typeof value === "object" &&
    value !== null &&
    Object.keys(value).length === 1 &&
    REMAP_KEY in value
  );
}

function remapValuesInPlaceInner(
  value: unknown,
  seen: WeakSet<object>,
): unknown {
  const remapped = remapValue(value);

  if (remapped !== value) {
    return remapped;
  }

  if (typeof value !== "object" || value === null) {
    return value;
  }

  if (seen.has(value)) {
    return value;
  }

  seen.add(value);

  if (Array.isArray(value)) {
    for (let index = 0; index < value.length; index += 1) {
      value[index] = remapValuesInPlaceInner(value[index], seen);
    }

    return value;
  }

  const record = value as Record<string, unknown>;

  for (const key in record) {
    if (Object.prototype.hasOwnProperty.call(record, key)) {
      record[key] = remapValuesInPlaceInner(record[key], seen);
    }
  }

  return value;
}
