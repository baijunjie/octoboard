import { Dropdown, Label, Separator } from "@heroui/react";
import { EllipsisVertical, type LucideIcon } from "lucide-react";
import React, { useEffect, useRef, useState } from "react";

import { useT } from "../i18n/react";
import { TitledControl } from "./TitledControl";
import { usePointerFocusReturn } from "./usePointerFocusReturn";

/** Set when a menu closed but could not hand focus back, because the item it ran opened a dialog
 * that holds it. The dialog takes it when it closes (see `takeMenuFocusToRestore`). */
let menuFocusToRestore: HTMLElement | null = null;

/** The element a menu could not give focus back to while a dialog was open, if any; the caller
 * returns focus to it once the dialog is gone. Each element is handed out once. */
export function takeMenuFocusToRestore(): HTMLElement | null {
  const element = menuFocusToRestore;
  menuFocusToRestore = null;
  return element;
}

/** How many menus are open right now; see `isActionMenuOpen`. */
let openMenus = 0;

/** The element a menu that just closed will give focus back to, until it does; a menu opened by a
 * right-click in the meantime takes it over, so focus is returned once, when that one closes. */
let handedFocus: HTMLElement | null = null;

/** Whether any action menu's popover is open, which holds keyboard focus like a modal does. */
export function isActionMenuOpen(): boolean {
  return openMenus > 0;
}

export interface ActionMenuItem {
  label: string;
  /** What the item shows in place of `label` (a path that has to read left to right); `label` stays
   * its text value for typeahead. */
  content?: React.ReactNode;
  /** A lucide glyph, or any element of about the same size (an agent's mark, a console's avatar). */
  icon: LucideIcon | React.ReactElement;
  onClick: () => void;
  destructive?: boolean;
  disabled?: boolean;
  /** The accessible name when it should say more than `label` (a console and what is going on in it). */
  ariaLabel?: string;
  /** Drawn after the label, at the item's end edge (a status marker). */
  end?: React.ReactNode;
  /** Makes the item one of a single-selection group (consecutive items that set it): HeroUI marks
   * the chosen one (`true`) with its check and announces it as checked. */
  selected?: boolean;
}

/** An item that opens a nested menu of its own items. */
export interface ActionMenuSubmenu {
  label: string;
  icon: LucideIcon | React.ReactElement;
  items: (ActionMenuItem | "separator")[];
}

export type ActionMenuEntry = ActionMenuItem | ActionMenuSubmenu | "separator";

/** A lucide glyph is drawn quieter than the label beside it, or in the danger colour with a
 * destructive item's label; an element (an agent's mark) keeps its own colours. */
function ItemIcon({ icon, destructive }: { icon: LucideIcon | React.ReactElement; destructive?: boolean }): React.ReactElement {
  if (React.isValidElement(icon)) return icon;
  const Icon = icon;
  return <Icon aria-hidden="true" className={`size-4 shrink-0 ${destructive ? "text-danger" : "text-muted"}`} />;
}

function isChoice(entry: ActionMenuEntry): entry is ActionMenuItem {
  return entry !== "separator" && !("items" in entry) && entry.selected !== undefined;
}

