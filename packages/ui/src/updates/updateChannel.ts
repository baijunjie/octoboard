import { createPersistedPreference } from "../preferences/persistedPreference";
import type { UpdateChannel } from "../platform";
import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";

/** The release stream the user chose in Settings, or `undefined` while they have chosen none: nothing stored, or
 * anything stored but `stable` or `beta`. Choosing either channel saves it, so a saved choice is kept whichever build
 * runs. Where nothing is saved the running build decides (`effectiveUpdateChannel`). */
export const updateChannel = createPersistedPreference<UpdateChannel | undefined>(
  PREFERENCE_KEYS.updateChannel,
  (raw) => (raw === "beta" || raw === "stable" ? raw : undefined),
  (value) => value ?? null,
);

/** Whether `version` is a prerelease: a version with a prerelease suffix (`0.1.0-beta.3`), build metadata aside. */
export function isPrerelease(version: string): boolean {
  return version.split("+")[0].includes("-");
}

/** The channel the application updates from: the one the user chose, else Beta on a prerelease build and Stable on
 * any other, so a beta build left alone keeps receiving betas. Stable while the running version is not yet known. */
export function effectiveUpdateChannel(saved: UpdateChannel | undefined, currentVersion: string | undefined): UpdateChannel {
  return saved ?? (currentVersion !== undefined && isPrerelease(currentVersion) ? "beta" : "stable");
}
