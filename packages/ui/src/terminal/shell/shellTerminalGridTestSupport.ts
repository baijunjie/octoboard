// A layout for jsdom, which gives no element a size: what a Terminal panel's start-size measurement
// (`shellTerminalGrid.ts`) reads of the page, set by hand for the rest of the test.
import { onTestFinished, vi } from "vitest";

/** The number of characters of the row the measurement lays out to find a cell's width. */
const MEASURED_ROW = "W".repeat(32);

export interface TerminalLayout {
  /** The size of every element, in pixels: its `clientWidth` and `clientHeight`, and its bounding box (which is
   * where the measurement reads the space a terminal gets, fractions of a pixel included). */
  width: number;
  height: number;
  /** The size of one character cell the probe row is drawn at. */
  cell: { width: number; height: number };
  /** The font sizes the probe row was drawn at, one per measurement. */
  drawnAt: string[];
}

/** Gives every element the size `layout` holds, and the measured row of characters (the `span` that holds it) the
 * width of its cell times its length and the height of one cell. Changing the returned object changes what is read
 * next; the page is as it was when the test finishes. */
export function stubTerminalLayout(layout: Pick<TerminalLayout, "width" | "height" | "cell">): TerminalLayout {
  const stub: TerminalLayout = { ...layout, drawnAt: [] };
  vi.spyOn(Element.prototype, "clientWidth", "get").mockImplementation(() => stub.width);
  vi.spyOn(Element.prototype, "clientHeight", "get").mockImplementation(() => stub.height);
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) {
    if (!(this instanceof HTMLSpanElement) || this.textContent !== MEASURED_ROW) return new DOMRect(0, 0, stub.width, stub.height);
    stub.drawnAt.push(this.style.fontSize);
    return new DOMRect(0, 0, stub.cell.width * MEASURED_ROW.length, stub.cell.height);
  });
  onTestFinished(() => void vi.restoreAllMocks());
  return stub;
}
