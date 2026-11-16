import { useCallback, useEffect, useState } from "react";

import { alertFailure } from "../components/alerts/raiseError";
import { t } from "../i18n/language";
import type { RemoteDaemonEntry, RemoteDaemonsCapability, RemoteDaemonStatus } from "../platform";

/** The shell's saved remote daemons and which of them are connected, read again whenever the shell says they
 * changed (`onChanged`: an entry saved or removed, a window opened or closed, a connection dropped or back).
 * `reload` reads them now, answering once both reads have ended. A failed read of the list is an alert, titled
 * in the language of the moment it fails, and leaves the list as it was; a failed read of the states leaves
 * those. `entriesRead` and `statusesRead` turn true once the list and the states were first read (or failed to
 * be). */
export function useRemoteDaemons(capability: RemoteDaemonsCapability | undefined): {
  entries: RemoteDaemonEntry[];
  entriesRead: boolean;
  statuses: RemoteDaemonStatus[];
  statusesRead: boolean;
  reload: () => Promise<void>;
} {
  const [entries, setEntries] = useState<RemoteDaemonEntry[]>([]);
  const [entriesRead, setEntriesRead] = useState(false);
  const [statuses, setStatuses] = useState<RemoteDaemonStatus[]>([]);
  const [statusesRead, setStatusesRead] = useState(false);
  const reload = useCallback(async () => {
    if (!capability) return;
    await Promise.all([
      capability.list().then(setEntries, (error: unknown) => alertFailure({ title: t("remoteDaemons.listFailedTitle") })(error)).finally(() => setEntriesRead(true)),
      capability.status().then(setStatuses, () => {}).finally(() => setStatusesRead(true)),
    ]);
  }, [capability]);
  useEffect(() => {
    void reload();
    return capability?.onChanged(() => void reload());
  }, [capability, reload]);
  return { entries, entriesRead, statuses, statusesRead, reload };
}
