import { EllipsisVertical } from "lucide-react";
import React, { useEffect, useRef, useState } from "react";

import { isSecondaryPress } from "../../input/keys";
import { useT } from "../../i18n/react";
import { useHost } from "../../platform/react";
import { TitledControl } from "../hint/TitledControl";
import { useLatest } from "../hooks/useLatest";
import { ICON_TRIGGER_CLASS } from "../controls/iconTrigger";
import {
  claimHandedFocus,
  countOpenMenu,
  isActionMenuOpen,
  keepMenuFocusToRestore,
  offerHandedFocus,
  takeHandedFocus,
  takeMenuFocusToRestore,
} from "../focus/menuFocus";
import { endKeyContextMenuSkip, skipKeyContextMenu } from "./keyContextMenuSkip";
import type { ActionMenuEntry } from "./menuEntries";
import { MenuItems } from "./MenuItems";
import { MenuShortcutScope } from "./menuShortcutScope";
import { usePointerFocusReturn } from "./usePointerFocusReturn";
import { BadgedControl, Menu, MenuPopover, MenuTrigger, partSelector } from "../../primitives";

/** The entries of a menu given as a function, read when the menu opens (the popover mounts its content
 * then), so that what they judge against the clock is the time they are looked at. */
function ResolvedMenuItems({ source, onGone, autoFocus }: { source: ActionMenuEntry[] | (() => ActionMenuEntry[]); onGone: () => void; autoFocus?: "first" | "menu" }): React.ReactElement {
  // The popover unmounts its content only once its exit animation has run, which is when the menu
  // has really left the page.
  const goneRef = useLatest(onGone);
  useEffect(() => () => goneRef.current(), [goneRef]);
  return <MenuItems entries={typeof source === "function" ? source() : source} autoFocus={autoFocus} />;
}

