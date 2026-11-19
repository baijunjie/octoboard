import { displayWirePath } from "../../../format/wirePath";
import { graphemeColumns, graphemeSegmenter } from "../../../i18n/displayColumns";
import type { ContentMatch } from "../../../protocol";
import { parentPath } from "../../tree";

/** A run of a matching line's text as a result shows it: `marked` for a run a match covers. */
export interface MatchSegment {
  text: string;
  marked: boolean;
}

/** How many cells of the line a row cut short before its match still shows ahead of the match. */
export const MATCH_LEAD = 12;

/** The tab stops of a row's text, every this many cells: the browser's default `tab-size`, which the row keeps. */
const TAB_STOP = 8;

/** The cell `text` ends at, drawn from the cell `column` on: a tab runs to the next tab stop, any other grapheme takes
 * the columns `graphemeColumns` gives it (two for a wide character or an emoji). */
function columnAfter(text: string, column: number): number {
  let at = column;
  for (const { segment } of graphemeSegmenter.segment(text)) at = segment === "\t" ? (Math.floor(at / TAB_STOP) + 1) * TAB_STOP : at + graphemeColumns(segment);
  return at;
}

/** The text of a matching line as a result row shows it, in runs, marked where the daemon's ranges say: never by
 * running the pattern here, whose syntax is the daemon's. The leading whitespace of a line that starts in full is left
 * out, so the code lines up by what it says. A range is an offset into the whole line, so it is moved by where the
 * clipped text starts, and held to the text: the part of a range outside it is not shown. `clippedStart` and
 * `clippedEnd` say whether the line goes on before or after what is shown.
 *
 * `fit` is how many cells of the fixed-width font the row has room for. Where the first match would end past them, the
 * text starts a few cells before it instead (`MATCH_LEAD`, less in a row too narrow for it), so the match is always in
 * view. Cells are counted as the row draws them: a tab to its tab stop, a wide character or an emoji as two. */
export function matchSegments(match: ContentMatch, fit?: number): { segments: MatchSegment[]; clippedStart: boolean; clippedEnd: boolean } {
  const indent = match.text_start > 0 ? 0 : match.text.length - match.text.trimStart().length;
  let text = match.text.slice(indent);
  let offset = match.text_start + indent;
  let clippedStart = match.text_start > 0;
  const first = match.ranges.find((range) => range.end - offset > 0 && range.start - offset < text.length);
  if (fit !== undefined && first !== undefined) {
    // The ellipses at either end take a cell each.
    if (columnAfter(text.slice(0, Math.max(0, first.end - offset)), clippedStart ? 1 : 0) > fit - 1) {
      const leadCells = Math.min(MATCH_LEAD, Math.floor(fit / 4));
      const at = Math.max(0, first.start - offset);
      // Back from the match a grapheme at a time, for as long as what goes ahead of it, drawn after the leading
      // ellipsis, fits in `leadCells`.
      const starts = [...graphemeSegmenter.segment(text.slice(0, at))].map((grapheme) => grapheme.index);
      let cut = at;
      for (let i = starts.length - 1; i >= 0 && columnAfter(text.slice(starts[i], at), 1) - 1 <= leadCells; i--) cut = starts[i]!;
      if (cut > 0) {
        text = text.slice(cut);
        offset += cut;
        clippedStart = true;
      }
    }
  }
  const segments: MatchSegment[] = [];
  let at = 0;
  for (const range of match.ranges) {
    const start = Math.max(range.start - offset, at);
    const end = Math.min(range.end - offset, text.length);
    if (end <= start) continue;
    if (start > at) segments.push({ text: text.slice(at, start), marked: false });
    segments.push({ text: text.slice(start, end), marked: true });
    at = end;
  }
  if (at < text.length || segments.length === 0) segments.push({ text: text.slice(at), marked: false });
  return { segments, clippedStart, clippedEnd: match.clipped_end };
}

/** The key of a result: one row per matching line. */
export function matchKey(match: Pick<ContentMatch, "path" | "line">): string {
  return `${match.line}:${match.path}`;
}

/** The folder of the file at the wire path `path`, in display form; empty for a file in the project's root. */
export function matchFolder(path: string): string {
  return displayWirePath(parentPath(path));
}
