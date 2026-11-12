import { LoaderCircle, type LucideIcon } from "lucide-react";
import React, { useCallback, useContext, useEffect, useId, useRef, useState } from "react";

import { errorPresentation, type ErrorPresentation } from "../../errors/errorPresentation";
import { isolate } from "../../i18n/isolate";
import { FailureAlert } from "../alerts/FailureAlert";
import { ReasonText } from "../text/ReasonText";
import { reasonText } from "../../format/reason";
import { AFTER_FOCUS_MOVE_DELAY_MS, StatusAnnouncer } from "../announce/StatusAnnouncer";
import { useT } from "../../i18n/react";
import { FadeOverflow } from "../text/FadeOverflow";
import { registerMenuCommand } from "./menuCommands";
import { useLatest } from "../hooks/useLatest";
import { ringForAutomaticFocus } from "../../input/lastInput";
import { MODAL_DIALOG, TABBABLE } from "../focus/selectors";
import { useResolvedMenuEntries } from "./useResolvedMenuEntries";
import { MenuShortcutScope } from "./menuShortcutScope";
import type { ActionMenuEntry, ActionMenuItem, ActionMenuPanelContext } from "./menuEntries";
import { activateMenuItem, Description, Kbd, KbdContent, Label, MenuChoices, MenuItem, MenuItemCheck, MenuList, MenuPopover, MenuSeparator, Submenu, SubmenuIndicator, useSubmenuPanel } from "../../primitives";

/** HeroUI's menu item label ignores the pointer, and its content inherits that, so a clipped label never sees
 * the hover that opens its tooltip. The label's content takes the pointer back, a disabled item's too (see
 * `DISABLED_ROW_CLASS`); a press on it still bubbles to the item, which runs it as before.
 *
 * The cursor over that content stays the one the row shows elsewhere: the row's hand on an item that can be
 * chosen, where WebKit gives the `label` element its own arrow, and on a disabled item the arrow the pointer
 * passing through the row shows, where HeroUI's disabled label would show `not-allowed`. */
function labelPointerClass(disabled: boolean): string {
  return `*:pointer-events-auto ${disabled ? "cursor-default" : "cursor-[inherit]"}`;
}

/** A disabled row that its label's content has given the pointer back to matches `:hover` and `:active` while the
 * pointer is on that content, so HeroUI's hover fill and press shrink are cancelled on it; it stays at full
 * opacity too (see `renderItem`). */
const DISABLED_ROW_CLASS = "ui-disabled:opacity-100 ui-disabled:hover:bg-transparent ui-disabled:active:transform-none";

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


/** What a panel of custom content sits in. react-aria hands a submenu's props to its menu, which a
 * panel is not, so the ones that make it part of the menu are applied here (`useSubmenuPanel`): the
 * ref that keeps the pointer's way to it open, and the keys that leave it. Content that moves focus
 * with the arrow keys marks the elements on its start edge `data-panel-edge="start"`: the key
 * towards the menu leaves from one of those, where anywhere else it moves within the content. */
function PanelBody({ children }: { children: (context: ActionMenuPanelContext) => React.ReactNode }): React.ReactElement {
  const { ref, id, labelledBy, autoFocus, close, onKeyDown, onKeyUp } = useSubmenuPanel();
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
      {children({ close, labelledBy, autoFocus })}
    </div>
  );
}

/** `autoFocus` is the root menu's, for a menu opened on its context target: the keyboard's focuses its first item
 * as a menu opened from its trigger with the keyboard does, a right-click's the menu itself. */
