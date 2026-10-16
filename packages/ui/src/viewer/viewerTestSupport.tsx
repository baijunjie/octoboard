// Shared by the viewer's tests: the providers the header needs around a `FileViewer`.
import React from "react";

import { errorText } from "../errorText";
import type { PlatformAdapter } from "../platform";
import { PlatformProvider } from "../platform/react";
import { createStateStore, DaemonProvider, type Daemon } from "../store";

const NO_TOASTS = { notice: () => {}, error: () => {} };

/** The platform and daemon a `FileViewer` renders inside. Without a `clipboard` the header offers
 * no Copy path; `toasts` is where the daemon's toasts go. */
export function ViewerHost({
  clipboard,
  links,
  toasts = NO_TOASTS,
  request,
  children,
}: {
  clipboard?: PlatformAdapter["clipboard"];
  links?: PlatformAdapter["links"];
  toasts?: { notice: (message: string) => void; error: (message: string) => void };
  request?: Daemon["request"];
  children: React.ReactNode;
}): React.ReactElement {
  const daemon = React.useMemo<Daemon>(
    () => ({
      store: createStateStore(),
      request: request ?? (() => Promise.reject(new Error("no daemon"))),
      toastError: (message) => toasts.error(errorText(message)),
      toastNotice: toasts.notice,
      onToast: () => () => {},
      dismissTrustPrompt: () => {},
      reconnect: () => {},
      terminalUrl: () => "",
    }),
    [toasts.error, toasts.notice, request],
  );
  return (
    <PlatformProvider value={{ kind: "browser", clipboard, links }}>
      <DaemonProvider value={daemon}>{children}</DaemonProvider>
    </PlatformProvider>
  );
}
