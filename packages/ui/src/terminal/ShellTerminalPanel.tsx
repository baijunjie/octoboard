import { Spinner, Tabs } from "@heroui/react";
import { Pencil, Plus, SquareTerminal, X } from "lucide-react";
import React, { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useFocusVisible } from "react-aria";

import { Button } from "../components/Button";
import type { ViewedFile } from "../browser/TerminalFileViewer";
import { ActionMenu, type ActionMenuEntry } from "../components/ActionMenu";
import { afterCollectionFocus } from "../components/afterCollectionFocus";
import { AnchoredTooltip } from "../components/AnchoredTooltip";
import { CenteredEmptyPanel } from "../components/EmptyPanel";
import { FadeOverflow } from "../components/FadeOverflow";
import { handFocusOff } from "../components/handFocusOff";
import { HandOffOnUnmount } from "../components/HandOffOnUnmount";
import { InlineReference } from "../components/InlineReference";
import { PanelHeader } from "../components/PanelHeader";
import { FOCUSABLE } from "../components/regionFocus";
import { ROW_ICON_BUTTON_CLASS } from "../components/RowControls";
import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { useTabScrollLabels } from "../components/tabScrollLabels";
import { TitledControl } from "../components/TitledControl";
import { useLatest, useStableHandler } from "../components/useLatest";
import { useListMotion } from "../components/useListMotion";
import { ConfirmDialog } from "../dialogs/ConfirmDialog";
import { dialogAround } from "../dialogs/dialogElement";
import { RenameDialog } from "../dialogs/RenameDialog";
import { errorPresentation } from "../errorPresentation";
import { isolate } from "../i18n/isolate";
import { t } from "../i18n/language";
import { Message, useT } from "../i18n/react";
import type { Project, ShellTerminal } from "../protocol";
import { useDaemon, type Daemon } from "../store";
import { ringForAutomaticFocus } from "../touchInput";
import { setActiveShellTerminal, startShellTerminal, useActiveShellTerminal, useProjectShellTerminals, useShellTerminalStarting } from "./shellTerminals";
import { ShellTerminalView, type ShellTerminalViewHandle } from "./ShellTerminalView";

/**
 * The Terminal panel's content: a project's shell terminals as tabs, with a header action that
 * starts another. The tab strip is HeroUI's, but the terminals are not in its tab panels, which
 * exist only while their tab is selected: every terminal's view stays mounted, hidden while its tab
 * is not the one shown, so it keeps its screen and its connection. The box holding them is named
 * the shown tab's panel by hand (`role="tabpanel"`, with the ids react-aria gives the tab).
 *
 * The caller mounts one per project (keyed by it), so switching projects shows that project's tabs;
 * the daemon keeps the others' shells running, and coming back attaches to them again. Which tab is
 * shown is kept per project and per client (`setActiveShellTerminal`).
 *
 * Closing a tab whose shell is running something asks first; an idle or exited one closes at once.
 * Tabs that arrive or go while the strip is shown move with the shared list motion.
 * A tab's name is the terminal's title, which follows the shell's until the user renames it.
 */
