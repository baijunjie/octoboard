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
 * the window; wide, the field over the tabs in a column on the start side and the panel beside both. */
const SEARCH_AREA = "max-tablet:row-start-1 tablet:col-start-1 tablet:row-start-1";
const TABS_AREA = "max-tablet:row-start-2 tablet:col-start-1 tablet:row-start-2";
const PANEL_AREA = "max-tablet:row-start-3 tablet:col-start-2 tablet:row-span-2 tablet:row-start-1";

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
 * The settings: a large modal over the whole window, the sections as vertical tabs on the start
 * side and the selected one beside it. At the `tablet` breakpoint and below the modal fills the window
 * and the sections become a row of tabs along the top, scrolling sideways when they overflow. It
 * is a plain modal, so everything under it, the terminal included, stays mounted and sized.
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
 * `useSettingsQuery`): while it has words in it, only the sections with a match are listed, each with how many, the
 * selected one (moved to the first listed when it has none) shows only its matching rows, and with no match anywhere
 * an empty state takes the place of the tabs' panel, which stays mounted, hidden with the tabs, so that what was
 * typed in it survives. Tabs and rows that a search takes away or brings back move with the shared list motion.
 * Escape in the field clears it before it closes the dialog, and clearing keeps the section on screen. A navigation
 * request clears it, so the section it asks for is shown whole; one that names no section keeps the section on
 * screen. Find (`FIND_BINDING`) focuses the field and selects its text.
 */
export function SettingsDialog({
  initialSection = SETTINGS_SECTION_IDS[0],
  navigation,
  onClose,
}: {
  /** The section to open on, for a dialog that asks for no `navigation`. A typo must not compile: a key that
   * names no section renders no panel and never focuses a tab, which would lose the dialog's keyboard rules. */
  initialSection?: SettingsSectionId;
  /** What the dialog is asked to show, first on opening and again by a request made while it is open (each has
   * its own `nonce`): a section, the Add agent dialog in front for `addAgent`, and an account's sharing switch to
   * focus for `account`. */
  navigation?: SettingsNavigation;
  onClose: () => void;
}): React.ReactElement {
  const t = useT();
  const host = useHost();
  // The privacy grants are the system's own on macOS alone: managed in the app there (`permissionsPane`), and in a
  // browser on a Mac the section says they are managed in the app.
  const sectionIds = SETTINGS_SECTION_IDS.filter((id) => id !== "permissions" || host.permissionsPane || host.os === "macos");
  const isNarrow = useIsNarrow();
  const fullScreen = isNarrow;
  const [sectionId, setSectionId] = useState<SettingsSectionId>(navigation?.section ?? initialSection);
  const search = useSettingsQuery({ sectionIds, sectionLabel: (id) => SECTIONS[id].label, selectedId: sectionId, select: setSectionId });
  const { matches, countOf, listedIds, shownId } = search;
  const searching = search.words.length > 0;
  // While nothing matches anywhere, the section on screen keeps its tab and panel, hidden, rather than unmounting them.
  const nothingFound = listedIds.length === 0;
  const tabIds = nothingFound ? [shownId] : listedIds;
  // Tabs that come and go slide along the column, whose height follows them so that the copies of those leaving stay
  // inside it rather than overflowing it (which would show its scroll button), or along the strip that is as wide as
  // its tabs when narrow.
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
    setSectionId(navigation.section ?? shownId);
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
    // The row of tabs below the breakpoint scrolls, so a section picked from a link may start off
    // screen. Only the tab row itself moves (`block: "nearest"` leaves the dialog where it is).
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
      className={`control-fills grid overflow-hidden p-0 ${fullScreen ? "grid-cols-[minmax(0,1fr)] grid-rows-[auto_auto_minmax(0,1fr)] gap-y-2" : "h-[calc(100dvh-32px-var(--bottom-chrome-height))] w-[min(1000px,calc(100vw-32px))] max-w-none grid-cols-[auto_minmax(0,1fr)] grid-rows-[auto_minmax(0,1fr)] gap-x-2 desktop:h-[min(85vh,calc(100vh-96px-var(--bottom-chrome-height)))] desktop:w-[min(1000px,calc(100vw-96px))]"}`}
    >
      <span ref={markerRef} hidden />
      <TitledControl title={t("common.close")}>
        {/* The panels' positioned scrollers overlap this corner. Keep the close button above them. */}
        <ModalCloseButton label={t("common.close")} data-settings-close className="z-10 max-phone:top-4.5 max-phone:end-3" />
      </TitledControl>
      {/* Outside `Tabs`, whose content react-aria also renders in a hidden copy, and before the tabs in the Tab order.
          Narrow, it is the row along the top, clear of the close button at its end. */}
      <div className={`min-w-0 ms-4 me-12 mt-3 max-phone:ms-3 max-phone:mt-2 tablet:me-0 tablet:mt-4 tablet:w-48 desktop:mt-6 ${SEARCH_AREA}`}>
        <SearchField
          label={t("settings.search.label")}
          inputRef={searchRef}
          {...search.queryField}
          fullWidth
          variant="secondary"
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
        onSelectionChange={(key) => setSectionId(key as SettingsSectionId)}
        className="contents"
      >
        {/* A direct child of `Tabs`, as HeroUI's styling of it expects; it is the section column
                itself, with no rule between it and the section. Narrow, it is the row under the search
                field instead, scrolling sideways on its own. `self-start` keeps the column at its own
                height rather than the dialog's, which is what bounds how many sections fit — see
                `SETTINGS_SECTION_IDS`. */}
        <TabListContainer
          hidden={nothingFound}
          className={`min-w-0 ms-4 max-phone:ms-3 max-phone:max-w-[calc(100%-1.5rem)] max-tablet:w-max max-tablet:max-w-[calc(100%-2rem)] max-tablet:self-start max-tablet:justify-self-start tablet:mt-2 tablet:mb-4 tablet:w-48 tablet:self-start desktop:mb-6 ${TABS_AREA}`}
        >
          {/* HeroUI sizes a vertical list to its widest tab; it fills the column here, and its
                  tabs read from the start like any list of names rather than centred. It is positioned, so the
                  copy of a tab that leaves is placed in it. */}
          <TabList ref={tabListRef} label={t("settings.sections")} className="relative max-tablet:min-w-0 max-tablet:p-1.5 tablet:w-full">
            {/* Hovered labels take the foreground colour for clear feedback. While a search filters the list, the
                selected tab draws HeroUI's indicator fill itself instead of the sliding indicator: a tab the search
                brings back can be selected in the same render, which leaves that indicator's move stuck part-way, and
                so can the motion of the tabs. The indicator stays mounted meanwhile, unseen and with no transition,
                so that it neither slides in from where it was before the search (the move that would get stuck)
                nor keeps a place to slide from: once the search is cleared it is simply shown in its tab. This is the
                exception that keeps the selected tab filled while list motion moves tabs, recorded in
                docs/ui-standard.md's "Conformance level and exceptions" section. */}
            {tabIds.map((id) => {
              const { label, Icon } = SECTIONS[id];
              return (
                <Tab
                  key={id}
                  id={id}
                  data-list-motion={id}
                  label={searching ? t("settings.search.sectionMatches", { section: t(label), count: countOf(id) }) : undefined}
                  indicatorClassName={searching ? "opacity-0 transition-none" : undefined}
                  className={`justify-start gap-2 whitespace-nowrap hover:text-foreground flex-none max-tablet:w-auto max-tablet:scroll-mx-1.5 ${searching ? "ui-selected:bg-segment ui-selected:shadow-surface" : ""}`}
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
