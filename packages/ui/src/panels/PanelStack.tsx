import React, { useEffect, useRef, useState } from "react";

import { HandOffOnUnmount } from "../components/HandOffOnUnmount";
import { FOCUSABLE } from "../components/regionFocus";
import { ResizeSeparator } from "../components/ResizeSeparator";
import { useT } from "../i18n/react";
import { PANEL_CARD_CLASS } from "../layout/paneOverlay";
import { ownerKey, type AsideOwner } from "../layout/asideOwner";
import type { Project } from "../protocol";
import { PanelFrameProvider } from "../components/PanelHeader";
import { PANEL_ENTRIES, type PanelContentProps } from "./panelRegistry";
import {
  closePanel,
  MIN_PANEL_HEIGHT,
  panelId,
  OWNER_KINDS,
  panelRegion,
  resetBoundary,
  resizeBoundary,
  sharesOf,
  shownKinds,
  toggleMaximized,
  visibleKinds,
  type PanelKind,
  type PanelStack as Stack,
} from "./stackState";
import { usePanelStacks } from "./usePanelStacks";

/** The heights, in px, of the cards `container` holds, top to bottom. */
function cardHeights(container: HTMLElement): number[] {
  return Array.from(container.querySelectorAll<HTMLElement>("[data-panel-card]:not([data-hidden])"), (card) => card.getBoundingClientRect().height);
}

/**
 * The right pane's stack of panel cards (`stackState.ts` has the state, `panelRegistry.tsx` the
 * kinds): the owner's open kinds, top to bottom. The pane pads the cards (`AsidePane.tsx`) and
 * `--panel-grip-gap` lies between neighbours, where a resize grip sits; it paints nothing, so the
 * agent terminal's background shows in it and the cards read as lying on the content. A single card
 * takes the full height. Each card is a named region and a stop of the F6 cycle.
 *
 * A panel that is not shown (another is maximized) stays mounted and is only hidden, so what it
 * holds, a file browser's scroll position and a terminal's session, is kept. A handle moves the
 * boundary between the pair it lies between and nothing else; the stack's own height does not
 * change with it, so the agent terminal beside the pane is not resized by dragging one.
 *
 * With no kind open the stack shows only the kind whose toggle the pointer is on (`peekKind`), while
 * the pane floats in, as a stack of its own that closing dismisses. Its contents are keyed by owner,
 * so a panel drops whatever it read for the owner before.
 */
