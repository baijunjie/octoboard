/** The three switches of the viewer's find bar. */
export interface FindOptions {
  matchCase: boolean;
  wholeWord: boolean;
  regex: boolean;
}

export const DEFAULT_FIND_OPTIONS: FindOptions = { matchCase: false, wholeWord: false, regex: false };

/** The matches the viewer finds and highlights at most. Each one is a `Range` and a highlight entry,
 * so a one-letter query in the largest file the viewer shows must stop somewhere; the count then
 * reads "5,000+" (`capped`). */
export const MAX_FIND_MATCHES = 5000;

/** What a query asks for: nothing yet, a regular expression that does not compile, or the pattern. */
export type FindPattern = { kind: "empty" } | { kind: "invalid" } | { kind: "pattern"; pattern: RegExp };

function escapeRegExp(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function compile(source: string, flags: string): RegExp | undefined {
  try {
    return new RegExp(source, flags);
  } catch {
    return undefined;
  }
}

/**
 * The pattern a query and its switches stand for. A plain query is matched literally. A regular
 * expression is compiled with the `u` flag, and without it when only the older syntax accepts it
 * (an escape such as `\-` outside a class); one that neither accepts is `invalid`. Whole word means
 * the match is not flanked by a letter, digit or underscore.
 */
export function findPattern(query: string, options: FindOptions): FindPattern {
  if (query === "") return { kind: "empty" };
  const source = options.regex ? query : escapeRegExp(query);
  const flags = options.matchCase ? "gm" : "gim";
  const word = (unicode: boolean) => (unicode ? "[\\p{L}\\p{N}_]" : "\\w");
  const wrap = (unicode: boolean) => (options.wholeWord ? `(?<!${word(unicode)})(?:${source})(?!${word(unicode)})` : source);
  const pattern = compile(wrap(true), `${flags}u`) ?? (options.regex ? compile(wrap(false), flags) : undefined);
  return pattern ? { kind: "pattern", pattern } : { kind: "invalid" };
}

/** The `[start, end)` offsets of the matches of `pattern` in `text`, in order, at most `limit`. An
 * empty match (a pattern such as `a*`) is no match, as it has nothing to highlight. */
export function matchOffsets(pattern: RegExp, text: string, limit: number): [number, number][] {
  const found: [number, number][] = [];
  pattern.lastIndex = 0;
  for (let match = pattern.exec(text); match && found.length < limit; match = pattern.exec(text)) {
    if (match[0] === "") {
      pattern.lastIndex += 1;
      continue;
    }
    found.push([match.index, match.index + match[0].length]);
  }
  return found;
}

/** The index of the match one step from `current` among `count`, wrapping around at both ends. */
export function stepMatch(current: number, count: number, direction: "next" | "previous"): number {
  if (count === 0) return 0;
  return (current + (direction === "next" ? 1 : -1) + count) % count;
}
