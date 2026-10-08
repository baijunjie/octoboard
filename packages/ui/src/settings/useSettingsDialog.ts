import { useEffect, useRef, useState } from "react";

import { isActionMenuOpen } from "../components/ActionMenu";
import { usePlatform } from "../platform/react";
import type { SettingsSectionId } from "./SettingsDialog";

/**
 * Whether the settings dialog is open, and how to open it (on its first section, or at a given one
 * with `openSettingsAt`) and close it, including from the native
 * menu's Settings item (Cmd+,). That request is ignored while anything else holds the user's
 * attention: another modal (`otherModalOpen`), an open action menu, the settings themselves, or
 * before the UI has anything to show (`ready` is false), where it would otherwise pop the dialog
 * up later, unasked. Closing hands keyboard focus to `focusTerminal`, whatever the modal's own
 * focus restore would pick.
 */
export function useSettingsDialog({
  ready,
  otherModalOpen,
  focusTerminal,
}: {
  ready: boolean;
  otherModalOpen: boolean;
  focusTerminal: () => void;
}): {
  settingsOpen: boolean;
  /** The section the dialog was asked to open on, none for its first. */
  settingsSection: SettingsSectionId | undefined;
  openSettings: () => void;
  openSettingsAt: (section: SettingsSectionId) => void;
  closeSettings: () => void;
} {
  const [open, setOpen] = useState(false);
  const [section, setSection] = useState<SettingsSectionId>();
  const { appMenu } = usePlatform();

  // Not on first render, and not when opening.
  const wasOpenRef = useRef(false);
  useEffect(() => {
    if (wasOpenRef.current && !open) focusTerminal();
    wasOpenRef.current = open;
  }, [open]);

  // Read through a ref so the subscription is made once.
  const requestFromMenuRef = useRef(() => {});
  requestFromMenuRef.current = () => {
    if (ready && !open && !otherModalOpen && !isActionMenuOpen()) setOpen(true);
  };
  useEffect(() => appMenu?.onSettingsRequested(() => requestFromMenuRef.current()), [appMenu]);

  return {
    settingsOpen: open,
    settingsSection: section,
    openSettings: () => setOpen(true),
    openSettingsAt: (target) => {
      setSection(target);
      setOpen(true);
    },
    closeSettings: () => {
      setSection(undefined);
      setOpen(false);
    },
  };
}
