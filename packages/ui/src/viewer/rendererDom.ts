/**
 * The one place outside `renderer.tsx` that reads the DOM the rendering library (`@pierre/diffs`)
 * produces, kept out of that module because it is loaded lazily and this is needed at a key press.
 * It depends on the library's markup (a shadow root under the frame, with one `[data-content]`
 * column of code per file or diff side), so a library upgrade is checked against it.
 */

/**
 * The code a rendered frame shows, as a range from its first content column to its last, leaving
 * out the line numbers before them. `undefined` when the frame holds no rendered code (plain text,
 * or the renderer's own loading state); `null` when the library's host is there but has drawn no
 * code yet.
 */
export function renderedCodeRange(frame: HTMLElement): Range | null | undefined {
  const host = Array.from(frame.querySelectorAll("*")).find((element) => element.shadowRoot);
  if (!host?.shadowRoot) return undefined;
  const columns = host.shadowRoot.querySelectorAll("[data-content]");
  if (columns.length === 0) return null;
  const last = columns[columns.length - 1];
  const range = frame.ownerDocument.createRange();
  range.setStart(columns[0], 0);
  range.setEnd(last, last.childNodes.length);
  return range;
}
