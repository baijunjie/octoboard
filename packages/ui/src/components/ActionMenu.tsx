import { Description, Dropdown, Label, Separator } from "@heroui/react";
import { EllipsisVertical, LoaderCircle, type LucideIcon } from "lucide-react";
import { MenuContext, useSlottedContext } from "react-aria-components";
import { setInteractionModality } from "react-aria";
import React, { useEffect, useId, useRef, useState } from "react";

import { isSecondaryPress } from "../useWindowShortcut";
import { errorPresentation, type ErrorPresentation } from "../errorPresentation";
import { isolate } from "../i18n/isolate";
import { FailureAlert } from "./FailureAlert";
import { ReasonText, reasonText, type Reason } from "./ReasonText";
import { AFTER_FOCUS_MOVE_DELAY_MS, StatusAnnouncer } from "./StatusAnnouncer";
import { useT } from "../i18n/react";
import { FadeOverflow } from "./FadeOverflow";
import { TitledControl } from "./TitledControl";
import { useLatest } from "./useLatest";
import { usePointerFocusReturn } from "./usePointerFocusReturn";

/** Set when a menu closed but could not hand focus back, because the item it ran opened a dialog
 * that holds it. The dialog takes it when it closes (see `takeMenuFocusToRestore`). */
let menuFocusToRestore: HTMLElement | null = null;

/** The element a menu could not give focus back to while a dialog was open, if any; the caller
 * returns focus to it once the dialog is gone. Each element is handed out once. The dialog's owner
 * may take it first and drop it, when the dialog's outcome sends focus somewhere else. */
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

/** Ends the wait set up by `skipKeyContextMenu`, if one is running. */
let endKeyContextMenuSkip: (() => void) | null = null;

/** Skips the `contextmenu` event a browser may still raise for a Menu key or Shift+F10 that a menu has
 * already opened from: some raise it only as the key comes back up (Chromium on Windows, for the Menu
 * key), when cancelling the keydown no longer prevents it, and it would open the menu a second time or
 * the browser's own menu over it. The wait ends with that event, the next keydown or pointer press, the
 * menu closing or going away, or a second. */
function skipKeyContextMenu(): void {
  endKeyContextMenuSkip?.();
  const listeners = new AbortController();
  const end = () => {
    listeners.abort();
    clearTimeout(timer);
    if (endKeyContextMenuSkip === end) endKeyContextMenuSkip = null;
  };
  const skip = (event: Event) => {
    event.preventDefault();
    event.stopImmediatePropagation();
    end();
  };
  window.addEventListener("contextmenu", skip, { capture: true, signal: listeners.signal });
  // A held key repeats its keydown before the keyup that raises the late event, so repeats leave the skip on.
  window.addEventListener("keydown", (event) => { if (!event.repeat) end(); }, { capture: true, signal: listeners.signal });
  window.addEventListener("pointerdown", end, { capture: true, signal: listeners.signal });
  const timer = setTimeout(end, 1000);
  endKeyContextMenuSkip = end;
}

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
  /** Added to a lucide glyph's own classes (`rtl:-scale-x-100` for one whose meaning is a direction);
   * an element keeps the classes it was given. */
  iconClassName?: string;
  /** Runs when the item is chosen. */
  onClick: () => void;
  destructive?: boolean;
  closeOnSelect?: boolean;
  disabled?: boolean;
  /** The accessible name when it should say more than `label` (a console and what is going on in it). */
  ariaLabel?: string;
  /** Drawn after the label, at the item's end edge (a status marker). */
  end?: React.ReactNode;
  /** Why the item cannot be chosen, or how to make it available ("Option-drag to select"): it is
   * disabled and the reason is drawn under the label in place of `end`. The accessible name is the
   * label (`ariaLabel` if set) followed by the reason, unless the item has a `description`, which then
   * carries the reason in full and leaves the name the label. */
  reason?: Reason;
  /** A note that assistive technology reads as the item's description and that is not drawn (what
   * remains of an account's quota, which the usage bar in `icon` shows only to the eye; for an item
   * with a `reason`, the reason in full). */
  description?: string;
  /** Makes the item one of a single-selection group (consecutive items that set it): HeroUI marks
   * the chosen one (`true`) with its check and announces it as checked. */
  selected?: boolean;
}

