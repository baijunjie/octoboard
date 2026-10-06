import { Modal, Tabs } from "@heroui/react";
import { Bell, FolderCheck, Settings2, type LucideIcon } from "lucide-react";
import React, { useCallback, useRef, useState } from "react";

import { TitledControl } from "../components/TitledControl";
import type { PlainMessageKey } from "../i18n/catalog";
import { useT } from "../i18n/react";
import { GeneralSection } from "./GeneralSection";
import { NotificationsSection } from "./NotificationsSection";
import { SettingsFocusContext } from "./useSectionRefocus";
import { TrustedFoldersSection } from "./TrustedFoldersSection";

const SECTIONS: { id: string; label: PlainMessageKey; Icon: LucideIcon; Content: () => React.ReactElement }[] = [
  { id: "general", label: "settings.section.general", Icon: Settings2, Content: GeneralSection },
  { id: "trusted-folders", label: "settings.section.trustedFolders", Icon: FolderCheck, Content: TrustedFoldersSection },
  { id: "notifications", label: "settings.section.notifications", Icon: Bell, Content: NotificationsSection },
];

/**
 * The settings: a large modal over the whole window, vertical tabs for the sections on the start side and
 * the selected one beside it. It is a plain modal, so everything under it, the terminal
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
            className="min-w-0 flex-1 gap-0"
          >
            <div className="flex w-52 shrink-0 flex-col gap-1 border-e border-separator p-3">
              <div className="px-5 pb-1 text-xs font-medium text-muted">{t("settings.title")}</div>
              {/* A sidebar list rather than HeroUI's segmented control: no list background, rows as
                  wide as the column, the selected one a flat fill. HeroUI dims a hovered tab to
                  70%, which takes its muted label below 4.5:1, so the hover darkens it instead. */}
              <Tabs.ListContainer className="rounded-none bg-transparent">
                <Tabs.List aria-label={t("settings.sections")} className="flex w-full">
                  {SECTIONS.map(({ id, label, Icon }) => (
                    <Tabs.Tab key={id} id={id} className="justify-start gap-2 rounded-lg text-start hover:text-foreground hover:opacity-100" ref={id === sectionId ? attachSelected : undefined}>
                      <Icon aria-hidden="true" className="size-4 shrink-0" />
                      {t(label)}
                      <Tabs.Indicator className="rounded-lg bg-default shadow-none dark:bg-segment" />
                    </Tabs.Tab>
                  ))}
                </Tabs.List>
              </Tabs.ListContainer>
            </div>
            {SECTIONS.map(({ id, label, Content }) => (
              <Tabs.Panel key={id} id={id} className="ms-0 min-w-0 flex-1 overflow-y-auto px-8 py-6">
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
