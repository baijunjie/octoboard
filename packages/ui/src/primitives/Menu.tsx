import { Dropdown, Separator } from "@heroui/react";
import React from "react";
import { MenuContext, useSlottedContext } from "react-aria-components";

import { useDisabled } from "./busy";
import { type DataAttributes, definedProps } from "./props";

/** HeroUI's dropdown: a `MenuTrigger` and the `MenuPopover` it opens. The caller holds the open state. */
export function Menu({ open, onOpenChange, children }: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  children: React.ReactNode;
}): React.ReactElement {
  return <Dropdown isOpen={open} onOpenChange={onOpenChange}>{children}</Dropdown>;
}

/** HeroUI's dropdown trigger, the button that opens its `Menu`. Its name and description are `label` and `describedBy`,
 * as a `Button`'s; `focusOnPress={false}` leaves focus where it was (the menu takes it once it opens), and `hidden` keeps
 * it out of sight and out of the tab order, for a menu opened some other way (a context menu). */
export function MenuTrigger({ ref, label, describedBy, hidden, focusOnPress, className, children }: {
  ref?: React.Ref<HTMLButtonElement>;
  label?: string;
  describedBy?: string;
  hidden?: boolean;
  focusOnPress?: boolean;
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <Dropdown.Trigger
      ref={ref}
      {...definedProps({ "aria-label": label, "aria-describedby": describedBy, hidden, excludeFromTabOrder: hidden, isDisabled: useDisabled(), className })}
      preventFocusOnPress={focusOnPress === false || undefined}
    >
      {children}
    </Dropdown.Trigger>
  );
}

/** HeroUI's dropdown popover, holding a `MenuList` or a panel of its own (`useSubmenuPanel`): under its trigger, or with
 * `at` at that point of the window (a context menu at the pointer).
 *
 * react-aria names the popover's dialog role after the trigger, as it does the menu inside it, so a screen reader would
 * speak the name twice. The dialog goes without one, a deliberate departure from the ARIA rule that a dialog has a
 * name: it only wraps the named menu, so a name would just repeat it. The empty value wins over react-aria's and
 * refers to nothing. */
export function MenuPopover({ at, className, children }: {
  at?: { x: number; y: number };
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <Dropdown.Popover
      aria-labelledby=""
      {...definedProps({ className, getTargetRect: at && (() => new DOMRect(at.x, at.y, 0, 0)) })}
    >
      {children as React.ReactElement}
    </Dropdown.Popover>
  );
}

// A disabled item stays in the arrow-key walk (react-aria's default skips it), so a reason under it (a quota used up)
// can be reached and read from the keyboard while the item still refuses to run. react-stately reads
// `disabledBehavior` from the menu's tree state and from a single-selection section's own selection state, but neither
// one's props declare it, hence the casts.
const KEEP_DISABLED = { disabledBehavior: "selection" } as object;

/** HeroUI's dropdown menu, holding `MenuItem`s, `MenuChoices`, `MenuSeparator`s and `Submenu`s. `onAction` is told the
 * id of the item chosen, in this menu or a submenu below it; `autoFocusFirst` focuses its first item as it opens. A
 * disabled item stays reachable by the arrow keys and announced, though it cannot be chosen. */
export function MenuList({ onAction, autoFocusFirst, children }: {
  onAction: (id: string) => void;
  autoFocusFirst?: boolean;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <Dropdown.Menu onAction={(key) => onAction(String(key))} {...(autoFocusFirst && { autoFocus: "first" as const })} {...KEEP_DISABLED}>
      {children as React.ReactElement}
    </Dropdown.Menu>
  );
}

/** HeroUI's dropdown item: `id` is what `onAction` is told, `textValue` what typing to it matches, `label` its accessible
 * name where its content does not give the right one. `ref` is the item's element. `danger` draws a destructive item;
 * `closeOnSelect={false}` keeps the menu open after it runs. */
export function MenuItem({ ref, id, textValue, label, danger, disabled, closeOnSelect, className, children, ...data }: DataAttributes & {
  ref?: React.Ref<HTMLDivElement>;
  id: string;
  textValue: string;
  label?: string;
  danger?: boolean;
  disabled?: boolean;
  closeOnSelect?: boolean;
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <Dropdown.Item
      {...data}
      ref={ref}
      id={id}
      textValue={textValue}
      {...definedProps({ "aria-label": label, className, isDisabled: disabled, shouldCloseOnSelect: closeOnSelect })}
      variant={danger ? "danger" : "default"}
    >
      {children as React.ReactElement}
    </Dropdown.Item>
  );
}

/** HeroUI's check mark of a `MenuItem` in `MenuChoices`, shown while it is the chosen one. */
export function MenuItemCheck(): React.ReactElement {
  return <Dropdown.ItemIndicator />;
}

/** A run of `MenuItem`s of which one is chosen (`selected`, by id): HeroUI's dropdown section with single selection,
 * unnamed, as a name could only repeat the menu's. The menu's other items stay plain items rather than radios. */
export function MenuChoices({ selected, children }: { selected: string[]; children: React.ReactNode }): React.ReactElement {
  return (
    <Dropdown.Section selectionMode="single" selectedKeys={selected} {...KEEP_DISABLED}>
      {children as React.ReactElement}
    </Dropdown.Section>
  );
}

/** HeroUI's separator between two runs of a menu's items. */
export function MenuSeparator(): React.ReactElement {
  return <Separator />;
}

/** HeroUI's submenu trigger: `children` are the `MenuItem` that opens it and the `MenuPopover` it opens. */
export function Submenu({ children }: { children: React.ReactNode }): React.ReactElement {
  return <Dropdown.SubmenuTrigger>{children as React.ReactElement[]}</Dropdown.SubmenuTrigger>;
}

/** HeroUI's chevron at the end of the item that opens a `Submenu`. */
export function SubmenuIndicator({ className }: { className?: string }): React.ReactElement {
  return <Dropdown.SubmenuIndicator {...definedProps({ className })} />;
}

/** What a submenu's popover hands the panel in it when that holds content of its own rather than a `MenuList`: the
 * props react-aria gives a submenu's menu, which a panel is not, that make it part of the menu — the `ref` that keeps
 * the pointer's way to it open, its `id`, the `labelledBy` naming it after its trigger, whether it is to take focus as
 * it opens, the keys that leave it, and `close`, which closes the whole menu. Read from react-aria's menu context: what
 * to check again when react-aria is upgraded. */
export interface SubmenuPanel {
  ref?: React.Ref<HTMLDivElement>;
  id?: string;
  labelledBy?: string;
  autoFocus: boolean;
  close: () => void;
  onKeyDown?: React.KeyboardEventHandler;
  onKeyUp?: React.KeyboardEventHandler;
}

export function useSubmenuPanel(): SubmenuPanel {
  const context = useSlottedContext(MenuContext);
  const { ref, id, "aria-labelledby": labelledBy, autoFocus, onClose } = context ?? {};
  // The two key handlers are passed along but not part of `MenuProps`.
  const { onKeyDown, onKeyUp } = (context ?? {}) as { onKeyDown?: React.KeyboardEventHandler; onKeyUp?: React.KeyboardEventHandler };
  return {
    ref: ref as React.Ref<HTMLDivElement> | undefined,
    id,
    labelledBy,
    autoFocus: Boolean(autoFocus),
    close: onClose ?? (() => {}),
    onKeyDown,
    onKeyUp,
  };
}
