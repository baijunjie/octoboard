import { Modal } from "@heroui/react";
import { Bell, FolderCheck, Palette, type LucideIcon } from "lucide-react";
import React, { useCallback, useEffect, useRef, useState } from "react";

import { AppearanceSection } from "./AppearanceSection";
import { NotificationsSection } from "./NotificationsSection";
import { SettingsFocusContext } from "./useSectionRefocus";
import { TrustedFoldersSection } from "./TrustedFoldersSection";

const SECTIONS: { id: string; label: string; Icon: LucideIcon; Content: () => React.ReactElement }[] = [
  { id: "appearance", label: "Appearance", Icon: Palette, Content: AppearanceSection },
  { id: "trusted-folders", label: "Trusted folders", Icon: FolderCheck, Content: TrustedFoldersSection },
  { id: "notifications", label: "Notifications", Icon: Bell, Content: NotificationsSection },
];

/**
 * The settings: a large modal over the whole window, a list of sections on the left and the
 * selected one on the right. It is a plain modal, so everything under it, the terminal included,
 * stays mounted and sized. Escape, the close button and a click on the backdrop call `onClose`;
 * opening moves focus to the selected section's entry (also where a section's `useSectionRefocus`
 * sends it back to), and `useSettingsDialog` puts it back on the terminal when this closes.
 */
export function SettingsDialog({ onClose }: { onClose: () => void }): React.ReactElement {
  const [sectionId, setSectionId] = useState(SECTIONS[0].id);
  const selectedRef = useRef<HTMLButtonElement>(null);
  const section = SECTIONS.find((s) => s.id === sectionId) ?? SECTIONS[0];

  useEffect(() => {
    selectedRef.current?.focus();
  }, []);

  const focusTarget = useCallback(() => selectedRef.current, []);

  return (
    <Modal.Backdrop isOpen onOpenChange={(open) => !open && onClose()}>
      <Modal.Container>
        <Modal.Dialog
          aria-label="Settings"
          className="flex h-[min(85vh,calc(100vh-96px))] w-[min(1000px,calc(100vw-96px))] max-w-none flex-row overflow-hidden p-0"
        >
          <Modal.CloseTrigger />
          <nav
            aria-label="Settings sections"
            className="flex w-52 shrink-0 flex-col gap-1 border-r border-separator bg-default/50 p-3"
          >
            <div className="px-2 pb-1 text-xs font-medium text-muted">Settings</div>
            {SECTIONS.map(({ id, label, Icon }) => (
              <button
                key={id}
                ref={id === sectionId ? selectedRef : undefined}
                type="button"
                aria-current={id === sectionId ? "page" : undefined}
                className={`flex min-h-8 items-center gap-2 rounded-lg px-2 text-left text-sm outline-none hover:bg-default focus-visible:ring-2 focus-visible:ring-focus ${id === sectionId ? "bg-default" : ""}`}
                onClick={() => setSectionId(id)}
              >
                <Icon aria-hidden="true" className="size-4 shrink-0 text-muted" />
                {label}
              </button>
            ))}
          </nav>
          <div className="min-w-0 flex-1 overflow-y-auto px-8 py-6">
            <h2 className="pb-2 text-xl font-semibold">{section.label}</h2>
            <SettingsFocusContext.Provider value={focusTarget}>
              <section.Content />
            </SettingsFocusContext.Provider>
          </div>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  );
}
