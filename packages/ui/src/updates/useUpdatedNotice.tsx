import { useEffect, useRef } from "react";

import { ExternalLink } from "../components/text/ExternalLink";
import { releaseUrl } from "../components/text/releaseUrl";
import { isolate } from "../i18n/isolate";
import { t } from "../i18n/language";
import { Message } from "../i18n/react";
import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";
import { useDaemon, useDaemonStore } from "../store";

/** The version this browser was last told of, read from storage at the moment of asking and not once at load: another tab
 * may have told it since, and each version is said once per browser, not once per tab. `undefined` where storage is
 * unavailable, so the notice then comes once per page load (see `said`). */
function toldVersion(): string | undefined {
  try {
    return localStorage.getItem(PREFERENCE_KEYS.daemonUpdatedNoticeVersion) ?? undefined;
  } catch {
    return undefined;
  }
}

/** Remembers that this browser was told of `version`, where storage allows. */
function remember(version: string): void {
  try {
    localStorage.setItem(PREFERENCE_KEYS.daemonUpdatedNoticeVersion, version);
  } catch {
    // Storage unavailable: the version is said again for each page load.
  }
}

/** Says "Updated to version X" once per browser per version, when the daemon this page talks to started by replacing
 * an older version of itself (`updated_from`, which it holds for as long as it runs), with a link to what is new. A
 * toast, since it is no more than news; it comes as soon as the daemon that greeted has sent its own status, in whichever
 * tab of the browser is first to see it. What is held from a daemon that went before (`daemonUpdateStale`) says nothing
 * of this one. */
export function useUpdatedNotice(): void {
  const { showNotice } = useDaemon();
  const updatedFrom = useDaemonStore((state) => (state.daemonUpdateStale ? undefined : state.daemonUpdate?.updated_from));
  const version = useDaemonStore((state) => state.daemon?.version);
  // Said once per page load in a browser with no storage.
  const said = useRef<string>(undefined);
  useEffect(() => {
    if (!updatedFrom || !version || said.current === version || toldVersion() === version) return;
    said.current = version;
    remember(version);
    showNotice(
      t("updates.toast.updated", { version: isolate(version) }),
      <Message id="updates.toast.updatedNotes" params={{ version: isolate(version), notes: <ExternalLink href={releaseUrl(version)}>{t("updates.notice.notes")}</ExternalLink> }} />,
    );
  }, [updatedFrom, version, showNotice]);
}
