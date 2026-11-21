import { Dropdown, Separator } from "@heroui/react";
import { dropdownVariants } from "@heroui/styles";
import React, { useEffect, useId, useRef } from "react";
import { Header, MenuContext, useSlottedContext } from "react-aria-components";

import { useDisabled } from "./busy";
import { setKeyboardModality } from "./focus";
import { FILTER_SCROLL_PADDING_CLASS, FILTERED_ITEM_CLASS, UnfilteredMenu, useMenuFiltered } from "./MenuFilter";
import { type DataAttributes, definedProps } from "./props";
import { mergeRefs } from "./refs";

/** The class names given, space-separated, or `undefined` where there is none. */
function joinClasses(...names: (string | undefined)[]): string | undefined {
  return names.filter(Boolean).join(" ") || undefined;
}

/** The classes of HeroUI's dropdown menu, which a `MenuList` outside a `Dropdown` does not get from context: the flex
 * stack with the gap between items and separators that a dropdown's menus show. */
export const menuListClassName = dropdownVariants().menu();

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
 * refers to nothing.
 *
 * `filtered` is for a popover that holds a `MenuFilter`: the field's row sticks to the top of the popover, which is the
 * scroller, so the popover keeps the row's height (and a few pixels) clear at its top for the highlighted item the
 * arrow keys scroll to; the usual few pixels would leave it under the row (see the scroll padding rule under "Focus" in
 * `docs/conventions/ui-accessibility.md`). Its content is an unfiltered menu unless it holds a `MenuFilter` itself, so
 * a submenu's popover is not narrowed by the filter of the menu it opens from. */
