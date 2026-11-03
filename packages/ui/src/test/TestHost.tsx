// The providers a component under test needs above it: the platform and the daemon.
import type React from "react";

import type { PlatformAdapter } from "../platform";
import { PlatformProvider } from "../platform/react";
import { DaemonProvider, type Daemon } from "../store";

/** Renders `children` inside `platform` (a browser's by default) and `daemon`. */
export function TestHost({ daemon, platform = { kind: "browser" }, children }: { daemon: Daemon; platform?: PlatformAdapter; children: React.ReactNode }): React.ReactElement {
  return (
    <PlatformProvider value={platform}>
      <DaemonProvider value={daemon}>{children}</DaemonProvider>
    </PlatformProvider>
  );
}