export function ActionMenu({
  label,
  items,
  trigger,
  triggerRef,
  className = "shrink-0",
  tooltip,
  badge,
  contextTargetRef,
  contextOnly = false,
  onContextOpen,
  leavesContextToBrowser,
  onClosed,
}: {
  label: string;
  /** The entries, or a function that makes them when the menu opens. */
  items: ActionMenuEntry[] | (() => ActionMenuEntry[]);
  /** What the trigger shows; a vertical ellipsis by default. */
  trigger?: React.ReactNode;
  /** The trigger control, for a shortcut to open this menu. */
  triggerRef?: React.Ref<HTMLButtonElement>;
  className?: string;
  /** The trigger's tooltip: "More actions" by default, short where `label` (its accessible name)
   * has to name the row it belongs to so each trigger is told apart; off for a trigger whose
   * visible text already says what it does. */
  tooltip?: React.ReactNode;
  /** A `Badge` for the corner of the trigger, beside the button rather than inside it so that it does not take the
   * button's pressed scale; what it marks is said by `label` and `tooltip`. */
  badge?: React.ReactNode;
  /** An element whose right-click (context menu) opens this same menu at the pointer: the row or
   * header the menu belongs to. A menu opened that way is anchored to the pointer, not the trigger.
   * The Menu key or Shift+F10 on the element, or on what has focus inside it, opens it as well,
   * anchored below the focused element; such a menu gives focus back to that element. */
  contextTargetRef?: React.RefObject<HTMLElement | null>;
  /** Opens only from the target's context menu gesture, without a visible or tabbable trigger. */
  contextOnly?: boolean;
  /** Refreshes actions before opening; runs before the target's own context menu handler. Told where the
   * menu opens (the pointer, or below the focused element for the keyboard) and whether the keyboard did it. */
  onContextOpen?: (opened: { clientX: number; clientY: number; byKey: boolean }) => void;
  /** Judges a right-click on the target: true leaves it to the browser's own context menu, as for a link or an
   * image whose menu is how it is opened elsewhere or saved. Not asked of the keyboard gestures. */
  leavesContextToBrowser?: (event: MouseEvent) => boolean;
  /** Called once the menu has closed and its popover has left the page (after its exit animation).
   * Focus has usually gone wherever the closing put it by then, but not without an exit animation
   * (reduced motion): a menu the pointer opened gives focus back a task later, so this runs while
   * focus is still on <body>. It is given the trigger when the menu was opened
   * from the keyboard, that is while the trigger held focus, and no pointer press came before the
   * popover was gone; `null` otherwise. Not called when this menu unmounts first. */
  onClosed?: (keyboardTrigger: HTMLElement | null) => void;
}): React.ReactElement {
  const t = useT();
  const host = useHost();
  const contextOpenRef = useLatest(onContextOpen);
  const leavesContextRef = useLatest(leavesContextToBrowser);
  const [isOpen, setIsOpen] = useState(false);
  // Where a context open (a right-click, or the Menu key or Shift+F10) placed the menu; `null` while it is anchored to
  // its trigger.
  const [pointerAt, setPointerAt] = useState<{ x: number; y: number } | null>(null);
  // Whether the Menu key or Shift+F10 opened the menu, which then focuses its first item; a
  // right-click's menu focuses the menu itself, as react-aria's does for a pointer. Either is given to
  // the menu for every context open, since react-aria would otherwise reuse the focus its trigger's
  // last opening chose (the first item after a key or a screen reader's press).
  const [openedByKey, setOpenedByKey] = useState(false);
  // Counts the context opens, which remount the menu's content: a menu opened again while its popover is still
  // mounted (a second right-click, which closes and opens it within one gesture) would otherwise keep the content
  // that took focus at its first open, and none would take it now that the surface behind is inert again.
  const [openCount, setOpenCount] = useState(0);
  // Set only for a pointer-opened menu: react-aria returns focus to the trigger when a menu closes,
  // which is right for the keyboard but would pull it off the terminal for the mouse.
  const pointerFocus = usePointerFocusReturn();
  const wrapperRef = useRef<HTMLDivElement>(null);
  const isOpenRef = useRef(false);
  const onClosedRef = useLatest(onClosed);
  // For `onClosed`: the trigger, while the menu (open, or closing) was opened from it with the keyboard,
  // and the listener that drops it at a pointer press.
  const keyboardOpenRef = useRef<{ trigger: HTMLElement; listeners: AbortController } | null>(null);
  const settleKeyboardOpen = () => {
    keyboardOpenRef.current?.listeners.abort();
    keyboardOpenRef.current = null;
  };
  const mountedRef = useRef(false);
  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      settleKeyboardOpen();
      endKeyContextMenuSkip();
    };
  }, []);
  const onPopoverGone = () => {
    // React StrictMode remounts the content of a menu that is still open; that is no close.
    if (isOpenRef.current) return;
    const trigger = keyboardOpenRef.current?.trigger ?? null;
    settleKeyboardOpen();
    // After the rest of the commit that took the popover away, which unmounts this menu too when its
    // row went away.
    queueMicrotask(() => {
      if (mountedRef.current) onClosedRef.current?.(trigger);
    });
  };

  // Counted as an open menu. A row can unmount with its menu open (the session it belongs to is
  // deleted), which reports no close, so the unmount settles the count too.
  const countedOpenRef = useRef(false);
  const countOpen = (isOpen: boolean) => {
    if (countedOpenRef.current === isOpen) return;
    countedOpenRef.current = isOpen;
    countOpenMenu(isOpen);
  };
  useEffect(() => () => countOpen(false), []);

  const onOpenChange = (open: boolean) => {
    setIsOpen(open);
    isOpenRef.current = open;
    countOpen(open);
    const restore = pointerFocus.opened(open);
    if (open) {
      takeMenuFocusToRestore();
      settleKeyboardOpen();
      // Opened from the keyboard, the menu opens from its focused trigger. react-aria focuses the
      // trigger for a pointer press too, so it is the pointer's own record that tells them apart. The
      // popover is portalled, so the wrapper holds nothing but the trigger.
      const active = document.activeElement;
      if (!pointerFocus.isPointerOpened() && active instanceof HTMLElement && wrapperRef.current?.contains(active)) {
        const listeners = new AbortController();
        // A pointer press (on the page, dismissing the menu, or on an item) hands the menu over to
        // the pointer, whose focus is where it pressed.
        document.addEventListener("pointerdown", settleKeyboardOpen, { capture: true, signal: listeners.signal });
        keyboardOpenRef.current = { trigger: active, listeners };
      }
      return;
    }
    setPointerAt(null);
    setOpenedByKey(false);
    endKeyContextMenuSkip();
    const previous = restore?.previous;
    // Offered at once to a menu opened in the same gesture (a right-click on another row closes this
    // one, and on macOS opens that one before the timeout below runs); that menu takes it.
    if (previous) offerHandedFocus(previous);
    // Deferred by one task: a synchronous `focus()` here is overridden by react-aria moving focus to
    // the trigger as the menu closes. Its later restore on unmount only acts while focus has fallen to
    // <body>, so it leaves this alone.
    setTimeout(() => {
      if (!previous?.isConnected) return;
      // Another menu is open, or took the element over: focusing now would pull focus out from under it.
      if (isActionMenuOpen() || !claimHandedFocus(previous)) return;
      previous.focus();
      // A dialog the item opened keeps focus inside itself and refuses this.
      if (document.activeElement !== previous) keepMenuFocusToRestore(previous);
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
      if (leavesContextRef.current?.(event)) return;
      if (isSecondaryPress(event, host, !contextOnly)) {
        event.preventDefault();
        // xterm handles a secondary press itself; leaving it through would change the selection
        // or send a mouse report before the editing menu can use it.
        if (contextOnly) event.stopPropagation();
      }
    };
    const onContextMenu = (event: MouseEvent, byKey = false) => {
      if (!byKey && leavesContextRef.current?.(event)) return;
      event.preventDefault();
      if (contextOnly) event.stopPropagation();
      contextOpenRef.current?.({ clientX: event.clientX, clientY: event.clientY, byKey });
      const active = document.activeElement;
      const handed = takeHandedFocus();
      const adopt =
        handed?.isConnected && (active === document.body || active?.closest(partSelector.menuPopover))
          ? handed
          : undefined;
      pointerFocus.notePress(adopt);
      setPointerAt({ x: event.clientX, y: event.clientY });
      setOpenedByKey(byKey);
      setOpenCount((count) => count + 1);
      onOpenChange(true);
    };
    // The Menu key and Shift+F10 are handled here rather than left to the `contextmenu` event they
    // raise, because WebKit raises none for either key. Chromium raises one for the Menu key as the
    // keydown's default action, which `preventDefault` cancels, so the menu opens once; one raised
    // later is skipped (`skipKeyContextMenu`). An inner target that has opened its own menu has
    // already cancelled the key, and an outer one leaves it.
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || (event.key !== "ContextMenu" && !(event.key === "F10" && event.shiftKey && !event.ctrlKey && !event.altKey && !event.metaKey))) return;
      event.preventDefault();
      if (contextOnly) event.stopPropagation();
      skipKeyContextMenu();
      const rect = (event.target instanceof Element ? event.target : target).getBoundingClientRect();
      onContextMenu(new MouseEvent("contextmenu", { clientX: rect.left, clientY: rect.bottom }), true);
    };
    target.addEventListener("mousedown", onMouseDown, contextOnly);
    target.addEventListener("contextmenu", onContextMenu, contextOnly);
    target.addEventListener("keydown", onKeyDown, contextOnly);
    return () => {
      target.removeEventListener("mousedown", onMouseDown, contextOnly);
      target.removeEventListener("contextmenu", onContextMenu, contextOnly);
      target.removeEventListener("keydown", onKeyDown, contextOnly);
    };
  }, [contextTargetRef, contextOnly, contextOpenRef, leavesContextRef, host]);

  // react-aria's popover ignores a press outside it made with anything but the primary button, so
  // a right-click while the menu is open would leave it open (and, on the web, show the browser's
  // own menu over it). Close it the way react-aria's own menu trigger closes on a context menu.
  useEffect(() => {
    if (!isOpen) return;
    const inMenu = (event: Event) => event.target instanceof Element && event.target.closest(partSelector.menuPopover);
    const onMouseDown = (event: MouseEvent) => {
      if (inMenu(event)) return;
      if (isSecondaryPress(event, host, !contextOnly)) onOpenChange(false);
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
  }, [isOpen, host]);

  return (
    // Always a flex container, so the trigger is a flex item and not an inline box in a line: left
    // inline it sits on the baseline of a line box taller than itself, a pixel above the centre of
    // its row.
    <div
      ref={wrapperRef}
      className={`flex sidebar-fills ${className}`}
      onClick={(e) => e.stopPropagation()}
      onPointerDownCapture={pointerFocus.onPointerDownCapture}
    >
      <Menu open={isOpen} onOpenChange={onOpenChange}>
        {/* `focusOnPress={false}` keeps a press from moving focus off whatever had it (typically
            the terminal); the menu itself takes focus once it opens. */}
        <BadgedControl badge={badge}>
          <TitledControl title={tooltip === false ? undefined : (tooltip ?? t("common.moreActions"))}>
            <MenuTrigger
              ref={triggerRef}
              label={label}
              hidden={contextOnly}
              focusOnPress={false}
              className={contextOnly ? "hidden" : ICON_TRIGGER_CLASS}
            >
              {trigger ?? <EllipsisVertical aria-hidden="true" className="size-4" />}
            </MenuTrigger>
          </TitledControl>
        </BadgedControl>
        {/* Key caps spelled out (Ctrl Shift F, off an Apple keyboard) are wider than macOS's glyphs; a menu that has
            them is that much wider, so the label beside them keeps the room it has next to glyphs. */}
        <MenuPopover className="min-w-48 max-w-72 has-[[data-kbd-spelled]]:max-w-82" at={pointerAt ?? undefined}>
          <MenuShortcutScope value={isOpen}>
            <ResolvedMenuItems key={openCount} source={items} onGone={onPopoverGone} autoFocus={pointerAt ? (openedByKey ? "first" : "menu") : undefined} />
          </MenuShortcutScope>
        </MenuPopover>
      </Menu>
    </div>
  );
}