export function ShellTerminalPanel({
  project,
  switching,
  resizing = false,
  onViewFile,
}: {
  project: Project;
  /** The panel appears in a pane already on screen as the project changed under it: it fades in
   * where it stands. Read once, as it mounts. */
  switching: boolean;
  /** The panel is being resized every frame: its views keep their size until this ends. */
  resizing?: boolean;
  onViewFile?: (file: ViewedFile, returnFocus?: () => boolean) => void;
}): React.ReactElement {
  const t = useT();
  const daemon = useDaemon();
  const tabScrollLabels = useTabScrollLabels();
  const [fadesIn] = useState(switching);
  const terminals = useProjectShellTerminals(project.id);
  const starting = useShellTerminalStarting(project.id);
  // The tab to show in place of each one closed from here, once it has gone: the tab after it,
  // or before it. The closed one stays shown until the daemon has ended it, so focus inside it is
  // handed on as it goes (`HandOffOnUnmount`) rather than lost to a hidden tab.
  const [replacements, setReplacements] = useState<ReadonlyMap<string, string>>(new Map());
  const activeId = useActiveShellTerminal(project.id, terminals, replacements);
  useEffect(() => {
    if (activeId !== undefined) setActiveShellTerminal(project.id, activeId);
  }, [activeId]);
  const activeTerminal = terminals.find((terminal) => terminal.id === activeId);
  const [closingIds, setClosingIds] = useState<ReadonlySet<string>>(new Set());
  // The busy terminal a close is being asked about, and where the close was asked from.
  const [confirmClose, setConfirmClose] = useState<{ id: string; from: CloseOrigin }>();
  // Whether that question was declined, after which its dialog gives focus back where it was asked from.
  const closeDeclined = useRef(false);
  // An element inside the confirmation's dialog, kept past its unmount, so the dialog can be found as it goes.
  const confirmScope = useRef<HTMLElement | null>(null);
  const [renaming, setRenaming] = useState<string>();
  const rootRef = useRef<HTMLDivElement>(null);
  const bodyRef = useRef<HTMLDivElement>(null);
  const newRef = useRef<HTMLButtonElement>(null);
  const views = useRef(new Map<string, ShellTerminalViewHandle>());
  const stripRef = useListMotion<HTMLDivElement>({ contentWidth: true });
  const tabs = useRef(new Map<string, HTMLElement>());
  // The tab elements again, as state: a tab's context menu attaches to its element once, as it
  // mounts, and react-aria renders the tabs in a commit of their own, after this component's.
  const [tabNodes, setTabNodes] = useState<ReadonlyMap<string, HTMLElement>>(new Map());
  // One callback for every tab, for the life of the panel: a new one each render would detach and
  // attach every tab again. The tab's id is its element's `data-key`.
  const [registerTab] = useState(() => (element: HTMLDivElement | null) => {
    const id = element?.dataset.key;
    if (!element || id === undefined) return;
    tabs.current.set(id, element);
    setTabNodes((nodes) => (nodes.get(id) === element ? nodes : new Map(nodes).set(id, element)));
    return () => {
      if (tabs.current.get(id) === element) tabs.current.delete(id);
      setTabNodes((nodes) => {
        if (nodes.get(id) !== element) return nodes;
        const next = new Map(nodes);
        next.delete(id);
        return next;
      });
    };
  });
  // The shown tab scrolls into view in the strip as it arrives, and again whenever the strip is
  // resized: it changes size as the scroll buttons' room is reserved or released, after the first
  // reveal. Only the strip scrolls: `scrollIntoView` would scroll the page's ancestors too.
  const activeTab = activeId === undefined ? undefined : tabNodes.get(activeId);
  useLayoutEffect(() => {
    if (!activeTab) return;
    const scroller = activeTab.closest<HTMLElement>(".tabs__list-container__scroller");
    if (!scroller) return;
    const reveal = () => revealInScroller(scroller, activeTab);
    reveal();
    const resize = new ResizeObserver(reveal);
    resize.observe(scroller);
    return () => resize.disconnect();
  }, [activeTab]);
  const latest = useLatest({ activeId, keyboard: useFocusVisible().isFocusVisible });

  /** Focus for the panel once what held it has gone: the shown terminal, or the panel's first
   * control when it has none. Deferred to the frame after the commit that changed the tabs. */
  const focusShown = () =>
    requestAnimationFrame(() => {
      const view = latest.current.activeId === undefined ? undefined : views.current.get(latest.current.activeId);
      if (view) view.focus();
      else rootRef.current?.querySelector<HTMLElement>(FOCUSABLE)?.focus();
    });

  /** The shown tab, or the panel's first control when no tab is left. */
  const focusShownTabNow = () => {
    const id = latest.current.activeId;
    const tab = id === undefined ? undefined : tabs.current.get(id);
    if (tab) tab.focus();
    else rootRef.current?.querySelector<HTMLElement>(FOCUSABLE)?.focus();
  };

  /** Focus for the strip once the tab that held it (or its context menu) has gone: the shown tab, or
   * the panel's first control when no tab is left. After the strip's own re-aiming of focus, and only
   * while focus has fallen to the page, so a place the user or a dialog has put it stays. */
  const focusShownTab = () =>
    afterCollectionFocus(() => {
      if (focusFellToPage()) focusShownTabNow();
    });

  // The terminal a start from here made, to be focused once its view is shown: its record can come
  // after the reply that names it.
  const focusOnShown = useRef<string>(undefined);
  const previousActiveId = useRef(activeId);
  // A tab that becomes the shown one while focus is in the one it replaces (a terminal arriving, or
  // the shown one chosen from elsewhere) takes the focus, which the replaced view's hiding would
  // otherwise drop to `<body>`; and so does the terminal a start from here made.
  useLayoutEffect(() => {
    const previous = previousActiveId.current;
    previousActiveId.current = activeId;
    const view = activeId === undefined ? undefined : views.current.get(activeId);
    if (!view) return;
    const leavesFocus = previous !== undefined && previous !== activeId && document.getElementById(`shell-terminal-${previous}`)?.contains(document.activeElement);
    if (leavesFocus || focusOnShown.current === activeId) {
      focusOnShown.current = undefined;
      view.focus();
    }
  }, [activeId, terminals]);

  const select = (id: string) => {
    setActiveShellTerminal(project.id, id);
    // A tab pressed with the pointer is for typing into; one reached with the arrow keys keeps
    // focus in the strip.
    if (!latest.current.keyboard) requestAnimationFrame(() => views.current.get(id)?.focus());
  };

  /** Starts a terminal from the panel's own controls and puts focus in it: a press that leaves
   * focus where it was would leave it in a view about to be hidden, or in the empty state that goes.
   * With none yet it starts only if no start is already in flight, as the empty state's button
   * (gone while one is) would. */
  const start = async () => {
    const none = terminals.length === 0;
    // The empty state's button goes as the start begins, and focus on it would fall to `<body>`.
    if (none) handFocusOff(bodyRef.current, newRef.current, latest.current.keyboard);
    const id = await startShellTerminal(daemon, project.id, { ifNone: none });
    if (id === undefined) {
      focusShown();
      return;
    }
    requestAnimationFrame(() => {
      const view = views.current.get(id);
      if (view && latest.current.activeId === id) view.focus();
      else focusOnShown.current = id;
    });
  };

  const close = async (terminal: ShellTerminal) => {
    if (closingIds.has(terminal.id)) return;
    const at = terminals.indexOf(terminal);
    const next = terminals[at + 1] ?? terminals[at - 1];
    if (next) setReplacements((held) => new Map(held).set(terminal.id, next.id));
    setClosingIds((ids) => new Set(ids).add(terminal.id));
    await closeShellTerminal(daemon, terminal.id);
    setClosingIds((ids) => {
      const next = new Set(ids);
      next.delete(terminal.id);
      return next;
    });
  };
  /** Closes at once a terminal with nothing running, and asks first about one that is busy. */
  const requestClose = useStableHandler((terminal: ShellTerminal, from: CloseOrigin) => {
    if (terminal.state === "running" && terminal.busy) {
      closeDeclined.current = false;
      setConfirmClose({ id: terminal.id, from });
    } else void close(terminal);
  });
  const declineClose = useStableHandler(() => {
    closeDeclined.current = true;
    setConfirmClose(undefined);
  });

  /** Focus for the panel once the close confirmation has gone, when the element its dialog would give
   * focus back to went with the terminal (its tab, the menu item, its view), so focus fell to the page:
   * the shown tab after a close asked from a menu, the shown terminal after one asked from the terminal
   * itself, with the focus ring. The dialog's own return of focus, a frame after it closes, comes first
   * and stands, and so does focus left where it was when the daemon refused the close. */
  const focusAfterConfirmedClose = (from: CloseOrigin) =>
    afterCollectionFocus(() => {
      if (!focusFellToPage()) return;
      ringForAutomaticFocus();
      if (from === "menu") focusShownTabNow();
      else focusShown();
    });

  const actionsOf = (terminal: ShellTerminal): ActionMenuEntry[] => [
    { label: t("shellTerminal.rename"), icon: Pencil, onClick: () => setRenaming(terminal.id) },
    { label: t("shellTerminal.close"), icon: X, onClick: () => requestClose(terminal, "menu"), disabled: closingIds.has(terminal.id) },
  ];

  // The box of terminals is the shown tab's panel, under the ids react-aria gave the tab: the tab
  // names its panel in `aria-controls`, and the panel is labelled by the tab. A tab that arrives in
  // a commit of its own renders this component again (`tabNodes`).
  const linkPanel = () => {
    const body = bodyRef.current;
    if (!body) return;
    const tab = latest.current.activeId === undefined ? undefined : tabs.current.get(latest.current.activeId);
    const panelId = tab?.getAttribute("aria-controls");
    if (tab && panelId) {
      body.id = panelId;
      body.setAttribute("aria-labelledby", tab.id);
    } else {
      body.removeAttribute("id");
      body.removeAttribute("aria-labelledby");
    }
  };
  useLayoutEffect(linkPanel);

  const tabLabel = (terminal: ShellTerminal) =>
    terminal.state === "exited" ? t("shellTerminal.tab.exited", { title: isolate(terminal.title) }) : terminal.title;

  return (
    <>
      <div ref={rootRef} className={`${fadesIn ? "panel-switch " : ""}relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden`}>
        <Tabs
          selectedKey={activeId ?? ""}
          onSelectionChange={(key) => select(String(key))}
          className="flex min-h-0 flex-1 flex-col gap-0"
        >
          <PanelHeader
            title={t("panel.terminal.name")}
            hideTitleWhenNarrow
            actions={
              <>
                <TitledControl title={t("shellTerminal.new")}>
                  <Button ref={newRef} isIconOnly size="xs" variant="ghost" className={ROW_ICON_BUTTON_CLASS} aria-label={t("shellTerminal.new")} preventFocusOnPress onPress={() => void start()}>
                    <Plus aria-hidden="true" className="size-4" />
                  </Button>
                </TitledControl>
                {activeTerminal && (
                  <ActionMenu
                    label={t("shellTerminal.actions", { title: isolate(activeTerminal.title) })}
                    items={actionsOf(activeTerminal)}
                  />
                )}
              </>
            }
          >
            {terminals.length > 0 && (
              <Tabs.ListContainer
                // HeroUI lays its scroll buttons over the strip's ends, which here would cover a
                // tab's close mark: while the strip scrolls, its ends are padded to hold them.
                className="min-w-0 rounded-none bg-transparent has-[>:is([data-left-scroll=true],[data-right-scroll=true],[data-left-right-scroll=true])]:px-6"
                render={(props) => <div {...props}>{tabScrollLabels(props.children)}</div>}>
                {/* The scroller clips outside this padding; leave room for the focus ring and its offset.
                    Positioned, so a departing tab's visual stays in the strip's scroll. */}
                <Tabs.List ref={stripRef} aria-label={t("shellTerminal.tabs")} className="relative w-max min-w-0 p-1.5">
                  {terminals.map((terminal) => (
                    <Tabs.Tab
                      key={terminal.id}
                      id={terminal.id}
                      ref={registerTab}
                      data-list-motion={terminal.id}
                      // The selected tab is drawn with the fill HeroUI's sliding indicator has, as a
                      // fill of its own: the indicator's shared-element transition is left stuck
                      // part-way when its tab renders again within the frame it was selected in,
                      // which a tab created and selected in one go does. Content-wide, not filling the
                      // strip, which keeps its width while a closed tab's neighbours slide up.
                      className={`group/tab h-6 w-auto flex-none scroll-mx-1.5 gap-0.5 ps-1.5 pe-0.5 whitespace-nowrap hover:text-foreground hover:opacity-100 data-[selected=true]:bg-(--segment) data-[selected=true]:shadow-(--surface-shadow)${closingIds.has(terminal.id) ? " opacity-50" : ""}${terminal.state === "exited" ? " text-app-muted" : ""}`}
                    >
                      {/* First among what goes away with the tab, so focus inside it is still found. */}
                      <HandOffOnUnmount from={`[data-key="${CSS.escape(terminal.id)}"]`} to={focusShownTab} />
                      <FadeOverflow as="span" dir="auto" className="max-w-40 min-w-0" titleWhenClipped={tabLabel(terminal)}>{tabLabel(terminal)}</FadeOverflow>
                      <TabCloseMark title={t("shellTerminal.close")} onClose={() => requestClose(terminal, "terminal")} />
                    </Tabs.Tab>
                  ))}
                </Tabs.List>
              </Tabs.ListContainer>
            )}
          </PanelHeader>
          {/* A column, so the starting and empty states centre in it; the views are absolute. */}
          <div ref={bodyRef} role={activeTerminal ? "tabpanel" : undefined} className="relative flex min-h-0 flex-1 flex-col">
            {terminals.map((terminal) => (
              <React.Fragment key={terminal.id}>
                {/* First among what goes away with the view, so focus inside it is still found. */}
                <HandOffOnUnmount from={`#shell-terminal-${terminal.id}`} to={focusShown} />
                <ShellTerminalView
                  ref={(handle: ShellTerminalViewHandle | null) => {
                    if (handle) views.current.set(terminal.id, handle);
                    else views.current.delete(terminal.id);
                  }}
                  terminal={terminal}
                  shown={terminal.id === activeId}
                  resizing={resizing}
                  onViewFile={onViewFile}
                  onClose={() => requestClose(terminal, "terminal")}
                />
              </React.Fragment>
            ))}
            <StatusAnnouncer text={terminals.length === 0 && starting ? t("shellTerminal.starting") : undefined} />
            {terminals.length === 0 && starting && (
              <div aria-hidden="true" className="flex min-h-0 flex-1 flex-col items-center justify-center-safe gap-3 text-sm text-app-muted">
                <Spinner size="md" color="current" aria-hidden="true" />
                {t("shellTerminal.starting")}
              </div>
            )}
            {terminals.length === 0 && !starting && (
              <CenteredEmptyPanel
                icon={SquareTerminal}
                message={t("shellTerminal.none")}
                description={t("shellTerminal.none.description")}
                action={{ label: t("shellTerminal.new"), icon: Plus, onPress: () => void start() }}
              />
            )}
          </div>
        </Tabs>
      </div>
      {terminals.map((terminal) => {
        const tab = tabNodes.get(terminal.id);
        return tab && <TabContextMenu key={terminal.id} label={t("shellTerminal.actions", { title: isolate(terminal.title) })} items={actionsOf(terminal)} tab={tab} onGone={focusShownTab} />;
      })}
      {confirmClose !== undefined && terminals.some((terminal) => terminal.id === confirmClose.id) && (
        <>
          {/* First, so focus is still found in the dialog as it goes. */}
          <HandOffOnUnmount
            from={() => (closeDeclined.current ? null : dialogAround(confirmScope) ?? null)}
            to={() => focusAfterConfirmedClose(confirmClose.from)}
            ring={false}
          />
          <ConfirmDialog
            title={<span ref={(element) => { if (element) confirmScope.current = element; }}>{t("shellTerminal.closeBusy.title")}</span>}
            subject={<Message id="shellTerminal.closeBusy.subject" params={{ title: <InlineReference dir="auto" wrap>{terminals.find((terminal) => terminal.id === confirmClose.id)!.title}</InlineReference> }} />}
            message={t("shellTerminal.closeBusy.message")}
            confirmLabel={t("shellTerminal.close")}
            destructive
            onCancel={declineClose}
            errorTitle={t("shellTerminal.closeFailedTitle")}
            onConfirm={async () => {
              const terminal = terminals.find((candidate) => candidate.id === confirmClose.id);
              setConfirmClose(undefined);
              if (terminal) await close(terminal);
            }}
          />
        </>
      )}
      {renaming !== undefined && terminals.some((terminal) => terminal.id === renaming) && (
        <RenameDialog
          title={t("shellTerminal.renameDialog.title")}
          errorTitle={t("shellTerminal.renameDialog.failedTitle")}
          label={t("shellTerminal.renameDialog.field")}
          initialValue={terminals.find((terminal) => terminal.id === renaming)!.title}
          placeholder={t("shellTerminal.renameDialog.nameExample")}
          requiredMessage={t("shellTerminal.renameDialog.required")}
          onSubmit={async (title) => {
            await daemon.request({ type: "rename_shell_terminal", shell_terminal: renaming, title: title.trim() });
          }}
          onClose={() => setRenaming(undefined)}
        />
      )}
    </>
  );
}

