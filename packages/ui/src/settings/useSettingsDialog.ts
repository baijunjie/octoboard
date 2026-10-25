import { useEffect, useRef, useState } from "react";

import { isActionMenuOpen } from "../components/ActionMenu";
import { useLatest } from "../components/useLatest";
import { focusOpenDialog } from "../components/regionFocus";
import { usePlatform } from "../platform/react";
import type { SettingsSectionId } from "./SettingsDialog";

/**
 * Whether the settings dialog is open, and how to open it (on its first section, or at a given one with
 * `openSettingsAt`, optionally with the Add agent dialog in front, or on an account's sharing switch) and close it,
 * including from the native menu's Settings item (Cmd+,). That request is ignored while anything else holds the user's
 * attention: another modal (`otherModalOpen`), an open action menu, the settings themselves, or before the UI has
 * anything to show (`ready` is false), where it would otherwise pop the dialog up later, unasked. The native menu's
 * About item opens Settings on its About section the same way, and moves an open Settings there unless another modal or
 * an open action menu holds the user's attention. Closing hands keyboard focus to `focusTerminal`, whatever the modal's
 * own focus restore would pick, unless a dialog is open under it (Settings was opened from one): focus then stays with
 * that dialog, which it is handed back to if the restore left it on `<body>`.
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
  /** What the dialog was last asked for: the section (none for its first), whether to put its Add agent dialog in
   * front, and the account (if any) whose sharing switch to focus. `nonce` changes with every request, so an open
   * dialog can follow a later one. */
  settingsNavigation: {
    nonce: number;
    section: SettingsSectionId | undefined;
    addAgent: boolean;
    account: string | undefined;
  };
  openSettings: () => void;
  openSettingsAt: (section: SettingsSectionId, options?: { addAgent?: boolean; account?: string }) => void;
  closeSettings: () => void;
} {
  const [open, setOpen] = useState(false);
  const [section, setSection] = useState<SettingsSectionId>();
  const [addAgent, setAddAgent] = useState(false);
  const [account, setAccount] = useState<string>();
  const [nonce, setNonce] = useState(0);
  const { appMenu } = usePlatform();

  // Not on first render, and not when opening.
  const wasOpenRef = useRef(false);
  useEffect(() => {
    if (wasOpenRef.current && !open) {
      if (!otherModalOpen) focusTerminal();
      else {
        // A dialog is open under Settings: React Aria's focus restore (a frame later) puts focus back on the
        // control that opened Settings, and only a focus it left on `<body>` is handed to that dialog, after it.
        requestAnimationFrame(() => {
          if (!document.activeElement || document.activeElement === document.body) focusOpenDialog();
        });
      }
    }
    wasOpenRef.current = open;
  }, [open]);

  // Read through a ref so the subscription is made once.
  const requestFromMenuRef = useLatest(() => {
    if (ready && !open && !otherModalOpen && !isActionMenuOpen()) setOpen(true);
  });
  useEffect(() => appMenu?.onSettingsRequested(() => requestFromMenuRef.current()), [appMenu]);
  const requestAboutFromMenuRef = useLatest(() => {
    if (ready && !otherModalOpen && !isActionMenuOpen()) openSettingsAt("about");
  });
  useEffect(() => appMenu?.onAboutRequested?.(() => requestAboutFromMenuRef.current()), [appMenu]);

  function openSettingsAt(target: SettingsSectionId, options?: { addAgent?: boolean; account?: string }): void {
    setSection(target);
    setAddAgent(options?.addAgent ?? false);
    setAccount(options?.account);
    setNonce((count) => count + 1);
    setOpen(true);
  }

  return {
    settingsOpen: open,
    settingsNavigation: { nonce, section, addAgent, account },
    openSettings: () => setOpen(true),
    openSettingsAt,
    closeSettings: () => {
      setSection(undefined);
      setAddAgent(false);
      setAccount(undefined);
      setOpen(false);
    },
  };
}
