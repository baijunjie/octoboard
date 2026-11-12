import { AppWindow, Bell, BrushCleaning, FileCog, FolderCheck, GitBranch, Info, Keyboard, KeyRound, SearchX, Settings2, ShieldCheck, ShieldUser, type LucideIcon } from "lucide-react";
import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { StatusAnnouncer } from "../components/announce/StatusAnnouncer";
import { FieldClearButton } from "../components/controls/FieldClearButton";
import { TitledControl } from "../components/hint/TitledControl";
import { EmptyPanel } from "../components/panel/EmptyPanel";
import { FadedScroller } from "../components/scroll/scrollFade";
import { STICKY_HEADING_HEIGHT } from "../components/scroll/StickyHeading";
import { CountBadge } from "../components/status/CountBadge";
import { FadeOverflow } from "../components/text/FadeOverflow";
import { useListMotion } from "../components/motion/useListMotion";
import { Modal, ModalCloseButton, openingFocus, setKeyboardModality, SearchField, Tab, TabList, TabListContainer, TabPanel, Tabs } from "../primitives";
import { dialogAround } from "../components/dialog/dialogElement";
import { tryEachFrame } from "../components/focus/tryEachFrame";
import { useEscapeWhileTooltipOpen } from "../components/dialog/useEscapeWhileTooltipOpen";
import type { PlainMessageKey } from "../i18n/catalog";
import { useT } from "../i18n/react";
import { isImeKey } from "../input/imeKey";
import { useHost } from "../platform/react";
import { useIsNarrow } from "../components/hooks/breakpoint";
import { AboutSection } from "./AboutSection";
import { AppBehaviorSection } from "./AppBehaviorSection";
import { AccountsNavigationContext } from "./accounts/AccountsNavigationContext";
import { AgentAccountsSection, AGENT_STRIP_HEIGHT } from "./accounts/AgentAccountsSection";
import { AgentBehaviorSection } from "./AgentBehaviorSection";
import { CleanupSection } from "./CleanupSection";
import { ConfigurationSection } from "./ConfigurationSection";
import { GeneralSection } from "./GeneralSection";
import { GitSection } from "./GitSection";
import { NotificationsSection } from "./NotificationsSection";
import { PermissionsSection } from "./PermissionsSection";
import { ShortcutsSection } from "./ShortcutsSection";
import { matches as keyMatches } from "../shortcuts/binding";
import { FIND_BINDING } from "../shortcuts/commands";
import { SETTINGS_SECTION_IDS, type SettingsNavigation, type SettingsSectionId } from "./sections";
import { SettingsReadsProvider } from "./SettingsReadsProvider";
import { SettingsSearchProvider } from "./settingsSearchFilter";
import { SettingsFocusContext } from "./useSectionRefocus";
import { useFocusRescue } from "./useFocusRescue";
import { useSettingsQuery } from "./useSettingsQuery";
import { TrustedFoldersSection } from "./TrustedFoldersSection";

interface Section {
  label: PlainMessageKey;
  Icon: LucideIcon;
  Content: () => React.ReactElement;
  /** The height of a heading the section pins to the top of its scroller, if it has one. */
  stickyHeadingHeight?: number;
}

