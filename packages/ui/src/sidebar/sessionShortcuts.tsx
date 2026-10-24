import type { ReactElement, ReactNode } from "react";

import type { CommandId } from "../shortcuts/commands";
import { ShortcutKbd } from "../shortcuts/ShortcutKbd";
import { matchesCommand } from "../shortcuts/store";

/** The commands that act on sessions, which a menu item or a tooltip shows the key of. */
export type SessionShortcutAction = Extract<CommandId, "newSession" | "quickCreate" | "lock" | "pin" | "archive" | "copyId">;

type SessionAction = Extract<SessionShortcutAction, "lock" | "pin" | "archive" | "copyId">;

const SESSION_ACTIONS: readonly SessionAction[] = ["lock", "pin", "archive", "copyId"];

export function matchCreateShortcut(event: KeyboardEvent): "newSession" | "quickCreate" | undefined {
  if (matchesCommand("newSession", event)) return "newSession";
  return matchesCommand("quickCreate", event) ? "quickCreate" : undefined;
}

export function matchSessionActionShortcut(event: KeyboardEvent): SessionAction | undefined {
  return SESSION_ACTIONS.find((action) => matchesCommand(action, event));
}

export function ShortcutTooltip({ children, action }: { children: ReactNode; action: SessionShortcutAction }): ReactElement {
  return <span className="flex items-center gap-3"><span>{children}</span><ShortcutKbd command={action} /></span>;
}
