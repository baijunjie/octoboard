import { useEffect } from "react";

import { usePlatform } from "../platform/react";
import { useDaemonStoreOrEmpty } from "../store";
import { useSerialPush } from "../components/hooks/useSerialPush";

/**
 * Tells the shell, on mount and whenever the daemon's choice changes, whether the system ending the session (logging
 * out, restarting or shutting down) may quit the app without the confirmation: it may while the daemon resumes the
 * sessions that quit leaves interrupted (Resume sessions on start). The shell asks as for any quit until it has been
 * told. Best-effort: a shell left on its previous answer is not worth surfacing to the user.
 *
 * Mounted under the `DaemonProvider` but outside the error boundary, so the answer follows the setting for as long as
 * the window lives, hidden or not. The screen shown when the daemon failed to start mounts it too, with the setting at
 * its default, so a system quit there asks as for any quit.
 */
export function useSystemQuitSync(): void {
  const { exit } = usePlatform();
  const resumes = useDaemonStoreOrEmpty((state) => state.settings.resume_sessions_on_start);
  // Serial, so two in-flight pushes cannot settle out of order and leave the shell on the earlier answer.
  const push = useSerialPush();

  useEffect(() => {
    if (!exit) return;
    push(() => exit.setSystemQuitWithoutAsking(resumes));
  }, [exit, resumes, push]);
}
