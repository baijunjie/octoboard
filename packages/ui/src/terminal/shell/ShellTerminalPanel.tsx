import { Tabs } from "@heroui/react";
import { Pencil, Plus, SquareTerminal, X } from "lucide-react";
import React, { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useFocusVisible } from "react-aria";

import { Button } from "../../components/controls/Button";
import type { ViewedFile } from "../../viewer/viewedFile";
import { ActionMenu } from "../../components/menus/ActionMenu";
import type { ActionMenuEntry } from "../../components/menus/menuEntries";
import { afterCollectionFocus } from "../../components/focus/afterCollectionFocus";
import { CenteredEmptyPanel } from "../../components/panel/EmptyPanel";
import { LoadingState } from "../../components/panel/LoadingState";
import { FadeOverflow } from "../../components/text/FadeOverflow";
import { handFocusOff } from "../../components/focus/handFocusOff";
import { HandoffOnUnmount } from "../../components/focus/HandoffOnUnmount";
import { InlineReference } from "../../i18n/InlineReference";
import { PanelHeader } from "../../components/panel/PanelHeader";
import { focusFellToPage } from "../../components/focus/regionFocus";
import { ROW_ICON_BUTTON_CLASS, RowIconGroup } from "../../components/controls/RowControls";
import { StatusAnnouncer } from "../../components/announce/StatusAnnouncer";
import { INACTIVE_VEIL_CLASS } from "../../components/status/inactiveVeil";
import { LocalizedTabListContainer } from "../../components/tabs/LocalizedTabListContainer";
import { TitledControl } from "../../components/hint/TitledControl";
import { useLatest, useStableHandler } from "../../components/hooks/useLatest";
import { useListMotion } from "../../components/motion/useListMotion";
import { ConfirmDialog } from "../../components/dialog/ConfirmDialog";
import { dialogAround } from "../../components/dialog/dialogElement";
import { RenameDialog } from "../../components/dialog/RenameDialog";
import { isolate } from "../../i18n/isolate";
import { Message, useT } from "../../i18n/react";
import type { Project, ShellTerminal } from "../../protocol";
import { useDaemon } from "../../store";
import { ringForAutomaticFocus } from "../../input/touchInput";
import { shellTerminalTitle } from "./shellTerminalTitle";
import { setActiveShellTerminal, startShellTerminal, takeShellTerminalFocus, useActiveShellTerminal, useProjectShellTerminals, useShellTerminalStarting } from "./shellTerminals";
import { closeShellTerminal, revealInScroller, TabCloseMark, TabContextMenu } from "./shellTerminalTabs";
import { ShellTerminalView, type ShellTerminalViewHandle } from "./ShellTerminalView";
import { useShellTabFocus } from "./useShellTabFocus";

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
 * Tabs sit below the header, with automatic path titles shortened to their last folder.
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
  const [fadesIn] = useState(switching);
  const terminals = useProjectShellTerminals(project.id);
  const starting = useShellTerminalStarting(project.id);
  // The tab to show in place of each one closed from here, once it has gone: the tab after it,
  // or before it. The closed one stays shown until the daemon has ended it, so focus inside it is
  // handed on as it goes (`HandoffOnUnmount`) rather than lost to a hidden tab.
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

  const { focusShown, focusShownTabNow, focusShownTab } = useShellTabFocus({ latest, views, tabs, newRef });

  // The terminal a start from here made, to be focused once its view is shown: its record can come
  // after the reply that names it.
  const focusOnShown = useRef<string>(undefined);
  const previousActiveId = useRef(activeId);
  // A tab that becomes the shown one while focus is in the one it replaces (a terminal arriving, or
  // the shown one chosen from elsewhere) takes the focus, which the replaced view's hiding would
  // otherwise drop to `<body>`; and so does the terminal a start from here made, or one a start from
  // outside asked to have focused (`takeShellTerminalFocus`).
  useLayoutEffect(() => {
    const previous = previousActiveId.current;
    previousActiveId.current = activeId;
    const view = activeId === undefined ? undefined : views.current.get(activeId);
    if (!view) return;
    const leavesFocus = previous !== undefined && previous !== activeId && document.getElementById(`shell-terminal-${previous}`)?.contains(document.activeElement);
    // Taken whenever its terminal is shown, so a request not acted on cannot act on a later choice of that tab.
    const requested = activeId !== undefined && takeShellTerminalFocus(activeId);
    if (leavesFocus || focusOnShown.current === activeId || requested) {
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

  const tabLabel = (terminal: ShellTerminal, title = shellTerminalTitle(terminal)) =>
    terminal.state === "exited" ? t("shellTerminal.tab.exited", { title: isolate(title) }) : title;

  const newTerminalButton = (
    <TitledControl title={t("shellTerminal.new")}>
      <Button ref={newRef} data-region-landing={terminals.length === 0 ? "" : undefined} isIconOnly size="xs" variant="ghost" className={ROW_ICON_BUTTON_CLASS} aria-label={t("shellTerminal.new")} preventFocusOnPress onPress={() => void start()}>
        <Plus aria-hidden="true" className="size-4" />
      </Button>
    </TitledControl>
  );

  return (
    <>
      <div ref={rootRef} className={`${fadesIn ? "panel-switch " : ""}relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden`}>
        <Tabs
          selectedKey={activeId ?? ""}
          onSelectionChange={(key) => select(String(key))}
          className="flex flex-none flex-col gap-0"
        >
          <PanelHeader title={t("panel.terminal.name")} />
          <div className="flex min-h-11 min-w-0 items-center groove-b px-[calc(var(--pane-inset)-6px)]">
            {terminals.length > 0 && (
              <LocalizedTabListContainer
                // HeroUI lays its scroll buttons over the strip's ends, which here would cover a
                // tab's close mark: reserve space only at an edge that has a scroll button.
                className="min-w-0 flex-initial rounded-none bg-transparent has-[>:is([data-left-scroll=true],[data-left-right-scroll=true])]:ps-6 has-[>:is([data-right-scroll=true],[data-left-right-scroll=true])]:pe-6">
                {/* The scroller clips outside this padding; leave room for the focus ring and its offset.
                    Positioned, so a departing tab's visual stays in the strip's scroll. */}
                <Tabs.List ref={stripRef} aria-label={t("shellTerminal.tabs")} className="relative w-max min-w-0 gap-1 p-1.5">
                  {terminals.map((terminal) => (
                    <Tabs.Tab
                      key={terminal.id}
                      id={terminal.id}
                      aria-label={tabLabel(terminal, terminal.title)}
                      ref={registerTab}
                      data-list-motion={terminal.id}
                      data-closing={closingIds.has(terminal.id) || undefined}
                      className={tabClass(terminal)}
                    >
                      {/* First among what goes away with the tab, so focus inside it is still found. */}
                      <HandoffOnUnmount from={`[data-key="${CSS.escape(terminal.id)}"]`} to={focusShownTab} />
                      <FadeOverflow as="span" dir="auto" className="max-w-40 min-w-0" title={shellTerminalTitle(terminal) !== terminal.title ? tabLabel(terminal, terminal.title) : undefined} titleWhenClipped={tabLabel(terminal)}>{tabLabel(terminal)}</FadeOverflow>
                      <TabCloseMark title={t("shellTerminal.close")} onClose={() => requestClose(terminal, "terminal")} />
                    </Tabs.Tab>
                  ))}
                </Tabs.List>
              </LocalizedTabListContainer>
            )}
            <RowIconGroup className="ms-auto px-1.5">
              {newTerminalButton}
              {activeTerminal && (
                <ActionMenu
                  label={t("shellTerminal.actions", { title: isolate(activeTerminal.title) })}
                  items={actionsOf(activeTerminal)}
                />
              )}
            </RowIconGroup>
          </div>
        </Tabs>
        {/* Outside `Tabs`, which renders a hidden copy of all it holds to build its collection: a
            terminal view in that copy would be a second emulator with a connection of its own to
            the same terminal.
            A column, so the starting and empty states centre in it; the views are absolute. */}
        <div ref={bodyRef} role={activeTerminal ? "tabpanel" : undefined} className="relative flex min-h-0 flex-1 flex-col">
          {terminals.map((terminal) => (
            <React.Fragment key={terminal.id}>
              {/* First among what goes away with the view, so focus inside it is still found. */}
              <HandoffOnUnmount from={`#shell-terminal-${terminal.id}`} to={focusShown} />
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
            <LoadingState size="md" label={t("shellTerminal.starting")} />
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
      </div>
      {terminals.map((terminal) => {
        const tab = tabNodes.get(terminal.id);
        return tab && <TabContextMenu key={terminal.id} label={t("shellTerminal.actions", { title: isolate(terminal.title) })} items={actionsOf(terminal)} tab={tab} onGone={focusShownTab} />;
      })}
      {confirmClose !== undefined && terminals.some((terminal) => terminal.id === confirmClose.id) && (
        <>
          {/* First, so focus is still found in the dialog as it goes. */}
          <HandoffOnUnmount
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

const TAB_CLASS = `${INACTIVE_VEIL_CLASS} h-8 w-auto flex-none scroll-mx-1.5 gap-2 rounded-md border border-transparent px-2 whitespace-nowrap after:-m-px after:rounded-md hover:bg-default hover:text-foreground data-[selected=true]:border-accent data-[selected=true]:bg-accent-soft data-[selected=true]:text-(--tint-accent-fg) data-closing:after:bg-panel-sidebar/55`;

/** A tab's classes. Built from whole class names, none of them glued to an interpolation, which
 * Tailwind's scanner would read as one unknown class and drop. A tab being closed (`data-closing`) is
 * dimmed with a veil of the panel card's colour, not `opacity` (see `INACTIVE_VEIL_CLASS`): it holds a
 * fading overflow mark and a close mark with a colour transition. The veil's negative margin grows it
 * over the 1px border, so a selected tab's accent border dims with the rest. */
function tabClass(terminal: ShellTerminal): string {
  return terminal.state === "exited" ? `${TAB_CLASS} text-app-muted` : TAB_CLASS;
}

/** Where a close was asked from: a menu (a tab's context menu, the tab row's) or the terminal itself (its
 * tab's close mark, its view's Close button), which decides where focus goes once a confirmed close has
 * taken away what held it. */
type CloseOrigin = "menu" | "terminal";
