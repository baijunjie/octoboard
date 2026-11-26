import { createPersistedPreference } from "../preferences/persistedPreference";
import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";

/** Whether the message composer shows below the selected session's terminal: the user's choice in Settings, off by
 * default and kept by this client for itself in its `localStorage` (the application's, so every window of the desktop
 * application follows it), never by the daemon: the phone may use the composer while the Mac uses the terminal. */
export const sessionComposerEnabled = createPersistedPreference<boolean>(
  PREFERENCE_KEYS.sessionComposer,
  (raw) => raw === "true",
  (value) => (value ? "true" : null),
);
