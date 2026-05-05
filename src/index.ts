/** Field name used by the Rust serde wrapper and JavaScript remapper. */
export const REMAP_KEY = "$$jsone$remap$$";

/** Wire-format object used to represent a JavaScript bigint in JSON. */
export type RemappedBigInt = {
  [REMAP_KEY]: string | 1 | 2 | 3;
};

/**
 * Serializes a value with bigint support.
 *
 * Bigint values are encoded as `{ "$$jsone$remap$$": "<value>" }` so the
 * resulting string is valid JSON and can be deserialized by this package's Rust
 * `BigInt<T>` wrapper.
 */
export function stringify(value: unknown, space?: string | number): string {
  return JSON.stringify(
    value,
    (_key, value) => (typeof value === "bigint" ? remapBigInt(value) : value),
    space,
  );
}

/**
 * Remaps every bigint in an existing object graph without using JSON.parse or
 * cloning the graph. Object and array containers are mutated in place by
 * reassigning bigint properties/elements to their remapped wrapper objects,
 * which avoids the extra allocation cost of stringify/parse-style copying.
 *
 * If the root value itself is a bigint, a new remapped wrapper is returned
 * because primitives cannot be reassigned in place.
 */
export function remapBigIntsInPlace<T>(value: T): T | RemappedBigInt {
  return remapBigIntsInPlaceInner(value, new WeakSet<object>()) as
    | T
    | RemappedBigInt;
}

/**
 * Parses JSON produced by `stringify` and restores remapped bigint wrappers to
 * native JavaScript `bigint` values.
 *
 * Special numeric codes emitted by the Rust side for `NaN` and infinities cannot
 * be represented as JavaScript bigint values and will throw a `TypeError` if
 * encountered.
 */
export function parse<T = unknown>(text: string): T {
  return JSON.parse(text, (_key, value) => {
    if (isRemappedBigInt(value)) {
      return restoreBigInt(value);
    }

    return value;
  }) as T;
}

function remapBigInt(value: bigint): RemappedBigInt {
  return { [REMAP_KEY]: value.toString() };
}

function restoreBigInt(value: RemappedBigInt): bigint {
  const remapped = value[REMAP_KEY];

  if (typeof remapped !== "string") {
    throw new TypeError(
      "special numeric bigint codes cannot be restored as JavaScript bigint",
    );
  }

  return BigInt(remapped);
}

function isRemappedBigInt(value: unknown): value is RemappedBigInt {
  return (
    typeof value === "object" &&
    value !== null &&
    Object.keys(value).length === 1 &&
    REMAP_KEY in value
  );
}

function remapBigIntsInPlaceInner(
  value: unknown,
  seen: WeakSet<object>,
): unknown {
  if (typeof value === "bigint") {
    return remapBigInt(value);
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
      value[index] = remapBigIntsInPlaceInner(value[index], seen);
    }

    return value;
  }

  const record = value as Record<string, unknown>;

  for (const key in record) {
    if (Object.prototype.hasOwnProperty.call(record, key)) {
      record[key] = remapBigIntsInPlaceInner(record[key], seen);
    }
  }

  return value;
}
