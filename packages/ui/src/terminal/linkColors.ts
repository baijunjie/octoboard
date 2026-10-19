import type { IDecoration, IDisposable, IMarker, Terminal } from "@xterm/xterm";

import { type ExtraLinkSegments, segmentKey, visibleLinkSegments } from "./terminalLinks";
import { XTERM_LINK_COLORS } from "./xtermThemes";

let nextFixedMarkerId = -1;

/** Alternate-buffer rows have no scrollback and are reconciled after each write. An owned
 * marker keeps its row stable during disposal: xterm's markers become -1 before notifying the
 * decoration service, breaking its sorted lookup when several alternate rows are cleared. */
function fixedMarker(line: number): IMarker {
  const listeners = new Set<() => void>();
  let disposed = false;
  const marker: IMarker = {
    id: nextFixedMarkerId--,
    line,
    get isDisposed() { return disposed; },
    onDispose: (listener) => {
      listeners.add(listener);
      return { dispose: () => { listeners.delete(listener); } };
    },
    dispose: () => {
      if (marker.isDisposed) return;
      disposed = true;
      for (const listener of listeners) listener();
      listeners.clear();
    },
  };
  return marker;
}

/**
 * Draws the links in the terminal's output in a link colour all the time, where xterm only
 * underlines one under the pointer. It puts a decoration on the cells of each link in the rows on
 * screen, looked up again once per frame after output, a scroll or a resize, and applies only the
 * differences. Both buffers, plain-text addresses and OSC 8 hyperlink labels are covered, and so
 * are the file paths the daemon confirmed, where it is given a way to find them.
 */
export class TerminalLinkColors {
  /** A decoration is kept where its marker is now: a marker follows its line when the buffer is
   * trimmed, or lines are inserted or deleted, so where it was put is not where it is. */
  private decorations: { decoration: IDecoration; col: number; width: number }[] = [];
  private readonly listeners: IDisposable[];
  private frame?: number;
  private disposed = false;

  constructor(
    private readonly term: Terminal,
    private mode: "light" | "dark",
    /** The cells to colour besides the addresses: the file paths the daemon found. */
    private readonly extra?: ExtraLinkSegments,
  ) {
    this.listeners = [
      term.onWriteParsed(() => this.schedule()),
      term.onScroll(() => this.schedule()),
      term.onResize(() => this.schedule()),
      // Every decoration belongs to the buffer it was made in.
      term.buffer.onBufferChange(() => this.reset()),
    ];
  }

  setTheme(mode: "light" | "dark"): void {
    this.mode = mode;
    this.reset();
  }

  /** Drops every decoration and draws them afresh at the next frame. */
  reset(): void {
    for (const { decoration } of this.decorations) this.drop(decoration);
    this.decorations = [];
    this.schedule();
  }

  dispose(): void {
    if (this.frame !== undefined) cancelAnimationFrame(this.frame);
    this.disposed = true;
    for (const listener of this.listeners) listener.dispose();
    for (const { decoration } of this.decorations) this.drop(decoration);
    this.decorations = [];
  }

  /** Looks the links up again at the next frame, for a link that has appeared without any output. */
  refresh(): void {
    this.schedule();
  }

  private schedule(): void {
    if (this.frame !== undefined || this.disposed) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = undefined;
      this.update();
    });
  }

  /** The marker is disposed too: xterm's own reset (a program's `ESC c`) disposes a decoration
   * and leaves its marker. */
  private drop(decoration: IDecoration): void {
    decoration.dispose();
    decoration.marker.dispose();
  }

  private update(): void {
    const { term } = this;
    const segments = visibleLinkSegments(term, this.extra);
    const wanted = new Map(segments.map((segment) => [segmentKey(segment.y, segment.col, segment.width), segment]));
    const kept: typeof this.decorations = [];
    for (const entry of this.decorations) {
      const { decoration, col, width } = entry;
      const key = decoration.isDisposed ? undefined : segmentKey(decoration.marker.line + 1, col, width);
      if (key !== undefined && wanted.delete(key)) kept.push(entry);
      else this.drop(decoration);
    }
    const { baseY, cursorY } = term.buffer.active;
    for (const { y, col, width } of wanted.values()) {
      const marker = term.buffer.active.type === "alternate"
        ? fixedMarker(y - 1)
        : term.registerMarker(y - 1 - (baseY + cursorY));
      const decoration = term.registerDecoration({
        marker,
        x: col,
        width,
        foregroundColor: XTERM_LINK_COLORS[this.mode],
        layer: "top",
      });
      if (decoration) kept.push({ decoration, col, width });
      else marker.dispose();
    }
    this.decorations = kept;
  }
}