/** An item that opens a nested menu of its own items. */
export interface ActionMenuSubmenu {
  label: string;
  icon: LucideIcon | React.ReactElement;
  items: ActionMenuEntry[];
  /** Loads fresh items when this submenu opens. */
  loadItems?: () => Promise<ActionMenuEntry[]>;
}

/** What a panel is given to be part of the menu it opens from. */
export interface ActionMenuPanelContext {
  /** Closes the whole menu, the submenu with it. */
  close: () => void;
  /** The id of the item that opened the panel, which names what the panel holds (the panel itself
   * goes unnamed, so the name is not spoken twice). */
  labelledBy: string | undefined;
  /** Whether keyboard focus should move into the panel as it opens: it does for a submenu opened
   * from the keyboard and not for one a hover opened. */
  autoFocus: boolean;
}

/** An item that opens a panel of custom content beside the menu in place of a nested menu (a
 * palette of swatches). The panel is part of the menu: arrow keys leave it towards the menu as they
 * do a nested one, and it closes the whole menu to finish. */
export interface ActionMenuPanel {
  label: string;
  icon: LucideIcon | React.ReactElement;
  panel: (context: ActionMenuPanelContext) => React.ReactNode;
}

/** A line of muted text at the menu's end that says something about the items above it (a limit the list
 * hit): it carries no glyph, its text lines up with the items' labels (past the check indicator of a menu with
 * choices too), and it cannot be chosen. It stays in
 * the arrow-key walk, so it is reached and read from the keyboard. */
export interface ActionMenuNote {
  /** The note's text. */
  label: string;
  note: true;
}

export type ActionMenuEntry = ActionMenuItem | ActionMenuSubmenu | ActionMenuPanel | ActionMenuNote | "separator";

/** Whether an item cannot be chosen: it is disabled, or it gives a reason it cannot be. */
function isDisabledItem(entry: ActionMenuItem): boolean {
  return Boolean(entry.disabled || entry.reason);
}

/** A lucide glyph is drawn quieter than the label beside it, or in the danger colour with a
 * destructive item's label; an element (an agent's mark) keeps its own colours. */
function ItemIcon({
  icon,
  destructive,
  className,
}: {
  icon: LucideIcon | React.ReactElement;
  destructive?: boolean;
  className?: string;
}): React.ReactElement {
  if (React.isValidElement(icon)) return icon;
  const Icon = icon;
  return <Icon aria-hidden="true" className={`size-4 shrink-0 ${destructive ? "text-danger" : "text-app-muted"}${className ? ` ${className}` : ""}`} />;
}

function isLeaf(entry: ActionMenuEntry): entry is ActionMenuItem {
  return entry !== "separator" && !("items" in entry) && !("panel" in entry) && !("note" in entry);
}

function isChoice(entry: ActionMenuEntry): entry is ActionMenuItem {
  return isLeaf(entry) && entry.selected !== undefined;
}

/** What can take focus with the Tab key: the first such element of a panel receives focus as it opens. */
const TABBABLE = 'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/** What a panel of custom content sits in. react-aria hands a submenu's props to its menu, which a
 * panel is not, so the ones that make it part of the menu are applied here: the ref that keeps the
 * pointer's way to it open, and the keys that leave it. Content that moves focus with the arrow
 * keys marks the elements on its start edge `data-panel-edge="start"`: the key towards the menu
 * leaves from one of those, where anywhere else it moves within the content. The values come from
 * `SubmenuTrigger` through `MenuContext`: what to check again when react-aria is upgraded. */
