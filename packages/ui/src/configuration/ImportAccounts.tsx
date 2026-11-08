import { CircleCheck, CircleDot } from "lucide-react";
import React from "react";

import { providerConfigDir, providerLabel } from "../agents/providers";
import { InlineReference } from "../i18n/InlineReference";
import { MarkLineBox } from "../components/status/MarkLineBox";
import { StatusBadge, type StatusBadgeColor } from "../components/status/StatusBadge";
import { StatusMark } from "../components/status/StatusMark";
import { accountNameText } from "../errors/daemonMessage";
import { OptionSelect } from "../components/form/OptionSelect";
import type { PlainMessageKey } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import type { Agent, InspectedAccount, InspectedAccounts } from "../protocol";
import { useProviders } from "../store";
import { useHostPaths } from "../components/hooks/useHostPaths";
import { AgentCardSections, AgentSettingsCard, type CurrentAgentSettings, useAgentSettingLines } from "./AgentSettingsCard";
import { useFailureNode } from "./failureText";
import { accountChoicesFor, agentImported, cardAgents, directoryProblem, hasForeignDirectoryProblem, hasNameProblem, withAccountRow, withAgentImported, type DirectoryProblem, type AccountReviewState, type AccountRowState } from "./importPlan";
import { FlatCard } from "../components/card/FlatCard";
import { choiceHeader, DirectoryChoiceField, DirectoryLine, ItemName, ReviewRow, type Shown, type StatusEntry, ToneLine } from "./importParts";

const CHOICE_LABEL: Record<AccountRowState["choice"], PlainMessageKey> = {
  create: "configuration.import.accounts.choice.create",
  directory: "configuration.import.choice.directory",
  leave_out: "configuration.import.choice.skip",
};

/** What an account of the document would come to here when it can be used, as the badge after its name. A directory in
 * use and an account that is not valid are a warning mark after the name instead (`UnusableMark`). */
const STATUS: Record<"new" | "existing", Pick<StatusEntry, "label" | "icon"> & { badge: StatusBadgeColor }> = {
  new: { label: "configuration.import.accounts.status.new", icon: CircleCheck, badge: "success" },
  existing: { label: "configuration.import.accounts.status.existing", icon: CircleDot, badge: "default" },
};

function AccountRow({
  agent,
  account,
  row,
  problem,
  onChange,
}: {
  agent: Agent;
  account: InspectedAccount;
  row: AccountRowState;
  /** What is wrong with the directory typed for this account, once the user has tried to import. */
  problem: DirectoryProblem | undefined;
  onChange: (row: AccountRowState) => void;
}): React.ReactElement {
  const t = useT();
  const configDir = providerConfigDir(useProviders(), agent);
  const choices = accountChoicesFor(account);
  const settled = account.status === "new" || account.status === "existing" ? STATUS[account.status] : undefined;
  // An account used as it is has no directory to place, and one given another directory shows that one below.
  const showsFileDirectory = account.status !== "existing" && row.choice !== "directory";
  return (
    <ReviewRow
      lead={
        <>
          <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <span className="flex items-start gap-1">
              <ItemName>{account.name}</ItemName>
              {!settled && (
                <MarkLineBox size="text-sm">
                  <UnusableMark account={account} />
                </MarkLineBox>
              )}
            </span>
            {settled && (
              <StatusBadge variant="soft" color={settled.badge} className="inline-flex items-center gap-1" overflow="wrap">
                <settled.icon aria-hidden="true" className="size-3 shrink-0" />
                {t(settled.label)}
              </StatusBadge>
            )}
            {account.disabled && <StatusBadge variant="soft">{t("configuration.import.accounts.badge.disabled")}</StatusBadge>}
          </span>
          {/* In the lead, beside which the choice stands, so it sits right under the name rather than under the row the
              choice makes as tall as itself. */}
          {showsFileDirectory && <DirectoryLine label={t("configuration.import.accounts.field.directory")} value={account.directory} />}
        </>
      }
      control={
        choices.length > 0 && (
          <OptionSelect
            hideLabel
            label={t("configuration.import.accounts.choice.label", { account: isolate(account.name) })}
            options={choices.map((choice) => ({ value: choice, label: t(CHOICE_LABEL[choice]) }))}
            value={row.choice}
            onChange={(choice) => onChange({ ...row, choice })}
          />
        )
      }
    >
      {row.choice === "directory" && (
        <DirectoryChoiceField
          label={t("configuration.import.accounts.directory.label", { account: isolate(account.name) })}
          placeholder={configDir === undefined ? "" : t("configuration.import.accounts.directory.placeholder", { example: `${configDir}-work` })}
          requiredMessage={t("configuration.import.accounts.directory.required")}
          value={row.directory}
          onChange={(directory) => onChange({ ...row, directory })}
          problem={problem}
        />
      )}
    </ReviewRow>
  );
}

