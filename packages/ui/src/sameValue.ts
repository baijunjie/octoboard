/** Whether two parsed-JSON values hold the same data: primitives by `Object.is`, arrays and plain
 * objects member by member, an absent key being the same as one that holds `undefined`. What the
 * daemon sends again unchanged is told apart from what changed with this, so the store keeps the
 * record it already has (and every component holding it) instead of replacing it by a copy. */
export function sameValue(a: unknown, b: unknown): boolean {
  if (Object.is(a, b)) return true;
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false;
  if (Array.isArray(a) || Array.isArray(b)) {
    return Array.isArray(a) && Array.isArray(b) && a.length === b.length && a.every((member, i) => sameValue(member, b[i]));
  }
  const left = a as Record<string, unknown>;
  const right = b as Record<string, unknown>;
  const keys = new Set([...Object.keys(left), ...Object.keys(right)]);
  for (const key of keys) {
    if (!sameValue(left[key], right[key])) return false;
  }
  return true;
}
