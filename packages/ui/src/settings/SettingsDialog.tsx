import { Modal, Tabs } from "@heroui/react";
import { Bell, FolderCheck, GitBranch, KeyRound, Settings2, type LucideIcon } from "lucide-react";
import React, { useCallback, useRef, useState } from "react";

import { TitledControl } from "../components/TitledControl";
import type { PlainMessageKey } from "../i18n/catalog";
import { useT } from "../i18n/react";
import { useIsNarrow } from "../layout/breakpoint";
import { AgentAccountsSection } from "./AgentAccountsSection";
import { GeneralSection } from "./GeneralSection";
import { GitSection } from "./GitSection";
import { NotificationsSection } from "./NotificationsSection";
import { SettingsFocusContext } from "./useSectionRefocus";
import { TrustedFoldersSection } from "./TrustedFoldersSection";

const SECTIONS = [
  { id: "general", label: "settings.section.general", Icon: Settings2, Content: GeneralSection },
  { id: "git", label: "settings.section.git", Icon: GitBranch, Content: GitSection },
  { id: "accounts", label: "settings.section.accounts", Icon: KeyRound, Content: AgentAccountsSection },
  { id: "trusted-folders", label: "settings.section.trustedFolders", Icon: FolderCheck, Content: TrustedFoldersSection },
  { id: "notifications", label: "settings.section.notifications", Icon: Bell, Content: NotificationsSection },
] as const satisfies readonly { id: string; label: PlainMessageKey; Icon: LucideIcon; Content: () => React.ReactElement }[];

export type SettingsSectionId = (typeof SECTIONS)[number]["id"];

/**
 * The settings: a large modal over the whole window, the sections as vertical tabs on the start
 * side and the selected one beside it. Below the `docked` breakpoint the modal fills the window
 * and the sections become a row of tabs along the top, scrolling sideways when they overflow. It
 * is a plain modal, so everything under it, the terminal included, stays mounted and sized.
 * Escape, the close button and a click on the backdrop call `onClose`; opening moves focus to the
 * selected section's entry (also where a section's `useSectionRefocus` sends it back to), and
 * `useSettingsDialog` puts it back on the terminal when this closes. It opens on `initialSection`
 * when given, the first section otherwise.
 */
export function SettingsDialog({
  initialSection = SECTIONS[0].id,
  onClose,
}: {
  /** The section to open on. A typo must not compile: a key that names no section renders no panel
   * and never focuses a tab, which would lose the dialog's keyboard rules. */
  initialSection?: SettingsSectionId;
  onClose: () => void;
}): React.ReactElement {
  const t = useT();
  const isNarrow = useIsNarrow();
  const [sectionId, setSectionId] = useState<string>(initialSection);
  const selectedRef = useRef<HTMLDivElement | null>(null);
  const focusedOnOpen = useRef(false);

  // HeroUI's tabs build their collection in a first pass before the tab elements exist, so the
  // selected tab is not in `selectedRef` yet when an effect on mount runs. Focus it as it attaches.
  const attachSelected = useCallback((tab: HTMLDivElement | null) => {
    selectedRef.current = tab;
    if (tab && !focusedOnOpen.current) {
      focusedOnOpen.current = true;
      tab.focus();
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
          16px margin on every side instead, so the padding stays 16px until `docked`. The height
          also gives way to the connection banner's strip, which the container ends above (see the
          same note in `dialogs/Dialog.tsx`). */}
      <Modal.Container className="sm:p-4 docked:p-10">
        <Modal.Dialog
          aria-label={t("settings.title")}
          className="flex h-[calc(100dvh-32px-var(--bottom-chrome-height))] w-[calc(100vw-32px)] max-w-none flex-col overflow-hidden p-0 docked:h-[min(85vh,calc(100vh-96px-var(--bottom-chrome-height)))] docked:w-[min(1000px,calc(100vw-96px))] docked:flex-row"
        >
          <TitledControl title={t("common.close")}>
            <Modal.CloseTrigger aria-label={t("common.close")} />
          </TitledControl>
          <Tabs
            orientation={isNarrow ? "horizontal" : "vertical"}
            selectedKey={sectionId}
            onSelectionChange={(key) => setSectionId(String(key))}
            className="min-w-0 flex-1"
          >
            {/* A direct child of `Tabs`, as HeroUI's styling of it expects; it is the section column
                itself, with no rule between it and the section. Narrow, it is the row along the top
                instead, clear of the close button at its end and scrolling sideways on its own. */}
            <Tabs.ListContainer className="ms-4 me-12 mt-3 min-w-0 shrink-0 docked:my-6 docked:me-0 docked:w-48 docked:self-start">
              {/* HeroUI sizes a vertical list to its widest tab; it fills the column here, and its
                  tabs read from the start like any list of names rather than centred. */}
              <Tabs.List aria-label={t("settings.sections")} className="docked:w-full">
                {SECTIONS.map(({ id, label, Icon }) => (
                  // HeroUI dims a hovered tab to 70%, which takes its muted label under WCAG AA's
                  // 4.5:1; the hover darkens it instead.
                  <Tabs.Tab
                    key={id}
                    id={id}
                    className="justify-start gap-2 whitespace-nowrap hover:text-foreground hover:opacity-100"
                    ref={id === sectionId ? attachSelected : undefined}
                  >
                    <Icon aria-hidden="true" className="size-4 shrink-0" />
                    {t(label)}
                    <Tabs.Indicator />
                  </Tabs.Tab>
                ))}
              </Tabs.List>
            </Tabs.ListContainer>
            {SECTIONS.map(({ id, label, Content }) => (
              <Tabs.Panel key={id} id={id} className="min-h-0 min-w-0 flex-1 overflow-y-auto px-4 py-4 docked:px-8 docked:py-6">
                <h2 className="pb-2 text-xl font-semibold">{t(label)}</h2>
                <SettingsFocusContext.Provider value={focusTarget}>
                  <Content />
                </SettingsFocusContext.Provider>
              </Tabs.Panel>
            ))}
          </Tabs>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  );
}
