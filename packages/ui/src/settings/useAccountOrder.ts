import { useCallback, useEffect, useRef, useState } from "react";

import { beginAccountDrag } from "./accountDrag";
import { orderedAccounts } from "../accountOrder";
import { errorPresentation } from "../errorPresentation";
import { useT } from "../i18n/react";
import type { Account, Agent } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";

/** An optimistic order survives the reply arriving before the settings broadcast. A changed account set or a newer
 * saved order supersedes it; only one write per agent may be in flight. */
export function useAccountOrder(agent: Agent, accounts: Account[]) {
  const t = useT();
  const { request, toastError } = useDaemon();
  const saved = useDaemonStore((s) => s.settings.account_order?.[agent]);
  const base = JSON.stringify([saved, accounts.map((account) => account.id)]);
  const [pending, setPending] = useState<{ base: string; order: (string | null)[] }>();
  const [busy, setBusy] = useState(false);
  const writing = useRef(false);
  const cancelDrag = useRef<(() => void) | undefined>(undefined);
  const stopDrag = useCallback(() => {
    cancelDrag.current?.();
    cancelDrag.current = undefined;
  }, []);
  useEffect(() => stopDrag, [base, busy, stopDrag]);
  useEffect(() => {
    setPending((held) => held && held.base !== base ? undefined : held);
  }, [base]);
  const order = pending?.base === base ? pending.order : saved;
  const rows = orderedAccounts(agent, accounts, { [agent]: order });
  const ids = rows.map((account) => account?.id ?? null);
  const reorder = (next: (string | null)[]) => {
    if (writing.current || JSON.stringify(ids) === JSON.stringify(next)) return;
    writing.current = true;
    setBusy(true);
    const change = { base, order: next };
    setPending(change);
    void request({ type: "reorder_accounts", agent, accounts: next }).catch((err) => {
      setPending((held) => held === change ? undefined : held);
      toastError(errorPresentation(err as Error), undefined, t("settings.accounts.reorderAccountsFailedTitle"));
    }).finally(() => {
      writing.current = false;
      setBusy(false);
    });
  };
  const move = (id: string | null, by: -1 | 1) => {
    const from = ids.indexOf(id);
    const to = from + by;
    if (from < 0 || to < 0 || to >= ids.length) return;
    const next = ids.filter((other) => other !== id);
    next.splice(to, 0, id);
    reorder(next);
  };
  const drag = (id: string | null, event: React.PointerEvent<HTMLSpanElement>) => {
    if (writing.current || ids.length < 2) return;
    const row = event.currentTarget.closest("tr")!;
    cancelDrag.current?.();
    cancelDrag.current = beginAccountDrag(event.nativeEvent, row, (to) => {
      const next = ids.filter((other) => other !== id);
      next.splice(to, 0, id);
      reorder(next);
    });
  };
  return { rows, ids, busy, reorder, move, drag, stopDrag };
}