function MenuItems({ entries }: { entries: ActionMenuEntry[] }): React.ReactElement {
  // Items are keyed by position: two entries may share a label (two sessions of one title).
  const run = (key: React.Key) => {
    const entry = entries[Number(key)];
    if (entry && entry !== "separator" && !("items" in entry)) entry.onClick();
  };

  const renderItem = (entry: ActionMenuItem, index: number) => (
    <Dropdown.Item
      key={index}
      id={String(index)}
      textValue={entry.label}
      aria-label={entry.ariaLabel}
      variant={entry.destructive ? "danger" : "default"}
      isDisabled={entry.disabled}
    >
      <ItemIcon icon={entry.icon} destructive={entry.destructive} />
      {/* A label is often a name the user typed, so it reads in its own direction; `content` sets
          its own. */}
      <Label dir={entry.content ? undefined : "auto"} className="min-w-0 flex-1 truncate">{entry.content ?? entry.label}</Label>
      {entry.end}
      {entry.selected !== undefined && <Dropdown.ItemIndicator />}
    </Dropdown.Item>
  );

  // A run of consecutive `selected` items is a section of its own with single selection, so the
  // menu's other items stay plain menu items rather than becoming radios.
  const nodes: React.ReactNode[] = [];
  for (let index = 0; index < entries.length; index++) {
    const entry = entries[index];
    if (isChoice(entry)) {
      const group: number[] = [];
      while (index < entries.length && isChoice(entries[index])) group.push(index++);
      index--;
      nodes.push(
        // Left unnamed: a name here could only repeat the menu's.
        <Dropdown.Section
          key={group[0]}
          selectionMode="single"
          selectedKeys={group.filter((i) => (entries[i] as ActionMenuItem).selected).map(String)}
        >
          {group.map((i) => renderItem(entries[i] as ActionMenuItem, i))}
        </Dropdown.Section>,
      );
    } else if (entry === "separator") {
      nodes.push(<Separator key={index} />);
    } else if ("items" in entry) {
      nodes.push(
        <Dropdown.SubmenuTrigger key={index}>
          <Dropdown.Item id={String(index)} textValue={entry.label}>
            <ItemIcon icon={entry.icon} />
            <Label>{entry.label}</Label>
            <Dropdown.SubmenuIndicator />
          </Dropdown.Item>
          {/* The submenu's menu is named after its trigger item, so its dialog goes without. */}
          <Dropdown.Popover aria-labelledby="" className="max-w-72">
            <MenuItems entries={entry.items} />
          </Dropdown.Popover>
        </Dropdown.SubmenuTrigger>,
      );
    } else {
      nodes.push(renderItem(entry, index));
    }
  }
  return <Dropdown.Menu onAction={run}>{nodes}</Dropdown.Menu>;
}