/** The warning mark after the name of an account that cannot be created as it is in the file: a directory in use
 * or a name or directory the Add account form would refuse. Its popover says which, why where the daemon says, that
 * the account is left out — unless it is given another directory, where the cause is not its name — and any raw text
 * behind it. */
function UnusableMark({ account }: { account: InspectedAccount }): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const failureNode = useFailureNode();
  // The mark says why a directory of another operating system cannot be used, so the daemon's "not absolute" stays out.
  const failure = hasForeignDirectoryProblem(account) ? null : account.error;
  const remedy = failure && failureNode(failure, "remedy");
  return (
    <StatusMark
      tone="warning"
      label={t(account.status === "directory_in_use" ? "configuration.import.accounts.status.directoryInUseMark" : "configuration.import.accounts.status.invalidMark", { account: isolate(account.name) })}
    >
      <p dir="auto">
        {account.status === "directory_in_use" ? (
          <Message id="configuration.import.accounts.status.directoryInUse" params={{ name: <InlineReference dir="auto" wrap>{accountNameText(language, account.in_use_by ?? "")}</InlineReference> }} />
        ) : failure ? (
          <Message id="configuration.import.accounts.status.invalidReason" params={{ reason: failureNode(failure, "summary") }} />
        ) : hasForeignDirectoryProblem(account) ? (
          t("configuration.import.accounts.status.foreignDirectory")
        ) : (
          t("configuration.import.accounts.status.invalid")
        )}
      </p>
      {remedy && <p dir="auto">{remedy}</p>}
      <p dir="auto">{t(hasNameProblem(account) ? "configuration.import.accounts.status.leftOut" : "configuration.import.accounts.status.remedy")}</p>
      {failure && failureNode(failure, "details")}
    </StatusMark>
  );
}

/** This machine's values of the Agent accounts settings the document carries, in the shape of the document's own. */
function currentAgentSettings(settings: NonNullable<InspectedAccounts["agent_settings"]>): CurrentAgentSettings {
  return {
    auto_select_account: settings.current_auto_select_account,
    auto_switch_on_usage_limit: settings.current_auto_switch_on_usage_limit,
    auto_start_quota_windows: settings.current_auto_start_quota_windows,
    agent_tags: settings.current_agent_tags,
    provider_settings: settings.current_provider_settings,
  };
}

/** Whether the import creates an account: an account of an imported agent that is not there already and that the user
 * has not left out, whether in the directory the file gives it or in another. */
function createsAccount(accounts: InspectedAccounts, review: AccountReviewState): boolean {
  return accounts.agents.some((agent, a) =>
    agentImported(review, agent.agent) && agent.accounts.some((account, i) => account.status !== "existing" && review.rows[a][i].choice !== "leave_out"),
  );
}

/**
 * The accounts of the document, the panel of the review's Accounts tab, which is there only while the accounts are
 * being imported: first the card of the Agent accounts settings that are no agent's, the switches the file carries
 * (what applying each would change); then a card for every agent the file has anything for, titled with its name and a
 * checkbox that is checked for every agent to start with, holding in this order that it will be added to the agent
 * list, which default account would be named and turned off or on, its provider settings and tags (what applying each
 * would change), and a row for each of its accounts (its name, with a warning mark after it where it cannot be created
 * as it is in the file, whether it is disabled, its status here, the directory the file gives it as a line led by a
 * folder icon, and where the status allows a choice, a choice for it); and, where an imported agent has an account that
 * is created, a note that the accounts are not logged in. An agent that is unchecked imports nothing of its own and its
 * card collapses to its heading, marked as left out. An agent the file gives nothing has no card, and one with no
 * account has none of the account rows. An account that exists already is used as it is, never changed, and has no
 * choice or directory. Whether an account shares the default configuration is not shown, since an import never turns
 * that on. The state of every choice is the caller's.
 */