/** What each section of `SETTINGS_SECTION_IDS` shows; a section missing here does not compile. */
const SECTIONS: Record<SettingsSectionId, Section> = {
  general: { label: "settings.section.general", Icon: Settings2, Content: GeneralSection, stickyHeadingHeight: STICKY_HEADING_HEIGHT.group },
  "app-behavior": { label: "settings.section.appBehavior", Icon: AppWindow, Content: AppBehaviorSection, stickyHeadingHeight: STICKY_HEADING_HEIGHT.group },
  git: { label: "settings.section.git", Icon: GitBranch, Content: GitSection, stickyHeadingHeight: STICKY_HEADING_HEIGHT.group },
  accounts: { label: "settings.section.accounts", Icon: KeyRound, Content: AgentAccountsSection, stickyHeadingHeight: AGENT_STRIP_HEIGHT },
  "agent-permissions": { label: "settings.section.agentPermissions", Icon: ShieldUser, Content: AgentBehaviorSection },
  "trusted-folders": { label: "settings.section.trustedFolders", Icon: FolderCheck, Content: TrustedFoldersSection },
  permissions: { label: "settings.section.permissions", Icon: ShieldCheck, Content: PermissionsSection },
  notifications: { label: "settings.section.notifications", Icon: Bell, Content: NotificationsSection },
  shortcuts: { label: "settings.section.shortcuts", Icon: Keyboard, Content: ShortcutsSection, stickyHeadingHeight: STICKY_HEADING_HEIGHT.group },
  cleanup: { label: "settings.section.cleanup", Icon: BrushCleaning, Content: CleanupSection },
  configuration: { label: "settings.section.configuration", Icon: FileCog, Content: ConfigurationSection },
  about: { label: "settings.section.about", Icon: Info, Content: AboutSection },
};

/** How many animation frames (about half a second) a tab waits for a modal in front of Settings to go before it
 * gives up taking focus. */
export const MAX_FRAMES_TO_WAIT_FOR_MODAL = 30;

/** Where the dialog's grid puts the search field, the section tabs and what fills the rest (the selected section's
 * panel, or the empty state of a search that finds nothing, beside the hidden tabs and panel): narrow, three rows down
 * the window; wide, the field over the tabs in a column on the start side and the panel beside both, on a ground a step
 * below the dialog's, divided from the panel (`COLUMN_GROUND_AREA`). */
const SEARCH_AREA = "max-tablet:row-start-1 tablet:col-start-1 tablet:row-start-1";
const TABS_AREA = "max-tablet:row-start-2 tablet:col-start-1 tablet:row-start-2";
const PANEL_AREA = "max-tablet:row-start-3 tablet:col-start-2 tablet:row-span-2 tablet:row-start-1";
/** The column's ground: the column's cells, the full height of the dialog, stretched across the grid's gap so that its
 * edge, the divider (a groove), meets the panel, whose own padding then sets the content's distance from it (as from
 * the dialog's end edge). The column ends `COLUMN_END_INSET` before the gap, so its rows sit as far from the divider as
 * from the dialog's start edge. */
const COLUMN_GROUND_AREA = "tablet:col-start-1 tablet:row-span-2 tablet:row-start-1 tablet:-me-2";
/** Added to the gap at the column's end to match its 16px start margin (the field's `ms-4`, the tab list's
 * `--strip-inset`): 8px plus the 8px gap. */
const COLUMN_END_INSET = "tablet:me-2";
/** The wide column's search field stands raised on the column's darker ground (`raised-fills` on the dialog sets
 * its `--menu-raised`): HeroUI's primary field, white in light, recoloured in dark, where HeroUI's dark field is
 * darker than the ground. Its hover and focus fills are derived on the root, so they are set again here. A user-approved departure
 * from HeroUI's look, recorded in `docs/ui-standard.md`'s "The Settings section menu" section. Narrow, the field sits on the dialog's surface itself and is the usual filled field. */
const MENU_SEARCH_FIELD_CLASS = "[--field-background:var(--menu-raised)] [--field-focus:var(--menu-raised)] [--field-hover:color-mix(in_oklab,var(--menu-raised)_90%,var(--foreground)_2%)]";
/** A section's tab in the wide menu column: a row of the column's width that reads from the start, with the hover tint
 * of a sidebar row (translucent, so in dark it stays below the selected row rather than outshining it as a menu item's
 * fill would), and the selected row in `--menu-selected` (darker than the ground in light, a step lighter in
 * dark), in the foreground colour. The selection is drawn on the tab itself (the tab has no indicator there), and
 * `scroll-my-1` keeps the focus ring of a revealed tab in view inside the list's scroller. A user-approved departure
 * from HeroUI's tab look, recorded in `docs/ui-standard.md`'s "The Settings section menu" section. */
