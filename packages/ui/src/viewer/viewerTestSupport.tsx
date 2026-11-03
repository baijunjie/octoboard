// Shared by the viewer's tests: the providers the header needs around a `FileViewer`.
import React from "react";

import type { RequestHandler } from "../requests";
import { forwardAlerts } from "../components/alerts/alertTestSupport";
import { errorText } from "../errors/errorText";
import { errorPresentation } from "../errors/errorPresentation";
import type { PlatformAdapter } from "../platform";
import type { Daemon } from "../store";
import { TestHost } from "../test/TestHost";
import { testDaemon } from "../test/testDaemon";

const NO_TOASTS = { notice: () => {}, error: () => {} };

/** The platform and daemon a `FileViewer` renders inside. `toasts` is where the daemon's toasts go, and the error
 * alerts raised without a daemon (`raiseError` from `components/alerts/raiseError`) too. */
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
  request?: RequestHandler;
  children: React.ReactNode;
}): React.ReactElement {
  const daemon = React.useMemo<Daemon>(
    () => testDaemon({ request, raiseError: (error) => toasts.error(errorText(errorPresentation(error))), showNotice: toasts.notice }),
    [toasts.error, toasts.notice, request],
  );
  // A layout effect, so it is installed before the children's passive effects (where an alert raised as they mount
  // would be missed otherwise) and is replaced when `toasts.error` changes.
  React.useLayoutEffect(() => forwardAlerts(toasts.error), [toasts.error]);
  return <TestHost daemon={daemon} platform={{ kind: "browser", clipboard, links }}>{children}</TestHost>;
}

/** The accessible names of the status marks under `root`, in document order: a popover mark is a
 * button, a note is the named trigger of a tooltip, and both carry `data-status-mark`. */
export function markNames(root: ParentNode = document): string[] {
  return [...root.querySelectorAll("[data-status-mark]")].map((mark) => mark.getAttribute("aria-label") ?? "");
}
