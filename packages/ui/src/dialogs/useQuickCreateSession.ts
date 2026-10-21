import { ActionableError } from "../errorRecovery";
import { useRef } from "react";

import { noAgentState } from "../agents";
import { useLatest } from "../components/useLatest";
import { errorPresentation } from "../errorPresentation";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import { sessionDisplayTitle } from "../sessionLabel";
import { liveConsoleSessions } from "../sidebar/order";
import { useDaemon } from "../store";
import { useOctoboardTheme } from "../theme";
import type { NewSessionRequest } from "./dialogRequest";
import { defaultOpenSessionRequest } from "./openSessionRequest";

/**
 * Opens the session `target` asks the session dialog for without the dialog, with the values the
 * dialog opens on untouched (`defaultOpenSessionRequest`), and hands the new session's id to
 * `onOpened`, unless the window moved to another console (`currentConsoleId`) while the daemon was
 * answering: there is no dialog holding the window still, and the session is not on that screen.
 * With every account of the agent it would open on used up it opens nothing and says so, naming the `action`
 * of the control that asked. A refusal has no dialog to show it in, so it uses the shared error alert. A press
 * while the previous one is still being answered is ignored, so one gesture cannot open two sessions.
 */
export function useQuickCreateSession(
  onOpened: (sessionId: string) => void,
  currentConsoleId: string | undefined,
): (target: NewSessionRequest, action: string) => void {
  const t = useT();
  const { resolved: appearance } = useOctoboardTheme();
  const { request, store, toastError } = useDaemon();
  const pending = useRef(false);
  const latestConsoleId = useLatest(currentConsoleId);
  return (target, action) => {
    if (pending.current) return;
    const state = store.getState();
    const { sessions, settings, agentAvailability } = state;
    const project = target.kind === "new-session" ? target.project : undefined;
    const binding = target.kind === "new-session" ? target.binding : { kind: "unbound" as const };
    if (noAgentState(agentAvailability, settings.added_agents)) return;
    if (binding.kind === "bound") {
      // An owner that is no longer live cannot take a session, and there is no dialog to say so.
      const owner = sessions.get(binding.to.id) ?? binding.to;
      if (!liveConsoleSessions([...sessions.values()], target.console.id).some((s) => s.id === owner.id)) {
        toastError(new ActionableError(t("dialog.session.ownerGone", { name: isolate(sessionDisplayTitle(t, owner)) }), "session", { code: "quick_create_owner_gone", params: { name: sessionDisplayTitle(t, owner) } }));
        return;
      }
    }
    const opening = defaultOpenSessionRequest(t, target.console, project, binding, state, Date.now(), appearance);
    if ("usedUp" in opening) {
      toastError(new ActionableError(t("sidebar.project.openSessionQuickUsedUp", { action: isolate(action) }), "quotaUsedUpAgent", { code: "quick_create_quota_refused", params: { action } }));
      return;
    }
    const startedIn = latestConsoleId.current;
    pending.current = true;
    request(opening.request)
      .then((reply) => {
        if (reply.type === "session_opened" && latestConsoleId.current === startedIn) onOpened(reply.session.id);
      })
      .catch((err: unknown) => toastError(errorPresentation(err as Error)))
      .finally(() => {
        pending.current = false;
      });
  };
}
