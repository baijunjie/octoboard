import { useEffect } from "react";

import { useLatest } from "../components/hooks/useLatest";
import { useT } from "../i18n/react";
import { useDaemon } from "../store";
import { endEdit, sessionsEditing } from "./composerActions";
import { sessionComposerEnabled } from "./composerSetting";

/** Ends every edit of a queued message under way in this window as the composer is turned off, whether here or in
 * another window of the application (the switch is shared through the preference's storage), so a message an edit held
 * back as manual gets its mode back rather than waiting unseen. Mounted once per window. */
export function useEndEditsWhenOff(): void {
  const enabled = sessionComposerEnabled.useValue();
  const { store, request } = useDaemon();
  const t = useT();
  const context = useLatest({ store, request, t });
  useEffect(() => {
    if (enabled) return;
    for (const session of sessionsEditing(context.current.store)) endEdit(context.current, session);
  }, [enabled]);
}
