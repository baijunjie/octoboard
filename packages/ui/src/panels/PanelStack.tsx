import React, { useEffect, useLayoutEffect, useRef, useState } from "react";

import { HandOffOnUnmount } from "../components/HandOffOnUnmount";
import { focusLanding } from "../components/regionFocus";
import { ResizeSeparator } from "../components/ResizeSeparator";
import { useT } from "../i18n/react";
import { PANEL_CARD_CLASS } from "../layout/paneOverlay";
import { ownerKey, type AsideOwner } from "../layout/asideOwner";
import type { PanePeek } from "../layout/usePaneToggles";
import type { Project } from "../protocol";
import { PanelFrameProvider } from "../components/PanelHeader";
import { PANEL_ENTRIES, type PanelContentProps } from "./panelRegistry";
import {
  closePanel,
  maximizedKind,
  MIN_PANEL_HEIGHT,
  panelId,
  OWNER_KINDS,
  panelRegion,
  resetBoundary,
  resizeBoundary,
  sharesOf,
  shownKinds,
  stackedKinds,
  toggleMaximized,
  type PanelKind,
  type PanelStack as Stack,
} from "./stackState";
import { usePanelStacks } from "./usePanelStacks";

/**
 * A card's content, keyed by owner so a panel drops whatever it read for the owner before. Once the
 * owner has changed from the one this card mounted with, the content appears because it changed
 * under a card that is already on screen, and slides in, going back to the first owner included;
 * what mounts for the first owner docks, floats or opens in with the card, however late (a lazily
 * loaded panel mounts when its chunk arrives). The card's own owner is what counts, so a panel
 * opened beside another after an owner change does not slide.
 */
function CardContent({ Content, ...props }: Omit<PanelContentProps, "switching"> & { Content: React.ComponentType<PanelContentProps> }): React.ReactElement {
  const key = ownerKey(props.owner);
  const [firstOwner] = useState(key);
  const [moved, setMoved] = useState(false);
  if (!moved && key !== firstOwner) setMoved(true);
  return <Content key={key} {...props} switching={moved} />;
}

/** Where the agent terminal's area is in the row they share (`main` in `App.tsx`, the containing
 * block of the cards' absolute positions), in px, and which way the row runs. */
interface AreaBox {
  left: number;
  top: number;
  width: number;
  height: number;
  rtl: boolean;
}

function measureArea(area: HTMLElement): AreaBox {
  return { left: area.offsetLeft, top: area.offsetTop, width: area.offsetWidth, height: area.offsetHeight, rtl: getComputedStyle(area).direction === "rtl" };
}

/** Over the agent terminal's area and what is in it (`z-1` at the breakpoint), under the pane's
 * width handle (`z-10`) and every overlay (the floating cards and the sidebar's drawer, `z-40`). */
const MAXIMIZED_LAYER_CLASS = "absolute z-5";

/** The backdrop the maximized card lies on: the whole area, in the agent terminal's background, so
 * the card keeps its gaps and nothing of the covered terminal shows in them. */
function backdropStyle(box: AreaBox): React.CSSProperties {
  return { left: box.left, top: box.top, width: box.width, height: box.height };
}

/** The maximized card: the area inset by `--peek-gap` on every side, as a docked card is from the top
 * bar and the window's edges, except on the end side while the pane's column is beside it, whose
 * own grip gap already parts the two. */
function cardStyle(box: AreaBox, columnShown: boolean): React.CSSProperties {
  const gap = "var(--peek-gap)";
  const end = columnShown ? "0px" : gap;
  return {
    top: `calc(${box.top}px + ${gap})`,
    height: `calc(${box.height}px - 2 * ${gap})`,
    left: `calc(${box.left}px + ${box.rtl ? end : gap})`,
    width: `calc(${box.width}px - ${gap} - ${end})`,
  };
}

