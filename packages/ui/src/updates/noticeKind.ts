import type { UpdateState } from "./updateStore";

/** Which notice the window shows for `state`, if any. A daemon that now runs another version than this page was loaded
 * for outranks everything (nothing here is dismissed, as the page is out of date until it is reloaded), then a restart
 * waiting to be taken, and a download in progress cannot be dismissed. An update the daemon cannot put in place itself is
 * `manual`: it is offered with the way to install it by hand, and no download. */
export function updateNoticeKind(state: UpdateState): "reload" | "installed" | "downloading" | "available" | "manual" | undefined {
  if (state.reloadNeeded) return "reload";
  if (state.phase === "installed") return state.restartNoticeHidden ? undefined : "installed";
  if (state.phase === "downloading") return "downloading";
  if (!state.update || state.noticeDismissed) return undefined;
  return state.manual ? "manual" : "available";
}
