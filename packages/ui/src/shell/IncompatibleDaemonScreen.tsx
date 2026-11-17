import React, { useState } from "react";

import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import { useAppVersion } from "../platform/useAppVersion";
import { PROTOCOL_GENERATION } from "../protocol";
import { useDaemonStore } from "../store";
import { StartupFailure, StartupScreen } from "../StartupScreen";
import { reloadPage } from "../platform/reloadPage";

/**
 * The whole window while the daemon speaks another protocol generation than this window (connection state
 * `incompatible`): nothing it sends can be read, so the window shows nothing of it, and the client has closed the
 * connection for good. Says which versions the two are, the daemon's from its `hello` (unknown for a daemon older than
 * the hello), and offers the one way out there is: restarting on the desktop, which brings the application's own daemon
 * up again afresh, and reloading in a browser, which loads the page again from wherever it is served.
 * Report issue follows it, for a mismatch the way out does not resolve; it is a failure the application names, so it
 * is not called a bug.
 */
export function IncompatibleDaemonScreen({ onRestart }: { onRestart: () => Promise<boolean> }): React.ReactElement {
  const t = useT();
  const platform = usePlatform();
  const daemon = useDaemonStore((s) => s.daemon);
  const appVersion = useAppVersion();
  const unknown = t("connection.incompatible.versionUnknown");
  const desktop = platform.exit !== undefined;
  // Held from the press until the window goes, or until `onRestart` says the restart did not happen.
  const [restarting, setRestarting] = useState(false);
  const restart = () => {
    setRestarting(true);
    void onRestart().then((underWay) => { if (!underWay) setRestarting(false); });
  };
  // The report describes the failure in fixed English, never the screen's localized wording.
  const diagnostic = `The window (protocol generation ${PROTOCOL_GENERATION}, version ${appVersion ?? "unknown"}) connected to a daemon of version ${daemon?.version ?? "unknown"}${daemon ? ` speaking protocol generation ${daemon.generation}` : " that sent no hello"}.`;
  return (
    <StartupScreen
      message={
        <StartupFailure
          failure={{
            title: t("connection.incompatible.title"),
            description: t("connection.incompatible.body"),
            remedy: t(desktop ? "connection.incompatible.remedy.desktop" : "connection.incompatible.remedy.browser"),
            details: <p>{t("connection.incompatible.versions", { appVersion: appVersion === undefined ? unknown : isolate(appVersion), daemonVersion: daemon ? isolate(daemon.version) : unknown })}</p>,
          }}
          action={desktop
            ? { label: t("common.restart"), onSelect: restart, isPending: restarting }
            : { label: t("connection.incompatible.reload"), onSelect: reloadPage }}
          diagnostic={diagnostic}
          known={{ title: "The app and its daemon are different versions" }}
        />
      }
    />
  );
}
