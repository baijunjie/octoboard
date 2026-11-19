import { useEffect, useState } from "react";

import { canHighlight, textRangeIn } from "../find/findDom";
import { LINE_MARK_HIGHLIGHT } from "../find/findHighlight";
import { codeLineOf, lineStart } from "./rendererDom";

/** A run of a line's text to mark when a file is opened at that line, such as a match of a content search: `start` and
 * `end` are offsets into the line's text in UTF-16 code units, as a JavaScript string counts them, `end` excluded. A
 * range past the line's end is held to it, and one wholly past it marks nothing. */
export interface LineRange {
  start: number;
  end: number;
}

// The ranges each surface showing a file at a line has marked. One CSS highlight paints them all, so a surface that
// clears its own marks leaves another's (a preview behind the viewer) painted.
const painted = new Map<object, Range[]>();

function repaint(): void {
  if (!canHighlight()) return;
  const ranges = Array.from(painted.values()).flat();
  if (ranges.length === 0) {
    CSS.highlights.delete(LINE_MARK_HIGHLIGHT);
    return;
  }
  const highlight = new Highlight(...ranges);
  // Under the find bar's highlights, which are at the default priority and above.
  highlight.priority = -1;
  CSS.highlights.set(LINE_MARK_HIGHLIGHT, highlight);
}

/** Paints `ranges` as `owner`'s line marks, in place of the ones it painted before. */
export function paintLineMarks(owner: object, ranges: Range[]): void {
  if (ranges.length > 0) painted.set(owner, ranges);
  else if (!painted.delete(owner)) return;
  repaint();
}

/** Takes `owner`'s line marks off the page. */
export function clearLineMarks(owner: object): void {
  if (painted.delete(owner)) repaint();
}

/** An owner for a surface's line marks, which are taken off the page when the surface unmounts. */
export function useLineMarkOwner(): object {
  const [owner] = useState(() => ({}));
  useEffect(() => () => clearLineMarks(owner), [owner]);
  return owner;
}

/** The ranges of `marks` on the line numbered `line` (1-based) of a rendered file, none past the line's end: none while
 * the frame has drawn no such line. */
export function codeLineMarks(frame: HTMLElement, line: number, marks: readonly LineRange[]): Range[] {
  const element = codeLineOf(frame, line);
  if (!element) return [];
  // The library draws an empty line as a newline, which is no text of the line to mark.
  const length = (element.textContent ?? "").replace(/\n$/, "").length;
  return marks.flatMap((mark) => textRangeIn(element, mark.start, Math.min(mark.end, length)) ?? []);
}

/** The ranges of `marks` on the line numbered `line` (1-based) of the text in a preformatted element, which holds it as
 * one text node: none for a line the text does not have. */
export function textLineMarks(pre: HTMLElement, line: number, marks: readonly LineRange[]): Range[] {
  const node = pre.firstChild;
  if (node?.nodeType !== Node.TEXT_NODE) return [];
  const text = node.textContent ?? "";
  const start = lineStart(text, line);
  if (start === undefined) return [];
  const newline = text.indexOf("\n", start);
  const end = newline < 0 ? text.length : newline;
  return marks.flatMap((mark) => {
    const from = start + Math.max(0, mark.start);
    const to = Math.min(start + mark.end, end);
    if (from >= to) return [];
    const range = pre.ownerDocument.createRange();
    range.setStart(node, from);
    range.setEnd(node, to);
    return [range];
  });
}
