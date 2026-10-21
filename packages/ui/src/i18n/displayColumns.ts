// Measuring and cutting text by the columns a menu draws it in, for lines with a width cap.

const segmenter = new Intl.Segmenter(undefined, { granularity: "grapheme" });
/** The characters a menu draws two columns wide: East Asian wide and fullwidth ones, and emoji. */
const WIDE = /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}\p{Extended_Pictographic}\u3000-\u303F\uFF00-\uFF60\uFFE0-\uFFE6]/u;

/** How many columns one grapheme takes: none for the bidi isolates, two if it starts with a wide character. */
function graphemeColumns(grapheme: string): number {
  if (grapheme === "\u2068" || grapheme === "\u2069") return 0;
  return WIDE.test(grapheme) ? 2 : 1;
}

/** How wide `text` is in columns, counting East Asian wide characters and emoji as two: the measure the
 * menu's line cap is kept in, since a line of such characters is about twice as wide as one of Latin ones. */
export function displayColumns(text: string): number {
  let columns = 0;
  for (const { segment } of segmenter.segment(text)) columns += graphemeColumns(segment);
  return columns;
}

/** `text` cut to `max` columns with a "…" in the last one, at a grapheme boundary so that an emoji sequence
 * or a letter with its combining marks is not split. An isolate the cut leaves open (`isolate`) is closed after
 * the ellipsis, so it cannot reorder what is put after the text. */
export function ellipsize(text: string, max: number): string {
  if (displayColumns(text) <= max) return text;
  let kept = "";
  let columns = 0;
  let open = 0;
  for (const { segment } of segmenter.segment(text)) {
    const width = graphemeColumns(segment);
    if (columns + width > max - 1) break;
    columns += width;
    kept += segment;
    if (segment === "\u2068") open += 1;
    else if (segment === "\u2069") open -= 1;
  }
  return `${kept.trimEnd()}…${"\u2069".repeat(Math.max(0, open))}`;
}
