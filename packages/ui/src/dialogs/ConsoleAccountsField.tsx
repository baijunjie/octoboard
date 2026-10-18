import { Button } from "@heroui/react";
import React, { useEffect, useId, useRef } from "react";
import { setInteractionModality } from "react-aria";

import { choiceGroups, choiceKey, findEntry, storedAccount } from "../accountChoices";
import { AGENT_ACCOUNT_FIELD, AGENT_LABEL, AGENT_OPTIONS, isAgentUnavailable } from "../agents";
import { AgentIcon } from "../components/AgentIcon";
import { useFocusHandoff } from "../components/useFocusHandoff";
import { useT } from "../i18n/react";
import type { AccountField, Agent } from "../protocol";
import { useDaemonStore } from "../store";
import { GroupRow, GroupRows } from "./GroupRows";
import { LabelledGroup } from "./LabelledGroup";
import { OptionSelect } from "./OptionSelect";

/**
 * The account a console's sessions use, for each agent that can be used: one row per agent, with a
 * picker (capped in width) where the agent has named accounts to choose between and the default
 * account stated where it has none, followed by the way to Settings to add one. An agent known to
 * be missing has no row, unless the console still pins a named account for it, which stays until it
 * is moved off.
 * `values` holds each agent's account (`null` for the default account); a reference to an account
 * that was since removed is shown as the default but stays as stored, and is sent only if another
 * account is chosen.
 *
 * The Settings dialog is the caller's: `onManageAccounts` asks for it with the agent's row, and
 * `managingAccounts` says while it is open. When it closes, focus goes to that row's control,
 * which may no longer be the button that opened it.
 */
export function ConsoleAccountsField({
  label,
  description,
  values,
  onChange,
  onManageAccounts,
  managingAccounts,
}: {
  label: string;
  description: string;
  values: Record<AccountField, string | null>;
  onChange: (field: AccountField, value: string | null) => void;
  onManageAccounts: () => void;
  managingAccounts: boolean;
}): React.ReactElement | null {
  const t = useT();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const agentAvailability = useDaemonStore((s) => s.agentAvailability);
  const groups = choiceGroups(accounts, agentAvailability, t("settings.accounts.defaultName"));
  const rows = AGENT_OPTIONS.flatMap(({ value: agent }) => {
    const entries = groups.find((group) => group.agent === agent)?.entries ?? [];
    const picked = storedAccount(values[AGENT_ACCOUNT_FIELD[agent]], agent, accounts);
    return isAgentUnavailable(agentAvailability, agent) && picked === null ? [] : [{ agent, entries, picked }];
  });
  const rowsRef = useRef<HTMLDivElement>(null);
  const controlOf = (agent: Agent) =>
    rowsRef.current?.querySelector<HTMLElement>(`[data-agent="${agent}"]`)?.querySelector<HTMLElement>('[data-slot="select-trigger"], button');
  // A row's control can go away under the user: a row once its agent is missing and moved off its
  // account. Focus goes to the first picker left.
  const focusHandoff = useFocusHandoff(() => rowsRef.current?.querySelector<HTMLElement>('[data-slot="select-trigger"]')?.focus());
  // The Settings button can go away while Settings is open (the agent gained an account), which
  // leaves nothing for the dialog's own focus return to find. The user did not ask for the move
  // either, so it is shown as a keyboard focus (see "Focus moved by a change the user did not make"
  // in `docs/ui-accessibility.md`).
  const managed = useRef<Agent | undefined>(undefined);
  const wasManaging = useRef(false);
  useEffect(() => {
    if (wasManaging.current && !managingAccounts && managed.current) {
      const control = controlOf(managed.current);
      if (control) {
        setInteractionModality("keyboard");
        control.focus();
      }
    }
    wasManaging.current = managingAccounts;
  }, [managingAccounts]);
  const idPrefix = useId();
  if (rows.length === 0) return null;
  return (
    <LabelledGroup label={label} description={description}>
      <div ref={rowsRef} {...focusHandoff}>
        <GroupRows>
          {rows.map(({ agent, entries, picked }) => {
            const installed = !isAgentUnavailable(agentAvailability, agent);
            const nameId = `${idPrefix}-${agent}`;
            return (
              <GroupRow key={agent} label={<AgentName id={nameId} agent={agent} installed={installed} />}>
                {entries.length > 1 ? (
                  <div data-agent={agent} className="min-w-0 max-w-xs">
                    <OptionSelect
                      hideLabel
                      label={t("dialog.agentSettings.account", { agent: AGENT_LABEL[agent] })}
                      // The row's name is hidden from assistive technology, and says here that the
                      // agent is not installed.
                      describedBy={installed ? undefined : nameId}
                      options={entries.map((entry) => ({ value: entry.key, label: entry.name }))}
                      value={choiceKey({ agent, account: picked })}
                      onChange={(key) => {
                        const entry = findEntry(groups, key);
                        if (entry) onChange(AGENT_ACCOUNT_FIELD[agent], entry.account);
                      }}
                    />
                  </div>
                ) : (
                  <div data-agent={agent} className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1.5">
                    <span className="min-w-0 text-sm text-app-muted">{t("dialog.console.defaultAccountOnly")}</span>
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      aria-describedby={nameId}
                      onPress={() => {
                        managed.current = agent;
                        onManageAccounts();
                      }}
                    >
                      {t("dialog.console.manageAccounts")}
                    </Button>
                  </div>
                )}
              </GroupRow>
            );
          })}
        </GroupRows>
      </div>
    </LabelledGroup>
  );
}

/** The agent's mark and name; `id` lets a control in the row be described by the name, which the row
 * hides from assistive technology otherwise. */
function AgentName({ id, agent, installed }: { id: string; agent: Agent; installed: boolean }): React.ReactElement {
  const t = useT();
  return (
    <>
      <AgentIcon agent={agent} />
      <span id={id} className="min-w-0">{installed ? AGENT_LABEL[agent] : t("agents.notInstalled", { agent: AGENT_LABEL[agent] })}</span>
    </>
  );
}
