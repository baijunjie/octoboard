import { CodedError } from "../errors/codedError";
import type { PlainMessageKey, Translate } from "../i18n/catalog";
import { reportDaemonRestartFailure } from "./restartFailure";
import type { UpdateState, UpdateStore } from "./updateStore";

/** What each reason a restart is blocked (`UpdateState.restartBlocked`) says: the copy that tells why Restart now is
 * unavailable, and the code of the refusal the daemon would answer the restart with, which the press raises. */
const BLOCKS: Record<NonNullable<UpdateState["restartBlocked"]>, { reason: PlainMessageKey; code: string; message: PlainMessageKey }> = {
  agentUpdate: { reason: "updates.restartBlocked", code: "agent_update_running", message: "daemon.agent_update_running" },
};

/** What Restart now needs to be unavailable while the window knows the daemon would refuse the restart (see
 * `UpdateState.restartBlocked`): `unavailable`, the reason for `Button` and its tooltip (undefined when nothing blocks),
 * and `onPress`, which sends the restart when nothing blocks and otherwise explains by raising the refusal the daemon
 * would answer with, without sending it (see "Buttons" in `docs/ui-standard.md`). */
export function restartPress(t: Translate, state: UpdateState, store: UpdateStore): { unavailable: string | undefined; onPress: () => void } {
  if (state.restartBlocked === undefined) return { unavailable: undefined, onPress: () => void store.restart() };
  const block = BLOCKS[state.restartBlocked];
  return {
    unavailable: t(block.reason),
    onPress: () => reportDaemonRestartFailure(new CodedError(t(block.message), block.code, {})),
  };
}
