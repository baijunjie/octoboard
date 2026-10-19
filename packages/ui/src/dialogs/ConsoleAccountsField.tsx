import { Button } from "@heroui/react";
import { KeyRound } from "lucide-react";
import React, { useId, useRef } from "react";

import { choiceGroups, choiceKey, findEntry, storedAccount } from "../accountChoices";
import { AGENT_ACCOUNT_FIELD, AGENT_LABEL, AGENT_OPTIONS, defaultAccountName, isAgentUnavailable, isDefaultAccountRenamed } from "../agents";
import { AgentIcon } from "../components/AgentIcon";
import { TitledControl } from "../components/TitledControl";
import { useFocusHandoff } from "../components/useFocusHandoff";
import { useT } from "../i18n/react";
import type { AccountField, Agent } from "../protocol";
import { useDaemonStore } from "../store";
import { GroupRow, GroupRows } from "./GroupRows";
import { LabelledGroup } from "./LabelledGroup";
import { OptionSelect } from "./OptionSelect";

/**
 * The account a console's sessions use, for each agent that can be used: one row per agent, with a
 * picker (capped in width), which for an agent with no named account offers just the default account, and
 * one icon-only way to Settings beside the section's title. An agent known to
 * be missing has no row, unless the console still pins a named account for it, which stays until it
 * is moved off.
 * `values` holds each agent's account (`null` for the default account); a reference to an account
 * that was since removed is shown as the default but stays as stored, and is sent only if another
 * account is chosen.
 *
 * The Settings dialog is the caller's: `onManageAccounts` asks for it, and focus comes back to the button.
 */
export function ConsoleAccountsField({
  label,
  description,
  values,
  onChange,
  onManageAccounts,
}: {
  label: string;
  description: string;
  values: Record<AccountField, string | null>;
  onChange: (field: AccountField, value: string | null) => void;
  onManageAccounts: () => void;
}): React.ReactElement | null {
  const t = useT();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const agentAvailability = useDaemonStore((s) => s.agentAvailability);
  const defaultNames = useDaemonStore((s) => s.settings.default_account_names);
  const groups = choiceGroups(accounts, agentAvailability, (agent) => defaultAccountName(t, defaultNames, agent));
  const rows = AGENT_OPTIONS.flatMap(({ value: agent }) => {
    const entries = groups.find((group) => group.agent === agent)?.entries ?? [];
    const picked = storedAccount(values[AGENT_ACCOUNT_FIELD[agent]], agent, accounts);
    return isAgentUnavailable(agentAvailability, agent) && picked === null ? [] : [{ agent, entries, picked }];
  });
  const rowsRef = useRef<HTMLDivElement>(null);
  // A row's control can go away under the user: a row once its agent is missing and moved off its
  // account. Focus goes to the first picker left.
  const focusHandoff = useFocusHandoff(() => rowsRef.current?.querySelector<HTMLElement>('[data-slot="select-trigger"]')?.focus());
  const idPrefix = useId();
  if (rows.length === 0) return null;
  return (
    <LabelledGroup
      label={label}
      description={description}
      help={
        // The size, colour and hit area of the "?" beside a title (`HelpTip`).
        <TitledControl title={t("dialog.console.manageAccounts")}>
          <Button
            isIconOnly
            size="sm"
            variant="ghost"
            aria-label={t("dialog.console.manageAccounts")}
            className="row-icon-control -my-0.5 rounded-full text-app-muted"
            onPress={onManageAccounts}
          >
            <KeyRound aria-hidden="true" className="size-3.5" />
          </Button>
        </TitledControl>
      }
    >
      <div ref={rowsRef} {...focusHandoff}>
        <GroupRows>
          {rows.map(({ agent, entries, picked }) => {
            const installed = !isAgentUnavailable(agentAvailability, agent);
            const nameId = `${idPrefix}-${agent}`;
            return (
              <GroupRow key={agent} label={<AgentName id={nameId} agent={agent} installed={installed} />}>
                <div className="min-w-0 max-w-xs">
                  <OptionSelect
                    hideLabel
                    label={t("dialog.agentSettings.account", { agent: AGENT_LABEL[agent] })}
                    // The row's name is hidden from assistive technology, and says here that the
                    // agent is not installed.
                    describedBy={installed ? undefined : nameId}
                    // An agent with only its default account offers just that one: under the name the user gave
                    // it, else as "Default account".
                    options={entries.map((entry) => ({
                      value: entry.key,
                      label: entries.length === 1 && !isDefaultAccountRenamed(defaultNames, agent) ? t("dialog.console.defaultAccountOnly") : entry.name,
                    }))}
                    value={choiceKey({ agent, account: picked })}
                    onChange={(key) => {
                      const entry = findEntry(groups, key);
                      if (entry) onChange(AGENT_ACCOUNT_FIELD[agent], entry.account);
                    }}
                  />
                </div>
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
