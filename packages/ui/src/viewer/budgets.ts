/**
 * How much the viewer hands to the syntax-highlighting renderer. Past these it shows the text as
 * plain preformatted text, which the browser lays out as one text node, so even the largest body
 * the daemon sends (4 MiB) opens without stalling the window. They are separate from the daemon's
 * read budgets, which bound what reaches the client at all.
 */
export const RENDER_BUDGETS = {
  /** A file highlighted as code: its length in UTF-16 code units and its line count. Measured in
   * WebKit, inserting 10,000 highlighted lines holds the main thread for about 0.6 s; the
   * tokenizing itself runs in workers. */
  highlightChars: 1_000_000,
  highlightLines: 10_000,
  /** A line longer than this is shown without tokens, inside highlighted code. */
  tokenizeLineLength: 1_000,
  /** A change rendered as a diff: its patch's length and line count. 10,000 diff lines hold the
   * main thread for about 0.8 s. */
  diffChars: 1_000_000,
  diffLines: 10_000,
  /** The fenced blocks of a rendered document that are highlighted: their count and their total
   * length in UTF-16 code units. Each block is a renderer instance of its own (a shadow root, a
   * grammar load and a draw, queued on a pool of two workers), unlike a file, which is one, so a
   * document of many fences stays within the file budget and would still make thousands. Blocks
   * past either are shown as plain text. */
  documentFences: 40,
  documentFenceChars: 200_000,
  /** Diagrams drawn in one document, and the length of one diagram's source. One past either is
   * shown as its source. An over-size diagram does not take a place in the count, so one long
   * diagram does not turn the later, small ones into source. The length is per diagram, not a
   * total: each diagram is a draw of its own. */
  documentDiagrams: 12,
  documentDiagramChars: 8_000,
  /** Formulas drawn in one document, and the length of one formula's source. Counted the same way
   * as diagrams. */
  documentFormulas: 80,
  documentFormulaChars: 2_000,
  /** Images read for one document. The byte cap is the daemon's own file limit, so a file it will
   * send is within it and the count is what bounds the document; a reply at the cap is still
   * checked, and one past it is left as its description. */
  documentImages: 30,
  documentImageBytes: 4 * 1024 * 1024,
} as const;

/** Counts the lines of `text`, a final line without a newline included. */
export function countLines(text: string): number {
  if (text.length === 0) return 0;
  let lines = 1;
  for (let at = text.indexOf("\n"); at !== -1 && at < text.length - 1; at = text.indexOf("\n", at + 1)) lines += 1;
  return lines;
}

/** Whether `text` is within the highlighting budget, or shown as plain text. */
export function textPlan(text: string): "highlight" | "plain" {
  if (text.length > RENDER_BUDGETS.highlightChars) return "plain";
  return countLines(text) > RENDER_BUDGETS.highlightLines ? "plain" : "highlight";
}

/** Whether a change's patch is rendered as a diff, or past the budget shown as plain text — still
 * the whole change, only unrendered. A change is only ever drawn from its patch, never compared
 * from its two bodies here: a line diff of two bodies runs on the main thread with no way to stop
 * it, and two 5,000-line files with nothing in common already hold the window for about 2 s. */
export function diffPlan(patch: string): "render" | "patch" {
  return patch.length <= RENDER_BUDGETS.diffChars && countLines(patch) <= RENDER_BUDGETS.diffLines ? "render" : "patch";
}

/** Whether a change's lines can be expanded within the render budget: the whole of the longer
 * side is drawn once every collapsed run is shown, and past the budget the collapsed lines stay as
 * they are, as the patch alone is drawn. */
export function bodiesPlan(bodies: { old: string; new: string }): "expand" | "none" {
  const longest = bodies.old.length > bodies.new.length ? bodies.old : bodies.new;
  return longest.length <= RENDER_BUDGETS.diffChars && Math.max(countLines(bodies.old), countLines(bodies.new)) <= RENDER_BUDGETS.diffLines
    ? "expand"
    : "none";
}
