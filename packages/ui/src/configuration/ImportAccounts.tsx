import { CircleCheck, CircleDot } from "lucide-react";
import React from "react";

import { providerConfigDir, providerLabel } from "../agents/providers";
import { InlineReference } from "../i18n/InlineReference";
import { MarkLineBox } from "../components/status/MarkLineBox";
import { StatusBadge } from "../components/status/StatusBadge";
import { StatusMark } from "../components/status/StatusMark";
import { accountNameText } from "../errors/daemonMessage";
import { OptionSelect } from "../components/form/OptionSelect";
import type { PlainMessageKey } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import type { Agent, InspectedAccount, InspectedAccounts } from "../protocol";
import { useProviders } from "../store";
import { ProviderName } from "../components/text/ProviderName";
import { AgentSettingsCard, type CurrentAgentSettings } from "./AgentSettingsCard";
import { useFailureNode } from "./failureText";
import { accountChoicesFor, directoryProblem, hasNameProblem, withAccountRow, type DirectoryProblem, type AccountReviewState, type AccountRowState } from "./importPlan";
import { AgentList, Card, CardHeading, CardRows, DirectoryChoiceField, Field, FieldList, ItemName, ReviewRow, type Shown, STATUS_TONE, type StatusEntry, StatusGlyph, ToneLine } from "./importParts";

const CHOICE_LABEL: Record<AccountRowState["choice"], PlainMessageKey> = {
  create: "configuration.import.accounts.choice.create",
  directory: "configuration.import.choice.directory",
  leave_out: "configuration.import.choice.skip",
};

/** What an account of the document would come to here when it can be used. A directory in use and an account that is
 * not valid are a warning mark after the name instead (`UnusableMark`). */
const STATUS: Record<"new" | "existing", StatusEntry> = {
  new: { label: "configuration.import.accounts.status.new", icon: CircleCheck, tone: STATUS_TONE.success },
  existing: { label: "configuration.import.accounts.status.existing", icon: CircleDot, tone: STATUS_TONE.muted },
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
            {account.disabled && <StatusBadge variant="soft">{t("configuration.import.accounts.badge.disabled")}</StatusBadge>}
          </span>
          {settled && (
            <span className="flex items-start gap-1.5 text-sm text-app-muted">
              <StatusGlyph entry={settled} size="mt-0.5 size-3.5" />
              <span className="min-w-0 [overflow-wrap:anywhere]" dir="auto">{t(settled.label)}</span>
            </span>
          )}
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
      {showsFileDirectory && (
        <FieldList>
          <Field label={t("configuration.import.accounts.field.directory")} value={account.directory} />
        </FieldList>
      )}
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
  const failure = account.error;
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

/**
 * The accounts of the document, the panel of the review's Accounts tab, which is there only while the accounts are
 * being imported: what would be added to the added agents, which default accounts would be named and which would be
 * turned off or on, the card of the Agent accounts settings the file carries (what applying each would change), a card
 * for every agent the file gives accounts with a row for each of them (its name, with a warning mark after it where it
 * cannot be created as it is in the file, whether it is disabled, its status here, the directory the file gives it,
 * and where the status allows a choice, a choice for it), and, where there is such a card, a note that the accounts
 * are not logged in. An account that
 * exists already is used as it is, never changed, and has no choice or directory. Whether an account shares the
 * default configuration is not shown, since an import never turns that on. The state of every choice is the caller's.
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
  const named = accounts.agents.filter((agent) => agent.default_name_applies && agent.default_name);
  const switched = accounts.agents.filter((agent) => agent.default_disabled_applies);
  return (
    <>
      {accounts.agents_to_add.length > 0 && (
        <AgentList lead={t("configuration.import.accounts.toAdd", { count: accounts.agents_to_add.length })} agents={accounts.agents_to_add} />
      )}
      {named.map((agent) => (
        <p key={agent.agent} className="text-sm">
          <Message
            id="configuration.import.accounts.defaultNamed"
            params={{ agent: <ProviderName agent={agent.agent} />, name: <InlineReference dir="auto" wrap>{agent.default_name!}</InlineReference> }}
          />
        </p>
      ))}
      {switched.map((agent) => (
        <p key={agent.agent} className="text-sm">
          <Message
            id={agent.default_disabled ? "configuration.import.accounts.defaultDisabled" : "configuration.import.accounts.defaultEnabled"}
            params={{ agent: <ProviderName agent={agent.agent} /> }}
          />
        </p>
      ))}
      {accounts.agent_settings && (
        <AgentSettingsCard settings={accounts.agent_settings} current={currentAgentSettings(accounts.agent_settings)} heading={t("configuration.import.agentSettings.heading")} />
      )}
      {accounts.agents.map((agent, a) =>
        // An agent with no account of its own has no card: its default account's name and switch are said above.
        agent.accounts.length === 0 ? null : (
          <AgentCard
            key={agent.agent}
            agent={agent.agent}
            accounts={agent.accounts}
            review={review[a]}
            shown={shown}
            onRow={(i, row) => onReview(withAccountRow(review, a, i, row))}
          />
        ),
      )}
      {accounts.agents.some((agent) => agent.accounts.length > 0) && <ToneLine>{t("configuration.import.accounts.note")}</ToneLine>}
    </>
  );
}

function AgentCard({
  agent,
  accounts,
  review,
  shown,
  onRow,
}: {
  agent: Agent;
  accounts: InspectedAccount[];
  review: AccountRowState[];
  shown: Shown;
  onRow: (account: number, row: AccountRowState) => void;
}): React.ReactElement {
  const t = useT();
  const providers = useProviders();
  return (
    <Card header={(headingId) => <CardHeading id={headingId}>{providerLabel(t, providers, agent)}</CardHeading>}>
      <CardRows>
        {accounts.map((account, i) => (
          <AccountRow
            key={i}
            agent={agent}
            account={account}
            row={review[i]}
            problem={shown(directoryProblem(review[i]))}
            onChange={(row) => onRow(i, row)}
          />
        ))}
      </CardRows>
    </Card>
  );
}
