import { DRAG_THRESHOLD } from "../components/tabDrag";
import { releaseListMotion } from "../components/useListMotion";
import { onReducedMotion, prefersReducedMotion } from "../reducedMotion";

/** A grip moves the table rows visually until drop. Touch keeps its native scrolling; Escape, pointer cancellation,
 * blur and teardown restore the rows without writing an order. */
export function beginAccountDrag(event: PointerEvent, source: HTMLTableRowElement, onDrop: (to: number) => void): () => void {
  if (event.button !== 0 || event.pointerType === "touch") return () => {};
  const rows = Array.from(source.parentElement!.querySelectorAll<HTMLTableRowElement>(":scope > tr[data-account-row]"));
  const rects = rows.map((row) => row.getBoundingClientRect());
  const from = rows.indexOf(source);
  const start = event.clientY;
  let dragging = false;
  let target = from;
  const move = (e: PointerEvent) => {
    if (e.pointerId !== event.pointerId) return;
    if (!source.isConnected || source.closest("[inert]")) { cancel(); return; }
    const dy = Math.max(rects[0]!.top - rects[from]!.top, Math.min(e.clientY - start, rects.at(-1)!.bottom - rects[from]!.bottom));
    if (!dragging && Math.abs(dy) < DRAG_THRESHOLD) return;
    dragging = true;
    source.style.position = "relative";
    source.style.zIndex = "2";
    source.style.transform = `translateY(${dy}px)`;
    const top = rects[from]!.top + dy;
    const bottom = rects[from]!.bottom + dy;
    target = rects.filter((rect, i) => i < from ? top >= rect.top + rect.height / 2 : i > from && bottom > rect.top + rect.height / 2).length;
    rows.forEach((row, i) => {
      if (i === from) return;
      const shift = from < target && i > from && i <= target ? -rects[from]!.height : from > target && i >= target && i < from ? rects[from]!.height : 0;
      row.style.transition = prefersReducedMotion() ? "" : "transform 150ms";
      row.style.transform = shift ? `translateY(${shift}px)` : "";
    });
  };
  const stopReduced = onReducedMotion(() => {
    for (const row of rows) row.style.transition = "";
  });
  const noSelect = (e: Event) => e.preventDefault();
  const cancel = () => {
    stopReduced();
    document.removeEventListener("pointermove", move, true);
    document.removeEventListener("pointerup", up, true);
    document.removeEventListener("pointercancel", pointerCancel, true);
    document.removeEventListener("keydown", key, true);
    document.removeEventListener("selectstart", noSelect, true);
    window.removeEventListener("blur", cancel);
    for (const row of rows) {
      row.style.transform = "";
      row.style.transition = "";
      row.style.position = "";
      row.style.zIndex = "";
    }
  };
  const up = (e: PointerEvent) => {
    if (e.pointerId !== event.pointerId) return;
    if (!source.isConnected || source.closest("[inert]")) { cancel(); return; }
    if (dragging && target !== from) {
      releaseListMotion(source);
      onDrop(target);
    }
    cancel();
  };
  const pointerCancel = (e: PointerEvent) => { if (e.pointerId === event.pointerId) cancel(); };
  const key = (e: KeyboardEvent) => {
    if (e.key !== "Escape") return;
    e.preventDefault();
    e.stopPropagation();
    cancel();
  };
  document.addEventListener("pointermove", move, true);
  document.addEventListener("pointerup", up, true);
  document.addEventListener("pointercancel", pointerCancel, true);
  document.addEventListener("keydown", key, true);
  document.addEventListener("selectstart", noSelect, true);
  window.addEventListener("blur", cancel);
  return cancel;
}
