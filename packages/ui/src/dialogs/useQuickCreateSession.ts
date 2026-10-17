import { useRef } from "react";

import { noAgentAvailable } from "../agents";
import { useLatest } from "../components/useLatest";
import { errorPresentation } from "../errorPresentation";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import { sessionDisplayTitle } from "../sessionLabel";
import { liveConsoleSessions } from "../sidebar/order";
import { useDaemon } from "../store";
import type { NewSessionRequest } from "./dialogRequest";
import { defaultOpenSessionRequest } from "./openSessionRequest";

/**
 * Opens the session `target` asks the session dialog for without the dialog, with the values the
 * dialog opens on untouched (`defaultOpenSessionRequest`), and hands the new session's id to
 * `onOpened`, unless the window moved to another console (`currentConsoleId`) while the daemon was
 * answering: there is no dialog holding the window still, and the session is not on that screen.
 * A refusal has no dialog to show it in, so it is a toast. A press while the previous one is still
 * being answered is ignored, so one gesture cannot open two sessions.
 */
export function useQuickCreateSession(
  onOpened: (sessionId: string) => void,
  currentConsoleId: string | undefined,
): (target: NewSessionRequest) => void {
  const t = useT();
  const { request, store, toastError } = useDaemon();
  const pending = useRef(false);
  const latestConsoleId = useLatest(currentConsoleId);
  return (target) => {
    if (pending.current) return;
    const { sessions, settings, agentAvailability } = store.getState();
    const project = target.kind === "new-session" ? target.project : undefined;
    const binding = target.kind === "new-session" ? target.binding : { kind: "unbound" as const };
    if (noAgentAvailable(agentAvailability)) return;
    if (binding.kind === "bound") {
      // An owner that is no longer live cannot take a session, and there is no dialog to say so.
      const owner = sessions.get(binding.to.id) ?? binding.to;
      if (!liveConsoleSessions([...sessions.values()], target.console.id).some((s) => s.id === owner.id)) {
        toastError(t("dialog.session.ownerGone", { name: isolate(sessionDisplayTitle(t, owner)) }));
        return;
      }
    }
    const startedIn = latestConsoleId.current;
    pending.current = true;
    request(defaultOpenSessionRequest(target.console, project, binding, settings.accounts, agentAvailability))
      .then((reply) => {
        if (reply.type === "session_opened" && latestConsoleId.current === startedIn) onOpened(reply.session.id);
      })
      .catch((err: unknown) => toastError(errorPresentation(err as Error)))
      .finally(() => {
        pending.current = false;
      });
  };
}
