import type { Agent } from "../protocol";
import { onReducedMotion, prefersReducedMotion } from "../reducedMotion";

/** How far the pointer has to move before a press on a tab is a drag rather than a press. */
const DRAG_THRESHOLD = 5;

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
 * returns. Escape and a cancelled pointer end the drag
 * with nothing dropped. A mouse or pen only: a touch on a tab scrolls the strip. The click that follows a drag is
 * swallowed, so the end of a drag does not also press whatever is under the pointer. */
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
      source.style.zIndex = "1";
    }
    source.style.transform = `translateX(${dx}px)`;
    const centre = rects[from]!.left + rects[from]!.width / 2 + dx;
    target = agents.filter((_, i) => i !== from && (rtl ? rects[i]!.left + rects[i]!.width / 2 > centre : rects[i]!.left + rects[i]!.width / 2 < centre)).length;
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
  const finish = () => {
    stopReduced();
    document.removeEventListener("pointermove", move, true);
    document.removeEventListener("pointerup", up, true);
    document.removeEventListener("pointercancel", cancel, true);
    document.removeEventListener("keydown", key, true);
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
  const key = (e: KeyboardEvent) => {
    if (e.key !== "Escape") return;
    e.stopPropagation();
    cancel();
  };
  document.addEventListener("pointermove", move, true);
  document.addEventListener("pointerup", up, true);
  document.addEventListener("pointercancel", cancel, true);
  document.addEventListener("keydown", key, true);
}