export function PanelStack({
  owner,
  project,
  active,
  selectedSessionId,
  focusTerminal,
  peek,
  peekKind,
  frameKeys,
  onDismiss,
}: {
  owner: AsideOwner;
  project: Project | undefined;
  active: boolean;
  selectedSessionId: string | undefined;
  focusTerminal: () => void;
  peek: PanelContentProps["peek"];
  peekKind: PanelKind | undefined;
  frameKeys: PanelContentProps["frameKeys"];
  /** Closing a panel that was only floating in dismisses the floating pane. */
  onDismiss: () => void;
}): React.ReactElement {
  const t = useT();
  const order = OWNER_KINDS[owner.kind];
  const { stacks, setStack: setStackOf, persist } = usePanelStacks();
  const stored = stacks[owner.kind];
  const setStack = (next: Stack, options?: { persist?: boolean }) => setStackOf(owner.kind, next, options);
  const open = shownKinds(stored, order);
  // Once the owner has changed from the one the stack mounted with, a panel appears because it
  // changed under a pane that is already on screen, and slides in, going back to the first owner
  // included; what mounts for the first owner docks or floats in with the stack, however late (a
  // lazily loaded panel mounts when its chunk arrives).
  const [firstOwner] = useState(() => ownerKey(owner));
  const [moved, setMoved] = useState(false);
  if (!moved && ownerKey(owner) !== firstOwner) setMoved(true);
  const floating = open.length === 0 && peekKind !== undefined && order.includes(peekKind);
  const stack: Stack = open.length > 0 ? stored : { open: floating && peekKind ? [peekKind] : [], shares: {} };
  const kinds = shownKinds(stack, order);
  const visible = visibleKinds(stack, order);
  const shares = sharesOf(stack, kinds);
  const containerRef = useRef<HTMLDivElement>(null);
  // The heights the cards are laid out at, which the handles' values and the drag's arithmetic need
  // in px; the shares alone cannot say, as the gaps and the minimum heights take their part.
  const [heights, setHeights] = useState<number[]>([]);
  useEffect(() => {
    const container = containerRef.current;
    if (!container || typeof ResizeObserver === "undefined") return;
    const measure = () => setHeights(cardHeights(container));
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(container);
    for (const card of container.querySelectorAll("[data-panel-card]")) observer.observe(card);
    return () => observer.disconnect();
  }, [visible.join("\0")]);
  // Maximizing hides the other cards, and the press that does it does not move focus, which may
  // sit in one of them: it goes, before they are hidden, to where F6 would land in the card kept.
  const maximize = (kind: PanelKind) => {
    const container = containerRef.current;
    const card = container?.querySelector(`#${panelId(kind)}`);
    if (stored.maximized !== kind && card && container?.contains(document.activeElement) && !card.contains(document.activeElement)) {
      (card.querySelector<HTMLElement>("[data-region-landing]") ?? card.querySelector<HTMLElement>(FOCUSABLE))?.focus();
    }
    setStack(toggleMaximized(stored, kind), { persist: false });
  };
  const available = heights.reduce((sum, height) => sum + height, 0);

  const close = (kind: PanelKind) => {
    if (open.length === 0) onDismiss();
    else setStack(closePanel(stored, kind, order));
  };

  return (
    <div ref={containerRef} className="flex min-h-0 flex-1 flex-col">
      {kinds.map((kind, position) => {
        const entry = PANEL_ENTRIES[kind as PanelKind];
        const Content = entry.Content;
        const shown = visible.includes(kind);
        const index = visible.indexOf(kind);
        return (
          <React.Fragment key={kind}>
            {/* First among what goes away with the card, so focus inside it is still found. */}
            <HandOffOnUnmount from={`#${panelId(kind as PanelKind)}`} to={focusTerminal} />
            {index > 0 && (
              <div className="relative h-(--panel-grip-gap) shrink-0">
                <ResizeSeparator
                  orientation="horizontal"
                  value={Math.round(heights[index - 1] ?? 0)}
                  min={MIN_PANEL_HEIGHT}
                  max={Math.max(MIN_PANEL_HEIGHT, Math.round((heights[index - 1] ?? 0) + (heights[index] ?? 0) - MIN_PANEL_HEIGHT))}
                  label={t("pane.resizePanels", {
                    upper: t(PANEL_ENTRIES[kinds[position - 1] as PanelKind].name),
                    lower: t(entry.name),
                  })}
                  grow={1}
                  appearance="grip"
                  tooltip={t("pane.resize")}
                  onChange={(height, options) =>
                    setStack(resizeBoundary(stored, position - 1, height, available, MIN_PANEL_HEIGHT, order), { persist: options?.commit !== false })
                  }
                  onCommit={persist}
                  onReset={() => setStack(resetBoundary(stored, position - 1, order))}
                  region={panelRegion(kinds[position - 1] as PanelKind)}
                  className="absolute inset-x-0 -inset-y-0.5 z-10"
                />
              </div>
            )}
            <section
              id={panelId(kind as PanelKind)}
              data-panel-card
              data-hidden={shown ? undefined : ""}
              data-region={shown ? panelRegion(kind as PanelKind) : undefined}
              role="region"
              aria-label={entry.label(t, project)}
              style={{ flex: `${visible.length === 1 ? 1 : shares[position]} 1 0px`, minHeight: shown ? MIN_PANEL_HEIGHT : undefined }}
              className={`control-fills pointer-events-auto relative min-w-0 flex-col ${PANEL_CARD_CLASS} ${shown ? "flex" : "hidden"}`}
            >
              <PanelFrameProvider
                value={{
                  canMaximize: kinds.length > 1,
                  maximized: stack.maximized === kind,
                  onToggleMaximized: () => maximize(kind as PanelKind),
                  onClose: () => close(kind as PanelKind),
                }}
              >
                <Content key={ownerKey(owner)} owner={owner} switching={moved} project={project} active={active && shown} selectedSessionId={selectedSessionId} focusTerminal={focusTerminal} peek={peek} frameKeys={frameKeys} />
              </PanelFrameProvider>
            </section>
          </React.Fragment>
        );
      })}
    </div>
  );
}
