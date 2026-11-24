import { FADE_SIZE } from "./scrollFade";
import { fadeMask } from "./fadeMask";

/** A layout-ordered column lets us seek to the edge without measuring every row on scroll. */
function edgeRows(column: Element, from: number, to: number, rows: Map<HTMLElement, number>): void {
  const children = column.children;
  let low = 0;
  let high = children.length;
  while (low < high) {
    const middle = Math.floor((low + high) / 2);
    if (children[middle].getBoundingClientRect().bottom <= from) low = middle + 1;
    else high = middle;
  }
  for (let index = low; index < children.length; index += 1) {
    const row = children[index];
    const { top, height } = row.getBoundingClientRect();
    if (top >= to) break;
    if (height && row instanceof HTMLElement) rows.set(row, top);
  }
}

const MASK = "--scroll-row-fade-mask";
const CONTENT_MASK = "--scroll-row-content-mask";
const SIZE = "--scroll-row-fade-size";
const OFFSET = "--scroll-row-fade-offset";

function clear(row: HTMLElement): void {
  row.style.removeProperty(MASK);
  row.style.removeProperty(CONTENT_MASK);
  row.style.removeProperty(SIZE);
  row.style.removeProperty(OFFSET);
}

/** Gives the paint layer of each edge row a slice of the same viewport-sized mask. Its caller
 * supplies the row's own fill as the paint, so the foreground fades into that fill. A row that
 * needs to fade as a whole can use the complementary mask. */
export function watchScrollRowFade(scroller: HTMLElement, content: HTMLElement, columns: Iterable<Element>): () => void {
  let painted = new Map<HTMLElement, number>();
  let frame: number | undefined;
  const measure = () => {
    frame = undefined;
    const height = scroller.clientHeight;
    const start = Math.min(FADE_SIZE, height / 2, Math.max(0, scroller.scrollTop));
    const end = Math.min(FADE_SIZE, height / 2, Math.max(0, scroller.scrollHeight - height - scroller.scrollTop));
    const top = scroller.getBoundingClientRect().top + scroller.clientTop;
    const bottom = top + height;
    const next = new Map<HTMLElement, number>();
    for (const column of columns) {
      if (start) edgeRows(column, top, top + start, next);
      if (end) edgeRows(column, bottom - end, bottom, next);
    }
    const mask = fadeMask("bottom", start, end, "edge");
    const contentMask = fadeMask("bottom", start, end);
    for (const row of painted.keys()) {
      if (!next.has(row)) clear(row);
    }
    for (const [row, rowTop] of next) {
      row.style.setProperty(MASK, mask);
      row.style.setProperty(CONTENT_MASK, contentMask);
      row.style.setProperty(SIZE, `${height}px`);
      row.style.setProperty(OFFSET, `${top - rowTop}px`);
    }
    painted = next;
  };
  const schedule = () => {
    if (frame === undefined) frame = requestAnimationFrame(measure);
  };
  const resize = new ResizeObserver(schedule);
  resize.observe(scroller);
  resize.observe(content);
  scroller.addEventListener("scroll", schedule, { passive: true });
  measure();
  return () => {
    resize.disconnect();
    scroller.removeEventListener("scroll", schedule);
    if (frame !== undefined) cancelAnimationFrame(frame);
    for (const row of painted.keys()) clear(row);
  };
}