const MENU_TAB_CLASS = "w-full justify-start gap-2 scroll-my-1 rounded-lg px-3 whitespace-nowrap hover:bg-panel-hover hover:text-foreground ui-selected:bg-(--menu-selected) ui-selected:text-foreground ui-selected:hover:bg-(--menu-selected)";

/** A section's panel. The section scrolls on its own inside the dialog and fades at the edge it can
 * still be scrolled towards, as the body of a `Dialog` does. The scroller sits inside the panel, not
 * on it, because the panel has an exit transition. Its vertical padding is `--section-padding`, which
 * a sticky heading takes negated as its `top` to pin flush with the scroller's edge rather than the
 * padding's; `stickyHeadingHeight` is the height of such a heading (see `FadedScroller`). What the
 * section holds is one list motion, over its rows, blocks and group headings (`data-list-motion`, at
 * any depth), so they leave, come back and slide into place as a search hides and shows them; the
 * lists inside it move their own rows. `hidden` hides the panel, still mounted, while a search finds
 * nothing anywhere. */
function SectionPanel({ id, stickyHeadingHeight, hidden, children }: { id: string; stickyHeadingHeight?: number; hidden: boolean; children: React.ReactNode }): React.ReactElement {
  const listRef = useListMotion<HTMLDivElement>({ descendants: true });
  return (
    <TabPanel id={id} hidden={hidden} className={`flex min-h-0 min-w-0 flex-col max-tablet:p-0 ${PANEL_AREA}`}>
      <FadedScroller
        stickyHeadingHeight={stickyHeadingHeight}
        stickySurface="overlay"
        className="min-h-0 flex-1 px-4 py-(--section-padding) [--section-padding:calc(var(--spacing)*4)] max-phone:px-3 max-phone:[--section-padding:calc(var(--spacing)*3)] desktop:px-8 desktop:[--section-padding:calc(var(--spacing)*6)]"
      >
        {/* Positioned, so the copies of what leaves are placed in it. */}
        <div ref={listRef} className="relative">{children}</div>
      </FadedScroller>
    </TabPanel>
  );
}

/**
 * The settings: a large modal over the whole window, the sections as a vertical menu on the start
 * side and the selected one beside it, a divider between the two. At the `tablet` breakpoint and below the modal
 * fills the window and the sections become the shared tab strip along the top, scrolling sideways when it overflows.
 * It is a plain modal, so everything under it, the terminal included, stays mounted and sized.
 *
 * It is assembled from HeroUI's modal parts rather than built on `Dialog`, deliberately: a `Dialog`
 * is a titled frame around one scrolling body, where this holds a panel per section, each scrolling
 * on its own, with the sections beside the selected panel rather than under a title.
 *
 * Escape, the close button and a click on the backdrop call `onClose`; opening moves focus to the
 * selected section's entry (also where a section's `useSectionRefocus` sends it back to), and
 * `useSettingsDialog` puts it back on the terminal when this closes. It opens on `initialSection`
 * when given, the first section otherwise; `navigation` says the same and more (see its prop).
 *
 * The search field above the sections filters them by the settings they hold (`SETTINGS_SEARCH_INDEX`, see
 * `useSettingsQuery`): while it has words in it, only the sections with a match are listed, each with how many; the
 * section on screen, which stays while it is listed and gives way to the first listed otherwise, shows only its
 * matching rows; and with no match anywhere an empty state takes the place of the tabs' panel, which stays mounted,
 * hidden with the tabs, so that what was typed in it survives. A row that waits on a read of the system (launch at
 * login, the macOS permissions) is counted once the read has answered, as it appears. Tabs and rows that a search takes
 * away or brings back move with the shared list motion. Escape in the field clears it before it closes the dialog, and
 * clearing keeps the section on screen. A navigation request clears it, so the section it asks for is shown whole; one
 * that names no section keeps the section on screen. Find (`FIND_BINDING`) focuses the field and selects its text.
 */
