import { Button } from "@heroui/react";
import type { LucideIcon } from "lucide-react";
import React from "react";

import { TitledControl } from "../components/TitledControl";
import { FadeOverflow } from "../components/FadeOverflow";

/** Stops a row's own mousedown from moving focus off whatever had it (typically the terminal) —
 * click still fires normally afterward. Shared by every row in the sidebar. */
export function keepFocus(e: React.MouseEvent): void {
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
  onActivate,
  selected,
  expanded,
  className = "min-h-8 items-center gap-2 px-2",
  children,
  ref,
}: {
  ariaLabel: string;
  onActivate: () => void;
  selected?: boolean;
  expanded?: boolean;
  className?: string;
  children: React.ReactNode;
  /** The row's element, so its action menu can open on a right-click anywhere on the row. */
  ref?: React.Ref<HTMLDivElement>;
}): React.ReactElement {
  return (
    <div
      ref={ref}
      role="button"
      tabIndex={0}
      aria-label={ariaLabel}
      aria-current={selected ? "true" : undefined}
      aria-expanded={expanded}
      data-selected={selected || undefined}
      data-marquee-scope
      className={`group flex cursor-pointer rounded-lg text-sm outline-none select-none transition-colors hover:bg-panel-hover focus-visible:ring-2 focus-visible:ring-focus data-selected:bg-panel-selected ${className}`}
      onMouseDown={keepFocus}
      onClick={onActivate}
      onKeyDown={rowKeyHandler(onActivate)}
    >
      {children}
    </div>
  );
}

/** The controls at a row's end that show only while the row is hovered, focused, selected or has
 * its menu open. Hidden ones take no width, so the name gets the room; they are clipped rather than
 * `display: none`, so they stay in the tab order and a keyboard user reaching one makes it visible.
 * They fade and grow in and out together: the width is a `max-w` that only has to exceed the
 * widest controls (two size-6 buttons and the gap, 50px), as `auto` cannot be transitioned; the
 * tighter the cap, the less of the transition is spent growing past the content. They stay clipped
 * while shown, the width still animating, except under keyboard focus, where a focus ring has to
 * paint outside the box. */
export function RowControls({ children, always }: { children: React.ReactNode; always?: boolean }): React.ReactElement {
  return (
    <div
      className={`flex shrink-0 items-center gap-0.5 transition-[opacity,max-width] duration-150 motion-reduce:transition-none ${
        always
          ? ""
          : "max-w-0 overflow-hidden opacity-0 group-hover:max-w-14 group-hover:opacity-100 group-focus-within:max-w-14 group-focus-within:overflow-visible group-focus-within:opacity-100 group-data-selected:max-w-14 group-data-selected:opacity-100 has-[[aria-expanded=true]]:max-w-14 has-[[aria-expanded=true]]:opacity-100"
      }`}
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

/** A small icon-only button inside a row (the "+" that opens a session), with its tooltip. Its click
 * is kept from reaching the row behind it, and a mouse press leaves focus where it was. */
export function RowIconButton({
  icon: Icon,
  label,
  onPress,
  isDisabled,
  iconClassName,
}: {
  icon: LucideIcon;
  label: string;
  onPress: () => void;
  isDisabled?: boolean;
  /** Extra classes on the glyph itself, such as a right-to-left mirror. */
  iconClassName?: string;
}): React.ReactElement {
  return (
    <div className="flex" onClick={(e) => e.stopPropagation()}>
      <TitledControl title={label}>
        <Button
          isIconOnly
          size="sm"
          variant="ghost"
          aria-label={label}
          preventFocusOnPress
          onPress={onPress}
          isDisabled={isDisabled}
          className="size-6 min-w-0 rounded-md text-muted hover:text-foreground"
        >
          <Icon aria-hidden="true" className={iconClassName ? `size-4 ${iconClassName}` : "size-4"} />
        </Button>
      </TitledControl>
    </div>
  );
}

/** A heading over a group of rows: what follows the label (a filter in force) sits right after it
 * and wraps onto further lines when it does not fit, and `action` at the row's end — several
 * controls there are spaced by the heading, so a caller hands them over as a fragment rather than
 * wrapping them itself. */
export function SectionHeading({
  children,
  after,
  action,
}: {
  children: React.ReactNode;
  after?: React.ReactNode;
  action?: React.ReactNode;
}): React.ReactElement {
  return (
    // Top-aligned, with the label and the controls each a line high, so they stay on the first line
    // when what follows the label wraps.
    <div className="mt-2 mb-0.5 flex items-start gap-2 px-2">
      <h3 className="flex h-7 shrink-0 items-center text-xs font-medium text-muted">{children}</h3>
      {/* What follows the label gives way first when the row is short; the controls never do. */}
      <div className="flex min-h-7 min-w-0 flex-1 flex-wrap items-center gap-1.5 py-1">{after}</div>
      {action && <div className="flex h-7 shrink-0 items-center gap-0.5">{action}</div>}
    </div>
  );
}
