import { KeyRound } from "lucide-react";
import React, { useId, useRef } from "react";

import { Button } from "../components/Button";
import { choiceGroups, choiceKey, findEntry, storedAccount } from "../accountChoices";
import { AGENT_ACCOUNT_FIELD, AGENT_LABEL, agentChoiceBadge, defaultAccountName, isAgentPickable, isDefaultAccountRenamed, listedPickerAgents } from "../agents";
import { AgentIcon } from "../components/AgentIcon";
import { FadeOverflow } from "../components/FadeOverflow";
import { StatusBadge } from "../components/StatusBadge";
import { TitledControl } from "../components/TitledControl";
import { useFocusHandoff } from "../components/useFocusHandoff";
import { useT } from "../i18n/react";
import type { AccountField, Agent } from "../protocol";
import { useDaemonStore } from "../store";
import { LabelledGroup } from "./LabelledGroup";
import { OptionSelect } from "./OptionSelect";

/**
 * The account a console's sessions use, for each agent that can be used: one row per added agent, in the
 * order they were added, each on the two columns of the Default agent pickers above (the agent's name, then
 * its account picker at the column's full width), with a picker that for an agent with no named account
 * offers just the default account, and one icon-only way to Settings beside the section's title. An agent
 * known to be missing or not added has no row, unless the console still pins a named account for it, which
 * stays, badged and dimmed, until it is moved off.
 * `values` holds each agent's account (`null` for the default account); a reference to an account
 * that was since removed is shown as the default but stays as stored, and is sent only if another
 * account is chosen.
 *
 * The Settings dialog is the caller's: `onManageAccounts` asks for it and is handed the button, which focus comes back to once Settings closes.
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
  onManageAccounts: (opener: HTMLButtonElement) => void;
}): React.ReactElement | null {
  const t = useT();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const agentAvailability = useDaemonStore((s) => s.agentAvailability);
  const addedAgents = useDaemonStore((s) => s.settings.added_agents);
  const defaultNames = useDaemonStore((s) => s.settings.default_account_names);
  const groups = choiceGroups(accounts, agentAvailability, addedAgents, (agent) => defaultAccountName(t, defaultNames, agent));
  const pinned = (agent: Agent) => storedAccount(values[AGENT_ACCOUNT_FIELD[agent]], agent, accounts);
  const rows = listedPickerAgents(agentAvailability, addedAgents, (agent) => pinned(agent) !== null).map((agent) => ({
    agent,
    entries: groups.find((group) => group.agent === agent)?.entries ?? [],
    picked: pinned(agent),
  }));
  const rowsRef = useRef<HTMLDivElement>(null);
  const manageRef = useRef<HTMLButtonElement>(null);
  // A row's control can go away under the user: a row once its agent is missing or not added and
  // moved off its account. Focus goes to the first picker left.
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
            size="xs"
            variant="ghost"
            aria-label={t("dialog.console.manageAccounts")}
            className="shrink-0 -my-0.5 rounded-full text-app-muted"
            ref={manageRef}
            onPress={() => {
              if (manageRef.current) onManageAccounts(manageRef.current);
            }}
          >
            <KeyRound aria-hidden="true" className="size-3.5" />
          </Button>
        </TitledControl>
      }
    >
      <div ref={rowsRef} {...focusHandoff} className="flex flex-col gap-3">
        {rows.map(({ agent, entries, picked }) => {
          const marked = !isAgentPickable(agentAvailability, addedAgents, agent);
          const nameId = `${idPrefix}-${agent}`;
          return (
          // The same two columns as the Default agent pickers above: the agent's name where the first
          // picker is, the account picker where the second is, one under the other below `md`.
            <div key={agent} className="grid grid-cols-[minmax(0,1fr)] items-center gap-x-6 gap-y-1.5 md:grid-cols-2">
              <div aria-hidden="true" className="flex min-w-0 items-center gap-2 text-sm">
                <AgentName id={nameId} agent={agent} badge={agentChoiceBadge(t, agentAvailability, addedAgents, agent)} dimmed={marked} />
              </div>
              <div className="min-w-0">
                <OptionSelect
                  hideLabel
                  label={t("dialog.agentSettings.account", { agent: AGENT_LABEL[agent] })}
                  // The row's name is hidden from assistive technology, and says here that the
                  // agent is not installed or not added.
                  describedBy={marked ? nameId : undefined}
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
            </div>
          );
        })}
      </div>
    </LabelledGroup>
  );
}

/** The agent's mark and name, with the badge saying it is not installed or not added; `id` lets a control in the
 * row be described by the name, which the row hides from assistive technology otherwise. A `dimmed` agent, one
 * that may not be picked, has its name in the secondary colour (4.8:1 or better, against the foreground
 * colour of the others) and its mark faded. */
function AgentName({ id, agent, badge, dimmed }: { id: string; agent: Agent; badge?: string; dimmed: boolean }): React.ReactElement {
  return (
    <>
      <AgentIcon agent={agent} className={dimmed ? "size-4 fill-current text-app-muted opacity-60" : "size-4"} />
      <span id={id} className="flex min-w-0 items-center gap-2">
        <FadeOverflow as="span" className={`min-w-0 ${dimmed ? "text-app-muted" : "text-foreground"}`} titleWhenClipped={AGENT_LABEL[agent]}>{AGENT_LABEL[agent]}</FadeOverflow>
        {badge && <StatusBadge>{badge}</StatusBadge>}
      </span>
    </>
  );
}
