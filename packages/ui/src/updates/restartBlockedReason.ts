import type { PlainMessageKey, Translate } from "../i18n/catalog";
import type { UpdateState } from "./updateStore";

/** What each reason a restart is blocked (`UpdateState.restartBlocked`) says: the copy that tells why Restart now is
 * unavailable. */
const BLOCKS: Record<NonNullable<UpdateState["restartBlocked"]>, PlainMessageKey> = {
  agentUpdate: "updates.restartBlocked",
};

/** Why Restart now is unavailable while the window knows the daemon would refuse the restart (see
 * `UpdateState.restartBlocked`), for `Button`'s `unavailable`; undefined when nothing blocks. While blocked the button
 * takes no press, so its restart is not reached (see "Buttons" in `docs/conventions/ui-standard.md`). */
export function restartBlockedReason(t: Translate, state: UpdateState): string | undefined {
  return state.restartBlocked === undefined ? undefined : t(BLOCKS[state.restartBlocked]);
}
