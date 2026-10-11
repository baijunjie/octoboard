import { Button } from "@heroui/react";
import { Pin, PinOff, type LucideIcon } from "lucide-react";
import React, { type RefObject, useRef } from "react";

import { handFocusOff } from "../components/handFocusOff";
import { TitledControl } from "../components/TitledControl";
import { useFocusVisibleProps } from "../components/useFocusVisibleProps";
import { FadeOverflow } from "../components/FadeOverflow";
import { useT } from "../i18n/react";

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

/** What a nested level of a session list looks like, written once for every list that draws one — the
 * sidebar's project list, a focus mode's archived rows and its cards, the archive view — so a row is
 * inset the same wherever it is listed.
 *
 * Both levels are laid out to one set of numbers: the line 12px in and 1px wide, and whatever hangs
 * under it 4px past the line, 17px in. `NESTED_RAIL_CLASS` draws a level as a border on the element
 * wrapping the rows; `NESTED_ROW_CLASS` draws it on a nested row itself, which is all a list keeping
 * its rows in one flat list has to put it on; `NESTED_INSET_CLASS` is the inset alone, for a list
 * that wants the nesting without the line. The figures are literal pixels rather than spacing steps,
 * so that the two levels stay built to the same numbers, and every edge of them lands on a whole
 * pixel at the root font size the app ships with.
 *
 * `NESTED_ROW_CLASS` insets with a margin rather than a padding, since where a list has no element
 * to wrap its rows in the class goes on the row itself, and a padding there would land inside the
 * row's own fill instead of moving it. Its line overflows the row by 1px at either end — half the
 * 2px every list carrying it leaves between its rows (`gap-0.5`), so the line runs on between two
 * sessions of one team rather than breaking at every gap. A list that spaces its rows further apart
 * than that (a focus mode's cards) leaves the line off and insets alone. */
export const NESTED_RAIL_CLASS = "ms-[12px] border-s border-separator ps-[4px]";
export const NESTED_INSET_CLASS = "ms-[17px]";
export const NESTED_ROW_CLASS = `relative ${NESTED_INSET_CLASS} before:absolute before:-inset-y-px before:-start-[5px] before:w-px before:bg-separator`;

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
  const focusVisible = useFocusVisibleProps();
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
      {...focusVisible}
      className={`group flex cursor-pointer rounded-lg text-sm outline-none select-none transition-colors hover:bg-panel-hover data-focus-visible:ring-2 data-focus-visible:ring-focus data-selected:bg-panel-selected ${className}`}
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

/** A small icon-only button inside a row (the "+" that opens a session), with its tooltip. Its click
 * is kept from reaching the row behind it, and a mouse press leaves focus where it was. */
export function RowIconButton({
  icon: Icon,
  label,
  onPress,
  isDisabled,
  iconClassName,
  ref,
}: {
  icon: LucideIcon;
  label: string;
  onPress: () => void;
  isDisabled?: boolean;
  /** Extra classes on the glyph itself, such as a right-to-left mirror. */
  iconClassName?: string;
  /** The element around the button, for focus to be handed to. */
  ref?: React.Ref<HTMLDivElement>;
}): React.ReactElement {
  return (
    <div ref={ref} className="flex" onClick={(e) => e.stopPropagation()}>
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

/** The pin mark of a pinned project or session, which is also the way to unpin it: a pin glyph that
 * becomes the unpin one while the button is hovered or holds keyboard focus. Placed like
 * `RowIconButton`, so pressing it neither selects the row behind it nor moves focus. `name` is
 * what it unpins, for the accessible name, as the tooltip is the bare verb. Unpinning takes the
 * button away, and keyboard focus with it, so `returnFocusTo` is the element that takes the focus
 * over first when the button holds it: the enclosing row, or an element holding the first button
 * that stays (a header, which is no tab stop itself). */
export function PinButton({
  name,
  onUnpin,
  returnFocusTo,
  className = "",
}: {
  name: string;
  onUnpin: () => void;
  returnFocusTo: RefObject<HTMLElement | null>;
  className?: string;
}): React.ReactElement {
  const t = useT();
  const holder = useRef<HTMLDivElement>(null);
  const unpin = () => {
    handFocusOff(holder.current, returnFocusTo.current, true);
    onUnpin();
  };
  return (
    <div ref={holder} className={`flex shrink-0 ${className}`} onClick={(e) => e.stopPropagation()}>
      <TitledControl title={t("sidebar.unpin")}>
        <Button
          isIconOnly
          size="sm"
          variant="ghost"
          aria-label={t("sidebar.unpinItem", { name })}
          preventFocusOnPress
          onPress={unpin}
          className="group/pin size-6 min-w-0 rounded-md text-muted hover:text-foreground"
        >
          <Pin aria-hidden="true" className="size-3 group-hover/pin:hidden group-data-[focus-visible=true]/pin:hidden" />
          <PinOff aria-hidden="true" className="hidden size-3 group-hover/pin:block group-data-[focus-visible=true]/pin:block" />
        </Button>
      </TitledControl>
    </div>
  );
}

/** A heading over a group of rows: the label with `action` at the row's end, and under them, when
 * given, `below` (a filter in force) across the sidebar's whole width, wrapping onto further lines
 * when it does not fit, with `belowAction` at that row's end. Several controls in either action slot
 * are spaced by the heading, so a caller hands them over as a fragment rather than wrapping them
 * itself. */
export function SectionHeading({
  children,
  action,
  below,
  belowAction,
}: {
  children: React.ReactNode;
  action?: React.ReactNode;
  below?: React.ReactNode;
  belowAction?: React.ReactNode;
}): React.ReactElement {
  return (
    <div className="mt-2 mb-0.5 px-2">
      <div className="flex items-center gap-2">
        <h3 className="flex h-7 min-w-0 flex-1 items-center text-xs font-medium text-muted">{children}</h3>
        {action && <div className="flex h-7 shrink-0 items-center gap-0.5">{action}</div>}
      </div>
      {below && (
        // Top-aligned, the action a line high, so it stays on the first line when the row wraps.
        <div className="flex items-start gap-2">
          <div className="flex min-h-7 min-w-0 flex-1 flex-wrap items-center gap-1.5 py-1">{below}</div>
          {belowAction && <div className="flex h-7 shrink-0 items-center gap-0.5">{belowAction}</div>}
        </div>
      )}
    </div>
  );
}