function PanelBody({ children }: { children: (context: ActionMenuPanelContext) => React.ReactNode }): React.ReactElement {
  const context = useSlottedContext(MenuContext);
  const { ref, id, "aria-labelledby": labelledBy, autoFocus, onClose } = context ?? {};
  // The two key handlers are passed along but not part of `MenuProps`.
  const { onKeyDown, onKeyUp } = (context ?? {}) as { onKeyDown?: React.KeyboardEventHandler; onKeyUp?: React.KeyboardEventHandler };
  return (
    <div
      ref={ref}
      id={id}
      // Focusable so that the arrow key which opens a submenu a hover opened can land here (react-aria
      // focuses this element); focus is passed on to the content's first tabbable element at once.
      tabIndex={-1}
      onFocus={(event) => {
        if (event.target === event.currentTarget) event.currentTarget.querySelector<HTMLElement>(TABBABLE)?.focus();
      }}
      // react-aria's handler stops the key it acts on, which keeps the content from also moving
      // focus; one it does not act on (the key away from the menu) goes on to the content.
      onKeyDownCapture={(event) => {
        if ((event.key === "ArrowLeft" || event.key === "ArrowRight") && (event.target as Element).closest('[data-panel-edge="start"]')) onKeyDown?.(event);
      }}
      onKeyDown={onKeyDown}
      onKeyUp={onKeyUp}
    >
      {children({ close: onClose ?? (() => {}), labelledBy, autoFocus: Boolean(autoFocus) })}
    </div>
  );
}

/** `autoFocus` is the root menu's, for a menu opened from the keyboard on its context target, which
 * focuses its first item as a menu opened from its trigger with the keyboard does. */
