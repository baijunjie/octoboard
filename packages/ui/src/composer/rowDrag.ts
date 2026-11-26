import { onWindowFocusLoss } from "../components/hooks/windowFocus";
import { onReducedMotion, prefersReducedMotion } from "../components/motion/reducedMotion";

/** How far the pointer has to move before a press on a row's handle is a drag rather than a press. */
export const ROW_DRAG_THRESHOLD = 5;

/** The order `ids` take when `id` is put at index `to` among the others. */
export function movedRow(ids: readonly string[], id: string, to: number): string[] {
  const others = ids.filter((other) => other !== id);
  others.splice(to, 0, id);
  return others;
}

/** Follows a pointer drag that began on the handle of `id`'s row, in a vertical list: the row follows the pointer, the
 * rows it passes make room for it, and releasing it over a new place calls `onDrop` with the whole new order, the row's
 * new index and the row moved. The rows are only moved by transform while it lasts, so the order itself is left to the
 * caller; `onDrop` is called while those transforms still stand, and they are cleared as it returns. A row makes room as
 * soon as the dragged row's edge passes its middle, and the dragged row is drawn over the others. Escape, a cancelled or
 * lost pointer and the window losing focus end the drag with nothing dropped. A mouse or pen only: a touch scrolls the
 * list, and the rows' menus carry Move earlier and Move later for every input. The vertical counterpart of the agent
 * tabs' drag (`components/tabs/tabDrag.ts`). */
export function beginRowDrag({ id, ids, elements, event, onDrop }: {
  id: string;
  ids: readonly string[];
  elements: ReadonlyMap<string, HTMLElement>;
  event: PointerEvent;
  onDrop: (order: string[], to: number, moved: string) => void;
}): void {
  const source = elements.get(id);
  const from = ids.indexOf(id);
  if (!source || from < 0 || event.button !== 0 || event.pointerType === "touch") return;
  const rows = ids.map((other) => elements.get(other));
  if (rows.some((row) => !row)) return;
  const rects = rows.map((row) => row!.getBoundingClientRect());
  const gap = ids.length > 1 ? Math.max(0, rects[1]!.top - rects[0]!.bottom) : 0;
  const slot = rects[from]!.height + gap;
  const startY = event.clientY;
  let dragging = false;
  let target = from;

  const reset = () => {
    for (const row of rows) {
      row!.style.transform = "";
      row!.style.transition = "";
      row!.style.zIndex = "";
      row!.style.position = "";
    }
  };
  const move = (e: PointerEvent) => {
    const dy = e.clientY - startY;
    if (!dragging) {
      if (Math.abs(dy) < ROW_DRAG_THRESHOLD) return;
      dragging = true;
      for (const row of rows) if (row !== source && !prefersReducedMotion()) row!.style.transition = "transform 150ms";
      source.style.position = "relative";
      source.style.zIndex = "2";
    }
    source.style.transform = `translateY(${dy}px)`;
    const top = rects[from]!.top + dy;
    const bottom = rects[from]!.bottom + dy;
    // A row above the dragged one stays above it until the dragged row's top passes its middle; a row below it comes
    // above it once the dragged row's bottom passes its middle.
    target = ids.filter((_, i) => {
      const middle = rects[i]!.top + rects[i]!.height / 2;
      return i < from ? top > middle : i > from && middle < bottom;
    }).length;
    ids.forEach((_, i) => {
      if (i === from) return;
      const shift = from < target && i > from && i <= target ? -slot : from > target && i >= target && i < from ? slot : 0;
      rows[i]!.style.transform = shift ? `translateY(${shift}px)` : "";
    });
  };
  const stopReduced = onReducedMotion(() => {
    for (const row of rows) row!.style.transition = "";
  });
  // A press that moves the pointer would otherwise select the text it passes over.
  const noSelect = (e: Event) => e.preventDefault();
  const finish = () => {
    stopReduced();
    document.removeEventListener("selectstart", noSelect, true);
    document.removeEventListener("pointermove", move, true);
    document.removeEventListener("pointerup", up, true);
    document.removeEventListener("pointercancel", cancel, true);
    document.removeEventListener("keydown", key, true);
    document.removeEventListener("lostpointercapture", lost, true);
    undoFocusLoss();
    reset();
  };
  const up = () => {
    try {
      if (dragging && target !== from) onDrop(movedRow(ids, id, target), target, id);
    } finally {
      finish();
    }
  };
  const cancel = () => finish();
  const lost = (e: PointerEvent) => {
    if (e.pointerType === "mouse") cancel();
  };
  const key = (e: KeyboardEvent) => {
    if (e.key !== "Escape") return;
    e.stopPropagation();
    cancel();
  };
  document.addEventListener("selectstart", noSelect, true);
  document.addEventListener("pointermove", move, true);
  document.addEventListener("pointerup", up, true);
  document.addEventListener("pointercancel", cancel, true);
  document.addEventListener("keydown", key, true);
  document.addEventListener("lostpointercapture", lost, true);
  const undoFocusLoss = onWindowFocusLoss(cancel);
}
