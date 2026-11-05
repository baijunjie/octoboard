// Shared by the viewer's tests: the providers the header needs around a `FileViewer`.
import React from "react";

import type { RequestHandler } from "../requests";
import { forwardAlerts } from "../components/alerts/alertTestSupport";
import type { PlatformAdapter } from "../platform";
import type { Daemon } from "../store";
import { TestHost } from "../test/TestHost";
import { testDaemon } from "../test/testDaemon";
import { BROWSER_HOST } from "../test/hosts";

const NO_NOTIFICATIONS = { notice: () => {}, error: () => {} };

/** The platform and daemon a `FileViewer` renders inside. `notifications` is where the daemon's notifications go, and
 * the error alerts too. */
export function ViewerHost({
  clipboard,
  links,
  notifications = NO_NOTIFICATIONS,
  request,
  children,
}: {
  clipboard?: PlatformAdapter["clipboard"];
  links?: PlatformAdapter["links"];
  notifications?: { notice: (message: string) => void; error: (message: string) => void };
  request?: RequestHandler;
  children: React.ReactNode;
}): React.ReactElement {
  const daemon = React.useMemo<Daemon>(
    () => testDaemon({ request, showNotice: notifications.notice }),
    [notifications.notice, request],
  );
  // A layout effect, so it is installed before the children's passive effects (where an alert raised as they mount
  // would be missed otherwise) and is replaced when `notifications.error` changes.
  React.useLayoutEffect(() => forwardAlerts(notifications.error), [notifications.error]);
  return <TestHost daemon={daemon} platform={{ kind: "browser", host: BROWSER_HOST, clipboard, links }}>{children}</TestHost>;
}

/** The accessible names of the status marks under `root`, in document order: a popover mark is a
 * button, a note is the named trigger of a tooltip, and both carry `data-status-mark`. */
export function markNames(root: ParentNode = document): string[] {
  return [...root.querySelectorAll("[data-status-mark]")].map((mark) => mark.getAttribute("aria-label") ?? "");
}
