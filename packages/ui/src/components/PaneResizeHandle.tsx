import React, { useEffect, useRef, useState } from "react";

import type { PaneSide, PaneWidth } from "../layout/paneWidth";

/** Pixels an arrow key press moves the pane's edge; Shift multiplies it. */
const KEY_STEP = 16;
const KEY_STEP_SHIFT = 64;

/** What differs between the sides. `grow` is the direction the handle moves in when the pane gets
 * wider: right for the sidebar, whose edge is its right one, left for the report panel. `span` is
 * how far up it runs: the sidebar's edge continues up through the top bar's left segment, so its
 * handle starts at the window top, and while the connection banner shows it also crosses the
 * banner, which has no border there; the report panel's edge starts below the top chrome, and so
 * does its handle. */
const SIDES = {
  sidebar: {
    label: "Resize sidebar",
    edge: "left",
    span: "inset-y-0",
    grow: 1,
    growKey: "ArrowRight",
    shrinkKey: "ArrowLeft",
  },
  report: {
    label: "Resize report panel",
    edge: "right",
    span: "top-(--top-chrome-height) bottom-0",
    grow: -1,
    growKey: "ArrowLeft",
    shrinkKey: "ArrowRight",
  },
} as const;

/**
 * The drag handle on a docked pane's inner edge (the sidebar's right, the report panel's left): a
 * thin hit area straddling the border, with a highlight line on hover, focus and while dragging.
 * Keyboard focus adds a ring around the handle and widens the line to fill it: the line alone is a
 * 2px stripe that, in the light theme, is under the 3:1 against the border pixel it replaces that
 * WCAG 2.2 SC 1.4.11 asks of a focus indicator, and smaller than SC 2.4.13 Focus Appearance (AAA)
 * suggests (an area at least that of a 2px perimeter around the control). It runs as far up as its
 * pane's edge does (see `span` above), so the line covers that border, plus the connection banner's
 * height for the sidebar while the banner shows. It is rendered right after its pane, so Tab
 * reaches it next to the pane, but it is positioned against the window: the nearest positioned
 * ancestor must be the app root, as tall as the window, and no element between them may be
 * positioned. `App.tsx` renders it only while its pane is docked and shown. Hidden below the
 * `docked` breakpoint, where the panes are fixed-width drawers. In the macOS app the top bar is a
 * window drag region, but pressing the sidebar's handle where it crosses the bar resizes instead of
 * dragging the window: Tauri's drag script walks the event's composed path looking for
 * `data-tauri-drag-region`, and none of the handle's ancestors carries it, since the handle is not
 * inside the bar. It is a `separator` that is also a focusable value control: the arrow keys (Shift
 * for larger steps), Home and End adjust it from the keyboard, and a double-click resets it.
 *
 * A mouse press that focused the handle would take keystrokes away from the terminal, so mousedown
 * is cancelled; reaching the handle by Tab still focuses it. The drag itself uses pointer capture,
 * so the pointer keeps driving it over the report panel's iframe, and while it lasts the root
 * carries `pane-resizing` (`style.css`) so the cursor stays a resize cursor over whatever the
 * pointer crosses. The width is persisted once, when the drag ends.
 *
 * It is hand-built because HeroUI has no splitter: its `Separator` is static, and its `Slider` is a
 * value slider with a track and a thumb, not an edge between two panes.
 */
export function PaneResizeHandle({
  side,
  paneWidth,
}: {
  side: PaneSide;
  paneWidth: PaneWidth;
}): React.ReactElement {
  const { width, min, max, setWidth, persist, reset } = paneWidth;
  const { label, edge, span, grow, growKey, shrinkKey } = SIDES[side];
  const [dragging, setDragging] = useState(false);
  const dragRef = useRef<{ startX: number; startWidth: number } | undefined>(undefined);

  useEffect(() => {
    if (!dragging) return;
    document.documentElement.classList.add("pane-resizing");
    return () => document.documentElement.classList.remove("pane-resizing");
  }, [dragging]);

  // The pointer capture ends with the press (release or cancel), which is what calls this.
  const endDrag = () => {
    if (!dragRef.current) return;
    dragRef.current = undefined;
    setDragging(false);
    persist();
  };

  const onKeyDown = (event: React.KeyboardEvent) => {
    const step = event.shiftKey ? KEY_STEP_SHIFT : KEY_STEP;
    const next =
      event.key === growKey
        ? width + step
        : event.key === shrinkKey
          ? width - step
          : event.key === "Home"
            ? min
            : event.key === "End"
              ? max
              : undefined;
    if (next === undefined) return;
    event.preventDefault();
    setWidth(next);
  };

  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      aria-valuenow={width}
      aria-valuemin={min}
      aria-valuemax={max}
      tabIndex={0}
      // `data-pane`: hiding the pane while this holds focus hands focus to the terminal.
      data-pane={side}
      // `data-region`: F6 from here moves on from the pane, as from any control inside it.
      data-region={side}
      // Centred on the pane's inner edge, 3px each side.
      style={{ [edge]: width - 3 }}
      className={`group absolute ${span} z-10 hidden w-1.5 cursor-col-resize touch-none outline-none focus-visible:ring-2 focus-visible:ring-focus docked:block`}
      onMouseDown={(event) => event.preventDefault()}
      onPointerDown={(event) => {
        if (event.button !== 0) return;
        event.currentTarget.setPointerCapture(event.pointerId);
        dragRef.current = { startX: event.clientX, startWidth: width };
        setDragging(true);
      }}
      onPointerMove={(event) => {
        const drag = dragRef.current;
        if (drag) setWidth(drag.startWidth + grow * (event.clientX - drag.startX), { persist: false });
      }}
      onLostPointerCapture={endDrag}
      onDoubleClick={reset}
      onKeyDown={onKeyDown}
    >
      <div
        className={`mx-auto h-full w-0.5 transition-colors group-hover:bg-accent group-focus-visible:w-full group-focus-visible:bg-accent ${dragging ? "bg-accent" : ""}`}
      />
    </div>
  );
}
