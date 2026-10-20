import { noAgentState } from "../agents";
import type { NewSessionRequest } from "../dialogs/dialogRequest";
import { quickCreateUsedUp } from "../dialogs/openSessionRequest";
import { useDaemonStore } from "../store";
import { useQuickCreateModifier } from "../useQuickCreateModifier";

/**
 * How a session-creating control behaves right now. `quick` is whether it shows and takes its quick
 * form: the modifier is held and there is an agent to create with. `blocked` is whether there is no
 * agent that is added and installed, in which case the control is disabled: the session dialog is
 * not opened without one to offer. `usedUp` is whether the agent's accounts are all used up where `target`
 * (what the control would open, if it is there) would create: its quick form has nothing to run on and
 * is unavailable, while the dialog still opens. The time the quotas are judged at is read as the store
 * is, not ticked: the modifier is held only for a moment.
 * Only the small controls call this, so the modifier re-renders them and not the view around them.
 */
export function useNewSessionMode(target?: () => NewSessionRequest | undefined): { quick: boolean; blocked: boolean; usedUp: boolean } {
  const held = useQuickCreateModifier();
  const blocked = useNoAgentToStart();
  const usedUp = useDaemonStore((s) => {
    const request = held && !blocked ? target?.() : undefined;
    return request !== undefined && quickCreateUsedUp(request, s, Date.now());
  });
  return { quick: held && !blocked, blocked, usedUp };
}

/** Whether no agent is both added and installed, so no session can be started. */
export function useNoAgentToStart(): boolean {
  return useDaemonStore((s) => noAgentState(s.agentAvailability, s.settings.added_agents) !== undefined);
}
