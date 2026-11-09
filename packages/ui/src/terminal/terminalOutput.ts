import type { IBuffer, IBufferRange, Terminal } from "@xterm/xterm";

import type { TermEvent } from "../protocol";

// The pure parts of the output pipeline: what output is made of and how it is handed to xterm. The
// controller keeps everything that depends on its session, socket and status.

/** A focus report (`CSI I`, `CSI O`): xterm does not mark it as the user's, but it is this view
 * gaining or losing focus, not an answer to a query. */
// eslint-disable-next-line no-control-regex -- Focus reports begin with the escape control character.
export const FOCUS_REPORT = /^\x1b\[[IO]$/;

/** Whether `data` is a mouse report that reaches `onData`: SGR and SGR-pixels (`CSI < b;x;y M|m`),
 * with the button code `b` captured. xterm's default encoding always goes through `onBinary`, and
 * it has no urxvt encoding. */
// eslint-disable-next-line no-control-regex -- Terminal mouse reports begin with the escape control character.
export const MOUSE_REPORT = /^\x1b\[<(\d+);\d+;\d+[Mm]$/;

/** Turns off the mouse tracking (X10 to any-motion, and the SGR encoding) and focus reporting an
 * agent may have switched on, which xterm answers with input of its own. `?1015l` (urxvt) is a
 * no-op in xterm 6.0.0, which does not support that encoding; it is harmless and kept for an
 * xterm that does. */
export const RESET_REPORTING_MODES = "\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1004l\x1b[?1006l\x1b[?1015l";

/** Output made of nothing but private mode sets and resets (`CSI ? Pm h`, `CSI ? Pm l`), with at
 * most charset designations (`ESC ( B`) and shifts (SI, SO) around them: what a program writes to
 * re-assert its modes, as Claude Code does after a pause in its input, on a resize and in other
 * cases. Of the modes, only the alternate screen's change the text on screen, by switching buffers,
 * which `keepSelectionThroughModes` checks for itself. */
// eslint-disable-next-line no-control-regex -- Mode sequences begin with the escape control character.
const MODES_ONLY = /^(?:\x1b\[\?\d+(?:;\d+)*[hl]|\x1b[()*+][0-9A-Za-z]|[\x0e\x0f])+$/;
/** Longer output is never a bare re-assertion of modes; past this it is not even looked at. */
const MODES_ONLY_MAX_BYTES = 256;

/** Whether `data` only sets or resets modes (see `MODES_ONLY`). */
function onlySetsModes(data: Uint8Array): boolean {
  return data.length > 0 && data.length <= MODES_ONLY_MAX_BYTES && MODES_ONLY.test(String.fromCharCode(...data));
}

/** Whether `data` is an SGR report of the pointer moving with no button held: motion (32) with the
 * "no button" code (3), not the wheel (64), whatever modifiers (4, 8, 16) are held. */
export function isButtonlessMotion(data: string): boolean {
  const code = MOUSE_REPORT.exec(data)?.[1];
  return code !== undefined && (Number(code) & 0b110_0011) === 0b010_0011;
}

/** The most a single `write` hands xterm. xterm parses one write as one task, and a session's whole
 * first paint arrives as one frame, which would hold the main thread for tens of milliseconds; between
 * writes it yields, and its decoders keep their state across them, so a character or an escape
 * sequence cut by a slice is still read whole. */
const WRITE_SLICE_BYTES = 64 * 1024;

/** One write of `planOutputWrites`: `offset` is the point in the output it ends at, `undefined` for
 * output whose queries were answered already. */
interface PlannedWrite {
  data: Uint8Array;
  offset: number | undefined;
}

/**
 * Splits a frame of a connection's output into the writes to make, each with the point in the output
 * it ends at for the answers xterm gives while parsing it, and returns the point the output ends at
 * after the frame. `mark` is the replay mark still waiting for the first frame it describes, which is
 * written in two, the output whose queries were answered already and the rest. `outputEnd` is the point the
 * connection's last frame ended at, unknown before the replay mark.
 */
export function planOutputWrites(
  data: Uint8Array,
  mark: TermEvent | undefined,
  outputEnd: number | undefined,
): { writes: PlannedWrite[]; outputEnd: number | undefined } {
  if (mark && mark.length === data.length) {
    return {
      writes: [
        { data: data.subarray(0, mark.answered), offset: undefined },
        { data: data.subarray(mark.answered), offset: mark.offset },
      ],
      outputEnd: mark.offset,
    };
  }
  const end = outputEnd === undefined ? undefined : outputEnd + data.length;
  return { writes: [{ data, offset: end }], outputEnd: end };
}

/** What `keepSelectionThroughModes` reads of the controller. */
export interface SelectionGuard {
  /** Counts what the user does that may clear or replace the selection on purpose. */
  userActs: () => number;
  disposed: () => boolean;
}

/** Writes `data` to xterm in slices of at most `WRITE_SLICE_BYTES`, in order; `onParsed` is the
 * last slice's write callback, so it fires once all of `data` has been parsed. `data` that only
 * sets modes keeps the terminal's selection through it (`keepSelectionThroughModes`): it is looked
 * at whole, so a re-assertion of modes counts only as output of its own, never inside a larger
 * one. */
export function writeInSlices(term: Terminal, data: Uint8Array, guard: SelectionGuard, onParsed?: () => void): void {
  const keepSelection = onlySetsModes(data) ? keepSelectionThroughModes(term, guard) : undefined;
  let start = 0;
  do {
    const end = start + WRITE_SLICE_BYTES;
    term.write(data.subarray(start, end), end >= data.length ? onParsed : undefined);
    start = end;
  } while (start < data.length);
  if (keepSelection) term.write("", keepSelection);
}

/**
 * Notes the selection as it stands right before the output written next is parsed, and returns
 * the callback to write right after that output, which puts the selection back if parsing it
 * cleared it. For output that only sets modes (`onlySetsModes`): xterm clears its selection
 * whenever a mouse tracking mode is set, even one already on, so a program re-asserting its modes
 * (Claude Code does so whenever input arrives after a few idle seconds, such as the focus report
 * of the window coming back) would take away a selection forced under mouse reporting, which
 * nothing on screen has invalidated.
 *
 * The selection is put back only when nothing else could have meant to clear it: the tracking
 * mode and the active buffer are what they were, and the user has done nothing in between
 * (`SelectionGuard.userActs`); xterm may parse queued output across tasks, letting a click or a
 * key in. Output that comes with anything else, text included, is never looked at, so a selection
 * is never put back over redrawn text.
 */
function keepSelectionThroughModes(term: Terminal, guard: SelectionGuard): () => void {
  let kept:
    | { range: IBufferRange; tracking: Terminal["modes"]["mouseTrackingMode"]; buffer: IBuffer["type"]; userActs: number }
    | undefined;
  term.write("", () => {
    const range = term.getSelectionPosition();
    kept = range && {
      range,
      tracking: term.modes.mouseTrackingMode,
      buffer: term.buffer.active.type,
      userActs: guard.userActs(),
    };
  });
  return () => {
    if (
      !kept ||
      guard.disposed() ||
      term.hasSelection() ||
      term.modes.mouseTrackingMode !== kept.tracking ||
      term.buffer.active.type !== kept.buffer ||
      guard.userActs() !== kept.userActs
    ) {
      return;
    }
    // The range's end is exclusive, and a selection that spans rows runs on through the ends of
    // the rows in between, which is what a length counted in whole rows gives back. A selection
    // ending at column 0 of a later row comes back ending at the end of the row before, without
    // the line break the original ended with: `select` takes a length, and xterm maps one that
    // reaches a row's end exactly to that end, never to the start of the next row. A column
    // (block) selection keeps its mode through `select`, so it comes back as the same rectangle,
    // except one whose end corner is at column 0: that end comes back at (cols, the row before),
    // a different rectangle.
    const { start, end } = kept.range;
    term.select(start.x, start.y, (end.y - start.y) * term.cols + end.x - start.x);
  };
}
