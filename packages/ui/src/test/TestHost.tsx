// The providers a component under test needs above it: the platform and the daemon.
import type React from "react";

import { announceHost } from "../i18n/platformTerms";
import type { PlatformAdapter } from "../platform";
import { PlatformProvider } from "../platform/react";
import { DaemonHostAnnouncement, DaemonProvider, type Daemon } from "../store";
import { BROWSER_HOST } from "../test/hosts";

/** Renders `children` inside `platform` (a browser's by default) and `daemon`. */
export function TestHost({ daemon, platform = { kind: "browser", host: BROWSER_HOST }, children }: { daemon: Daemon; platform?: PlatformAdapter; children: React.ReactNode }): React.ReactElement {
  announceHost(platform.host);
  return (
    <PlatformProvider value={platform}>
      <DaemonProvider value={daemon}>
        <DaemonHostAnnouncement />
        {children}
      </DaemonProvider>
    </PlatformProvider>
  );
}
