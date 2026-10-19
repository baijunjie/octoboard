import { noAgentState } from "../agents";
import { useDaemonStore } from "../store";
import { useQuickCreateModifier } from "../useQuickCreateModifier";

/**
 * How a session-creating control behaves right now. `quick` is whether it shows and takes its quick
 * form: the modifier is held and there is an agent to create with. `blocked` is whether there is no
 * agent that is added and installed, in which case the control is disabled: the session dialog is
 * not opened without one to offer.
 * Only the small controls call this, so the modifier re-renders them and not the view around them.
 */
export function useNewSessionMode(): { quick: boolean; blocked: boolean } {
  const held = useQuickCreateModifier();
  const blocked = useNoAgentToStart();
  return { quick: held && !blocked, blocked };
}

/** Whether no agent is both added and installed, so no session can be started. */
export function useNoAgentToStart(): boolean {
  return useDaemonStore((s) => noAgentState(s.agentAvailability, s.settings.added_agents) !== undefined);
}
