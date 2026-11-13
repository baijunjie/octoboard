import React from "react";

import { appConfig } from "../appConfig";
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
 * the hello), and offers the one way out there is: quitting on the desktop, where the application brings its own daemon
 * up again on the next launch, and reloading in a browser, which loads the page again from wherever it is served.
 * Report issue follows it, for a mismatch the way out does not resolve; it is a failure the application names, so it
 * is not called a bug.
 */
export function IncompatibleDaemonScreen({ onQuit }: { onQuit: () => void }): React.ReactElement {
  const t = useT();
  const platform = usePlatform();
  const daemon = useDaemonStore((s) => s.daemon);
  const appVersion = useAppVersion();
  const unknown = t("connection.incompatible.versionUnknown");
  const desktop = platform.exit !== undefined;
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
            ? { label: t("connection.incompatible.quit"), onSelect: onQuit }
            : { label: t("connection.incompatible.reload"), onSelect: reloadPage }}
          diagnostic={diagnostic}
          known={{ title: `${appConfig.name} and its daemon are different versions` }}
        />
      }
    />
  );
}
