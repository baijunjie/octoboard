import { noAgentAvailable } from "../agents";
import { useDaemonStore } from "../store";
import { useQuickCreateModifier } from "../useQuickCreateModifier";

/**
 * How a session-creating control behaves right now. `quick` is whether it shows and takes its quick
 * form: the modifier is held and there is an agent to create with. `blocked` is whether there is no
 * agent, in which case the control stays a plain "+" that opens the dialog, which explains why.
 * Only the small controls call this, so the modifier re-renders them and not the view around them.
 */
export function useNewSessionMode(): { quick: boolean; blocked: boolean } {
  const held = useQuickCreateModifier();
  const blocked = useDaemonStore((s) => noAgentAvailable(s.agentAvailability));
  return { quick: held && !blocked, blocked };
}
