import { Modal, Tabs } from "@heroui/react";
import { Bell, FileCog, FolderCheck, GitBranch, Keyboard, KeyRound, Settings2, ShieldCheck, UserCheck, type LucideIcon } from "lucide-react";
import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { TitledControl } from "../components/TitledControl";
import { FadedScroller } from "../components/scrollFade";
import { useTabScrollLabels } from "../components/tabScrollLabels";
import { dialogAround } from "../dialogs/dialogElement";
import { useEscapeWhileTooltipOpen } from "../dialogs/useEscapeWhileTooltipOpen";
import type { PlainMessageKey } from "../i18n/catalog";
import { useT } from "../i18n/react";
import { useIsNarrow } from "../layout/breakpoint";
import { AccountsNavigationContext } from "./AccountsNavigationContext";
import { AgentAccountsSection, AGENT_STRIP_HEIGHT } from "./AgentAccountsSection";
import { AgentPermissionsSection } from "./AgentPermissionsSection";
import { ConfigurationSection } from "./ConfigurationSection";
import { GeneralSection } from "./GeneralSection";
import { GitSection } from "./GitSection";
import { NotificationsSection } from "./NotificationsSection";
import { PermissionsSection } from "./PermissionsSection";
import { SHORTCUT_GROUP_HEADING_HEIGHT, ShortcutsSection } from "./ShortcutsSection";
import { SettingsFocusContext } from "./useSectionRefocus";
import { TrustedFoldersSection } from "./TrustedFoldersSection";

interface Section {
  id: string;
  label: PlainMessageKey;
  Icon: LucideIcon;
  Content: () => React.ReactElement;
  /** The height of a heading the section pins to the top of its scroller, if it has one. */
  stickyHeadingHeight?: number;
}

const SECTIONS = [
  { id: "general", label: "settings.section.general", Icon: Settings2, Content: GeneralSection },
  { id: "git", label: "settings.section.git", Icon: GitBranch, Content: GitSection },
  { id: "accounts", label: "settings.section.accounts", Icon: KeyRound, Content: AgentAccountsSection, stickyHeadingHeight: AGENT_STRIP_HEIGHT },
  { id: "trusted-folders", label: "settings.section.trustedFolders", Icon: FolderCheck, Content: TrustedFoldersSection },
  { id: "permissions", label: "settings.section.permissions", Icon: ShieldCheck, Content: PermissionsSection },
  { id: "agent-permissions", label: "settings.section.agentPermissions", Icon: UserCheck, Content: AgentPermissionsSection },
  { id: "notifications", label: "settings.section.notifications", Icon: Bell, Content: NotificationsSection },
  { id: "shortcuts", label: "settings.section.shortcuts", Icon: Keyboard, Content: ShortcutsSection, stickyHeadingHeight: SHORTCUT_GROUP_HEADING_HEIGHT },
  { id: "configuration", label: "settings.section.configuration", Icon: FileCog, Content: ConfigurationSection },
] as const satisfies readonly Section[];

/** How many animation frames (about half a second) a tab waits for a modal in front of Settings to go before it
 * gives up taking focus. */
const MAX_FRAMES_TO_WAIT_FOR_MODAL = 30;

export type SettingsSectionId = (typeof SECTIONS)[number]["id"];

/** A section's panel. The section scrolls on its own inside the dialog and fades at the edge it can
 * still be scrolled towards, as the body of a `Dialog` does. The scroller sits inside the panel, not
 * on it, because the panel has an exit transition. Its vertical padding is `--section-padding`, which
 * a sticky heading takes negated as its `top` to pin flush with the scroller's edge rather than the
 * padding's; `stickyHeadingHeight` is the height of such a heading (see `FadedScroller`). */
function SectionPanel({ id, stickyHeadingHeight, children }: { id: string; stickyHeadingHeight?: number; children: React.ReactNode }): React.ReactElement {
  return (
    <Tabs.Panel id={id} className="flex min-h-0 min-w-0 flex-1 flex-col">
      <FadedScroller
        stickyHeadingHeight={stickyHeadingHeight}
        stickySurface="overlay"
        className="min-h-0 flex-1 px-4 py-(--section-padding) [--section-padding:calc(var(--spacing)*4)] docked:px-8 docked:[--section-padding:calc(var(--spacing)*6)]"
      >{children}</FadedScroller>
    </Tabs.Panel>
  );
}