export function MenuPopover({ at, filtered, className, children }: {
  at?: { x: number; y: number };
  filtered?: boolean;
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  const classes = joinClasses(filtered ? FILTER_SCROLL_PADDING_CLASS : undefined, className);
  return (
    <Dropdown.Popover
      aria-labelledby=""
      {...definedProps({ className: classes, getTargetRect: at && (() => new DOMRect(at.x, at.y, 0, 0)) })}
    >
      <UnfilteredMenu>{children}</UnfilteredMenu>
    </Dropdown.Popover>
  );
}

// A disabled item stays in the arrow-key walk (react-aria's default skips it), so a reason under it (a quota used up)
// can be reached and read from the keyboard while the item still refuses to run. react-stately reads
// `disabledBehavior` from the menu's tree state and from a single-selection section's own selection state, but neither
// one's props declare it, hence the casts.
const KEEP_DISABLED = { disabledBehavior: "selection" } as object;

/** HeroUI's dropdown menu, holding `MenuItem`s, `MenuChoices`, `MenuSeparator`s and `Submenu`s. `onAction` is told the
 * id of the item chosen, in this menu or a submenu below it. `autoFocus` is what takes focus as it opens, its first item
 * (the first that can be chosen) or the menu itself; left out, that follows how its trigger opened it. A disabled item
 * stays reachable by the arrow keys and announced, though it cannot be chosen.
 *
 * react-aria keeps the trigger's choice from its last opening until the next one through the trigger, so a menu opened
 * some other way (a context menu) gives `autoFocus` for each opening. A menu drawn in the page, outside any popover
 * (the notification-area flyout), has no trigger to name it, so it is given a `label`. A menu drawn outside a
 * `MenuPopover` has no dropdown around it either, so HeroUI's menu class, the stack and its gap, is given as
 * `className` (`menuListClassName`). */
export function MenuList({ label, onAction, autoFocus, className, children }: {
  label?: string;
  onAction: (id: string) => void;
  autoFocus?: "first" | "menu";
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  const root = useRef<HTMLDivElement>(null);
  // A disabled item stays in the arrow-key walk (`KEEP_DISABLED`), so react-aria's "first" can be one that cannot be
  // chosen (a line saying there is nothing): focus goes to the first item that can.
  useEffect(() => {
    if (autoFocus !== "first") return;
    const frame = requestAnimationFrame(() => {
      const items = [...(root.current?.querySelectorAll<HTMLElement>('[role="menuitem"], [role="menuitemradio"], [role="menuitemcheckbox"]') ?? [])];
      const focused = items.find((item) => item === document.activeElement);
      if (!focused || focused.getAttribute("aria-disabled") !== "true") return;
      items.find((item) => item.getAttribute("aria-disabled") !== "true")?.focus();
    });
    return () => cancelAnimationFrame(frame);
  }, [autoFocus]);
  return (
    <div ref={root} className="contents">
      <Dropdown.Menu onAction={(key) => onAction(String(key))} {...definedProps({ "aria-label": label, className, autoFocus: autoFocus && (autoFocus === "menu" ? true : "first") })} {...KEEP_DISABLED}>
        {children as React.ReactElement}
      </Dropdown.Menu>
    </div>
  );
}

/** HeroUI's dropdown item: `id` is what `onAction` is told, `textValue` what typing to it matches, `label` its
 * accessible name where its content does not give the right one. `ref` is the item's element. `shortcut` is the key
 * that runs the item, announced as its `aria-keyshortcuts` (the caller handles the key, and runs the item with
 * `activateMenuItem`); in a filtered menu no key runs an item, so none is announced. `expanded` marks an item that
 * opens its content in place (`aria-expanded`, which react-aria filters out too), open or closed; left out, the item
 * says nothing of it. `danger` draws a destructive item; `closeOnSelect={false}` keeps the menu open after it runs. */
export function MenuItem({ ref, id, textValue, label, shortcut, expanded, danger, disabled, closeOnSelect, className, children, ...data }: DataAttributes & {
  ref?: React.Ref<HTMLDivElement>;
  id: string;
  textValue: string;
  label?: string;
  shortcut?: string;
  expanded?: boolean;
  danger?: boolean;
  disabled?: boolean;
  closeOnSelect?: boolean;
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  const filtered = useMenuFiltered();
  const descriptionId = useId();
  const classes = joinClasses(filtered ? FILTERED_ITEM_CLASS : undefined, className);
  // react-aria filters `aria-keyshortcuts` and `aria-expanded` out of a menu item's props, so they are set on the
  // element.
  const patchItemAria = (node: HTMLDivElement | null) => {
    if (!node) return;
    linkDescription(node, descriptionId);
    if (shortcut && !filtered) node.setAttribute("aria-keyshortcuts", shortcut);
    else node.removeAttribute("aria-keyshortcuts");
    if (expanded === undefined) node.removeAttribute("aria-expanded");
    else node.setAttribute("aria-expanded", String(expanded));
  };
  return (
    <Dropdown.Item
      {...data}
      ref={mergeRefs(ref, patchItemAria)}
      id={id}
      textValue={textValue}
      {...definedProps({ "aria-label": label, className: classes, isDisabled: disabled, shouldCloseOnSelect: closeOnSelect })}
      variant={danger ? "danger" : "default"}
    >
      {children as React.ReactElement}
    </Dropdown.Item>
  );
}

/** Links a menu item to its description (`slot="description"`) when react-aria has not: it settles the link when the item
 * mounts, so a description that arrives later (a note that comes with a probe's answer) is drawn but never described. The
 * description keeps the id it has, else takes one from the item's, and joins the ids the item already names. */
function linkDescription(node: HTMLElement, fallbackId: string): void {
  const description = node.querySelector<HTMLElement>('[data-slot="description"]');
  if (!description) return;
  const named = node.getAttribute("aria-describedby")?.split(" ").filter(Boolean) ?? [];
  if (named.some((id) => document.getElementById(id) === description)) return;
  description.id ||= fallbackId;
  node.setAttribute("aria-describedby", [...named.filter((id) => document.getElementById(id)), description.id].join(" "));
}

/** Runs `item`, the element of a `MenuItem`, as its own keyboard activation does, for a key the caller handles itself:
 * the item takes focus, with its ring, and gets Enter, so that the choice, a submenu's opening and the menu's closing
 * (or staying open) stay react-aria's. */
export function activateMenuItem(item: HTMLElement): void {
  setKeyboardModality();
  item.focus();
  item.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", code: "Enter", bubbles: true, cancelable: true }));
  item.dispatchEvent(new KeyboardEvent("keyup", { key: "Enter", code: "Enter", bubbles: true, cancelable: true }));
}

/** The open menu of the `MenuTrigger` whose button is `trigger`, or `null` while it is closed: react-aria labels a menu
 * by the button that opened it. */
export function menuOfTrigger(trigger: Element | null | undefined): Element | null {
  return trigger?.id ? document.querySelector(`[role=menu][aria-labelledby="${CSS.escape(trigger.id)}"]`) : null;
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

/** HeroUI's dropdown section: a run of items under `heading` (what names the group for assistive technology, and is
 * drawn as given: a label with a picture beside it). The heading is not an item and takes no focus. */
export function MenuSection({ heading, headingClassName, children }: {
  heading: React.ReactNode;
  headingClassName?: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <Dropdown.Section>
      <Header {...definedProps({ className: headingClassName })}>{heading}</Header>
      {children as React.ReactElement}
    </Dropdown.Section>
  );
}

/** HeroUI's separator between two runs of a menu's items; `className` sets it in from the row's edge. */
export function MenuSeparator({ className }: { className?: string }): React.ReactElement {
  return <Separator {...definedProps({ className })} />;
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