export function ActionMenu({
  label,
  items,
  trigger,
  triggerClassName,
  className = "shrink-0",
  tooltip,
  contextTargetRef,
}: {
  label: string;
  items: ActionMenuEntry[];
  /** What the trigger shows; a vertical ellipsis by default. */
  trigger?: React.ReactNode;
  triggerClassName?: string;
  className?: string;
  /** The trigger's tooltip: "More actions" by default, short where `label` (its accessible name)
   * has to name the row it belongs to so each trigger is told apart; off for a trigger whose
   * visible text already says what it does. */
  tooltip?: string | false;
  /** An element whose right-click (context menu) opens this same menu at the pointer: the row or
   * header the menu belongs to. A menu opened that way is anchored to the pointer, not the trigger. */
  contextTargetRef?: React.RefObject<HTMLElement | null>;
}): React.ReactElement {
  const t = useT();
  const [isOpen, setIsOpen] = useState(false);
  // Where a right-click opened the menu; `null` while it is anchored to its trigger.
  const [pointerAt, setPointerAt] = useState<{ x: number; y: number } | null>(null);
  // Set only for a pointer-opened menu: react-aria returns focus to the trigger when a menu closes,
  // which is right for the keyboard but would pull it off the terminal for the mouse.
  const pointerFocus = usePointerFocusReturn();

  // Counted into `openMenus`. A row can unmount with its menu open (the session it belongs to is
  // deleted), which reports no close, so the unmount settles the count too.
  const countedOpenRef = useRef(false);
  const countOpen = (isOpen: boolean) => {
    if (countedOpenRef.current === isOpen) return;
    countedOpenRef.current = isOpen;
    openMenus += isOpen ? 1 : -1;
  };
  useEffect(() => () => countOpen(false), []);

  const onOpenChange = (open: boolean) => {
    setIsOpen(open);
    countOpen(open);
    if (open) menuFocusToRestore = null;
    const restore = pointerFocus.opened(open);
    if (open) return;
    setPointerAt(null);
    const previous = restore?.previous;
    // Offered at once to a menu opened in the same gesture (a right-click on another row closes this
    // one, and on macOS opens that one before the timeout below runs); that menu takes it.
    if (previous) handedFocus = previous;
    // Deferred by one task: a synchronous `focus()` here is overridden by react-aria moving focus to
    // the trigger as the menu closes. Its later restore on unmount only acts while focus has fallen to
    // <body>, so it leaves this alone.
    setTimeout(() => {
      if (!previous?.isConnected) return;
      // Another menu is open, or took the element over: focusing now would pull focus out from under it.
      if (openMenus > 0 || handedFocus !== previous) return;
      handedFocus = null;
      previous.focus();
      // A dialog the item opened keeps focus inside itself and refuses this.
      if (document.activeElement !== previous) menuFocusToRestore = previous;
    }, 0);
  };

  // Listens natively, on the target itself: a React handler would also hear the right-clicks of this
  // menu's own portalled popover, which bubble through the component tree. The handlers only use
  // state setters and refs, so the ones from the render that set the effect up stay valid.
  useEffect(() => {
    const target = contextTargetRef?.current;
    if (!target) return;
    // A secondary press (right button, or ctrl+click on macOS, which also raises a context menu)
    // must not take focus: a target without its own focus handling would pull it off whatever had
    // it (typically the terminal), and the menu returns focus to where it stood at the gesture.
    const onMouseDown = (event: MouseEvent) => {
      if (event.button === 2 || (event.button === 0 && event.ctrlKey)) event.preventDefault();
    };
    const onContextMenu = (event: MouseEvent) => {
      event.preventDefault();
      const active = document.activeElement;
      const adopt =
        handedFocus?.isConnected && (active === document.body || active?.closest("[data-slot=dropdown-popover]"))
          ? handedFocus
          : undefined;
      handedFocus = null;
      pointerFocus.notePress(adopt);
      setPointerAt({ x: event.clientX, y: event.clientY });
      onOpenChange(true);
    };
    target.addEventListener("mousedown", onMouseDown);
    target.addEventListener("contextmenu", onContextMenu);
    return () => {
      target.removeEventListener("mousedown", onMouseDown);
      target.removeEventListener("contextmenu", onContextMenu);
    };
  }, [contextTargetRef]);

  // react-aria's popover ignores a press outside it made with anything but the primary button, so
  // a right-click while the menu is open would leave it open (and, on the web, show the browser's
  // own menu over it). Close it the way react-aria's own menu trigger closes on a context menu.
  useEffect(() => {
    if (!isOpen) return;
    const inMenu = (event: Event) => event.target instanceof Element && event.target.closest("[data-slot=dropdown-popover]");
    const onMouseDown = (event: MouseEvent) => {
      if (inMenu(event)) return;
      if (event.button === 2 || (event.button === 0 && event.ctrlKey)) onOpenChange(false);
    };
    const onContextMenu = (event: MouseEvent) => {
      if (!inMenu(event)) event.preventDefault();
    };
    document.addEventListener("mousedown", onMouseDown, true);
    document.addEventListener("contextmenu", onContextMenu, true);
    return () => {
      document.removeEventListener("mousedown", onMouseDown, true);
      document.removeEventListener("contextmenu", onContextMenu, true);
    };
  }, [isOpen]);

  return (
    // Always a flex container, so the trigger is a flex item and not an inline box in a line: left
    // inline it sits on the baseline of a line box taller than itself, a pixel above the centre of
    // its row.
    <div
      className={`flex ${className}`}
      onClick={(e) => e.stopPropagation()}
      onPointerDownCapture={pointerFocus.onPointerDownCapture}
    >
      <Dropdown isOpen={isOpen} onOpenChange={onOpenChange}>
        {/* `preventFocusOnPress` keeps a press from moving focus off whatever had it (typically
            the terminal); the menu itself takes focus once it opens. */}
        <TitledControl title={tooltip === false ? undefined : (tooltip ?? t("common.moreActions"))}>
          <Dropdown.Trigger
            aria-label={label}
            preventFocusOnPress
            className={
              triggerClassName ??
              "inline-flex size-6 min-w-0 items-center justify-center rounded-md p-0 text-muted hover:bg-default hover:text-foreground aria-expanded:bg-default aria-expanded:text-foreground"
            }
          >
            {trigger ?? <EllipsisVertical aria-hidden="true" className="size-4" />}
          </Dropdown.Trigger>
        </TitledControl>
        {/* react-aria names the popover's dialog role after the trigger, as it does the menu inside it,
            so a screen reader would speak the name twice. The dialog goes without one, a deliberate
            departure from the ARIA rule that a dialog has a name: it only wraps the named menu, so a
            name would just repeat it. The empty value wins over react-aria's and refers to nothing. */}
        <Dropdown.Popover aria-labelledby="" className="min-w-48 max-w-72" getTargetRect={pointerAt ? () => new DOMRect(pointerAt.x, pointerAt.y, 0, 0) : undefined}>
          <MenuItems entries={items} />
        </Dropdown.Popover>
      </Dropdown>
    </div>
  );
}
