import type { UpdateState } from "./updateStore";

/** Which notice the window shows for `state`, if any. A restart waiting to be taken outranks everything else,
 * and a download in progress cannot be dismissed. */
export function updateNoticeKind(state: UpdateState): "installed" | "downloading" | "available" | undefined {
  if (state.phase === "installed") return state.restartNoticeHidden ? undefined : "installed";
  if (state.phase === "downloading") return "downloading";
  return state.update && !state.noticeDismissed ? "available" : undefined;
}
