import type { ReactElement, ReactNode } from "react";

import type { ModifierHost } from "../shortcuts/binding";
import type { CommandId } from "../shortcuts/commands";
import { ShortcutKbd } from "../shortcuts/ShortcutKbd";
import { matchesCommand } from "../shortcuts/store";

/** The commands that act on sessions, which a menu item or a tooltip shows the key of. */
export type SessionShortcutAction = Extract<CommandId, "newSession" | "quickCreate" | "focusMode">;

export function matchCreateShortcut(event: KeyboardEvent, host: ModifierHost): "newSession" | "quickCreate" | undefined {
  if (matchesCommand("newSession", event, host)) return "newSession";
  return matchesCommand("quickCreate", event, host) ? "quickCreate" : undefined;
}

export function ShortcutTooltip({ children, action }: { children: ReactNode; action: SessionShortcutAction }): ReactElement {
  return <span className="flex items-center gap-3"><span>{children}</span><ShortcutKbd command={action} /></span>;
}
