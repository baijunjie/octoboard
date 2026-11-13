import type { HostFacts } from "../platform/host";

/** The message that says where a finished download stands. Where the application replaces itself in place
 * (`updater: "in-place"`) the update is installed by then; where an installer applies it (`"installer"`) it is only
 * downloaded, and is installed when the application quits or restarts. `known` is whether its version is known;
 * `blocked` is whether the restart is held back by an agent's update (`UpdateState.restartBlocked`), which the message
 * then says in place of how the update takes effect. */
export function installedMessageKey(
  updater: HostFacts["updater"],
  surface: "notice",
  known: boolean,
  blocked: boolean,
):
  | "updates.notice.downloaded"
  | "updates.notice.downloadedUnknown"
  | "updates.notice.installed"
  | "updates.notice.installedUnknown"
  | "updates.notice.installedAgentUpdate"
  | "updates.notice.installedAgentUpdateUnknown";
export function installedMessageKey(
  updater: HostFacts["updater"],
  surface: "settings",
  known: boolean,
  blocked: boolean,
):
  | "settings.updates.version.downloaded"
  | "settings.updates.version.downloadedUnknown"
  | "settings.updates.version.installed"
  | "settings.updates.version.installedUnknown"
  | "settings.updates.version.installedAgentUpdate"
  | "settings.updates.version.installedAgentUpdateUnknown";
export function installedMessageKey(updater: HostFacts["updater"], surface: "notice" | "settings", known: boolean, blocked: boolean): string {
  const installer = updater === "installer";
  if (surface === "notice") {
    if (blocked) return known ? "updates.notice.installedAgentUpdate" : "updates.notice.installedAgentUpdateUnknown";
    if (installer) return known ? "updates.notice.downloaded" : "updates.notice.downloadedUnknown";
    return known ? "updates.notice.installed" : "updates.notice.installedUnknown";
  }
  if (blocked) return known ? "settings.updates.version.installedAgentUpdate" : "settings.updates.version.installedAgentUpdateUnknown";
  if (installer) return known ? "settings.updates.version.downloaded" : "settings.updates.version.downloadedUnknown";
  return known ? "settings.updates.version.installed" : "settings.updates.version.installedUnknown";
}