export function MenuItems({ entries: source, autoFocus }: { entries: ActionMenuEntry[]; autoFocus?: "first" | "menu" }): React.ReactElement {
  const entries = useResolvedMenuEntries(source);
  const t = useT();
  const shortcutsActive = useContext(MenuShortcutScope);
  const prefix = useId();
  const root = useRef<HTMLDivElement>(null);
  const itemRefs = useRef(new Map<number, HTMLElement>());
  const itemId = (index: number) => `${prefix}:${index}`;
  const rememberItem = useCallback((node: HTMLElement | null, index: number) => {
    if (node) itemRefs.current.set(index, node);
    else itemRefs.current.delete(index);
  }, []);
  const latest = useLatest({ entries, shortcutsActive });
  useEffect(() => registerMenuCommand((event) => {
    if (!latest.current.shortcutsActive || event.repeat || event.isComposing || event.keyCode === 229) return false;
    const menu = root.current?.querySelector('[role="menu"]');
    if (!menu || document.activeElement?.closest('[role="menu"]') !== menu || document.querySelectorAll('[role="menu"]').length !== 1) return false;
    const dialog = root.current?.closest('[role="dialog"]');
    if ([...document.querySelectorAll(MODAL_DIALOG)].some((held) => held !== dialog)) return false;
    const index = latest.current.entries.findIndex((entry) => isLeaf(entry) && entry.commandShortcut?.matches(event));
    if (index < 0) return false;
    const entry = latest.current.entries[index];
    const item = itemRefs.current.get(index);
    if (!isLeaf(entry) || isDisabledItem(entry) || !item) return false;
    activateMenuItem(item);
    return true;
  }), [latest]);
  // Each menu owns its keys, including actions bubbling from a deeper submenu.
  const run = (key: string) => {
    if (!key.startsWith(`${prefix}:`)) return;
    const entry = entries[Number(key.slice(prefix.length + 1))];
    if (entry && isLeaf(entry)) entry.onClick();
  };

  // The disabled style fades the whole item, which would take its text below the contrast it needs; so a
  // disabled item stays at full opacity and is dimmed by its icon fading and its label taking the secondary
  // text colour. A shortcut at its end keeps its colour. Its label still takes the pointer, so that a clipped
  // name shows its tooltip, without the row lighting or shrinking under it. Overriding HeroUI's disabled look
  // this way, the row's fade and the label's (`[data-disabled=true] .label`) and the label taking the pointer, is
  // by the user's decision.
  const renderItem = (entry: ActionMenuItem, index: number) => {
    const disabled = isDisabledItem(entry);
    return (
      <MenuItem
        key={index}
        id={itemId(index)}
        textValue={entry.label}
        ref={(node) => rememberItem(node, index)}
        shortcut={entry.commandShortcut?.label ?? entry.shortcut}
        data-marquee-scope
        label={entry.reason && !entry.description ? t("menu.nameWithReason", { name: isolate(entry.ariaLabel ?? entry.label), reason: reasonText(entry.reason) }) : entry.ariaLabel}
        className={disabled ? DISABLED_ROW_CLASS : undefined}
        danger={entry.destructive}
        disabled={disabled}
        closeOnSelect={entry.closeOnSelect}
      >
        {disabled ? (
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
          <Label className={`min-w-0 max-w-full text-clip ${labelPointerClass(disabled)}${disabled ? " text-app-muted opacity-100" : ""}`}>
            {entry.content ?? <FadeOverflow as="span" className="block" dir="auto" titleWhenClipped={entry.label}>{entry.label}</FadeOverflow>}
          </Label>
          {entry.description && <Description className="sr-only">{entry.description}</Description>}
          {entry.reason && <span aria-hidden="true" className="text-xs text-app-muted"><ReasonText reason={entry.reason} /></span>}
        </span>
        {!entry.reason && (entry.shortcut ? <MenuShortcut letter={entry.shortcut} /> : entry.end)}
        {entry.selected !== undefined && <MenuItemCheck />}
      </MenuItem>
    );
  };

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
        <MenuChoices key={group[0]} selected={group.filter((i) => (entries[i] as ActionMenuItem).selected).map(itemId)}>
          {group.map((i) => renderItem(entries[i] as ActionMenuItem, i))}
        </MenuChoices>,
      );
    } else if (entry === "separator") {
      // A menu built from conditional items can leave a separator with nothing on one side; a separator
      // is drawn only between two items, and a run of them draws one.
      if (nodes.length > 0 && index < entries.length - 1 && entries[index + 1] !== "separator") nodes.push(<MenuSeparator key={index} />);
    } else if ("note" in entry) {
      nodes.push(
        // Disabled so that it cannot be chosen, yet drawn at full opacity as the quiet text it is, not as a
        // branch that is off (HeroUI's fade of the row and of its `Label` cancelled, by the user's decision); the
        // spacer holds the place of the glyph the items have, and `ps-7` that of the check indicator a menu with
        // choices reserves at the start of each of its items (HeroUI's `ps-7` for them).
        <MenuItem key={index} id={itemId(index)} textValue={entry.label} disabled className={`cursor-default ui-disabled:opacity-100 ${entries.some(isChoice) ? "ps-7" : ""}`}>
          <span aria-hidden="true" className="size-4 shrink-0" />
          <Label className="min-w-0 flex-1 text-sm whitespace-normal text-app-muted opacity-100" dir="auto">{entry.label}</Label>
        </MenuItem>,
      );
    } else if ("items" in entry || "panel" in entry) {
      nodes.push(
        <Submenu key={index}>
          {/* HeroUI gives submenu triggers ps-2, while Dropdown items use px-2.5. Keep their leading edges aligned. */}
          <MenuItem id={itemId(index)} textValue={entry.label} data-marquee-scope className="ps-2.5!">
            <ItemIcon icon={entry.icon} className={entry.iconClassName} />
            <Label className={`min-w-0 flex-1 text-clip ${labelPointerClass(false)}`}>
              <FadeOverflow as="span" className="block" dir="auto" titleWhenClipped={entry.label}>{entry.label}</FadeOverflow>
            </Label>
            {/* HeroUI's indicator is a chevron that always points right, while the submenu opens
                towards the start of the line in a right-to-left interface; a flip points it towards
                the submenu. */}
            <SubmenuIndicator className="rtl:-scale-x-100" />
          </MenuItem>
          {/* The submenu's menu is named after its trigger item, so its dialog goes without. */}
          <MenuPopover className="max-w-72">
            {"panel" in entry ? <PanelBody>{entry.panel}</PanelBody> : entry.loadItems ? <LoadedMenuItems load={entry.loadItems} /> : <MenuItems entries={entry.items} />}
          </MenuPopover>
        </Submenu>,
      );
    } else {
      nodes.push(renderItem(entry, index));
    }
  }
  return (
    <div ref={root} className="contents" onKeyDownCapture={(event) => {
      if (!shortcutsActive || event.defaultPrevented || event.repeat || event.nativeEvent.isComposing || event.nativeEvent.keyCode === 229 || event.ctrlKey || event.metaKey || event.altKey || event.shiftKey) return;
      if (!(event.target instanceof Element) || event.target.closest('[role="menu"]') !== root.current?.querySelector('[role="menu"]')) return;
      const index = entries.findIndex((entry) => isLeaf(entry) && entry.shortcut?.toLowerCase() === event.key.toLowerCase());
      if (index < 0) return;
      event.preventDefault();
      event.stopPropagation();
      const entry = entries[index];
      if (isLeaf(entry) && isDisabledItem(entry)) return;
      const item = itemRefs.current.get(index);
      if (!item) return;
      // Selection and closing stay react-aria's, including entries that keep their menu open.
      activateMenuItem(item);
    }}>
      <MenuList onAction={run} autoFocus={autoFocus}>
        {nodes}
      </MenuList>
    </div>
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
      // pointer: the ring follows that input, so only a key's shows where focus went.
      ringForAutomaticFocus();
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

function MenuShortcut({ letter }: { letter: string }): React.ReactElement {
  return <Kbd decorative dir="ltr" slot="keyboard" className="ms-auto shrink-0 in-ui-hovered:bg-surface"><KbdContent>{letter}</KbdContent></Kbd>;
}
