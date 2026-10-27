import { useEffect, useState } from "react";

import type { AsideOwner } from "../layout/asideOwner";
import type { Session } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import { listPages } from "./listPages";

/**
 * The project session whose Report panel the right pane carries while a project owns it: the
 * selected one, when it belongs to that project and has pages; otherwise none, and the pane has no
 * Report panel and its toggle is not offered (`unavailableKinds` in `App.tsx`).
 *
 * Pages are not in the daemon's snapshot, so whether the session has any is found by listing them,
 * when the session comes into question and again after every reconnect or `snapshot`, as the panel
 * does (`ReportPanel.tsx`, which shares the listing: `listPages.ts`). A `snapshot` empties the
 * store's pages until that listing answers, so the last answer for the session stands meanwhile:
 * the panel and its toggle do not leave and come back. A listing that fails offers the panel, whose
 * own failure card says why and has Try again. A page pushed or deleted afterwards arrives as an
 * event and moves the answer with no listing.
 */
export function useProjectReportSession(owner: AsideOwner | undefined, selected: Session | undefined): string | undefined {
  const { request } = useDaemon();
  const connectionState = useDaemonStore((s) => s.connectionState);
  const snapshotEpoch = useDaemonStore((s) => s.snapshotEpoch);
  const session = owner?.kind === "project" && selected?.role === "project" && selected.project_id === owner.project ? selected.id : undefined;
  // How many pages the store holds for the session; `undefined` while it holds no list.
  const count = useDaemonStore((s) => (session === undefined ? undefined : s.pages.get(session)?.length));
  // The last answer for a session, and the session whose listing failed, kept while the store has no list.
  const [answer, setAnswer] = useState<{ session: string; has: boolean }>();
  const [failed, setFailed] = useState<string>();
  if (session !== undefined && count !== undefined && (answer?.session !== session || answer.has !== count > 0)) {
    setAnswer({ session, has: count > 0 });
  }
  // Once the store holds a list the failure is over: a snapshot emptying it later must not bring it back.
  if (count !== undefined && failed !== undefined) setFailed(undefined);

  useEffect(() => {
    // Before the first snapshot the panel cannot be asked for yet, and the snapshot is what lists.
    if (session === undefined || connectionState !== "open" || snapshotEpoch === 0) return;
    let stale = false;
    listPages(request, session, snapshotEpoch).then(
      () => {
        if (!stale) setFailed(undefined);
      },
      () => {
        if (!stale) setFailed(session);
      },
    );
    return () => {
      stale = true;
    };
  }, [session, connectionState, snapshotEpoch, request]);

  if (session === undefined) return undefined;
  const hasPages = count !== undefined ? count > 0 : failed === session || (answer?.session === session && answer.has);
  return hasPages ? session : undefined;
}
