import { Lock, LockOpen, Pin, PinOff, type LucideIcon } from "lucide-react";
import React, { type RefObject, useId, useRef } from "react";
import type { PressEvent } from "react-aria-components";

import { Button } from "../components/Button";
import { ROW_ICON_BUTTON_BASE_CLASS, ROW_ICON_BUTTON_CLASS, RowIconGroup } from "../components/RowControls";
import { handFocusOff } from "../components/handFocusOff";
import { TitledControl } from "../components/TitledControl";
import { useFocusVisibleProps } from "../components/useFocusVisibleProps";
import { FadeOverflow } from "../components/FadeOverflow";
import { useStickyHeadingFocus } from "../components/stickyHeadingFocus";
import { isolate } from "../i18n/isolate";
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
  className = "min-h-8 items-center gap-2 [--row-controls-gap:8px] px-(--row-content-inset)",
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
      className={`group flex cursor-pointer rounded-lg text-sm outline-none select-none transition-colors hover:bg-panel-hover data-focus-visible:ring-2 data-focus-visible:ring-app-focus data-selected:bg-panel-selected ${className}`}
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
  title,
  ariaLabel = label,
  onPress,
  isDisabled,
  unavailable,
  iconClassName,
  ref,
}: {
  icon: LucideIcon;
  label: string;
  /** Rich visible tooltip; `label` remains the plain action name. */
  title?: React.ReactNode;
  /** The accessible name, when it says more than the tooltip's bare verb (whose row it acts on). */
  ariaLabel?: string;
  onPress: (event: PressEvent) => void;
  isDisabled?: boolean;
  /** Why the action cannot be done now, for a button that looks disabled but stays focusable and under
   * the pointer so that the reason is reachable: it is the tooltip and the button's description, while
   * the name stays the action. HeroUI's disabled style turns pointer events off for `aria-disabled`, so
   * they are turned back on, and the hover tint and press shrink are left off. A press still reaches
   * `onPress`, which answers it. */
  unavailable?: string;
  /** Extra classes on the glyph itself, such as a right-to-left mirror. */
  iconClassName?: string;
  /** The element around the button, for focus to be handed to. */
  ref?: React.Ref<HTMLDivElement>;
}): React.ReactElement {
  const reasonId = useId();
  return (
    <div ref={ref} className="flex" onClick={(e) => e.stopPropagation()}>
      <TitledControl title={unavailable ?? title ?? label}>
        <Button
          isIconOnly
          size="xs"
          variant="ghost"
          aria-label={ariaLabel}
          aria-describedby={unavailable ? reasonId : title && ariaLabel === label ? "" : undefined}
          aria-disabled={unavailable ? true : undefined}
          preventFocusOnPress
          onPress={onPress}
          isDisabled={isDisabled}
          className={unavailable ? `${ROW_ICON_BUTTON_BASE_CLASS} pointer-events-auto data-[pressed=true]:[transform:none] active:[transform:none]` : ROW_ICON_BUTTON_CLASS}
        >
          {unavailable && <span id={reasonId} className="sr-only">{unavailable}</span>}
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
}: {
  name: string;
  onUnpin: () => void;
  returnFocusTo: RefObject<HTMLElement | null>;
}): React.ReactElement {
  const t = useT();
  const holder = useRef<HTMLDivElement>(null);
  const unpin = () => {
    handFocusOff(holder.current, returnFocusTo.current, true);
    onUnpin();
  };
  return (
    <div ref={holder} className="flex shrink-0" onClick={(e) => e.stopPropagation()}>
      <TitledControl title={t("sidebar.unpin")}>
        <Button
          isIconOnly
          size="xs"
          variant="ghost"
          aria-label={t("sidebar.unpinItem", { name: isolate(name) })}
          preventFocusOnPress
          onPress={unpin}
          className="group/pin shrink-0 rounded-md text-app-muted hover:text-foreground"
        >
          <Pin aria-hidden="true" className="size-4 group-hover/pin:hidden group-data-[focus-visible=true]/pin:hidden" />
          <PinOff aria-hidden="true" className="hidden size-4 group-hover/pin:block group-data-[focus-visible=true]/pin:block" />
        </Button>
      </TitledControl>
    </div>
  );
}

export function LockButton({ name, onUnlock, returnFocusTo }: {
  name: string;
  onUnlock: () => void;
  returnFocusTo: RefObject<HTMLElement | null>;
}): React.ReactElement {
  const t = useT();
  const holder = useRef<HTMLDivElement>(null);
  return (
    <div ref={holder} className="flex shrink-0" onClick={(e) => e.stopPropagation()}>
      <TitledControl title={t("sidebar.session.unlock")}>
        <Button
          isIconOnly
          size="xs"
          variant="ghost"
          aria-label={t("sidebar.session.unlockItem", { name: isolate(name) })}
          preventFocusOnPress
          onPress={() => {
            handFocusOff(holder.current, returnFocusTo.current, true);
            onUnlock();
          }}
          className="group/lock shrink-0 rounded-md text-warning-glyph hover:text-foreground"
        >
          <Lock aria-hidden="true" className="size-4 group-hover/lock:hidden group-data-[focus-visible=true]/lock:hidden" />
          <LockOpen aria-hidden="true" className="hidden size-4 group-hover/lock:block group-data-[focus-visible=true]/lock:block" />
        </Button>
      </TitledControl>
    </div>
  );
}

/** A heading over a group of rows: the label with `action` at the row's end, and between them, when
 * given, `inline` (a filter in force) on the same line, taking the room the two leave and sizing and
 * clipping itself within it. Several controls in the action slot are spaced by the heading, so a
 * caller hands them over as a fragment rather than wrapping them itself. */
export function SectionHeading({
  children,
  action,
  inline,
}: {
  children: React.ReactNode;
  action?: React.ReactNode;
  inline?: React.ReactNode;
}): React.ReactElement {
  return (
    <div className="mt-2 mb-0.5 px-(--row-content-inset)">
      <div className="flex items-center gap-2">
        <h3 className={`flex h-7 items-center text-xs font-medium text-app-muted ${inline ? "shrink-0" : "min-w-0 flex-1"}`}>{children}</h3>
        {inline && <div className="flex h-7 min-w-0 flex-1 items-center">{inline}</div>}
        {action && <RowIconGroup className="h-7">{action}</RowIconGroup>}
      </div>
    </div>
  );
}

export const STICKY_HEADING_HEIGHT = 38;

/** Each heading stays inside its section so the next section pushes it out of the way. */
export function StickySectionHeading(props: React.ComponentProps<typeof SectionHeading>): React.ReactElement {
  const pinnedFocus = useStickyHeadingFocus(STICKY_HEADING_HEIGHT);
  return (
    <div {...pinnedFocus} className={`sticky top-0 z-3 flow-root bg-panel-sidebar ${pinnedFocus.className}`}>
      <SectionHeading {...props} />
    </div>
  );
}