export function SettingsDialog(props: SettingsDialogProps): React.ReactElement {
  return (
    <SettingsReadsProvider>
      <SettingsDialogContent {...props} />
    </SettingsReadsProvider>
  );
}

interface SettingsDialogProps {
  /** The section to open on, for a dialog that asks for no `navigation`. A typo must not compile: a key that
   * names no section renders no panel and never focuses a tab, which would lose the dialog's keyboard rules. */
  initialSection?: SettingsSectionId;
  /** What the dialog is asked to show, first on opening and again by a request made while it is open (each has
   * its own `nonce`): a section, the Add agent dialog in front for `addAgent`, and an account's sharing switch to
   * focus for `account`. */
  navigation?: SettingsNavigation;
  onClose: () => void;
}

/** The dialog inside the reads of the system its rows wait on (`SettingsReadsProvider`): the search counts a row
 * from the same answer the row appears on. */
function SettingsDialogContent({ initialSection = SETTINGS_SECTION_IDS[0], navigation, onClose }: SettingsDialogProps): React.ReactElement {
  const t = useT();
  const host = useHost();
  // The privacy grants are the system's own on macOS alone: managed in the app there (`permissionsPane`), and in a
  // browser on a Mac the section says they are managed in the app.
  const sectionIds = SETTINGS_SECTION_IDS.filter((id) => id !== "permissions" || host.permissionsPane || host.os === "macos");
  const isNarrow = useIsNarrow();
  const fullScreen = isNarrow;
  const search = useSettingsQuery({ sectionIds, sectionLabel: (id) => SECTIONS[id].label, initialId: navigation?.section ?? initialSection });
  const { matches, countOf, listedIds, shownId } = search;
  const searching = search.words.length > 0;
  // While nothing matches anywhere, the section on screen keeps its tab and panel, hidden, rather than unmounting them.
  const nothingFound = listedIds.length === 0;
  const tabIds = nothingFound ? [shownId] : listedIds;
  // Tabs that come and go slide along the column, whose list's height follows them so that the copies of those leaving
  // stay inside it rather than overflowing the column's scroller, which would turn its scrolling and edge fades on for
  // the length of the motion; or along the strip that is as wide as its tabs when narrow.
  const tabListRef = useListMotion<HTMLDivElement>(isNarrow ? { contentWidth: true } : { fitHeight: true });
  const addAgentOnOpen = navigation?.addAgent ?? false;
  // Both ask the Agent accounts section to take focus, in place of the section's tab.
  const sectionTakesFocus = addAgentOnOpen || navigation?.account !== undefined;
  // Taken up once by the Agent accounts section, which may be mounted again later by switching back to it.
  const [addAgentPending, setAddAgentPending] = useState(addAgentOnOpen);
  const [accountPending, setAccountPending] = useState(navigation?.account);
  const [dismissAccountForms, setDismissAccountForms] = useState(0);
  const accountsNavigation = useMemo(() => ({ pending: addAgentPending, consume: () => setAddAgentPending(false), account: accountPending, consumeAccount: () => setAccountPending(undefined), dismissForms: dismissAccountForms }), [addAgentPending, accountPending, dismissAccountForms]);
  const lastNonce = useRef(navigation?.nonce);
  // Counts the requests whose section's tab takes focus (all but those for the Add agent dialog or an account).
  const [tabFocusRequests, setTabFocusRequests] = useState(0);
  useEffect(() => {
    if (!navigation || navigation.nonce === lastNonce.current) return;
    lastNonce.current = navigation.nonce;
    if (navigation.section) search.select(navigation.section);
    search.clear();
    setAddAgentPending(navigation.addAgent);
    setAccountPending(navigation.account);
    // Dismiss account forms before showing the requested section or Add agent dialog.
    setDismissAccountForms((count) => count + 1);
    if (!navigation.addAgent && navigation.account === undefined) setTabFocusRequests((count) => count + 1);
  }, [navigation?.nonce]);
  const selectedRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    if (tabFocusRequests === 0) return;
    // Read off the page: the tabs hand their element to `selectedRef` a render later than they select. While a
    // modal in front (the Add agent dialog being dismissed) has made this one inert, it waits, frame by frame,
    // and gives up after `MAX_FRAMES_TO_WAIT_FOR_MODAL`.
    return tryEachFrame(() => {
      const settings = dialogAround(markerRef);
      if (!settings) return true;
      if (settings.closest('[inert], [aria-hidden="true"]')) return false;
      settings.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]')?.focus();
      return true;
    }, MAX_FRAMES_TO_WAIT_FOR_MODAL);
  }, [tabFocusRequests]);
  const focusedOnOpen = useRef(false);
  const markerRef = useRef<HTMLSpanElement>(null);
  useEscapeWhileTooltipOpen(() => dialogAround(markerRef), onClose);

  // Find takes focus to the search field, its text selected. A listener on the dialog element, as the file viewer's
  // find has: a modal in front of Settings is its own element, so a key pressed in it never reaches this one.
  const searchRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    const dialog = dialogAround(markerRef);
    if (!dialog) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (isImeKey(event) || !keyMatches(FIND_BINDING, event, host)) return;
      event.preventDefault();
      event.stopPropagation();
      setKeyboardModality();
      searchRef.current?.focus();
      searchRef.current?.select();
    };
    dialog.addEventListener("keydown", onKeyDown);
    return () => dialog.removeEventListener("keydown", onKeyDown);
  }, [host]);
  // What a re-match hides or removes while it holds focus (another language's copy no longer matching, a setting
  // changed from another window) sends focus to the field rather than to the page. After a tap it goes to the selected
  // tab, or to Close while nothing is found, as the field would raise the on-screen keyboard.
  // The focus scope's fallback when focus is lost is the dialog's first control, Close.
  const closeButton = () => dialogAround(markerRef)?.querySelector<HTMLElement>("[data-settings-close]") ?? null;
  useFocusRescue(
    () => dialogAround(markerRef),
    () => searchRef.current,
    () => {
      // The tab list stays mounted, hidden, while nothing matches, so Close is the destination then.
      if (nothingFound) return closeButton();
      // HeroUI attaches a newly selected tab a render after selecting it.
      return dialogAround(markerRef)?.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]') ?? "pending";
    },
    closeButton,
  );

  // HeroUI's tabs build their collection in a first pass before the tab elements exist, so the
  // selected tab is not in `selectedRef` yet when an effect on mount runs. Focus it as it attaches.
  const attachSelected = useCallback((tab: HTMLDivElement | null) => {
    selectedRef.current = tab;
    if (tab && !focusedOnOpen.current) {
      focusedOnOpen.current = true;
      if (!sectionTakesFocus) tab.focus();
    }
    // The tab list scrolls on its own (sideways as the narrow strip, down the wide column on a short window), so a
    // section picked from a link may start out of view; `nearest` on both axes moves only the list, not the dialog.
    tab?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
  }, [sectionTakesFocus]);

  // Where a section that loses focus (see `useSectionRefocus`) sends it back: the selected tab.
  const focusTarget = useCallback(() => selectedRef.current, []);

  return (
    // Native full size removes narrow-screen margins and uses an edge-to-edge transition. Wider windows retain their
    // inset layout; the container ends above the bottom notices. Its grid places the parts (see `SEARCH_AREA`).
    <Modal
      onClose={onClose}
      size={fullScreen ? "full" : undefined}
      containerClassName={fullScreen ? undefined : "max-desktop:p-4"}
      label={t("settings.title")}
      className={`control-fills raised-fills grid overflow-hidden p-0 ${fullScreen ? "grid-cols-[minmax(0,1fr)] grid-rows-[auto_auto_minmax(0,1fr)] gap-y-2" : "h-[calc(100dvh-32px-var(--bottom-chrome-height))] w-[min(1000px,calc(100vw-32px))] max-w-none grid-cols-[auto_minmax(0,1fr)] grid-rows-[auto_minmax(0,1fr)] gap-x-2 desktop:h-[min(85vh,calc(100vh-96px-var(--bottom-chrome-height)))] desktop:w-[min(1000px,calc(100vw-96px))]"}`}
    >
      <span ref={markerRef} hidden />
      <TitledControl title={t("common.close")}>
        {/* The panels' positioned scrollers overlap this corner. Keep the close button above them. */}
        <ModalCloseButton label={t("common.close")} data-settings-close className="z-10 max-phone:top-4.5 max-phone:end-3" />
      </TitledControl>
      {/* The wide column's ground: HeroUI's secondary surface, the step below the dialog's own in both appearances,
          with the divider, a groove, as its edge (recorded in `docs/ui-standard.md`'s "The Settings section menu"
          section). Purely visual, so assistive technology meets nothing between the column and the panel; first among
          the grid's items, so the field and the tabs paint over it. */}
      {!isNarrow && <div aria-hidden="true" className={`pointer-events-none groove-e bg-surface-secondary ${COLUMN_GROUND_AREA}`} />}
      {/* Outside `Tabs`, whose content react-aria also renders in a hidden copy, and before the tabs in the Tab order.
          Narrow, it is the row along the top, clear of the close button at its end. Wide, it is inset by the menu
          list's own padding (the room its rows' focus rings need), so the field's edges, icon and text line up with
          the rows' below it. */}
      <div className={`min-w-0 ms-4 me-12 mt-3 max-phone:ms-3 max-phone:mt-2 tablet:mt-4 tablet:w-48 tablet:px-1 desktop:mt-6 ${COLUMN_END_INSET} ${SEARCH_AREA}`}>
        <SearchField
          label={t("settings.search.label")}
          inputRef={searchRef}
          {...search.queryField}
          fullWidth
          // Wide, the primary field on the column's ground (`MENU_SEARCH_FIELD_CLASS`); narrow, the usual filled field.
          variant={isNarrow ? "secondary" : "primary"}
          className={isNarrow ? undefined : MENU_SEARCH_FIELD_CLASS}
          placeholder={t("common.search")}
          clearButton={<FieldClearButton />}
        />
      </div>
      <StatusAnnouncer text={search.announcement} />
      {nothingFound && (
        // Spoken by the status region above, as one sentence.
        <div className={`flex min-h-0 min-w-0 flex-col justify-center p-4 ${PANEL_AREA}`}>
          <EmptyPanel silent icon={SearchX} message={t("settings.search.none")} />
        </div>
      )}
      {/* `contents`: the tab list and the panel are the grid's own items, the search field beside them. */}
      <Tabs
        orientation={isNarrow ? "horizontal" : "vertical"}
        selectedKey={shownId}
        onSelectionChange={(key) => search.select(key as SettingsSectionId)}
        className="contents"
      >
        {/* A direct child of `Tabs`, as HeroUI's styling of it expects. Wide, it is the menu column itself, with no
                track round the list, whose rows are filled by the tabs, and a divider (above) between it and the
                section. It stretches over its grid row and scrolls its list on its own when the sections outnumber
                what the dialog's height holds, while the field, the ground and the divider stay put; there its edge
                fades show that it scrolls, and HeroUI's scroll buttons, which sit centred over the rows' labels, are
                hidden. Both are user-approved departures from HeroUI's look, recorded in
                `docs/ui-standard.md`'s "The Settings section menu" section. Narrow, it is the shared tab strip under
                the search field (`fit="content"`), scrolling sideways on its own, and the width cap keeps its end clear
                of the dialog's edge by the start margin, both from `--strip-inset` so that the two cannot disagree. Its
                list pads for the focus rings, inside the scroller's clip. */}
        <TabListContainer
          fit={isNarrow ? "content" : undefined}
          hidden={nothingFound}
          className={`[--strip-inset:--spacing(4)] max-phone:[--strip-inset:--spacing(3)] ms-(--strip-inset) max-tablet:max-w-[calc(100%-2*var(--strip-inset))] tablet:mt-2 tablet:mb-4 tablet:min-h-0 tablet:w-48 tablet:self-stretch tablet:bg-transparent tablet:[&>button]:hidden desktop:mb-6 ${COLUMN_END_INSET} ${TABS_AREA}`}
        >
          {/* HeroUI sizes a vertical list to its widest tab; it fills the column here. It is positioned, so the
                  copy of a tab that leaves is placed in it. */}
          <TabList ref={tabListRef} label={t("settings.sections")} fit={isNarrow ? "content" : undefined} className="relative tablet:w-full">
            {/* While a search filters the narrow strip, the selected tab draws HeroUI's indicator fill itself instead
                of the sliding indicator: a tab the search brings back can be selected in the same render, which leaves
                that indicator's move stuck part-way, and so can the motion of the tabs. The indicator stays mounted
                meanwhile, unseen and with no transition, so that it neither slides in from where it was before the
                search (the move that would get stuck) nor keeps a place to slide from: once the search is cleared it
                is simply shown in its tab. This is the exception that keeps the selected tab filled while list motion
                moves tabs, recorded in docs/ui-standard.md's "Conformance level and exceptions" section. The wide
                menu has no indicator to get stuck. */}
            {tabIds.map((id) => {
              const { label, Icon } = SECTIONS[id];
              return (
                <Tab
                  key={id}
                  id={id}
                  size={isNarrow ? "content" : undefined}
                  indicator={isNarrow}
                  data-list-motion={id}
                  label={searching ? t("settings.search.sectionMatches", { section: t(label), count: countOf(id) }) : undefined}
                  indicatorClassName={isNarrow && searching ? "opacity-0 transition-none" : undefined}
                  className={isNarrow ? `gap-2 ${searching ? "ui-selected:bg-segment ui-selected:shadow-surface" : ""}` : MENU_TAB_CLASS}
                  ref={id === shownId ? attachSelected : undefined}
                  {...(id === shownId && !sectionTakesFocus && openingFocus(true))}
                >
                  <Icon aria-hidden="true" className="size-4 shrink-0" />
                  {/* A name too long for the column fades out before the count, which stays in view; the tab's own name already carries it in full. */}
                  <FadeOverflow as="span" className="min-w-0" titleWhenClipped={t(label)}>{t(label)}</FadeOverflow>
                  {searching && <span className="ms-auto flex shrink-0"><CountBadge count={countOf(id)} /></span>}
                </Tab>
              );
            })}
          </TabList>
        </TabListContainer>
        {tabIds.map((id) => {
          const { label, Content, stickyHeadingHeight } = SECTIONS[id];
          return (
            <SectionPanel key={id} id={id} stickyHeadingHeight={stickyHeadingHeight} hidden={nothingFound}>
              <h2 className="pb-2 text-xl font-semibold">{t(label)}</h2>
              <SettingsFocusContext.Provider value={focusTarget}>
                <AccountsNavigationContext.Provider value={accountsNavigation}>
                  <SettingsSearchProvider matches={matches.get(id)}>
                    <Content />
                  </SettingsSearchProvider>
                </AccountsNavigationContext.Provider>
              </SettingsFocusContext.Provider>
            </SectionPanel>
          );
        })}
      </Tabs>
    </Modal>
  );
}
