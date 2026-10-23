import { Kbd } from "@heroui/react";
import type { ReactElement, ReactNode } from "react";

import { IS_MAC, isPrimaryModifier } from "../useWindowShortcut";

const KEYS = { create: "N", quickCreate: "N", lock: "L", pin: "P", archive: "A", copyId: "C" } as const;
export type SessionShortcutAction = keyof typeof KEYS;

export function matchCreateShortcut(event: KeyboardEvent): "create" | "quickCreate" | undefined {
  if (event.code !== "KeyN" || event.altKey || !isPrimaryModifier(event)) return undefined;
  return event.shiftKey ? "quickCreate" : "create";
}

export function matchSessionActionShortcut(event: KeyboardEvent): Exclude<SessionShortcutAction, "create" | "quickCreate"> | undefined {
  if (!event.shiftKey || event.altKey || !isPrimaryModifier(event)) return undefined;
  switch (event.code) {
    case "KeyL": return "lock";
    case "KeyP": return "pin";
    case "KeyA": return "archive";
    case "KeyC": return "copyId";
    default: return undefined;
  }
}

export function SessionShortcutKbd({ action }: { action: SessionShortcutAction }): ReactElement {
  return (
    <Kbd dir="ltr" slot="keyboard" className="ms-auto shrink-0 in-data-[hovered=true]:bg-surface">
      {action !== "create" && <Kbd.Abbr keyValue="shift" />}
      <Kbd.Abbr keyValue={IS_MAC ? "command" : "ctrl"} />
      <Kbd.Content>{KEYS[action]}</Kbd.Content>
    </Kbd>
  );
}

export function ShortcutTooltip({ children, action }: { children: ReactNode; action: SessionShortcutAction }): ReactElement {
  return <span className="flex items-center gap-3"><span>{children}</span><SessionShortcutKbd action={action} /></span>;
}