function MenuItems({ entries, autoFocus }: { entries: ActionMenuEntry[]; autoFocus?: "first" }): React.ReactElement {
  const t = useT();
  const prefix = useId();
  const itemId = (index: number) => `${prefix}:${index}`;
  // Each menu owns its keys, including actions bubbling from a deeper submenu.
  const run = (key: React.Key) => {
    if (typeof key !== "string" || !key.startsWith(`${prefix}:`)) return;
    const entry = entries[Number(key.slice(prefix.length + 1))];
    if (entry && isLeaf(entry)) entry.onClick();
  };

  // The disabled style fades the whole item, which would take its text below the contrast it needs; so a
  // disabled item stays at full opacity and is dimmed by its icon fading and its label taking the secondary
  // text colour. A shortcut at its end keeps its colour. Overriding HeroUI's disabled look this way, the row's
  // fade and the label's (`[data-disabled=true] .label`), is by the user's decision.
  const renderItem = (entry: ActionMenuItem, index: number) => (
    <Dropdown.Item
      key={index}
      id={itemId(index)}
      textValue={entry.label}
      data-marquee-scope
      aria-label={entry.reason && !entry.description ? t("menu.nameWithReason", { name: isolate(entry.ariaLabel ?? entry.label), reason: reasonText(entry.reason) }) : entry.ariaLabel}
      className={isDisabledItem(entry) ? "data-[disabled=true]:opacity-100" : undefined}
      variant={entry.destructive ? "danger" : "default"}
      isDisabled={isDisabledItem(entry)}
      shouldCloseOnSelect={entry.closeOnSelect}
    >
      {isDisabledItem(entry) ? (
        <span aria-hidden="true" className="flex shrink-0 opacity-(--disabled-opacity)">
          <ItemIcon icon={entry.icon} destructive={entry.destructive} className={entry.iconClassName} />
        </span>
      ) : (
        <ItemIcon icon={entry.icon} destructive={entry.destructive} className={entry.iconClassName} />
      )}
      {/* A label is often a name the user typed, so it reads in its own direction; `content` sets
          its own. */}
      <span className="flex min-w-0 flex-1 flex-col">
        {/* HeroUI sizes a menu item's label to its text (`w-fit`), which would run a long one under
            `end`; `max-w-full` keeps it to its column, where it fades. */}
        <Label className={`min-w-0 max-w-full text-clip${isDisabledItem(entry) ? " text-app-muted opacity-100" : ""}`}>
          {entry.content ?? <FadeOverflow as="span" className="block" dir="auto" titleWhenClipped={entry.label}>{entry.label}</FadeOverflow>}
        </Label>
        {entry.description && <Description className="sr-only">{entry.description}</Description>}
        {entry.reason && <span aria-hidden="true" className="text-xs text-app-muted"><ReasonText reason={entry.reason} /></span>}
      </span>
      {!entry.reason && entry.end}
      {entry.selected !== undefined && <Dropdown.ItemIndicator />}
    </Dropdown.Item>
  );

  // A disabled item stays in the arrow-key walk (the default skips it), so the reason under it (a quota
  // used up) can be reached and read from the keyboard while the item still refuses to run. react-stately
  // reads `disabledBehavior` from the menu's tree state and from a single-selection section's own
  // selection state, but neither one's props declare it, hence the casts.
  const keepDisabled = { disabledBehavior: "selection" };
  const nodes: React.ReactNode[] = [];
  // A run of consecutive `selected` items is a section of its own with single selection, so the
  // menu's other items stay plain menu items rather than becoming radios.
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
          selectedKeys={group.filter((i) => (entries[i] as ActionMenuItem).selected).map(itemId)}
          {...(keepDisabled as React.ComponentProps<typeof Dropdown.Section>)}
        >
          {group.map((i) => renderItem(entries[i] as ActionMenuItem, i))}
        </Dropdown.Section>,
      );
    } else if (entry === "separator") {
      // A menu built from conditional items can leave a separator with nothing on one side; a separator
      // is drawn only between two items, and a run of them draws one.
      if (nodes.length > 0 && index < entries.length - 1 && entries[index + 1] !== "separator") nodes.push(<Separator key={index} />);
    } else if ("note" in entry) {
      nodes.push(
        // Disabled so that it cannot be chosen, yet drawn at full opacity as the quiet text it is, not as a
        // branch that is off (HeroUI's fade of the row and of its `Label` cancelled, by the user's decision); the
        // spacer holds the place of the glyph the items have, and `ps-7` that of the check indicator a menu with
        // choices reserves at the start of each of its items (HeroUI's `ps-7` for them).
        <Dropdown.Item key={index} id={itemId(index)} textValue={entry.label} isDisabled className={`cursor-default data-[disabled=true]:opacity-100${entries.some(isChoice) ? " ps-7" : ""}`}>
          <span aria-hidden="true" className="size-4 shrink-0" />
          <Label className="min-w-0 flex-1 text-sm whitespace-normal text-app-muted opacity-100" dir="auto">{entry.label}</Label>
        </Dropdown.Item>,
      );
    } else if ("items" in entry || "panel" in entry) {
      nodes.push(
        <Dropdown.SubmenuTrigger key={index}>
          <Dropdown.Item id={itemId(index)} textValue={entry.label} data-marquee-scope>
            <ItemIcon icon={entry.icon} />
            <Label className="min-w-0 flex-1 text-clip">
              <FadeOverflow as="span" className="block" dir="auto" titleWhenClipped={entry.label}>{entry.label}</FadeOverflow>
            </Label>
            {/* HeroUI's indicator is a chevron that always points right, while the submenu opens
                towards the start of the line in a right-to-left interface; a flip points it towards
                the submenu. */}
            <Dropdown.SubmenuIndicator className="rtl:-scale-x-100" />
          </Dropdown.Item>
          {/* The submenu's menu is named after its trigger item, so its dialog goes without. */}
          <Dropdown.Popover aria-labelledby="" className="max-w-72">
            {"panel" in entry ? <PanelBody>{entry.panel}</PanelBody> : entry.loadItems ? <LoadedMenuItems load={entry.loadItems} /> : <MenuItems entries={entry.items} />}
          </Dropdown.Popover>
        </Dropdown.SubmenuTrigger>,
      );
    } else {
      nodes.push(renderItem(entry, index));
    }
  }
  return (
    <Dropdown.Menu onAction={run} {...(autoFocus && { autoFocus })} {...(keepDisabled as React.ComponentProps<typeof Dropdown.Menu>)}>
      {nodes}
    </Dropdown.Menu>
  );
}

/** A submenu whose items are loaded as it opens. A failed load is an Alert holding the application's
 * sentence for the failure, the system's own text behind Details, and Try again, which announces itself since it arrives after the menu opened, so the status region
 * speaks the loading only. It comes with the focus move to Try again (or with the menu's own opening
 * when the load is quick), so its announcement waits that out. Focus that was inside the menu goes
 * with the change: to Try again when the load fails, and to the menu again when the retry has
 * started, rather than falling to the page. */
