/** The names of the CSS highlights the find bar paints: every match, and the current one on top of
 * it. The rules for them are in `find.css` for the page and `findHighlightCss` for the renderer's
 * shadow roots, where a `::highlight()` rule has to be written to reach text inside. */
export const FIND_MATCH_HIGHLIGHT = "octoboard-find";
export const FIND_CURRENT_HIGHLIGHT = "octoboard-find-current";
/** The name of the CSS highlight on the ranges a file was opened to show (`code/lineMarks.ts`), such as the matches of
 * a content search on the line it opened at. Painted like the find bar's current match, which it is the counterpart
 * of; the find bar's highlights are painted over it. */
export const LINE_MARK_HIGHLIGHT = "octoboard-line-mark";

/** The `::highlight()` rules for a shadow root's stylesheet, the line marks' among them, drawn from the colours `find.css`
 * defines (custom properties inherit into the shadow root). */
export const findHighlightCss = `
  ::highlight(${LINE_MARK_HIGHLIGHT}) { background-color: var(--find-current-bg); color: var(--find-current-fg); text-decoration: underline; }
  ::highlight(${FIND_MATCH_HIGHLIGHT}) { background-color: var(--find-match-bg); }
  ::highlight(${FIND_CURRENT_HIGHLIGHT}) { background-color: var(--find-current-bg); color: var(--find-current-fg); text-decoration: underline; }
`;
