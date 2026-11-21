import type { PlainMessageKey } from "../i18n/catalog";
import type { RemoteAccessFailureReason } from "../protocol";
import { knownValues } from "../values/knownValue";

/** The client's own sentence for why remote access (the owner socket or pipe another computer's app connects through) is
 * not served, per `RemoteAccessFailure.reason`; a reason that only browser access has is left out. It speaks of another
 * computer's app, never of a browser. */
const REMOTE_FAILURE: Partial<Record<RemoteAccessFailureReason, PlainMessageKey>> = {
  no_ui: "settings.remoteAccess.failed.noUi",
  not_private: "settings.remoteAccess.failed.notPrivateApp",
  socket_failed: "settings.remoteAccess.failed.socketFailedApp",
  owner_key_failed: "settings.remoteAccess.failed.ownerKeyFailedApp",
  pipe_failed: "settings.remoteAccess.failed.pipeFailed",
};

/** The same for browser access (the browser port); a reason that only remote access has is left out. */
const BROWSER_FAILURE: Partial<Record<RemoteAccessFailureReason, PlainMessageKey>> = {
  no_ui: "settings.remoteAccess.failed.noUi",
  owner_key_failed: "settings.remoteAccess.failed.ownerKeyFailed",
  port_in_use: "settings.remoteAccess.failed.portInUse",
  port_is_daemons: "settings.remoteAccess.failed.portIsDaemons",
  port_failed: "settings.remoteAccess.failed.portFailed",
};

const isFailureReason = knownValues<RemoteAccessFailureReason>({ no_ui: true, not_private: true, socket_failed: true, owner_key_failed: true, port_in_use: true, port_is_daemons: true, port_failed: true, pipe_failed: true });

/** The sentence for a failure's `reason` on the page of `way`, or none for a reason this build does not word there (a
 * newer daemon's, or one the other way in has), where the daemon's own `detail` stands alone. */
export function failureReasonKey(way: "remote" | "browser", reason: string): PlainMessageKey | undefined {
  return isFailureReason(reason) ? (way === "remote" ? REMOTE_FAILURE : BROWSER_FAILURE)[reason] : undefined;
}
