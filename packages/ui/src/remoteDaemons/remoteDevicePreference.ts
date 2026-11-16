import { createPersistedPreference } from "../preferences/persistedPreference";
import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";

/** The id of the remote device the Remote devices page last showed, which it opens on again while that device is still
 * saved. Kept per daemon, as the window's other view state is. */
export const remoteDevicePreference = createPersistedPreference<string | undefined>(
  PREFERENCE_KEYS.remoteDevice,
  (raw) => raw || undefined,
  (value) => value ?? null,
  { scope: "daemon" },
);