/** Whether focus has fallen to the page, as it does when the element holding it goes. */
function focusFellToPage(): boolean {
  const active = document.activeElement;
  return !active || active === document.body;
}

/** Where a close was asked from: a menu (a tab's context menu, the header's) or the terminal itself (its
 * tab's close mark, its view's Close button), which decides where focus goes once a confirmed close has
 * taken away what held it. */
type CloseOrigin = "menu" | "terminal";

/** Asks the daemon to close `terminal`; a refusal is reported as an error. Settles once it has answered. */
async function closeShellTerminal(daemon: Pick<Daemon, "request" | "toastError">, terminal: string): Promise<void> {
  try {
    await daemon.request({ type: "close_shell_terminal", shell_terminal: terminal });
  } catch (err) {
    daemon.toastError(errorPresentation(err), undefined, t("shellTerminal.closeFailedTitle"));
  }
}

/** A tab's context menu (a right-click, or Shift+F10 and the menu key on the focused tab), with the
 * actions the header's menu has for the shown tab. It attaches to `tab` as it mounts, so it is
 * mounted once the tab is. `onGone` is called when it unmounts with focus in its open menu, as the
 * tab goes while the menu is up. */
function TabContextMenu({ label, items, tab, onGone }: { label: string; items: ActionMenuEntry[]; tab: HTMLElement; onGone: () => void }): React.ReactElement {
  const [target] = useState<React.RefObject<HTMLElement | null>>(() => ({ current: tab }));
  // Kept past the wrapper's unmount, whose ref is cleared before the hand-off looks for the menu.
  const scope = useRef<HTMLDivElement | null>(null);
  // Only this menu: another one open (the header's) keeps its focus when a tab goes. The menu is
  // labelled by its hidden trigger, in the wrapper.
  const ownMenu = () => {
    const trigger = scope.current?.querySelector("button")?.id;
    return trigger ? document.querySelector(`[role=menu][aria-labelledby="${CSS.escape(trigger)}"]`) : null;
  };
  return (
    <div ref={(element) => { if (element) scope.current = element; }} className="contents">
      <HandOffOnUnmount from={ownMenu} to={onGone} />
      <ActionMenu label={label} items={items} contextTargetRef={target} contextOnly />
    </div>
  );
}

