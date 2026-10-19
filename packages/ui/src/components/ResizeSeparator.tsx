import React, { useEffect, useRef, useState } from "react";

import { AnchoredTooltip } from "./AnchoredTooltip";
import { useFocusVisibleProps } from "./useFocusVisibleProps";

/** Pixels an arrow key press moves the edge; Shift multiplies it. */
const KEY_STEP = 16;
const KEY_STEP_SHIFT = 64;

/** The two axes a separator can move along, named by the line it draws as ARIA does (so a vertical
 * line resizes width): a vertical line moves with the pointer's X, a horizontal one with its Y. */
const AXES = {
  vertical: { pointer: "clientX", increase: "ArrowRight", decrease: "ArrowLeft", cursor: "cursor-col-resize", dragCursor: "col-resize", line: "mx-auto h-full w-0.5 group-data-focus-visible:w-full", grip: "m-auto h-10 w-1" },
  horizontal: { pointer: "clientY", increase: "ArrowDown", decrease: "ArrowUp", cursor: "cursor-row-resize", dragCursor: "row-resize", line: "my-auto w-full h-0.5 group-data-focus-visible:h-full", grip: "m-auto h-1 w-10" },
} as const;

/** `grow` is the direction, along the reading direction for a vertical line and downward for a
 * horizontal one, that the edge moves in when the value gets larger. The pointer and the arrow keys
 * work in physical directions, so a vertical line turns it into +1 (right) or -1 (left) for the
 * direction in force; a horizontal one is not mirrored. */
function physicalGrow(element: HTMLElement, orientation: keyof typeof AXES, grow: 1 | -1): 1 | -1 {
  if (orientation === "horizontal") return grow;
  return getComputedStyle(element).direction === "rtl" ? (-grow as 1 | -1) : grow;
}

/**
 * The drag handle on the edge between two parts of the window that give each other room (a docked
 * pane and the terminal, two panels of the right pane's stack): a thin hit area straddling that
 * edge, with a highlight line on hover, focus and while dragging. Keyboard focus adds a ring around
 * the handle and widens the line to fill it: the line alone is a 2px stripe, smaller than SC 2.4.13
 * Focus Appearance (AAA) suggests (an area at least that of a 2px perimeter around the control).
 * It is a `separator` that is also a focusable value control: the arrow keys along its axis (Shift
 * for larger steps), Home and End adjust it from the keyboard, and a double-click resets it.
 *
 * A mouse press that focused the handle would take keystrokes away from the terminal, so mousedown
 * is cancelled; reaching the handle by Tab still focuses it. The drag itself uses pointer capture,
 * so the pointer keeps driving it over a report page's iframe, and while it lasts the root
 * carries `pane-resizing` (`style.css`) so the cursor stays a resize cursor over whatever the
 * pointer crosses. The value is committed once, when the drag ends.
 *
 * It is hand-built because HeroUI has no splitter: its `Separator` is static, and its `Slider` is a
 * value slider with a track and a thumb, not an edge between two parts.
 *
 * The caller positions it (`className`, `style`) and owns what the value means: a drag calls
 * `onChange` on every move with `commit: false`, then `onCommit` once; a key press calls it with
 * the default.
 */