function LoadedMenuItems({ load }: { load: () => Promise<ActionMenuEntry[]> }): React.ReactElement {
  const t = useT();
  const [entries, setEntries] = useState<ActionMenuEntry[]>();
  const [error, setError] = useState<ErrorPresentation>();
  const sequence = useRef(0);
  const root = useRef<HTMLDivElement>(null);
  // Where focus is to be put once the next render has replaced what held it.
  const focusNext = useRef<"retry" | "menu" | undefined>(undefined);
  const inside = () => root.current?.contains(document.activeElement) ?? false;
  const refresh = () => {
    const asked = ++sequence.current;
    if (inside()) focusNext.current = "menu";
    setEntries(undefined);
    setError(undefined);
    load().then(
      (items) => { if (asked === sequence.current) setEntries(items); },
      (err: unknown) => {
        if (asked !== sequence.current) return;
        if (inside()) focusNext.current = "retry";
        setError(errorPresentation(err));
      },
    );
  };
  useEffect(() => {
    refresh();
    return () => { sequence.current += 1; };
  }, []);
  useEffect(() => {
    const next = focusNext.current;
    focusNext.current = undefined;
    if (next === "retry") {
      // The render took the focused control away, not a press, and the last input was often a
      // pointer: the ring has to show where focus went.
      setInteractionModality("keyboard");
      // The recovery action stands last, at the trailing edge, after Details.
      [...(root.current?.querySelectorAll<HTMLElement>(TABBABLE) ?? [])].at(-1)?.focus();
    } else if (next === "menu") root.current?.querySelector<HTMLElement>("[role=menu]")?.focus();
  }, [error]);
  return <>
    <StatusAnnouncer text={entries || error !== undefined ? undefined : t("menu.loading")} />
    {/* A menu holds only items, so the failure is a panel in the submenu's popover; it unmounts as the
        retry starts and the menu takes its place. The wrapper is one element for both, so it tells
        where focus was. */}
    <div ref={root} className="contents">
      {error !== undefined && !entries ? (
        <PanelBody>
          {() => (
            <FailureAlert title={t("menu.loadFailed")} message={error} announce announceDelayMs={AFTER_FOCUS_MOVE_DELAY_MS} onRetry={refresh} />
          )}
        </PanelBody>
      ) : (
        <MenuItems entries={entries ?? [{ label: t("menu.loading"), icon: LoaderCircle, iconClassName: "motion-safe:animate-spin", disabled: true, onClick: () => {} }]} />
      )}
    </div>
  </>;
}

/** What every small icon button of a row or a strip shares, with no tint of its own: the size and shape, and the
 * `sidebar-fills` the tints are drawn from. */
export const ICON_TRIGGER_BASE_CLASS = "sidebar-fills inline-flex row-icon-control items-center justify-center rounded-md p-0 text-app-muted";

/** How the actions menu's trigger looks: the one look of a small icon button in a row or a strip, which the other icon
 * buttons beside it take too (`ICON_TRIGGER_CLASS` on a `Button`), so they read as one set. It carries `sidebar-fills`,
 * the tints the menu's own wrapper gives the trigger, so each hovers alike on a dialog's surface as on the sidebar's. */
export const ICON_TRIGGER_CLASS = `${ICON_TRIGGER_BASE_CLASS} hover:bg-default hover:text-foreground aria-expanded:bg-default aria-expanded:text-foreground`;

/** The entries of a menu given as a function, read when the menu opens (the popover mounts its content
 * then), so that what they judge against the clock is the time they are looked at. */