/**
 * The settings: a large modal over the whole window, the sections as vertical tabs on the start
 * side and the selected one beside it. Below the `docked` breakpoint the modal fills the window
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
  initialSection = SECTIONS[0].id,
  navigation,
  onClose,
}: {
  /** The section to open on, for a dialog that asks for no `navigation`. A typo must not compile: a key that
   * names no section renders no panel and never focuses a tab, which would lose the dialog's keyboard rules. */
  initialSection?: SettingsSectionId;
  /** What the dialog is asked to show, first on opening and again by a request made while it is open (each has
   * its own `nonce`): a section, the Add agent dialog in front for `addAgent`, and an account's sharing switch to
   * focus for `account`. */
  navigation?: { nonce: number; section: SettingsSectionId | undefined; addAgent: boolean; account?: string };
  onClose: () => void;
}): React.ReactElement {
  const t = useT();
  const tabScrollLabels = useTabScrollLabels();
  const isNarrow = useIsNarrow();
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
    let frames = 0;
    let frame = 0;
    const focusSelected = () => {
      const settings = dialogAround(markerRef);
      if (!settings) return;
      if (settings.closest('[inert], [aria-hidden="true"]')) {
        if (frames++ < MAX_FRAMES_TO_WAIT_FOR_MODAL) frame = requestAnimationFrame(focusSelected);
        return;
      }
      settings.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]')?.focus();
    };
    frame = requestAnimationFrame(focusSelected);
    return () => cancelAnimationFrame(frame);
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
  }, []);

  // Where a section that loses focus (see `useSectionRefocus`) sends it back: the selected tab.
  const focusTarget = useCallback(() => selectedRef.current, []);

  return (
    <Modal.Backdrop isOpen onOpenChange={(open) => !open && onClose()}>
      {/* HeroUI pads the container 40px from `sm` up; narrow, the dialog fills the window inside a
          16px margin on every side instead, so the padding stays 16px until `docked`. It is a
          `max-docked:` utility because a `docked:` one loses to an `sm:` twin on the same property.
          The height also gives way to the connection banner's strip, which the container ends above.
          Narrow, these are the same rules `dialogs/Dialog.tsx` states for a dialog that fills the
          window, down to the numbers, so the two move together. */}
      <Modal.Container className="max-docked:p-4">
        <Modal.Dialog
          aria-label={t("settings.title")}
          className="control-fills flex h-[calc(100dvh-32px-var(--bottom-chrome-height))] w-[calc(100vw-32px)] max-w-none flex-col overflow-hidden p-0 docked:h-[min(85vh,calc(100vh-96px-var(--bottom-chrome-height)))] docked:w-[min(1000px,calc(100vw-96px))] docked:flex-row"
        >
          <span ref={markerRef} hidden />
          <TitledControl title={t("common.close")}>
            {/* The panels' positioned scrollers overlap this corner. Keep the close button above them. */}
            <Modal.CloseTrigger aria-label={t("common.close")} className="z-10" />
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
                what bounds how many sections fit — see `SECTIONS`. */}
            <Tabs.ListContainer
              className="ms-4 me-12 mt-3 min-w-0 shrink-0 max-docked:w-max max-docked:max-w-[calc(100%-4rem)] max-docked:self-start docked:my-6 docked:me-0 docked:w-48 docked:self-start"
              render={(props) => <div {...props}>{tabScrollLabels(props.children)}</div>}
            >
              {/* HeroUI sizes a vertical list to its widest tab; it fills the column here, and its
                  tabs read from the start like any list of names rather than centred. */}
              <Tabs.List aria-label={t("settings.sections")} className="max-docked:min-w-0 max-docked:p-1.5 docked:w-full">
                {/* Keep hovered labels at full opacity and use the foreground colour for clear feedback. */}
                {SECTIONS.map(({ id, label, Icon }) => (
                  <Tabs.Tab
                    key={id}
                    id={id}
                    className="justify-start gap-2 whitespace-nowrap hover:text-foreground hover:opacity-100 max-docked:w-auto max-docked:flex-none max-docked:scroll-mx-1.5"
                    ref={id === sectionId ? attachSelected : undefined}
                  >
                    <Icon aria-hidden="true" className="size-4 shrink-0" />
                    {t(label)}
                    <Tabs.Indicator />
                  </Tabs.Tab>
                ))}
              </Tabs.List>
            </Tabs.ListContainer>
            {SECTIONS.map(({ id, label, Content, ...section }) => (
              <SectionPanel key={id} id={id} stickyHeadingHeight={"stickyHeadingHeight" in section ? section.stickyHeadingHeight : undefined}>
                <h2 className="pb-2 text-xl font-semibold">{t(label)}</h2>
                <SettingsFocusContext.Provider value={focusTarget}>
                  <AccountsNavigationContext.Provider value={accountsNavigation}>
                    <Content />
                  </AccountsNavigationContext.Provider>
                </SettingsFocusContext.Provider>
              </SectionPanel>
            ))}
          </Tabs>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  );
}
