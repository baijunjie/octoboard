import { Tabs } from "@heroui/react";
import { TriangleAlert } from "lucide-react";
import React, { useCallback, useEffect, useState } from "react";
import { useFocusVisible, useObjectRef } from "react-aria";

import { AGENT_LABEL } from "../agents";
import { useT } from "../i18n/react";
import type { Agent } from "../protocol";
import { AgentIcon } from "./AgentIcon";
import { AnchoredTooltip } from "./AnchoredTooltip";
import { beginTabDrag } from "./tabDrag";
import { useLatest } from "./useLatest";
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
 * order at once. `trailing` is a control at the end of the strip. It sits flush with the container's
 * end, where a row's actions button of the section beneath ends, so the two columns of buttons line up.
 * The container is padded at its end so the tabs scroll clear of it, and HeroUI's scroll-forward
 * chevron, which would sit under it, is moved to just before it. */
export function AgentTabList({
  agents,
  label,
  tabRef,
  notInstalledTip,
  onReorder,
  trailing,
  ...container
}: {
  agents: Agent[];
  /** The tab list's accessible name. */
  label: string;
  tabRef?: (agent: Agent) => React.Ref<HTMLDivElement> | undefined;
  notInstalledTip?: (agent: Agent) => React.ReactNode;
  onReorder?: (agents: Agent[], moved: Agent) => void;
  trailing?: React.ReactNode;
} & Omit<React.HTMLAttributes<HTMLDivElement>, "children">): React.ReactElement {
  const tabScrollLabels = useTabScrollLabels();
  const [elements] = useState(() => new Map<Agent, HTMLElement>());
  const latest = useLatest({ agents, onReorder });
  const dragStart = (agent: Agent, event: PointerEvent) => {
    const { agents: current, onReorder: reorder } = latest.current;
    if (reorder && current.length > 1) beginTabDrag({ agent, agents: current, elements, event, onDrop: reorder });
  };
  return (
    <Tabs.ListContainer
      {...container}
      data-trailing-control={trailing ? "" : undefined}
      className={`${container.className ?? ""} ${trailing ? "pe-14" : ""}`}
      render={(props) => (
        <div {...props}>
          {tabScrollLabels(props.children)}
          {trailing && <div className="absolute inset-y-0 end-0 z-3 flex items-center">{trailing}</div>}
        </div>
      )}
    >
      <Tabs.List aria-label={label} className="p-1.5">
        {agents.map((agent) => (
          <AgentTab key={agent} agent={agent} tabRef={tabRef?.(agent)} notInstalledTip={notInstalledTip?.(agent)} elements={elements} onDragStart={onReorder && dragStart} />
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
function AgentTab({ agent, tabRef, notInstalledTip, elements, onDragStart }: {
  agent: Agent;
  tabRef?: React.Ref<HTMLDivElement>;
  notInstalledTip?: React.ReactNode;
  /** Where the strip keeps its tabs' elements, for a drag to move them. */
  elements: Map<Agent, HTMLElement>;
  onDragStart?: (agent: Agent, event: PointerEvent) => void;
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
    const onPointerDown = (event: PointerEvent) => dragStartRef.current?.(agent, event);
    tab.addEventListener("pointerdown", onPointerDown);
    return () => {
      tab.removeEventListener("pointerdown", onPointerDown);
      if (elements.get(agent) === tab) elements.delete(agent);
      ref.current = null;
    };
  }, [agent, elements, ref, dragStartRef]);
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
      className="w-auto flex-none scroll-mx-1.5 gap-2 whitespace-nowrap hover:text-foreground hover:opacity-100"
    >
      <AgentIcon agent={agent} />
      {AGENT_LABEL[agent]}
      {notInstalledTip && (
        <>
          <TriangleAlert aria-hidden="true" className="-ms-1 size-3.5 shrink-0 text-warning-glyph" />
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
