/** The names of the CSS highlights the find bar paints: every match, and the current one on top of
 * it. The rules for them are in `style.css` for the page and `findHighlightCss` for the renderer's
 * shadow roots, where a `::highlight()` rule has to be written to reach text inside. */
export const FIND_MATCH_HIGHLIGHT = "octoboard-find";
export const FIND_CURRENT_HIGHLIGHT = "octoboard-find-current";

/** The `::highlight()` rules for a shadow root's stylesheet, drawn from the colours `style.css`
 * defines (custom properties inherit into the shadow root). */
export const findHighlightCss = `
  ::highlight(${FIND_MATCH_HIGHLIGHT}) { background-color: var(--find-match-bg); }
  ::highlight(${FIND_CURRENT_HIGHLIGHT}) { background-color: var(--find-current-bg); color: var(--find-current-fg); text-decoration: underline; }
`;
