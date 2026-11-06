import { Modal, Tabs } from "@heroui/react";
import { Bell, FileCog, FolderCheck, GitBranch, Info, Keyboard, KeyRound, Settings2, ShieldCheck, ShieldUser, type LucideIcon } from "lucide-react";
import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { TitledControl } from "../components/hint/TitledControl";
import { openingFocus } from "../components/focus/modalFocus";
import { FadedScroller } from "../components/scroll/scrollFade";
import { STICKY_HEADING_HEIGHT } from "../components/scroll/StickyHeading";
import { LocalizedTabListContainer } from "../components/tabs/LocalizedTabListContainer";
import { dialogAround } from "../components/dialog/dialogElement";
import { tryEachFrame } from "../components/focus/tryEachFrame";
import { useEscapeWhileTooltipOpen } from "../components/dialog/useEscapeWhileTooltipOpen";
import type { PlainMessageKey } from "../i18n/catalog";
import { useT } from "../i18n/react";
import { useIsNarrow } from "../components/hooks/breakpoint";
import { AboutSection } from "./AboutSection";
import { AccountsNavigationContext } from "./accounts/AccountsNavigationContext";
import { AgentAccountsSection, AGENT_STRIP_HEIGHT } from "./accounts/AgentAccountsSection";
import { AgentBehaviorSection } from "./AgentBehaviorSection";
import { ConfigurationSection } from "./ConfigurationSection";
import { GeneralSection } from "./GeneralSection";
import { GitSection } from "./GitSection";
import { NotificationsSection } from "./NotificationsSection";
import { PermissionsSection } from "./PermissionsSection";
import { ShortcutsSection } from "./ShortcutsSection";
import { SETTINGS_SECTION_IDS, type SettingsNavigation, type SettingsSectionId } from "./sections";
import { SettingsFocusContext } from "./useSectionRefocus";
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
  general: { label: "settings.section.general", Icon: Settings2, Content: GeneralSection },
  git: { label: "settings.section.git", Icon: GitBranch, Content: GitSection, stickyHeadingHeight: STICKY_HEADING_HEIGHT.group },
  accounts: { label: "settings.section.accounts", Icon: KeyRound, Content: AgentAccountsSection, stickyHeadingHeight: AGENT_STRIP_HEIGHT },
  "agent-permissions": { label: "settings.section.agentPermissions", Icon: ShieldUser, Content: AgentBehaviorSection },
  "trusted-folders": { label: "settings.section.trustedFolders", Icon: FolderCheck, Content: TrustedFoldersSection },
  permissions: { label: "settings.section.permissions", Icon: ShieldCheck, Content: PermissionsSection },
  notifications: { label: "settings.section.notifications", Icon: Bell, Content: NotificationsSection },
  shortcuts: { label: "settings.section.shortcuts", Icon: Keyboard, Content: ShortcutsSection, stickyHeadingHeight: STICKY_HEADING_HEIGHT.group },
  configuration: { label: "settings.section.configuration", Icon: FileCog, Content: ConfigurationSection },
  about: { label: "settings.section.about", Icon: Info, Content: AboutSection },
};

/** How many animation frames (about half a second) a tab waits for a modal in front of Settings to go before it
 * gives up taking focus. */
const MAX_FRAMES_TO_WAIT_FOR_MODAL = 30;

/** A section's panel. The section scrolls on its own inside the dialog and fades at the edge it can
 * still be scrolled towards, as the body of a `Dialog` does. The scroller sits inside the panel, not
 * on it, because the panel has an exit transition. Its vertical padding is `--section-padding`, which
 * a sticky heading takes negated as its `top` to pin flush with the scroller's edge rather than the
 * padding's; `stickyHeadingHeight` is the height of such a heading (see `FadedScroller`). */