export function ImportAccounts({
  accounts,
  review,
  onReview,
  shown,
}: {
  accounts: InspectedAccounts;
  review: AccountReviewState;
  onReview: (review: AccountReviewState) => void;
  /** Holds a field's message back until the user has tried to import. */
  shown: Shown;
}): React.ReactElement {
  const t = useT();
  const providers = useProviders();
  const agents = cardAgents(providers, accounts.agent_settings, accounts.agents.map(({ agent }) => agent), accounts.agents_to_add);
  const current = accounts.agent_settings ? currentAgentSettings(accounts.agent_settings) : undefined;
  return (
    <>
      <AgentSettingsCard
        settings={accounts.agent_settings}
        current={current}
        heading={t("configuration.import.agentSettings.heading")}
      />
      {agents.map((agent) => {
        const a = accounts.agents.findIndex((listed) => listed.agent === agent);
        return (
          <AgentCard
            key={agent}
            agent={agent}
            listed={accounts.agents[a]}
            settings={accounts.agent_settings}
            current={current}
            added={accounts.agents_to_add.includes(agent)}
            imported={agentImported(review, agent)}
            onImported={(on) => onReview(withAgentImported(review, agent, on))}
            rows={review.rows[a]}
            shown={shown}
            onRow={(i, row) => onReview(withAccountRow(review, a, i, row))}
          />
        );
      })}
      {createsAccount(accounts, review) && <ToneLine>{t("configuration.import.accounts.note")}</ToneLine>}
    </>
  );
}

/** One agent's card: that it will be added to the agent list, its default account's changes, its provider settings and
 * tags, and its accounts, as `AgentCardSections` lays them out; nothing where the file has none of them. With the
 * checkbox of its heading (`imported`) off it is the heading alone. `listed` is the agent's entry of the file's agents,
 * absent for an agent only its settings or its place in the added agents name. */
function AgentCard({
  agent,
  listed,
  settings,
  current,
  added,
  imported,
  onImported,
  rows: reviewRows,
  shown,
  onRow,
}: {
  agent: Agent;
  listed: InspectedAccounts["agents"][number] | undefined;
  settings: InspectedAccounts["agent_settings"];
  /** This machine's values of `settings`, absent with them. */
  current: CurrentAgentSettings | undefined;
  /** Whether the agent is among those the import adds to the agent list. */
  added: boolean;
  imported: boolean;
  onImported: (imported: boolean) => void;
  rows: AccountRowState[] | undefined;
  shown: Shown;
  onRow: (account: number, row: AccountRowState) => void;
}): React.ReactElement | null {
  const t = useT();
  const providers = useProviders();
  const { style } = useHostPaths();
  const lines = useAgentSettingLines(agent, settings, current);
  const defaults: React.ReactNode[] = [];
  if (added) {
    defaults.push(
      <p key="added" className="text-sm">
        <Message id="configuration.import.accounts.agentWillBeAdded" />
      </p>,
    );
  }
  if (listed?.default_name_applies && listed.default_name) {
    defaults.push(
      <p key="named" className="text-sm">
        <Message id="configuration.import.accounts.defaultAccountNamed" params={{ name: <InlineReference dir="auto" wrap>{listed.default_name}</InlineReference> }} />
      </p>,
    );
  }
  if (listed?.default_disabled_applies) {
    defaults.push(
      <p key="switched" className="text-sm">
        <Message id={listed.default_disabled ? "configuration.import.accounts.defaultAccountDisabled" : "configuration.import.accounts.defaultAccountEnabled"} />
      </p>,
    );
  }
  const rows = (listed?.accounts ?? []).map((account, i) => (
    <AccountRow
      key={i}
      agent={agent}
      account={account}
      row={reviewRows![i]}
      problem={shown(directoryProblem(reviewRows![i], style))}
      onChange={(row) => onRow(i, row)}
    />
  ));
  if (defaults.length === 0 && lines.length === 0 && rows.length === 0) return null;
  return (
    <FlatCard>
      {choiceHeader(providerLabel(t, providers, agent), { on: imported, onChange: onImported })}
      {imported && <AgentCardSections defaults={defaults} lines={lines} accounts={rows} />}
    </FlatCard>
  );
}