/** Scrolls `scroller` just far enough to bring `tab` into view, honouring the scroll padding and
 * margins the strip and the tab set. The pixel difference is between rects, so it holds in a
 * right-to-left strip, where `scrollLeft` counts from the right edge. */
function revealInScroller(scroller: HTMLElement, tab: HTMLElement): void {
  const px = (value: string) => parseFloat(value) || 0;
  const strip = getComputedStyle(scroller);
  const own = getComputedStyle(tab);
  const box = scroller.getBoundingClientRect();
  const rect = tab.getBoundingClientRect();
  const beforeEdge = box.left + px(strip.scrollPaddingLeft);
  const afterEdge = box.right - px(strip.scrollPaddingRight);
  const behind = rect.left - px(own.scrollMarginLeft) - beforeEdge;
  const ahead = rect.right + px(own.scrollMarginRight) - afterEdge;
  if (behind < 0) scroller.scrollLeft += behind;
  else if (ahead > 0) scroller.scrollLeft += Math.min(ahead, behind);
}

/** A tab's close mark, for the pointer only and kept out of the tab's own press so it closes rather
 * than selects: the keyboard closes a tab from the header's menu or the tab's context menu, as a
 * tab holds no control of its own. Drawn at 16, round as the pill-shaped tab it sits in, and hit at
 * 24 (`::after`), and its tooltip is anchored, as a tab cannot hold a trigger. */
function TabCloseMark({ title, onClose }: { title: string; onClose: () => void }): React.ReactElement {
  const ref = useRef<HTMLSpanElement>(null);
  const [hovered, setHovered] = useState(false);
  return (
    <>
      <span
        ref={ref}
        aria-hidden="true"
        className="relative flex size-4 items-center justify-center rounded-full opacity-0 after:absolute after:-inset-1 after:content-[''] group-hover/tab:opacity-100 group-data-[selected=true]/tab:opacity-100 hover:bg-default"
        onPointerEnter={() => setHovered(true)}
        onPointerLeave={() => setHovered(false)}
        onPointerDown={(event) => event.stopPropagation()}
        onMouseDown={(event) => event.preventDefault()}
        onClick={(event) => {
          event.stopPropagation();
          onClose();
        }}
      >
        <X className="size-3" />
      </span>
      <AnchoredTooltip tooltip={title} anchor={ref} hovered={hovered} />
    </>
  );
}