function SectionPanel({ id, stickyHeadingHeight, children }: { id: string; stickyHeadingHeight?: number; children: React.ReactNode }): React.ReactElement {
  return (
    <Tabs.Panel id={id} className="flex min-h-0 min-w-0 flex-1 flex-col max-tablet:p-0">
      <FadedScroller
        stickyHeadingHeight={stickyHeadingHeight}
        stickySurface="overlay"
        className="min-h-0 flex-1 px-4 py-(--section-padding) [--section-padding:calc(var(--spacing)*4)] max-phone:px-3 max-phone:[--section-padding:calc(var(--spacing)*3)] desktop:px-8 desktop:[--section-padding:calc(var(--spacing)*6)]"
      >{children}</FadedScroller>
    </Tabs.Panel>
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
  const isNarrow = useIsNarrow();
  const fullScreen = isNarrow;
  const [sectionId, setSectionId] = useState<string>(navigation?.section ?? initialSection);
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
    if (navigation.section) setSectionId(navigation.section);
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
    <Modal.Backdrop isOpen onOpenChange={(open) => !open && onClose()}>
      {/* Native full size removes narrow-screen margins and uses an edge-to-edge transition. Wider
          windows retain their inset layout; the container ends above the bottom notices. */}
      <Modal.Container size={fullScreen ? "full" : undefined} className={fullScreen ? undefined : "max-desktop:p-4"}>
        <Modal.Dialog
          aria-label={t("settings.title")}
          className={`control-fills flex flex-col overflow-hidden p-0 ${fullScreen ? "" : "h-[calc(100dvh-32px-var(--bottom-chrome-height))] w-[min(1000px,calc(100vw-32px))] max-w-none desktop:h-[min(85vh,calc(100vh-96px-var(--bottom-chrome-height)))] desktop:w-[min(1000px,calc(100vw-96px))] tablet:flex-row"}`}
        >
          <span ref={markerRef} hidden />
          <TitledControl title={t("common.close")}>
            {/* The panels' positioned scrollers overlap this corner. Keep the close button above them. */}
            <Modal.CloseTrigger aria-label={t("common.close")} className="z-10 max-phone:top-4.5 max-phone:end-3" />
          </TitledControl>
          <Tabs
            orientation={isNarrow ? "horizontal" : "vertical"}
            selectedKey={sectionId}
            onSelectionChange={(key) => setSectionId(String(key))}
            className="min-h-0 min-w-0 flex-1"
          >
            {/* A direct child of `Tabs`, as HeroUI's styling of it expects; it is the section column
                itself, with no rule between it and the section. Narrow, it is the row along the top
                instead, clear of the close button at its end and scrolling sideways on its own.
                `self-start` keeps the column at its own height rather than the dialog's, which is
                what bounds how many sections fit — see `SETTINGS_SECTION_IDS`. */}
            <LocalizedTabListContainer
              className="ms-4 me-12 mt-3 min-w-0 shrink-0 max-phone:ms-3 max-phone:mt-2 max-phone:max-w-[calc(100%-3.75rem)] max-tablet:w-max max-tablet:max-w-[calc(100%-4rem)] max-tablet:self-start tablet:my-4 tablet:me-0 tablet:w-48 tablet:self-start desktop:my-6"
            >
              {/* HeroUI sizes a vertical list to its widest tab; it fills the column here, and its
                  tabs read from the start like any list of names rather than centred. */}
              <Tabs.List aria-label={t("settings.sections")} className="max-tablet:min-w-0 max-tablet:p-1.5 tablet:w-full">
                {/* Hovered labels take the foreground colour for clear feedback. */}
                {SETTINGS_SECTION_IDS.map((id) => {
                  const { label, Icon } = SECTIONS[id];
                  return (
                    <Tabs.Tab
                      key={id}
                      id={id}
                      className="justify-start gap-2 whitespace-nowrap hover:text-foreground max-tablet:w-auto max-tablet:flex-none max-tablet:scroll-mx-1.5"
                      ref={id === sectionId ? attachSelected : undefined}
                      {...(id === sectionId && !sectionTakesFocus && openingFocus(true))}
                    >
                      <Icon aria-hidden="true" className="size-4 shrink-0" />
                      {t(label)}
                      <Tabs.Indicator />
                    </Tabs.Tab>
                  );
                })}
              </Tabs.List>
            </LocalizedTabListContainer>
            {SETTINGS_SECTION_IDS.map((id) => {
              const { label, Content, stickyHeadingHeight } = SECTIONS[id];
              return (
                <SectionPanel key={id} id={id} stickyHeadingHeight={stickyHeadingHeight}>
                  <h2 className="pb-2 text-xl font-semibold">{t(label)}</h2>
                  <SettingsFocusContext.Provider value={focusTarget}>
                    <AccountsNavigationContext.Provider value={accountsNavigation}>
                      <Content />
                    </AccountsNavigationContext.Provider>
                  </SettingsFocusContext.Provider>
                </SectionPanel>
              );
            })}
          </Tabs>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  );
}
