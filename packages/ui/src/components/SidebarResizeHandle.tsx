import React, { useEffect, useRef, useState } from "react";

import { SIDEBAR_MIN_WIDTH, type SidebarWidth } from "../layout/sidebarWidth";

/** Pixels a Left/Right key press moves the sidebar edge; Shift multiplies it. */
const KEY_STEP = 16;
const KEY_STEP_SHIFT = 64;

/**
 * The drag handle on the docked sidebar's right edge: a thin hit area straddling the border, with
 * a highlight line on hover, focus and while dragging. Hidden below the `docked` breakpoint, where
 * the sidebar is a fixed-width drawer. It is a `separator` that is also a focusable value control:
 * Left/Right (Shift for larger steps), Home and End adjust it from the keyboard, and a double-click
 * resets it.
 *
 * A mouse press that focused the handle would take keystrokes away from the terminal, so mousedown
 * is cancelled; reaching the handle by Tab still focuses it. The drag itself uses pointer capture,
 * so the pointer keeps driving it over the report panel's iframe, and while it lasts the root
 * carries `sidebar-resizing` (`style.css`) so the cursor stays a resize cursor over whatever the
 * pointer crosses. The width is persisted once, when the drag ends. The parent must be `relative`
 * from the `docked` breakpoint up.
 */
export function SidebarResizeHandle({ sidebarWidth }: { sidebarWidth: SidebarWidth }): React.ReactElement {
  const { width, max, setWidth, persist, reset } = sidebarWidth;
  const [dragging, setDragging] = useState(false);
  const dragRef = useRef<{ startX: number; startWidth: number } | undefined>(undefined);

  useEffect(() => {
    if (!dragging) return;
    document.documentElement.classList.add("sidebar-resizing");
    return () => document.documentElement.classList.remove("sidebar-resizing");
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
      event.key === "ArrowLeft"
        ? width - step
        : event.key === "ArrowRight"
          ? width + step
          : event.key === "Home"
            ? SIDEBAR_MIN_WIDTH
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
      aria-label="Resize sidebar"
      aria-valuenow={width}
      aria-valuemin={SIDEBAR_MIN_WIDTH}
      aria-valuemax={max}
      tabIndex={0}
      className="group absolute inset-y-0 -right-[3px] z-10 hidden w-1.5 cursor-col-resize touch-none outline-none docked:block"
      onMouseDown={(event) => event.preventDefault()}
      onPointerDown={(event) => {
        if (event.button !== 0) return;
        event.currentTarget.setPointerCapture(event.pointerId);
        dragRef.current = { startX: event.clientX, startWidth: width };
        setDragging(true);
      }}
      onPointerMove={(event) => {
        const drag = dragRef.current;
        if (drag) setWidth(drag.startWidth + event.clientX - drag.startX, { persist: false });
      }}
      onLostPointerCapture={endDrag}
      onDoubleClick={reset}
      onKeyDown={onKeyDown}
    >
      <div
        className={`mx-auto h-full w-0.5 transition-colors group-hover:bg-accent group-focus-visible:bg-accent ${dragging ? "bg-accent" : ""}`}
      />
    </div>
  );
}