export function ResizeSeparator({
  orientation,
  value,
  min,
  max,
  label,
  grow,
  onChange,
  onCommit,
  onReset,
  className = "",
  style,
  pane,
  region,
  appearance = "line",
  tooltip,
}: {
  orientation: keyof typeof AXES;
  value: number;
  min: number;
  max: number;
  label?: string;
  grow: 1 | -1;
  onChange: (value: number, options?: { commit?: boolean }) => void;
  onCommit: () => void;
  onReset: () => void;
  /** Layout classes: where the handle is and how large its hit area is. */
  className?: string;
  style?: React.CSSProperties;
  /** `data-pane`: hiding the pane while this holds focus hands focus to the terminal. */
  pane?: string;
  /** `data-region`: F6 from here moves on from the region, as from any control inside it. */
  region?: string;
  /** What it draws on hover, focus and drag: a `line` along the whole edge, or a `grip`, a short
   * rounded bar centred on the edge in the gap the handle sits in. */
  appearance?: "line" | "grip";
  /** What a tooltip on a grip says while a mouse pointer is on the handle, placed against the grip
   * itself rather than the handle's long box: beside it, toward the end, for a vertical grip, above
   * a horizontal one. It is no second name: the tooltip is not linked to
   * the separator, which keeps its `label`. */
  tooltip?: string;
}): React.ReactElement {
  const axis = AXES[orientation];
  const [dragging, setDragging] = useState(false);
  const focusVisible = useFocusVisibleProps();
  const gripRef = useRef<HTMLDivElement>(null);
  const [hovered, setHovered] = useState(false);
  const dragRef = useRef<{ start: number; startValue: number; grow: 1 | -1 } | undefined>(undefined);

  useEffect(() => {
    if (!dragging) return;
    const root = document.documentElement;
    root.style.setProperty("--pane-resize-cursor", axis.dragCursor);
    root.classList.add("pane-resizing");
    return () => {
      root.classList.remove("pane-resizing");
      root.style.removeProperty("--pane-resize-cursor");
    };
  }, [dragging, axis.dragCursor]);

  // The pointer capture ends with the press (release or cancel), which is what calls this.
  const endDrag = () => {
    if (!dragRef.current) return;
    dragRef.current = undefined;
    setDragging(false);
    onCommit();
  };

  const onKeyDown = (event: React.KeyboardEvent) => {
    const step = event.shiftKey ? KEY_STEP_SHIFT : KEY_STEP;
    const growKey = physicalGrow(event.currentTarget as HTMLElement, orientation, grow) > 0 ? axis.increase : axis.decrease;
    const shrinkKey = growKey === axis.increase ? axis.decrease : axis.increase;
    const next =
      event.key === growKey
        ? value + step
        : event.key === shrinkKey
          ? value - step
          : event.key === "Home"
            ? min
            : event.key === "End"
              ? max
              : undefined;
    if (next === undefined) return;
    event.preventDefault();
    onChange(next);
  };

  return (
    <div
      role="separator"
      aria-orientation={orientation}
      aria-label={label}
      aria-valuenow={value}
      aria-valuemin={min}
      aria-valuemax={max}
      tabIndex={0}
      data-pane={pane}
      data-region={region}
      {...focusVisible}
      style={style}
      className={`group touch-none outline-none ${appearance === "grip" ? "flex" : ""} data-focus-visible:ring-2 data-focus-visible:ring-app-focus ${axis.cursor} ${className}`}
      onPointerEnter={(event) => event.pointerType === "mouse" && setHovered(true)}
      onPointerLeave={() => setHovered(false)}
      onMouseDown={(event) => event.preventDefault()}
      onPointerDown={(event) => {
        if (event.button !== 0) return;
        event.currentTarget.setPointerCapture(event.pointerId);
        dragRef.current = { start: event[axis.pointer], startValue: value, grow: physicalGrow(event.currentTarget, orientation, grow) };
        setDragging(true);
      }}
      onPointerMove={(event) => {
        const drag = dragRef.current;
        if (drag) onChange(drag.startValue + drag.grow * (event[axis.pointer] - drag.start), { commit: false });
      }}
      onLostPointerCapture={endDrag}
      onDoubleClick={onReset}
      onKeyDown={onKeyDown}
    >
      {appearance === "grip" ? (
        <div ref={gripRef} className={`rounded-full transition-opacity motion-reduce:transition-none ${axis.grip} ${dragging ? "bg-(--grip-active) opacity-100" : "bg-(--grip-hover) opacity-0 group-hover:opacity-100 group-data-focus-visible:opacity-100"}`} />
      ) : (
        <div className={`transition-colors group-hover:bg-app-accent group-data-focus-visible:bg-app-accent ${axis.line} ${dragging ? "bg-app-accent" : ""}`} />
      )}
      {tooltip && appearance === "grip" && <AnchoredTooltip tooltip={tooltip} anchor={gripRef} hovered={hovered && !dragging} placement={orientation === "vertical" ? "end" : undefined} />}
    </div>
  );
}
