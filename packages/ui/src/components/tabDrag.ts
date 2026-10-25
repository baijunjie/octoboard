import type { Agent } from "../protocol";
import { onReducedMotion, prefersReducedMotion } from "../reducedMotion";

/** How far the pointer has to move before a press on a tab is a drag rather than a press. */
export const DRAG_THRESHOLD = 5;

/** The order `agents` take when `agent` is put at index `to` among the others. */
export function movedAgent(agents: Agent[], agent: Agent, to: number): Agent[] {
  const others = agents.filter((other) => other !== agent);
  others.splice(to, 0, agent);
  return others;
}

/** Follows a pointer drag that began on `agent`'s tab: the tab follows the pointer, the tabs it passes make room for it,
 * and releasing it over a new place calls `onDrop` with the whole new order and the agent that was moved. The tabs are only moved by
 * transform while it lasts, so the order itself is left to the caller. `onDrop` is called while those transforms still
 * stand, so the caller can start the tabs' move to the new order from where they are shown; they are cleared as it
 * returns. A tab makes room as soon as the dragged tab's leading edge passes its middle, and the dragged tab is drawn
 * over the others. Escape, a cancelled or lost pointer and the window losing focus end the drag with nothing dropped.
 * A mouse or pen only: a touch on a tab scrolls the strip. No text is selected from the press until it ends, and the
 * click that follows a drag is swallowed, so the end of a drag does not also press whatever is under the pointer. */
export function beginTabDrag({ agent, agents, elements, event, onDrop }: {
  agent: Agent;
  agents: Agent[];
  elements: ReadonlyMap<Agent, HTMLElement>;
  event: PointerEvent;
  onDrop: (agents: Agent[], moved: Agent) => void;
}): void {
  const source = elements.get(agent);
  const from = agents.indexOf(agent);
  if (!source || from < 0 || event.button !== 0 || event.pointerType === "touch") return;
  const tabs = agents.map((a) => elements.get(a));
  if (tabs.some((tab) => !tab)) return;
  const rects = tabs.map((tab) => tab!.getBoundingClientRect());
  const rtl = getComputedStyle(source).direction === "rtl";
  const gap = agents.length > 1 ? Math.max(0, rtl ? rects[0]!.left - rects[1]!.right : rects[1]!.left - rects[0]!.right) : 0;
  const slot = rects[from]!.width + gap;
  const startX = event.clientX;
  let dragging = false;
  let target = from;

  const reset = () => {
    for (const tab of tabs) {
      tab!.style.transform = "";
      tab!.style.transition = "";
      tab!.style.zIndex = "";
    }
  };
  const move = (e: PointerEvent) => {
    const dx = e.clientX - startX;
    if (!dragging) {
      if (Math.abs(dx) < DRAG_THRESHOLD) return;
      dragging = true;
      for (const tab of tabs) if (tab !== source && !prefersReducedMotion()) tab!.style.transition = "transform 150ms";
      // Above the other tabs, which HeroUI already raises to 1.
      source.style.zIndex = "2";
    }
    source.style.transform = `translateX(${dx}px)`;
    // The dragged tab's edges along the strip's direction, and whether a point lies before another along it.
    const start = rects[from]![rtl ? "right" : "left"] + dx;
    const end = rects[from]![rtl ? "left" : "right"] + dx;
    const before = (a: number, b: number) => (rtl ? a > b : a < b);
    // A tab before the dragged one stays before it until the dragged tab's start edge passes its middle; a tab after
    // it comes before it once the dragged tab's end edge passes its middle.
    target = agents.filter((_, i) => {
      const middle = rects[i]!.left + rects[i]!.width / 2;
      return i < from ? !before(start, middle) : i > from && before(middle, end);
    }).length;
    agents.forEach((_, i) => {
      if (i === from) return;
      const shift = from < target && i > from && i <= target ? -slot : from > target && i >= target && i < from ? slot : 0;
      tabs[i]!.style.transform = shift ? `translateX(${rtl ? -shift : shift}px)` : "";
    });
  };
  // The tabs then follow the drag in the same frame, instead of easing toward their places.
  const stopReduced = onReducedMotion(() => {
    for (const tab of tabs) tab!.style.transition = "";
  });
  // A press that moves the pointer would otherwise select the text it passes over, the panel's below the strip too.
  const noSelect = (e: Event) => e.preventDefault();
  const finish = () => {
    stopReduced();
    document.removeEventListener("selectstart", noSelect, true);
    document.removeEventListener("pointermove", move, true);
    document.removeEventListener("pointerup", up, true);
    document.removeEventListener("pointercancel", cancel, true);
    document.removeEventListener("keydown", key, true);
    document.removeEventListener("lostpointercapture", lost, true);
    window.removeEventListener("blur", cancel);
    reset();
  };
  const swallowClick = () => {
    const stop = (e: Event) => e.stopPropagation();
    source.addEventListener("click", stop, { capture: true, once: true });
    setTimeout(() => source.removeEventListener("click", stop, true), 0);
  };
  const up = () => {
    try {
      if (dragging && target !== from) onDrop(movedAgent(agents, agent, target), agent);
    } finally {
      finish();
    }
    if (dragging) swallowClick();
  };
  const cancel = () => {
    finish();
    if (dragging) swallowClick();
  };
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
  // A press that loses its pointer without either event (the capture taken away, the window losing focus) ends too, so
  // text stays selectable. Only a mouse's: a pen's implicit capture is released by the tab's press handling meanwhile.
  document.addEventListener("lostpointercapture", lost, true);
  window.addEventListener("blur", cancel);
}
