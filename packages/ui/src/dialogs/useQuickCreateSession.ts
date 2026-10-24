import { DaemonRequestError } from "../daemon-client";
import { ActionableError } from "../errorRecovery";
import { useRef } from "react";

import { AGENT_LABEL, noAgentState } from "../agents";
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
import type { ConfirmLaunch } from "./useConfirmIncompleteLaunch";

/**
 * Opens the session `target` asks the session dialog for without the dialog, with the values the
 * dialog opens on untouched (`defaultOpenSessionRequest`), and hands the new session's id to
 * `onOpened`, unless the window moved to another console (`currentConsoleId`) while the daemon was
 * answering: there is no dialog holding the window still, and the session is not on that screen.
 * With every account of the agent it would open on used up it opens nothing and says so, naming the
 * `action` of the control that asked; with the account it would open on turned off, and no other free,
 * it opens nothing and shows the daemon's own refusal. A refusal has no dialog to show it in, so it uses the shared
 * error alert. An account whose last sharing change did not finish is asked about first
 * (`confirmLaunch`), cancelling sending nothing. A press while the previous one is still being
 * answered is ignored, so one gesture cannot open two sessions.
 */
export function useQuickCreateSession(
  onOpened: (sessionId: string) => void,
  currentConsoleId: string | undefined,
  confirmLaunch: ConfirmLaunch,
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
    if ("disabled" in opening) {
      const { agent, name } = opening.disabled;
      toastError(new DaemonRequestError(t("daemon.account_disabled", { account: isolate(name), agent: AGENT_LABEL[agent] }), "account_disabled", { agent: AGENT_LABEL[agent], account: name }));
      return;
    }
    if ("changingSharing" in opening) {
      const { account, name } = opening.changingSharing;
      toastError(new DaemonRequestError(t("daemon.account_sharing_in_progress", { name: isolate(name) }), "account_sharing_in_progress", { account, name }));
      return;
    }
    const startedIn = latestConsoleId.current;
    pending.current = true;
    void confirmLaunch(opening.request.account, () => request(opening.request)
      .then((reply) => {
        if (reply.type === "session_opened" && latestConsoleId.current === startedIn) onOpened(reply.session.id);
      })
      .catch((err: unknown) => toastError(errorPresentation(err as Error))))
      .finally(() => {
        pending.current = false;
      });
  };
}
