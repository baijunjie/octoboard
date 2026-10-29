import { Tabs } from "@heroui/react";
import { TriangleAlert } from "lucide-react";
import React, { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { useFocusVisible, useObjectRef } from "react-aria";

import { AGENT_LABEL } from "../agents";
import { useT } from "../i18n/react";
import type { Agent } from "../protocol";
import { AgentIcon } from "./AgentIcon";
import { AnchoredTooltip } from "./AnchoredTooltip";
import { beginTabDrag, DRAG_THRESHOLD } from "./tabDrag";
import { useLatest, useStableHandler } from "./useLatest";
import { DURATION, releaseListMotion, useListMotion } from "./useListMotion";
import { useTabScrollLabels } from "./tabScrollLabels";

/** How long after keyboard focus arrives to wait for the scroll that reveals the tab to begin, and how long a
 * scroll has to be quiet before it counts as over. A tooltip is closed by a scroll under it, so one opened
 * before the reveal would be gone at once. */
const REVEAL_START_MS = 64;
const REVEAL_QUIET_MS = 100;

/** The underline strip of agent tabs, one per agent in `agents`, for the `Tabs` it sits in: each tab
 * the agent's icon and name at its content's width, from the strip's start, and the strip scrolls with
 * its edge labels when the tabs do not fit. Its container takes `className` and the other props of
 * `Tabs.ListContainer`. `tabRef` gives a tab's element a ref callback, for the one a caller means to
 * focus. `notInstalledTip` marks the agents that are not installed: it returns the tooltip content for
 * such an agent and nothing for any other. That agent's tab gets a warning glyph after its name, "not
 * installed" in its accessible name, and the tooltip over the tab, opened by pointer hover and by
 * keyboard focus (see `AgentTab`). With `onReorder` the tabs can be dragged with a mouse or pen, and it
 * is called with the whole new order and the agent moved when one is dropped elsewhere; the order itself
 * stays the caller's, and the tabs fall back to `agents` until it changes, so the caller shows the new
 * order at once. Tabs added, removed or reordered while the strip is shown move with the shared list motion.
 * `trailing` is a control at the end of the strip, flush with the container's end. With a trailing control, or with
 * `spanning` when there is none (for a divider drawn under it), the container spans its row; the tabs keep their
 * content width either way.
 * The container is padded at its end (`trailingWidth`, a Tailwind end-padding class) so the tabs scroll clear of it,
 * and HeroUI's scroll-forward chevron, which would sit under it, is moved to just before it.
 * `tabActions` gives each agent a control of its own at the end of its tab (see `useTabActionPlaces`): a tab is itself
 * the control, so nothing can be nested in it, and the control is a sibling placed over room the tab keeps for it. It is
 * hidden while its tab is not wholly in view, unless it holds keyboard focus, and a control that held focus when the
 * tabs were reordered (its menu's Move earlier / later) has it back once they have moved. While a tab is dragged, and
 * while the tabs move into their places after a drag or a reorder, every control but the one holding focus fades
 * out, and fades back in at its new place once they have settled. */
export function AgentTabList({
  agents,
  label,
  tabRef,
  notInstalledTip,
  onReorder,
  trailing,
  trailingWidth = "pe-14",
  spanning,
  tabActions,
  ...container
}: {
  agents: Agent[];
  /** The tab list's accessible name. */
  label: string;
  tabRef?: (agent: Agent) => React.Ref<HTMLDivElement> | undefined;
  notInstalledTip?: (agent: Agent) => React.ReactNode;
  onReorder?: (agents: Agent[], moved: Agent) => void;
  trailing?: React.ReactNode;
  trailingWidth?: string;
  spanning?: boolean;
  tabActions?: (agent: Agent) => React.ReactNode;
} & Omit<React.HTMLAttributes<HTMLDivElement>, "children">): React.ReactElement {
  const tabScrollLabels = useTabScrollLabels();
  const [elements] = useState(() => new Map<Agent, HTMLElement>());
  const listRef = useListMotion<HTMLDivElement>({ contentWidth: true });
  const latest = useLatest({ agents, onReorder });
  const dragStart = (agent: Agent, event: PointerEvent) => {
    const { agents: current, onReorder: reorder } = latest.current;
    // A dropped tab moves into its new place from where the drag left it, not from where it started.
    const drop = (order: Agent[], moved: Agent) => {
      const tab = elements.get(moved);
      if (tab) releaseListMotion(tab);
      reorder?.(order, moved);
    };
    if (reorder && current.length > 1) beginTabDrag({ agent, agents: current, elements, event, onDrop: drop });
  };
  const overlay = useRef<HTMLDivElement>(null);
  // The tabs are made by the collection a render after the strip, and this follows their arrival and departure.
  const [, setTabsVersion] = useState(0);
  const tabsChanged = useCallback(() => setTabsVersion((version) => version + 1), []);
  // The agent whose control holds keyboard focus, or last did before something took it away without the user
  // choosing another place.
  const heldFocus = useRef<Agent | undefined>(undefined);
  // The agent whose control holds focus right now, which stays visible while the others fade.
  const [focusedAgent, setFocusedAgent] = useState<Agent>();
  const { places, moving } = useTabActionPlaces(overlay, agents, tabActions !== undefined, heldFocus);
  const order = agents.join();
  // Moving a focused element in the document drops its focus: a reorder moves the controls with their tabs.
  useLayoutEffect(() => {
    const agent = heldFocus.current;
    if (!agent || document.activeElement !== document.body) return;
    overlay.current?.querySelector<HTMLElement>(`[data-tab-actions="${agent}"] button:not([hidden])`)?.focus();
  }, [order]);
  return (
    <Tabs.ListContainer
      {...container}
      data-trailing-control={trailing ? "" : undefined}
      className={`min-w-0 max-w-full ${trailing ? trailingWidth : spanning ? "" : "w-max self-start"} ${container.className ?? ""}`}
      render={(props) => (
        <div {...props}>
          {tabScrollLabels(props.children)}
          {tabActions && (
            <div ref={overlay} className="pointer-events-none absolute inset-0 z-2">
              {agents.map((agent) => {
                const place = places[agent];
                // While tabs are dragged or moving, the controls fade out (all but the one holding focus) so they do
                // not overlap the tabs, and fade back in at their new places.
                const faded = moving && focusedAgent !== agent;
                return (
                  <div
                    key={agent}
                    data-tab-actions={agent}
                    hidden={!place}
                    className={`absolute transition-opacity duration-150 motion-reduce:transition-none ${faded ? "pointer-events-none opacity-0" : "pointer-events-auto"}`}
                    style={place && { left: place.left, top: place.top }}
                    onFocus={() => { heldFocus.current = agent; setFocusedAgent(agent); }}
                    onBlur={(event) => {
                      // Focus going into this control's own menu is still its focus; going anywhere else is the user's.
                      const to = event.relatedTarget;
                      if (to instanceof Element && !to.closest("[data-slot=dropdown-popover]")) heldFocus.current = undefined;
                      if (!(to instanceof Element && event.currentTarget.contains(to))) setFocusedAgent((now) => (now === agent ? undefined : now));
                    }}
                  >
                    {tabActions(agent)}
                  </div>
                );
              })}
            </div>
          )}
          {trailing && <div className="absolute inset-y-0 end-0 z-3 flex items-center">{trailing}</div>}
        </div>
      )}
    >
      {/* Positioned, so a departing tab's visual stays in the strip's scroll. */}
      <Tabs.List ref={listRef} aria-label={label} className="relative w-max min-w-0 p-1.5">
        {agents.map((agent) => (
          <AgentTab key={agent} agent={agent} tabRef={tabRef?.(agent)} notInstalledTip={notInstalledTip?.(agent)} elements={elements} onDragStart={onReorder && dragStart} onElementChange={tabsChanged} reserve={tabActions !== undefined} />
        ))}
      </Tabs.List>
    </Tabs.ListContainer>
  );
}

/** One agent's tab. With a `notInstalledTip` its tooltip is an `AnchoredTooltip` placed against the tab
 * itself: the tab is itself the control, so no trigger can be nested inside it, and the glyph is decorative.
 * It opens while the pointer is on the tab and as the tab takes keyboard focus (by react-aria's input
 * modality, not CSS `:focus-visible`, so a press that focuses it with the mouse does not, nor does a later
 * key press while it stays focused). Keyboard focus governs while it lasts: the tooltip opens without the
 * hover delay, once the scroll that reveals the tab has settled, and a key or pointer press before that
 * cancels it. It closes on any key, Escape included, without taking focus, and then stays closed until the
 * pointer is off the tab and the tab has lost focus. Each arrival of keyboard focus mounts the tooltip
 * afresh, so it opens even when a key press had closed the one the pointer's rest had opened. The tooltip is not
 * linked to the tab for assistive technology; the tab's accessible name says "not installed", and whatever
 * more the tip says has to reach assistive technology elsewhere, which is up to the caller. */
function AgentTab({ agent, tabRef, notInstalledTip, elements, onDragStart, onElementChange, reserve }: {
  agent: Agent;
  tabRef?: React.Ref<HTMLDivElement>;
  notInstalledTip?: React.ReactNode;
  /** Where the strip keeps its tabs' elements, for a drag to move them. */
  elements: Map<Agent, HTMLElement>;
  onDragStart?: (agent: Agent, event: PointerEvent) => void;
  /** Told when the tab's element arrives or goes. */
  onElementChange?: () => void;
  /** Keeps room at the tab's end for the strip's control for it (`tabActions`). */
  reserve?: boolean;
}): React.ReactElement {
  const t = useT();
  const ref = useObjectRef(tabRef);
  const dragStartRef = useLatest(onDragStart);
  // The tab's element is made by the collection, not by this component, so the strip's record of it and the
  // drag's listener hang on the ref. They are native: the tab's own press handling takes the pointer events a React
  // prop would add to.
  const attach = useCallback((tab: HTMLDivElement | null) => {
    ref.current = tab;
    if (!tab) return;
    elements.set(agent, tab);
    onElementChange?.();
    const onPointerDown = (event: PointerEvent) => dragStartRef.current?.(agent, event);
    tab.addEventListener("pointerdown", onPointerDown);
    return () => {
      tab.removeEventListener("pointerdown", onPointerDown);
      if (elements.get(agent) === tab) elements.delete(agent);
      ref.current = null;
      onElementChange?.();
    };
  }, [agent, elements, ref, dragStartRef, onElementChange]);
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  // Counts the arrivals of keyboard focus: the tooltip's key, so each one opens it anew.
  const [arrivals, setArrivals] = useState(0);
  // Whether the scroll that took the tab into view as keyboard focus arrived is over (see `REVEAL_START_MS`).
  const [settled, setSettled] = useState(false);
  // Set when a key or press cancels the wait for the reveal; the tooltip stays away until the pointer has left
  // and focus is gone.
  const [dismissed, setDismissed] = useState(false);
  const latestRef = useLatest(ref);
  // Whether the focus is the keyboard's is decided in the render after focus arrives, not in `onFocus`: the key
  // press that flips react-aria's modality also moves focus before React renders again, so `isFocusVisible`
  // read in the handler would still be the previous one. The modality update and the focus count are
  // batched into that one render, which sees both.
  const { isFocusVisible } = useFocusVisible();
  // `hasFocus` is the last of `onFocus` and `onBlur`, so a focus and a blur in one batch leave it false.
  const [hasFocus, setHasFocus] = useState(false);
  const [focusCount, setFocusCount] = useState(0);
  const [handledCount, setHandledCount] = useState(0);
  if (focusCount !== handledCount) {
    setHandledCount(focusCount);
    setFocused(hasFocus && isFocusVisible);
    if (hasFocus && isFocusVisible) {
      setArrivals((count) => count + 1);
      setSettled(false);
      setDismissed(false);
    }
  }
  // A cancel holds until the pointer is off the tab and the tab has lost focus.
  if (dismissed && !hovered && !focused) setDismissed(false);
  useEffect(() => {
    if (!focused || settled || dismissed) return;
    const tab = latestRef.current.current;
    let timer = setTimeout(() => setSettled(true), REVEAL_START_MS);
    // Scroll events do not bubble, so they are caught in the capture phase; only the tab's own scrollers count.
    const onScroll = (event: Event) => {
      if (!(event.target instanceof Node) || !event.target.contains(tab)) return;
      clearTimeout(timer);
      timer = setTimeout(() => setSettled(true), REVEAL_QUIET_MS);
    };
    const onInput = () => setDismissed(true);
    document.addEventListener("scroll", onScroll, true);
    document.addEventListener("keydown", onInput, true);
    document.addEventListener("pointerdown", onInput, true);
    return () => {
      clearTimeout(timer);
      document.removeEventListener("scroll", onScroll, true);
      document.removeEventListener("keydown", onInput, true);
      document.removeEventListener("pointerdown", onInput, true);
    };
  }, [focused, settled, dismissed, latestRef]);
  return (
    <Tabs.Tab
      id={agent}
      ref={attach}
      data-list-motion={agent}
      aria-label={notInstalledTip ? t("agents.notInstalled", { agent: AGENT_LABEL[agent] }) : undefined}
      onHoverChange={setHovered}
      onFocus={() => {
        setHasFocus(true);
        setFocusCount((count) => count + 1);
      }}
      onBlur={() => {
        setHasFocus(false);
        setFocusCount((count) => count + 1);
      }}
      className={`w-auto flex-none scroll-mx-1.5 gap-2 whitespace-nowrap hover:text-foreground hover:opacity-100 ${reserve ? "pe-9" : ""}`}
    >
      <AgentIcon agent={agent} />
      {AGENT_LABEL[agent]}
      {notInstalledTip && (
        <>
          <TriangleAlert aria-hidden="true" className="-ms-1 size-3.5 shrink-0 text-warning-soft-foreground" />
          <AnchoredTooltip
            key={arrivals}
            tooltip={notInstalledTip}
            anchor={ref}
            hovered={!dismissed && (focused ? settled : hovered)}
            immediate={focused}
          />
        </>
      )}
      <Tabs.Indicator className="-bottom-1.5" />
    </Tabs.Tab>
  );
}

/** The side length of a tab's own control (`size-6`), in pixels. */
const TAB_ACTION_SIZE = 24;
const TAB_ACTION_INSET = 4;
/** How long a moving tab is followed after its motion should have ended: list motion's duration and a margin. */
const MOTION_MS = DURATION + 50;

type TabActionPlace = { left: number; top: number };

/** Where each agent's control (`tabActions`) sits within `overlay`, which covers the strip's container: at the end of
 * the agent's tab, centred on it, or none while that spot is not wholly inside the strip's own scroller, so a tab
 * scrolled partly out of view has no control hanging in the air. Measured after every commit, on every scroll and
 * resize of the strip, and on every animation frame while a tab is dragged or moving with list motion, so the control
 * that holds focus goes with its tab (the others are faded out meanwhile, `moving`); a place equal to the held one
 * changes nothing. */
function useTabActionPlaces(overlay: React.RefObject<HTMLElement | null>, agents: Agent[], enabled: boolean, heldFocus: React.RefObject<Agent | undefined>): { places: Partial<Record<Agent, TabActionPlace>>; moving: boolean } {
  const [places, setPlaces] = useState<Partial<Record<Agent, TabActionPlace>>>({});
  // A tab is being dragged or the tabs are moving into place: the controls are faded out meanwhile.
  const [moving, setMoving] = useState(false);
  const shownOrder = useRef(agents.join());
  const measure = () => {
    const frame = overlay.current;
    if (!frame) return;
    const origin = frame.getBoundingClientRect();
    const rtl = getComputedStyle(frame).direction === "rtl";
    const next: Partial<Record<Agent, TabActionPlace>> = {};
    // The tabs are found by the key the tab list gives them, in the strip the overlay sits in.
    for (const agent of agents) {
      const tab = frame.parentElement?.querySelector<HTMLElement>(`[role="tab"][data-key="${agent}"]`);
      if (!tab) continue;
      const box = tab.getBoundingClientRect();
      const scroller = tab.closest(".scroll-shadow")?.getBoundingClientRect();
      const left = (rtl ? box.left + TAB_ACTION_INSET : box.right - TAB_ACTION_SIZE - TAB_ACTION_INSET) - origin.left;
      const top = box.top + (box.height - TAB_ACTION_SIZE) / 2 - origin.top;
      const absolute = left + origin.left;
      if (scroller && (absolute < scroller.left || absolute + TAB_ACTION_SIZE > scroller.right)) continue;
      next[agent] = { left: Math.round(left), top: Math.round(top) };
    }
    setPlaces((held) => {
      // A control that holds focus stays where it was while its tab is out of reach, so it does not vanish under focus.
      const keep = heldFocus.current;
      const placed = keep && !next[keep] && held[keep] ? { ...next, [keep]: held[keep] } : next;
      const keys = Object.keys(placed) as Agent[];
      const same = keys.length === Object.keys(held).length && keys.every((key) => held[key]?.left === placed[key]!.left && held[key]?.top === placed[key]!.top);
      return same ? held : placed;
    });
  };
  const measureLater = useStableHandler(measure);
  // Re-run on every commit, as the strip's other measuring does: a tab the list replaced has a new element.
  useLayoutEffect(() => {
    if (enabled) measure();
  });
  useEffect(() => {
    const frame = overlay.current;
    if (!enabled || !frame) return;
    const resize = new ResizeObserver(() => measureLater());
    resize.observe(frame);
    for (const tab of frame.parentElement?.querySelectorAll('[role="tab"]') ?? []) resize.observe(tab);
    // Scroll events do not bubble, so the strip's scroller is heard in the capture phase.
    const onScroll = () => measureLater();
    frame.parentElement?.addEventListener("scroll", onScroll, { capture: true, passive: true });
    // A tab dragged, or moving into its place with list motion, is measured on every frame for as long as it moves, so
    // the control that holds focus goes with it: from the press on a tab until the pointer is released, and `MOTION_MS`
    // after that. The controls are `moving` from the moment the drag starts until then.
    let following = false;
    let pressed = false;
    let startX = 0;
    let until = 0;
    let frameId = 0;
    const follow = (ms: number) => {
      until = Math.max(until, performance.now() + ms);
      if (following) return;
      following = true;
      const step = () => {
        measureLater();
        if (pressed || performance.now() < until) frameId = requestAnimationFrame(step);
        else {
          following = false;
          setMoving(false);
        }
      };
      frameId = requestAnimationFrame(step);
    };
    const onPress = (event: PointerEvent) => {
      // A touch on a tab scrolls the strip and drags nothing.
      if (event.pointerType === "touch" || !(event.target instanceof Element) || !event.target.closest('[role="tab"]')) return;
      pressed = true;
      startX = event.clientX;
      follow(0);
    };
    // A press that does not move past the drag threshold is a click, which moves nothing: the controls fade out once
    // a drag has really started.
    const onMove = (event: PointerEvent) => { if (pressed && Math.abs(event.clientX - startX) >= DRAG_THRESHOLD) setMoving(true); };
    const onRelease = () => {
      if (!pressed) return;
      pressed = false;
      follow(MOTION_MS);
    };
    frame.parentElement?.addEventListener("pointerdown", onPress, true);
    document.addEventListener("pointermove", onMove, true);
    document.addEventListener("pointerup", onRelease, true);
    document.addEventListener("pointercancel", onRelease, true);
    // A drag that loses its pointer without either event (the capture taken away, the window losing focus) ends too.
    // Only a mouse's: a pen's or finger's implicit capture is released by `usePress` while the drag goes on.
    const onLostCapture = (event: PointerEvent) => { if (event.pointerType === "mouse") onRelease(); };
    document.addEventListener("lostpointercapture", onLostCapture, true);
    window.addEventListener("blur", onRelease);
    // A reorder asked for another way (the menu's Move earlier / later) moves the tabs with list motion too, but the
    // first order shown is not a move.
    const order = agents.join();
    if (order !== shownOrder.current) {
      shownOrder.current = order;
      setMoving(true);
    }
    follow(MOTION_MS);
    return () => {
      cancelAnimationFrame(frameId);
      resize.disconnect();
      frame.parentElement?.removeEventListener("scroll", onScroll, { capture: true });
      frame.parentElement?.removeEventListener("pointerdown", onPress, true);
      document.removeEventListener("pointermove", onMove, true);
      document.removeEventListener("pointerup", onRelease, true);
      document.removeEventListener("pointercancel", onRelease, true);
      document.removeEventListener("lostpointercapture", onLostCapture, true);
      window.removeEventListener("blur", onRelease);
    };
  }, [enabled, agents, measureLater]);
  return { places, moving };
}
