// The whole-value test of a page's `pattern`, which runs in the Web Worker (`patternWorker.ts`) and nowhere else: a
// pattern is the agent's, and a pathological one can run for as long as it likes, which must never be the window's own
// thread. Kept apart from the worker's wiring so that it can be tested directly.

/** What a `pattern` makes of a text: a match, or not one. */
export type PatternVerdict = "match" | "mismatch";

const compiled = new Map<string, RegExp | null>();

/** The page's `pattern` as a whole-value test, or `undefined` for one that does not compile (the daemon refuses such a
 * pattern, so this is only a page stored before it did; it is then ignored). Compiled once per pattern. */
function wholeValue(pattern: string): RegExp | undefined {
  const held = compiled.get(pattern);
  if (held !== undefined) return held ?? undefined;
  let found: RegExp | undefined;
  for (const flags of ["v", "u", ""]) {
    try {
      found = new RegExp(`^(?:${pattern})$`, flags);
      break;
    } catch {
      // Tried again with older flags: the page's pattern may be written for either.
    }
  }
  compiled.set(pattern, found ?? null);
  return found;
}

/** Whether `text` as a whole matches `pattern`; a pattern that does not compile matches everything. */
export function testPattern(pattern: string, text: string): PatternVerdict {
  const expression = wholeValue(pattern);
  return !expression || expression.test(text) ? "match" : "mismatch";
}
