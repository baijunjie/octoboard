import React, { useRef } from "react";

import { errorPresentation } from "../errorPresentation";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import type { Agent } from "../protocol";
import { accountDisabled } from "../accountChoices";
import { useDaemon, useDaemonStore } from "../store";
import { SettingSwitch } from "./SettingRow";

/** An account's Disable switch (see "Turning an account off" in `docs/product/agent-accounts.md`), for the default
 * account (`account` null) as for the accounts the user owns. On turns the account off for new work: automatic
 * selection skips it and no session can be started on it, while the sessions already on it keep running. The
 * switch shows the daemon's recorded choice, which every window shares, and moves when that changes; a refusal is
 * shown in an error alert and leaves it as it was, and a press while the previous one is unanswered is ignored.
 * The switch is bare: the column header of the accounts table says "Disable" and the row beside the switch in the
 * details dialog says "Disable account", and the accessible name (`label`, "Disable {name}" by default) adds the
 * account to the words the user sees. */
export function AccountDisableSwitch({ agent, account, name, label = "settings.accounts.disableLabel" }: {
  agent: Agent;
  account: string | null;
  name: string;
  label?: "settings.accounts.disableLabel" | "settings.accounts.disableAccountLabel";
}): React.ReactElement {
  const t = useT();
  const disabled = useDaemonStore((s) => accountDisabled(s.settings.accounts, s.settings.disabled_default_accounts, agent, account));
  const { request, toastError } = useDaemon();
  const asking = useRef(false);
  const change = (disable: boolean) => {
    if (asking.current) return;
    asking.current = true;
    void request({ type: "set_account_disabled", agent, account, disabled: disable })
      .catch((err) => toastError(errorPresentation(err as Error), undefined, t(disable ? "settings.accounts.disableFailedTitle" : "settings.accounts.enableFailedTitle")))
      .finally(() => { asking.current = false; });
  };
  return (
    <SettingSwitch label={t(label, { name: isolate(name) })} isSelected={disabled} onChange={change} />
  );
}
