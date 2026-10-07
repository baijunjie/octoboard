import { Modal, Tabs } from "@heroui/react";
import { Bell, FolderCheck, GitBranch, Settings2, type LucideIcon } from "lucide-react";
import React, { useCallback, useRef, useState } from "react";

import { TitledControl } from "../components/TitledControl";
import type { PlainMessageKey } from "../i18n/catalog";
import { useT } from "../i18n/react";
import { GeneralSection } from "./GeneralSection";
import { GitSection } from "./GitSection";
import { NotificationsSection } from "./NotificationsSection";
import { SettingsFocusContext } from "./useSectionRefocus";
import { TrustedFoldersSection } from "./TrustedFoldersSection";

const SECTIONS: { id: string; label: PlainMessageKey; Icon: LucideIcon; Content: () => React.ReactElement }[] = [
  { id: "general", label: "settings.section.general", Icon: Settings2, Content: GeneralSection },
  { id: "git", label: "settings.section.git", Icon: GitBranch, Content: GitSection },
  { id: "trusted-folders", label: "settings.section.trustedFolders", Icon: FolderCheck, Content: TrustedFoldersSection },
  { id: "notifications", label: "settings.section.notifications", Icon: Bell, Content: NotificationsSection },
];

/**
 * The settings: a large modal over the whole window, the sections as vertical tabs on the start
 * side and the selected one beside it. It is a plain modal, so everything under it, the terminal
 * included, stays mounted and sized. Escape, the close button and a click on the backdrop call
 * `onClose`; opening moves focus to the selected section's entry (also where a section's
 * `useSectionRefocus` sends it back to), and `useSettingsDialog` puts it back on the terminal when
 * this closes.
 */
export function SettingsDialog({ onClose }: { onClose: () => void }): React.ReactElement {
  const t = useT();
  const [sectionId, setSectionId] = useState(SECTIONS[0].id);
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
  }, []);

  // Where a section that loses focus (see `useSectionRefocus`) sends it back: the selected tab.
  const focusTarget = useCallback(() => selectedRef.current, []);

  return (
    <Modal.Backdrop isOpen onOpenChange={(open) => !open && onClose()}>
      <Modal.Container>
        <Modal.Dialog
          aria-label={t("settings.title")}
          className="flex h-[min(85vh,calc(100vh-96px))] w-[min(1000px,calc(100vw-96px))] max-w-none flex-row overflow-hidden p-0"
        >
          <TitledControl title={t("common.close")}>
            <Modal.CloseTrigger />
          </TitledControl>
          <Tabs
            orientation="vertical"
            selectedKey={sectionId}
            onSelectionChange={(key) => setSectionId(String(key))}
            className="min-w-0 flex-1"
          >
            {/* A direct child of `Tabs`, as HeroUI's styling of it expects; it is the section column
                itself, with no rule between it and the section. */}
            <Tabs.ListContainer className="my-6 ms-4 w-48 shrink-0 self-start">
              {/* HeroUI sizes a vertical list to its widest tab; it fills the column here, and its
                  tabs read from the start like any list of names rather than centred. */}
              <Tabs.List aria-label={t("settings.sections")} className="w-full">
                {SECTIONS.map(({ id, label, Icon }) => (
                  // HeroUI dims a hovered tab to 70%, which takes its muted label under WCAG AA's
                  // 4.5:1; the hover darkens it instead.
                  <Tabs.Tab
                    key={id}
                    id={id}
                    className="justify-start gap-2 hover:text-foreground hover:opacity-100"
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
              <Tabs.Panel key={id} id={id} className="min-w-0 flex-1 overflow-y-auto px-8 py-6">
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
