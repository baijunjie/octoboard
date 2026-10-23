import type { Session } from "../protocol";
import { useWindowShortcut } from "../useWindowShortcut";
import { matchSessionActionShortcut } from "./sessionShortcuts";
import type { SidebarHandlers } from "./types";

/** Window actions always target the selected session, regardless of which row has keyboard focus. */
export function useSessionActionShortcuts(session: Session | undefined, handlers: Pick<SidebarHandlers, "onSetLocked" | "onSetPinned" | "onArchiveSession" | "onCopySessionId">): void {
  useWindowShortcut((event) => {
    const action = matchSessionActionShortcut(event);
    return action === undefined ? undefined : { action, repeat: event.repeat };
  }, ({ action, repeat }) => {
    if (!session || repeat) return;
    if (action === "copyId") {
      handlers.onCopySessionId(session);
      return;
    }
    if (session.status === "archived") return;
    if (action === "lock") handlers.onSetLocked(session, !session.locked);
    else if (action === "pin") handlers.onSetPinned({ session }, !session.pinned);
    else if (!session.locked) handlers.onArchiveSession(session);
  }, session !== undefined);
}
