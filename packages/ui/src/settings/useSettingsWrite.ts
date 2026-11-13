import { useState } from "react";

import { alertAppDataFailure } from "../components/alerts/raiseError";
import { useLatest } from "../components/hooks/useLatest";
import { useT } from "../i18n/react";
import { createSettingsWriter, useDaemon, type SettingsChanges } from "../store";

/**
 * Sends the write of a settings control that changes `paths` (see `createSettingsWriter`, which holds the rules of a
 * checked write: based on the version the store holds of each path, and a press dropped while the store still holds the
 * version this window last wrote from). A failure is reported as the alert of a setting that could not be saved.
 */
export function useSettingsWrite(): (changes: SettingsChanges, paths: readonly string[]) => void {
  const wording = useLatest(useT());
  const { store, request } = useDaemon();
  const [write] = useState(() => createSettingsWriter({ store, request, onFailure: (error) => alertAppDataFailure(wording.current("settings.saveFailedTitle"))(error) }));
  return (changes, paths) => void write(changes, paths);
}
