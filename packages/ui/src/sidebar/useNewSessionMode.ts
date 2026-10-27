import { noAgentState } from "../agents";
import type { NewSessionRequest } from "../dialogs/dialogRequest";
import { quickCreateUnavailable } from "../dialogs/openSessionRequest";
import { useDaemonStore } from "../store";
import { useQuickCreateModifier } from "../useQuickCreateModifier";

/**
 * How a session-creating control behaves right now. `quick` is whether it shows and takes its quick
 * form: the modifier is held and there is an agent to create with. `blocked` is whether there is no
 * agent that is added and installed, in which case the control is disabled: the session dialog is
 * not opened without one to offer. `unavailable` is whether none of the agent's accounts can open a
 * session (each is used up, not logged in or turned off) where `target` (what the control would open, if it is
 * there) would create: its quick form has nothing to run on and is unavailable, while the dialog
 * still opens. The time the quotas are judged at is read as the store is, not ticked: the modifier
 * is held only for a moment.
 * Only the small controls call this, so the modifier re-renders them and not the view around them.
 */
export function useNewSessionMode(target?: () => NewSessionRequest | undefined): { quick: boolean; blocked: boolean; unavailable: boolean } {
  const held = useQuickCreateModifier();
  const blocked = useNoAgentToStart();
  const unavailable = useDaemonStore((s) => {
    const request = held && !blocked ? target?.() : undefined;
    return request !== undefined && quickCreateUnavailable(request, s, Date.now());
  });
  return { quick: held && !blocked, blocked, unavailable };
}

/** Whether no agent is both added and installed, so no session can be started. */
export function useNoAgentToStart(): boolean {
  return useDaemonStore((s) => noAgentState(s.agentAvailability, s.settings.added_agents) !== undefined);
}
