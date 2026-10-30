import React, { useLayoutEffect, useRef, useState } from "react";

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
  unavailableKinds,
  type PanelKind,
  type PanelStack as Stack,
} from "./stackState";
import { usePanelStacks } from "./usePanelStacks";
import { panelColumns } from "./panelColumns";

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
 * kinds): the owner's open kinds, top to bottom in columns of at most two. The pane pads the cards
 * (`AsidePane.tsx`) and `--panel-stack-gap` lies between neighbours, where a resize grip sits; it paints nothing, so the
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
  reportSessionId,
  focusTerminal,
  peek,
  peekKind,
  frameKeys,
  onViewFile,
  onDismiss,
  maximizeArea,
  areaEasing = false,
  floatOnly = false,
  floating,
}: {
  owner: AsideOwner;
  project: Project | undefined;
  active: boolean;
  selectedSessionId: string | undefined;
  /** The session whose pages the Report panel shows (`PanelContentProps`). */
  reportSessionId?: string;
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
  /** The window is too narrow to dock the pane (`AsidePane.tsx`): the stack shows `stack`, the one
   * panel floating over the agent terminal (`useFloatingPanel.ts`), rather than the stored stack,
   * which it leaves as it is; closing that panel calls `onClose`. No card is maximized there, as
   * there is no area beside the pane left to cover. */
  floating?: { stack: Stack; onClose: () => void };
}): React.ReactElement {
  const t = useT();
  const order = OWNER_KINDS[owner.kind];
  // A project's stack carries the Report panel of the selected session while it has pages.
  const { stacks, setStack: setStackOf, persist } = usePanelStacks(owner.kind === "project" ? reportSessionId : undefined);
  // A panel floating over the terminal is kept while it fades away after closing (`AsidePane.tsx`
  // unmounts the stack once it has gone), unless a hover's card has taken the pane meanwhile.
  const floatingKind = floating ? shownKinds(floating.stack, order)[0] : undefined;
  const ownerId = ownerKey(owner);
  const [leaving, setLeaving] = useState<{ owner: string | undefined; kind: string }>();
  if (floatingKind !== undefined && (floatingKind !== leaving?.kind || ownerId !== leaving?.owner)) setLeaving({ owner: ownerId, kind: floatingKind });
  // A session's kind with no session to show (the Report panel, once the last page is deleted) has no
  // content left to show, floating in or fading away: its card goes at once, and hands its focus on
  // (`HandOffOnUnmount` below).
  const sessionGone = (kind: string) => unavailableKinds(owner.kind, reportSessionId).includes(kind);
  // Another owner's card is never shown for this one (Back or Forward while the pane fades away).
  const keep = floating && floatingKind === undefined && leaving !== undefined && leaving.owner === ownerId && !peek?.active && !sessionGone(leaving.kind) ? leaving.kind : undefined;
  const base = floating ? (keep !== undefined ? { open: [keep], shares: {} } : floating.stack) : stacks[owner.kind];
  const setStack = (next: Stack, options?: { persist?: boolean }) => setStackOf(owner.kind, next, options);
  const open = floatOnly ? [] : shownKinds(base, order);
  // The kind a floating pane shows is taken while it floats in, and kept while it floats out, so a
  // stack emptied by closing its last docked panel shows nothing more on its way out (the pane goes
  // at once, but a float-out still fading would otherwise take the hovered kind's card).
  const [floatKind, setFloatKind] = useState<PanelKind>();
  if (open.length > 0) {
    if (floatKind !== undefined) setFloatKind(undefined);
  } else if (peek?.active && peekKind !== undefined && order.includes(peekKind) && floatKind !== peekKind) {
    setFloatKind(peekKind);
  }
  const stack: Stack = open.length > 0 ? base : { open: floatKind && !sessionGone(floatKind) ? [floatKind] : [], shares: {} };
  const kinds = shownKinds(stack, order);
  const stacked = stackedKinds(stack, order);
  const maximized = maximizedKind(stack, order);
  const [containerSize, setContainerSize] = useState({ width: 0, gap: 0 });
  const columns = panelColumns(stacked, containerSize.width, containerSize.gap);
  const containerRef = useRef<HTMLDivElement>(null);
  // The heights the cards are laid out at, which the handles' values and the drag's arithmetic need
  // in px; the shares alone cannot say, as the gaps and the minimum heights take their part.
  const [heights, setHeights] = useState<number[]>([]);
  useLayoutEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const measure = () => {
      const width = container.getBoundingClientRect().width;
      const gap = parseFloat(getComputedStyle(container).columnGap) || 0;
      setContainerSize((current) => current.width === width && current.gap === gap ? current : { width, gap });
      setHeights(cardHeights(container));
    };
    measure();
    if (typeof ResizeObserver === "undefined") return;
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
    if (base.maximized !== kind && card && maximizeArea.current?.contains(document.activeElement)) focusLanding(card);
    setStack(toggleMaximized(base, kind), { persist: false });
  };
  const resizeColumn = (column: string[], update: (stack: Stack) => Stack, options?: { persist?: boolean }) => {
    const changed = update({ ...base, open: column });
    setStack({ ...base, shares: { ...base.shares, ...changed.shares } }, options);
  };

  const close = (kind: PanelKind) => {
    if (open.length === 0) onDismiss();
    else if (floating) floating.onClose();
    else setStack(closePanel(base, kind, order));
  };

  return (
    <div ref={containerRef} className="grid min-h-0 min-w-0 flex-1 grid-rows-[minmax(0,1fr)] gap-x-(--panel-stack-gap)" style={{ gridTemplateColumns: `repeat(${Math.max(1, columns.length)}, minmax(0, 1fr))` }}>
      {maximized !== undefined && area && <div aria-hidden="true" className={`${MAXIMIZED_LAYER_CLASS} bg-panel`} style={backdropStyle(area)} />}
      {kinds.map((kind) => {
        const entry = PANEL_ENTRIES[kind as PanelKind];
        const Content = entry.Content;
        const covering = kind === maximized && area !== undefined;
        const columnIndex = Math.max(0, columns.findIndex((candidate) => candidate.includes(kind)));
        const column = columns[columnIndex] ?? [];
        const index = column.indexOf(kind);
        const shares = sharesOf(stack, column);
        const upper = column[0];
        const upperPosition = stacked.indexOf(upper ?? "");
        const upperHeight = heights[upperPosition] ?? 0;
        const lowerHeight = heights[upperPosition + 1] ?? 0;
        const available = upperHeight + lowerHeight;
        // Cards share one grid cell per column. Insets divide that cell independently, without
        // reparenting a report frame or terminal when maximizing or changing the open kinds.
        const placement = { gridColumn: columnIndex + 1, gridRow: 1 };
        const split = `clamp(${MIN_PANEL_HEIGHT}px, calc((100% - var(--panel-stack-gap)) * ${shares[0] ?? 1}), calc(100% - var(--panel-stack-gap) - ${MIN_PANEL_HEIGHT}px))`;
        return (
          <React.Fragment key={kind}>
            {/* First among what goes away with the card, so focus inside it is still found. */}
            <HandOffOnUnmount from={`#${panelId(kind as PanelKind)}`} to={focusTerminal} />
            {/* Only while no card is maximized: the column's shares are then all its own. */}
            {index > 0 && maximized === undefined && (
              <div className="relative self-start h-(--panel-stack-gap)" style={{ ...placement, top: split }}>
                <ResizeSeparator
                  orientation="horizontal"
                  value={Math.round(upperHeight)}
                  min={MIN_PANEL_HEIGHT}
                  max={Math.max(MIN_PANEL_HEIGHT, Math.round(available - MIN_PANEL_HEIGHT))}
                  label={t("pane.resizePanels", {
                    upper: t(PANEL_ENTRIES[upper as PanelKind].name),
                    lower: t(entry.name),
                  })}
                  grow={1}
                  appearance="grip"
                  tooltip={t("pane.resize")}
                  onChange={(height, options) =>
                    resizeColumn(column, (current) => resizeBoundary(current, 0, height, available, MIN_PANEL_HEIGHT, column), { persist: options?.commit !== false })
                  }
                  onCommit={persist}
                  onReset={() => resizeColumn(column, (current) => resetBoundary(current, 0, column))}
                  region={panelRegion(upper as PanelKind)}
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
                : {
                  ...placement,
                  height: column.length < 2 || index < 0 ? "100%" : index === 0 ? split : `calc(100% - var(--panel-stack-gap) - ${split})`,
                  top: index === 1 ? `calc(${split} + var(--panel-stack-gap))` : undefined,
                  minHeight: MIN_PANEL_HEIGHT,
                  alignSelf: "start",
                }}
              className={`control-fills pointer-events-auto flex min-w-0 flex-col ${PANEL_CARD_CLASS} ${covering ? MAXIMIZED_LAYER_CLASS : "relative"}`}
            >
              <PanelFrameProvider
                value={{
                  // Not while only floating in: that card is a hover's preview, not part of the layout.
                  compact: columns.length > 1 && !covering,
                  canMaximize: entry.maximizable === true && peek === undefined && !floating,
                  maximized: kind === maximized,
                  onToggleMaximized: () => toggleMaximize(kind as PanelKind),
                  onClose: () => close(kind as PanelKind),
                }}
              >
                <CardContent Content={Content} owner={owner} project={project} active={active} selectedSessionId={selectedSessionId} reportSessionId={reportSessionId} focusTerminal={focusTerminal} peek={peek} frameKeys={frameKeys} onViewFile={onViewFile} resizing={covering && areaEasing} />
              </PanelFrameProvider>
            </section>
          </React.Fragment>
        );
      })}
    </div>
  );
}
