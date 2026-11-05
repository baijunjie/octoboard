import type { HostFacts } from "../platform/host";

/** The message that says where a finished download stands. Where the application replaces itself in place
 * (`updater: "in-place"`) the update is installed by then; where an installer applies it (`"installer"`) it is only
 * downloaded, and is installed when the application quits or restarts. `known` is whether its version is known. */
export function installedMessageKey(
  updater: HostFacts["updater"],
  surface: "notice",
  known: boolean,
): "updates.notice.downloaded" | "updates.notice.downloadedUnknown" | "updates.notice.installed" | "updates.notice.installedUnknown";
export function installedMessageKey(
  updater: HostFacts["updater"],
  surface: "settings",
  known: boolean,
):
  | "settings.updates.version.downloaded"
  | "settings.updates.version.downloadedUnknown"
  | "settings.updates.version.installed"
  | "settings.updates.version.installedUnknown";
export function installedMessageKey(updater: HostFacts["updater"], surface: "notice" | "settings", known: boolean): string {
  const installer = updater === "installer";
  if (surface === "notice") {
    if (installer) return known ? "updates.notice.downloaded" : "updates.notice.downloadedUnknown";
    return known ? "updates.notice.installed" : "updates.notice.installedUnknown";
  }
  if (installer) return known ? "settings.updates.version.downloaded" : "settings.updates.version.downloadedUnknown";
  return known ? "settings.updates.version.installed" : "settings.updates.version.installedUnknown";
}
