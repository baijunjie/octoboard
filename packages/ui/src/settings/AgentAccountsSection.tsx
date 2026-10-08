import { Button } from "@heroui/react";
import React, { useState } from "react";

import { accountsByAgent, AGENT_LABEL } from "../agents";
import { AgentIcon } from "../components/AgentIcon";
import { FadeOverflow } from "../components/FadeOverflow";
import { PathText } from "../components/PathText";
import { ConfirmDialog } from "../dialogs/ConfirmDialog";
import { useT } from "../i18n/react";
import type { Account } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import { AccountDialog } from "./AccountDialog";
import { SettingRow } from "./SettingRow";
import { useSectionRefocus } from "./useSectionRefocus";

/** Every account, grouped by agent with the agent's default account first, and the means to add,
 * edit and remove the ones the user owns (see "Agent accounts" in `docs/product/settings.md`). The
 * default account is not a record: it is the state of pinning nothing, so it is shown from the
 * agent's availability and offers no action. */
export function AgentAccountsSection(): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const availability = useDaemonStore((s) => s.agentAvailability);
  // `undefined` is closed; `null` is the form for a new account.
  const [editing, setEditing] = useState<Account | null | undefined>();
  const [removing, setRemoving] = useState<Account>();
  // Bumped when a removal has gone through: the Remove button that was pressed goes with its
  // account and the confirmation closes with it, so focus has to be sent back to the section's tab.
  // Cancelling removes no control, so it must not.
  const [removedCount, setRemovedCount] = useState(0);
  useSectionRefocus([accounts, removedCount]);

  return (
    <>
      <p className="pb-2 text-sm text-muted">{t("settings.accounts.description")}</p>
      <Button size="sm" variant="outline" preventFocusOnPress onPress={() => setEditing(null)}>
        {t("settings.accounts.add")}
      </Button>
      {accountsByAgent(accounts).map(({ agent, accounts: owned }) => {
        const state = availability.get(agent);
        const unavailable = state?.availability === "unavailable";
        const notUsable = unavailable && (
          <div>{t("settings.accounts.notUsable", { agent: AGENT_LABEL[agent] })}</div>
        );
        return (
          <div key={agent} className="pt-6">
            <h3 className="flex items-center gap-2 pb-2 text-sm font-semibold">
              <AgentIcon agent={agent} />
              {unavailable ? t("agents.notInstalled", { agent: AGENT_LABEL[agent] }) : AGENT_LABEL[agent]}
            </h3>
            <div className="border-t border-separator">
              <SettingRow
                label={t("settings.accounts.defaultName")}
                description={
                  <>
                    <div>{t("settings.accounts.defaultDescription")}</div>
                    {state?.default_account_dir && <PathText path={state.default_account_dir} />}
                  </>
                }
              />
              {owned.map((account) => (
                <SettingRow
                  key={account.id}
                  label={
                    <FadeOverflow dir="auto" titleWhenClipped={account.name}>
                      {account.name}
                    </FadeOverflow>
                  }
                  description={
                    <>
                      <PathText path={account.config_dir} />
                      {notUsable}
                    </>
                  }
                >
                  <div className="flex gap-2">
                    <Button
                      size="sm"
                      variant="outline"
                      preventFocusOnPress
                      aria-label={t("settings.accounts.editLabel", { name: account.name, agent: AGENT_LABEL[agent] })}
                      onPress={() => setEditing(account)}
                    >
                      {t("common.edit")}
                    </Button>
                    <Button
                      size="sm"
                      variant="outline"
                      preventFocusOnPress
                      aria-label={t("settings.accounts.removeLabel", { name: account.name, agent: AGENT_LABEL[agent] })}
                      onPress={() => setRemoving(account)}
                    >
                      {t("common.remove")}
                    </Button>
                  </div>
                </SettingRow>
              ))}
            </div>
          </div>
        );
      })}
      {editing !== undefined && <AccountDialog account={editing ?? undefined} onClose={() => setEditing(undefined)} />}
      {removing && (
        <ConfirmDialog
          title={t("settings.accounts.removeTitle", { name: removing.name })}
          message={t("settings.accounts.removeMessage", { agent: AGENT_LABEL[removing.agent] })}
          confirmLabel={t("common.remove")}
          destructive
          resetKey={removing.id}
          onConfirm={async () => {
            await request({ type: "delete_account", account: removing.id });
            setRemoving(undefined);
            setRemovedCount((count) => count + 1);
          }}
          onCancel={() => setRemoving(undefined)}
        />
      )}
    </>
  );
}
