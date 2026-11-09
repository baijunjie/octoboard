import { useEffect, useState } from "react";

import { useT } from "../../i18n/react";
import { useLatest } from "../hooks/useLatest";
import type { ActionMenuEntry, ActionMenuItem } from "./menuEntries";

/** Resolves only the items needing host data; sibling actions and the menu's layout are available at once. */
export function useResolvedMenuEntries(entries: ActionMenuEntry[]): ActionMenuEntry[] {
  const t = useT();
  const latest = useLatest({ entries, t });
  const keys = JSON.stringify(entries.map((entry) => typeof entry === "object" && "load" in entry ? entry.load?.key : null));
  const [resolved, setResolved] = useState<{ keys: string; items: Map<number, ActionMenuItem> }>();
  if (resolved && resolved.keys !== keys) setResolved(undefined);
  useEffect(() => {
    let current = true;
    latest.current.entries.forEach((entry, index) => {
      if (typeof entry !== "object" || !("load" in entry) || !entry.load) return;
      Promise.resolve().then(entry.load.run).catch(() => ({ ...entry, disabled: true, reason: { state: latest.current.t("menu.loadFailed") } })).then((item) => {
        if (!current) return;
        setResolved((previous) => ({ keys, items: new Map(previous?.keys === keys ? previous.items : []).set(index, item) }));
      });
    });
    return () => { current = false; };
  }, [keys, latest]);
  return entries.map((entry, index) => {
    if (typeof entry !== "object" || !("load" in entry) || !entry.load) return entry;
    const item = resolved?.keys === keys ? resolved.items.get(index) : undefined;
    return item ? { ...entry, ...item } : { ...entry, disabled: true };
  });
}
