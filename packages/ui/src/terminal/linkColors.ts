import type { IDecoration, Terminal } from "@xterm/xterm";

import { segmentKey, visibleLinkSegments } from "./terminalLinks";
import { XTERM_LINK_COLORS } from "./xtermThemes";

/**
 * Draws the links in the terminal's output in a link colour all the time, where xterm only
 * underlines one under the pointer. It puts a decoration on the cells of each link in the rows on
 * screen, looked up again once per frame after output, a scroll or a resize, and applies only the
 * differences. Plain-text addresses and the ones the program broke across rows are covered; an
 * OSC 8 hyperlink is not, as xterm's public API does not say which cells carry one.
 */
export class TerminalLinkColors {
  /** A decoration is kept where its marker is now: a marker follows its line when the buffer is
   * trimmed, or lines are inserted or deleted, so where it was put is not where it is. */
  private decorations: { decoration: IDecoration; col: number; width: number }[] = [];
  private frame?: number;
  private disposed = false;

  constructor(
    private readonly term: Terminal,
    private mode: "light" | "dark",
  ) {
    term.onWriteParsed(() => this.schedule());
    term.onScroll(() => this.schedule());
    term.onResize(() => this.schedule());
    // Every decoration belongs to the buffer it was made in.
    term.buffer.onBufferChange(() => this.reset());
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
    // A full-screen program in the alternate buffer owns the colours there, and xterm clears the
    // markers on leaving it.
    const segments = term.buffer.active.type === "alternate" ? [] : visibleLinkSegments(term);
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
      const marker = term.registerMarker(y - 1 - (baseY + cursorY));
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