function ResolvedMenuItems({ source, onGone, autoFocus }: { source: ActionMenuEntry[] | (() => ActionMenuEntry[]); onGone: () => void; autoFocus?: "first" }): React.ReactElement {
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
  contextTargetRef,
  contextOnly = false,
  onContextOpen,
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
  /** An element whose right-click (context menu) opens this same menu at the pointer: the row or
   * header the menu belongs to. A menu opened that way is anchored to the pointer, not the trigger.
   * The Menu key or Shift+F10 on the element, or on what has focus inside it, opens it as well,
   * anchored below the focused element; such a menu gives focus back to that element. */
  contextTargetRef?: React.RefObject<HTMLElement | null>;
  /** Opens only from the target's context menu gesture, without a visible or tabbable trigger. */
  contextOnly?: boolean;
  /** Refreshes actions before opening; runs before the target's own context menu handler. */
  onContextOpen?: () => void;
  /** Called once the menu has closed and its popover has left the page (after its exit animation),
   * so focus has gone wherever the closing put it. It is given the trigger when the menu was opened
   * from the keyboard, that is while the trigger held focus, and no pointer press came before the
   * popover was gone; `null` otherwise. Not called when this menu unmounts first. */
  onClosed?: (keyboardTrigger: HTMLElement | null) => void;
}): React.ReactElement {
  const t = useT();
  const contextOpenRef = useLatest(onContextOpen);
  const [isOpen, setIsOpen] = useState(false);
  // Where a right-click opened the menu; `null` while it is anchored to its trigger.
  const [pointerAt, setPointerAt] = useState<{ x: number; y: number } | null>(null);
  // Whether the Menu key or Shift+F10 opened the menu, which then focuses its first item; a
  // right-click's menu focuses the menu itself, as react-aria's does for a pointer.
  const [openedByKey, setOpenedByKey] = useState(false);
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
      endKeyContextMenuSkip?.();
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
    isOpenRef.current = open;
    countOpen(open);
    const restore = pointerFocus.opened(open);
    if (open) {
      menuFocusToRestore = null;
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
    endKeyContextMenuSkip?.();
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
      if (isSecondaryPress(event, !contextOnly)) {
        event.preventDefault();
        // xterm handles a secondary press itself; leaving it through would change the selection
        // or send a mouse report before the editing menu can use it.
        if (contextOnly) event.stopPropagation();
      }
    };
    const onContextMenu = (event: MouseEvent, byKey = false) => {
      event.preventDefault();
      if (contextOnly) event.stopPropagation();
      contextOpenRef.current?.();
      const active = document.activeElement;
      const adopt =
        handedFocus?.isConnected && (active === document.body || active?.closest("[data-slot=dropdown-popover]"))
          ? handedFocus
          : undefined;
      handedFocus = null;
      pointerFocus.notePress(adopt);
      setPointerAt({ x: event.clientX, y: event.clientY });
      setOpenedByKey(byKey);
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
  }, [contextTargetRef, contextOnly, contextOpenRef]);

  // react-aria's popover ignores a press outside it made with anything but the primary button, so
  // a right-click while the menu is open would leave it open (and, on the web, show the browser's
  // own menu over it). Close it the way react-aria's own menu trigger closes on a context menu.
  useEffect(() => {
    if (!isOpen) return;
    const inMenu = (event: Event) => event.target instanceof Element && event.target.closest("[data-slot=dropdown-popover]");
    const onMouseDown = (event: MouseEvent) => {
      if (inMenu(event)) return;
      if (isSecondaryPress(event, !contextOnly)) onOpenChange(false);
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
      ref={wrapperRef}
      className={`flex sidebar-fills ${className}`}
      onClick={(e) => e.stopPropagation()}
      onPointerDownCapture={pointerFocus.onPointerDownCapture}
    >
      <Dropdown isOpen={isOpen} onOpenChange={onOpenChange}>
        {/* `preventFocusOnPress` keeps a press from moving focus off whatever had it (typically
            the terminal); the menu itself takes focus once it opens. */}
        <TitledControl title={tooltip === false ? undefined : (tooltip ?? t("common.moreActions"))}>
          <Dropdown.Trigger
            ref={triggerRef}
            aria-label={label}
            hidden={contextOnly}
            excludeFromTabOrder={contextOnly}
            preventFocusOnPress
            className={
              contextOnly
                ? "hidden"
                : ICON_TRIGGER_CLASS
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
          <ResolvedMenuItems source={items} onGone={onPopoverGone} autoFocus={openedByKey ? "first" : undefined} />
        </Dropdown.Popover>
      </Dropdown>
    </div>
  );
}
