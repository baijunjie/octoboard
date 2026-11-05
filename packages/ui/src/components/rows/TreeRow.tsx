import React from "react";

import { useFocusVisibleProps } from "../focus/useFocusVisibleProps";
import { FadeOverflow } from "../text/FadeOverflow";

/** Stops a row's own mousedown from moving focus off whatever had it (typically the terminal) —
 * click still fires normally afterward. Shared by every row. */
function keepFocus(e: React.MouseEvent): void {
  e.preventDefault();
}

/** Enter and Space on a row, so a row that announces itself as a button can be operated as one.
 * Clicking deliberately does not focus the row (`keepFocus`), which is what keeps the terminal's
 * keyboard focus where it is; reaching a row by Tab still focuses it normally. */
function rowKeyHandler(activate: () => void) {
  return (event: React.KeyboardEvent): void => {
    // Only the row's own key presses. An action menu lives inside the row, and swallowing its
    // Enter would both block the menu and fire the row's action in its place.
    if (event.target !== event.currentTarget) return;
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    activate();
  };
}

/** Insets stay identical across flat animated lists and wrapped lists. Connectors overlap the
 * 2px row gap, and stop at the final child's centre instead of extending below the team. */
export const NESTED_RAIL_CLASS = "ms-[17px]";
export const NESTED_INSET_CLASS = "ms-[17px]";
export const NESTED_ROW_CLASS = `relative session-nested-row ${NESTED_INSET_CLASS}`;

export function NestedBranch({ last, ancestor = false }: { last: boolean; ancestor?: boolean }): React.ReactElement {
  return <span aria-hidden="true" className="session-branch" data-last={last || undefined} data-ancestor={ancestor || undefined} />;
}

/** One row of the sidebar: a `div` announcing itself as a button, since it carries an action menu
 * and a button cannot hold another. That is also why the tree is hand-built: HeroUI 3 has no Tree
 * or GridList, and its `Disclosure` trigger is itself a button, so it cannot contain the row's menu.
 * `expanded` is for a row that shows or hides the rows beneath it, and is announced as
 * `aria-expanded`. It is not a popup trigger: without `aria-haspopup` it does not match the
 * open-popup lookup in `usePaneToggles`, which keeps a floating sidebar open while a menu is.
 *
 * `className` is the row's layout and replaces the default entirely, so a caller sets every part of it.
 *
 * The row is a `group`: controls inside it that only matter on hover (`RowControls`) show while it
 * is hovered, holds focus, is selected, or has its menu open. */
export function TreeRow({
  ariaLabel,
  describedBy,
  onActivate,
  selected,
  locked,
  expanded,
  className = "min-h-8 items-center gap-(--row-control-gap) [--row-controls-gap:var(--row-control-gap)] px-(--row-content-inset)",
  children,
  ref,
}: {
  ariaLabel: string;
  describedBy?: string;
  onActivate: () => void;
  selected?: boolean;
  locked?: boolean;
  expanded?: boolean;
  className?: string;
  children: React.ReactNode;
  /** The row's element, so its action menu can open on a right-click anywhere on the row. */
  ref?: React.Ref<HTMLDivElement>;
}): React.ReactElement {
  const focusVisible = useFocusVisibleProps();
  return (
    <div
      ref={ref}
      role="button"
      tabIndex={0}
      aria-label={ariaLabel}
      aria-describedby={describedBy}
      aria-current={selected ? "true" : undefined}
      aria-expanded={expanded}
      data-selected={selected || undefined}
      data-locked={locked || undefined}
      {...focusVisible}
      className={`group flex cursor-pointer rounded-lg text-sm outline-none select-none transition-colors motion-reduce:transition-none hover:bg-panel-hover data-focus-visible:ring-2 data-focus-visible:ring-focus data-selected:bg-panel-selected ${className}`}
      onMouseDown={keepFocus}
      onClick={onActivate}
      onKeyDown={rowKeyHandler(onActivate)}
    >
      {children}
    </div>
  );
}

/** A row's single-line name: fades out at its end edge when it does not fit, rather than
 * ending in an ellipsis. `title` is the full text, offered as a tooltip only while it is cut. */
export const RowLabel = ({
  title,
  className = "min-w-0 flex-1",
  children,
}: {
  title?: string;
  className?: string;
  children: React.ReactNode;
}) => (
  <FadeOverflow as="span" dir="auto" className={className} titleWhenClipped={title}>
    {children}
  </FadeOverflow>
);