/** The heights, in px, of the cards `container` holds, top to bottom. */
function cardHeights(container: HTMLElement): number[] {
  return Array.from(container.querySelectorAll<HTMLElement>("[data-panel-card]:not([data-panel-maximized])"), (card) => card.getBoundingClientRect().height);
}

/**
 * The right pane's stack of panel cards (`stackState.ts` has the state, `panelRegistry.tsx` the
 * kinds): the owner's open kinds, top to bottom. The pane pads the cards (`AsidePane.tsx`) and
 * `--panel-stack-gap` lies between neighbours, where a resize grip sits; it paints nothing, so the
 * agent terminal's background shows in it and the cards read as lying on the content. A single card
 * takes the full height. Each card is a named region and a stop of the F6 cycle.
 *
 * A maximizable kind's card (`panelRegistry.tsx`) can be maximized from its header while docked:
 * it then leaves the column and covers the agent terminal's area (`maximizeArea`), over a backdrop,
 * while the other open cards keep the column, sharing it without handles. It is the same element,
 * only positioned absolutely over the area, so what it holds (a terminal's views and their
 * connections) is neither moved nor mounted again, and restoring puts it back in the column as it
 * was. `App.tsx` makes the covered area inert, and folds the column away when no card is left in
 * it, the area then reaching the window's end. A handle moves the
 * boundary between the pair it lies between and nothing else; the stack's own height does not
 * change with it, so the agent terminal beside the pane is not resized by dragging one.
 *
 * With no kind open the stack shows only the kind whose toggle the pointer is on (`peekKind`), while
 * the pane floats in (`peek`), as a stack of its own that closing dismisses. Its contents are keyed by owner
 * (`CardContent`).
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
  onViewFile,
  onDismiss,
  maximizeArea,
  areaEasing = false,
  floatOnly = false,
}: {
  owner: AsideOwner;
  project: Project | undefined;
  active: boolean;
  selectedSessionId: string | undefined;
  focusTerminal: () => void;
  /** The floating pane's hover reveal; `undefined` while the pane is docked. */
  peek: PanePeek | undefined;
  peekKind: PanelKind | undefined;
  frameKeys: PanelContentProps["frameKeys"];
  onViewFile?: PanelContentProps["onViewFile"];
  /** Closing a panel that was only floating in dismisses the floating pane. */
  onDismiss: () => void;
  /** The agent terminal's area, which a maximized card covers. */
  maximizeArea: React.RefObject<HTMLElement | null>;
  /** The sidebar's dock is easing, which resizes the area, and so a maximized card, every frame. */
  areaEasing?: boolean;
  /** This stack is only the floating pane beside a folded column (`RightPane.tsx`): it shows the
   * hovered kind alone, whatever is open. */
  floatOnly?: boolean;
}): React.ReactElement {
  const t = useT();
  const order = OWNER_KINDS[owner.kind];
  const { stacks, setStack: setStackOf, persist } = usePanelStacks();
  const stored = stacks[owner.kind];
  const setStack = (next: Stack, options?: { persist?: boolean }) => setStackOf(owner.kind, next, options);
  const open = floatOnly ? [] : shownKinds(stored, order);
  // The kind a floating pane shows is taken while it floats in, and kept while it floats out, so a
  // stack emptied by closing its last docked panel shows nothing more on its way out (the pane goes
  // at once, but a float-out still fading would otherwise take the hovered kind's card).
  const [floatKind, setFloatKind] = useState<PanelKind>();
  if (open.length > 0) {
    if (floatKind !== undefined) setFloatKind(undefined);
  } else if (peek?.active && peekKind !== undefined && order.includes(peekKind) && floatKind !== peekKind) {
    setFloatKind(peekKind);
  }
  const stack: Stack = open.length > 0 ? stored : { open: floatKind ? [floatKind] : [], shares: {} };
  const kinds = shownKinds(stack, order);
  const stacked = stackedKinds(stack, order);
  const maximized = maximizedKind(stack, order);
  const shares = sharesOf(stack, stacked);
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
  }, [stacked.join("\0")]);
  // Where the area a maximized card covers is, measured as it changes size (with the window, the
  // sidebar or the pane's width) and laid out before the frame is painted.
  const [area, setArea] = useState<AreaBox>();
  useLayoutEffect(() => {
    const element = maximized === undefined ? null : maximizeArea.current;
    if (!element) return setArea(undefined);
    const measure = () => setArea(measureArea(element));
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    // The offsets are against `main`, which can move the area without resizing it (the row running
    // the other way, as in RTL, puts the pane's column on the other side).
    if (element.offsetParent) observer.observe(element.offsetParent);
    return () => observer.disconnect();
  }, [maximized, maximizeArea]);
  // Maximizing covers the agent terminal's area, and the press that does it does not move focus,
  // which may sit there: it goes, before the area turns inert, to where F6 would land in the card.
  // Restoring uncovers and moves nothing.
  const toggleMaximize = (kind: PanelKind) => {
    const card = containerRef.current?.querySelector(`#${panelId(kind)}`);
    if (stored.maximized !== kind && card && maximizeArea.current?.contains(document.activeElement)) focusLanding(card);
    setStack(toggleMaximized(stored, kind), { persist: false });
  };
  const available = heights.reduce((sum, height) => sum + height, 0);

  const close = (kind: PanelKind) => {
    if (open.length === 0) onDismiss();
    else setStack(closePanel(stored, kind, order));
  };

  return (
    <div ref={containerRef} className="flex min-h-0 flex-1 flex-col">
      {maximized !== undefined && area && <div aria-hidden="true" className={`${MAXIMIZED_LAYER_CLASS} bg-panel`} style={backdropStyle(area)} />}
      {kinds.map((kind, position) => {
        const entry = PANEL_ENTRIES[kind as PanelKind];
        const Content = entry.Content;
        const covering = kind === maximized && area !== undefined;
        const index = stacked.indexOf(kind);
        return (
          <React.Fragment key={kind}>
            {/* First among what goes away with the card, so focus inside it is still found. */}
            <HandOffOnUnmount from={`#${panelId(kind as PanelKind)}`} to={focusTerminal} />
            {/* Only while no card is maximized: the column's shares are then all its own. */}
            {index > 0 && maximized === undefined && (
              <div className="relative h-(--panel-stack-gap) shrink-0">
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
                  className="absolute inset-x-0 -inset-y-[calc((24px-var(--panel-stack-gap))/2)] z-10"
                />
              </div>
            )}
            <section
              id={panelId(kind as PanelKind)}
              data-panel-card
              data-panel-maximized={kind === maximized ? "" : undefined}
              data-region={panelRegion(kind as PanelKind)}
              role="region"
              aria-label={entry.label(t, project)}
              style={covering
                ? cardStyle(area, stacked.length > 0)
                : { flex: `${stacked.length === 1 || index < 0 ? 1 : shares[index]} 1 0px`, minHeight: MIN_PANEL_HEIGHT }}
              className={`control-fills pointer-events-auto flex min-w-0 flex-col ${PANEL_CARD_CLASS} ${covering ? MAXIMIZED_LAYER_CLASS : "relative"}`}
            >
              <PanelFrameProvider
                value={{
                  // Not while only floating in: that card is a hover's preview, not part of the layout.
                  canMaximize: entry.maximizable === true && peek === undefined,
                  maximized: kind === maximized,
                  onToggleMaximized: () => toggleMaximize(kind as PanelKind),
                  onClose: () => close(kind as PanelKind),
                }}
              >
                <CardContent Content={Content} owner={owner} project={project} active={active} selectedSessionId={selectedSessionId} focusTerminal={focusTerminal} peek={peek} frameKeys={frameKeys} onViewFile={onViewFile} resizing={covering && areaEasing} />
              </PanelFrameProvider>
            </section>
          </React.Fragment>
        );
      })}
    </div>
  );
}
