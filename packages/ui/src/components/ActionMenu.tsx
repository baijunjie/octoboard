import { Dropdown, Label } from "@heroui/react";
import { Ellipsis } from "lucide-react";
import React, { useEffect, useRef } from "react";

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

/** Whether any action menu's popover is open, which holds keyboard focus like a modal does. */
export function isActionMenuOpen(): boolean {
  return openMenus > 0;
}

export interface ActionMenuItem {
  label: string;
  onClick: () => void;
  destructive?: boolean;
}

/**
 * A small "more" (ellipsis) action menu; each item is a one-shot action. It lives inside a tree row
 * that is itself clickable, so a click is kept from reaching the row: the popover is portalled out
 * of the DOM but React still bubbles its events through this component's ancestors, which would
 * otherwise select or toggle the row behind the menu.
 */
export function ActionMenu({ label, items }: { label: string; items: ActionMenuItem[] }): React.ReactElement {
  // The element that had focus when a pointer press on the trigger began (typically the terminal's
  // textarea). It is held only until that press ends; if the press opened the menu it moves to
  // `restoreRef`, so a press that never opens it (dragged off the trigger) leaves nothing behind.
  const pressRef = useRef<HTMLElement | null>(null);
  // Set only for a pointer-opened menu. react-aria returns focus to the trigger when a menu closes,
  // which is right for the keyboard but would pull it off the terminal for the mouse.
  const restoreRef = useRef<HTMLElement | null>(null);

  // Counted into `openMenus`. A row can unmount with its menu open (the session it belongs to is
  // deleted), which reports no close, so the unmount settles the count too.
  const countedOpenRef = useRef(false);
  const countOpen = (isOpen: boolean) => {
    if (countedOpenRef.current === isOpen) return;
    countedOpenRef.current = isOpen;
    openMenus += isOpen ? 1 : -1;
  };
  useEffect(() => () => countOpen(false), []);

  const onOpenChange = (isOpen: boolean) => {
    countOpen(isOpen);
    if (isOpen) {
      menuFocusToRestore = null;
      restoreRef.current = pressRef.current;
      pressRef.current = null;
      return;
    }
    const previous = restoreRef.current;
    restoreRef.current = null;
    // Deferred by one task: a synchronous `focus()` here is overridden by react-aria moving focus to
    // the trigger as the menu closes. Its later restore on unmount only acts while focus has fallen to
    // <body>, so it leaves this alone.
    setTimeout(() => {
      if (!previous?.isConnected) return;
      previous.focus();
      // A dialog the item opened keeps focus inside itself and refuses this.
      if (document.activeElement !== previous) menuFocusToRestore = previous;
    }, 0);
  };

  return (
    <div
      className="shrink-0"
      onClick={(e) => e.stopPropagation()}
      onPointerDownCapture={(e) => {
        // Only a press on the trigger itself: the popover's items bubble through here as well.
        if (!e.currentTarget.contains(e.target as Node)) return;
        const active = document.activeElement;
        pressRef.current = active instanceof HTMLElement && active !== document.body ? active : null;
        // The press opens the menu during this gesture (on pointerdown for a mouse, on release for
        // touch), before this timeout runs.
        const listeners = new AbortController();
        const end = () => {
          listeners.abort();
          setTimeout(() => (pressRef.current = null), 0);
        };
        window.addEventListener("pointerup", end, { signal: listeners.signal });
        window.addEventListener("pointercancel", end, { signal: listeners.signal });
      }}
    >
      <Dropdown onOpenChange={onOpenChange}>
        {/* `preventFocusOnPress` keeps a press from moving focus off whatever had it (typically
            the terminal); the menu itself takes focus once it opens. */}
        <Dropdown.Trigger
          aria-label={label}
          preventFocusOnPress
          className="inline-flex size-6 min-w-0 items-center justify-center rounded-md bg-transparent p-0 text-muted hover:bg-transparent hover:text-foreground aria-expanded:text-foreground"
        >
          <Ellipsis aria-hidden="true" className="size-4" />
        </Dropdown.Trigger>
        <Dropdown.Popover>
          <Dropdown.Menu onAction={(key) => items.find((item) => item.label === key)?.onClick()}>
            {items.map((item) => (
              <Dropdown.Item
                key={item.label}
                id={item.label}
                textValue={item.label}
                variant={item.destructive ? "danger" : "default"}
              >
                <Label>{item.label}</Label>
              </Dropdown.Item>
            ))}
          </Dropdown.Menu>
        </Dropdown.Popover>
      </Dropdown>
    </div>
  );
}
