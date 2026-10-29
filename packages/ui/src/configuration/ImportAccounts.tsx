import { CircleCheck, CircleDot, Info, type LucideIcon } from "lucide-react";
import React, { useId } from "react";

import { AGENT_DEFAULT_CONFIG_DIR, AGENT_LABEL } from "../agents";
import { InlineReference } from "../components/InlineReference";
import { MarkLineBox } from "../components/MarkLineBox";
import { StatusBadge } from "../components/StatusBadge";
import { StatusMark } from "../components/StatusMark";
import { accountNameText } from "../daemonMessage";
import { OptionSelect } from "../dialogs/OptionSelect";
import type { PlainMessageKey } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import type { Agent, InspectedAccount, InspectedAccounts } from "../protocol";
import { useFailureNode } from "./failureText";
import { accountChoicesFor, directoryProblem, hasNameProblem, withAccountRow, type DirectoryProblem, type AccountReviewState, type AccountRowState } from "./importPlan";
import { AgentList, CARD_CLASS, CARD_HEADER_CLASS, DirectoryChoiceField, Field, type Shown } from "./importParts";

const CHOICE_LABEL: Record<AccountRowState["choice"], PlainMessageKey> = {
  create: "configuration.import.accounts.choice.create",
  directory: "configuration.import.choice.directory",
  leave_out: "configuration.import.choice.skip",
};

/** What an account of the document would come to here when it can be used: drawn and coloured so the colour only
 * strengthens the word and the glyph. A directory in use and an account that is not valid are a warning mark after the
 * name instead (`UnusableMark`). */
const STATUS: Record<"new" | "existing", { label: PlainMessageKey; icon: LucideIcon; tone: string }> = {
  new: { label: "configuration.import.accounts.status.new", icon: CircleCheck, tone: "text-success-soft-foreground" },
  existing: { label: "configuration.import.accounts.status.existing", icon: CircleDot, tone: "text-app-muted" },
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
  const choices = accountChoicesFor(account);
  const settled = account.status === "new" || account.status === "existing" ? STATUS[account.status] : undefined;
  // An account used as it is has no directory to place, and one given another directory shows that one below.
  const showsFileDirectory = account.status !== "existing" && row.choice !== "directory";
  return (
    <li className="@container flex flex-col gap-2 groove-b px-4 py-3 last:border-b-0">
      <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-1.5">
        <div className="flex min-w-40 flex-1 flex-col gap-0.5">
          <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <span className="flex items-start gap-1">
              <span className="min-w-0 font-medium text-foreground [overflow-wrap:anywhere]" dir="auto">{account.name}</span>
              {!settled && (
                <MarkLineBox size="text-sm">
                  <UnusableMark account={account} />
                </MarkLineBox>
              )}
            </span>
            {account.share_default_config && <StatusBadge color="accent" variant="soft">{t("configuration.import.accounts.badge.sharing")}</StatusBadge>}
            {account.disabled && <StatusBadge variant="soft">{t("configuration.import.accounts.badge.disabled")}</StatusBadge>}
          </span>
          {settled && (
            <span className="flex items-start gap-1.5 text-sm text-app-muted">
              <settled.icon aria-hidden="true" className={`mt-0.5 size-3.5 shrink-0 ${settled.tone}`} />
              <span className="min-w-0 [overflow-wrap:anywhere]" dir="auto">{t(settled.label)}</span>
            </span>
          )}
        </div>
        {choices.length > 0 && (
          <div className="w-full @md:w-56">
            <OptionSelect
              hideLabel
              label={t("configuration.import.accounts.choice.label", { account: isolate(account.name) })}
              options={choices.map((choice) => ({ value: choice, label: t(CHOICE_LABEL[choice]) }))}
              value={row.choice}
              onChange={(choice) => onChange({ ...row, choice })}
            />
          </div>
        )}
      </div>
      {showsFileDirectory && (
        <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1 text-sm">
          <Field label={t("configuration.import.accounts.field.directory")} value={account.directory} />
        </dl>
      )}
      {row.choice === "directory" && (
        <DirectoryChoiceField
          label={t("configuration.import.accounts.directory.label", { account: isolate(account.name) })}
          placeholder={t("configuration.import.accounts.directory.placeholder", { example: `${AGENT_DEFAULT_CONFIG_DIR[agent]}-work` })}
          requiredMessage={t("configuration.import.accounts.directory.required")}
          value={row.directory}
          onChange={(directory) => onChange({ ...row, directory })}
          problem={problem}
        />
      )}
    </li>
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

/**
 * The accounts of the document, the panel of the review's Accounts tab, which is there only while the accounts are
 * being imported: what would be added to the added agents and which default accounts would be named, a card for every
 * agent with a row for each of its accounts (its name, with a warning mark after it where it cannot be created as it
 * is in the file, what it shares or has disabled, its status here, the directory the file gives it, and where the
 * status allows a choice, a choice for it), and a note that the accounts are not logged in. An account that exists
 * already is used as it is, never changed, and has no choice or directory. The state of every choice is the caller's.
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
  return (
    <>
      {accounts.agents_to_add.length > 0 && (
        <p className="text-sm">
          <Message id="configuration.import.accounts.toAdd" params={{ count: accounts.agents_to_add.length, agents: <AgentList agents={accounts.agents_to_add} /> }} />
        </p>
      )}
      {named.map((agent) => (
        <p key={agent.agent} className="text-sm">
          <Message
            id="configuration.import.accounts.defaultNamed"
            params={{ agent: <InlineReference dir="auto" wrap>{AGENT_LABEL[agent.agent]}</InlineReference>, name: <InlineReference dir="auto" wrap>{agent.default_name!}</InlineReference> }}
          />
        </p>
      ))}
      {accounts.agents.map((agent, a) => (
        <AgentCard
          key={agent.agent}
          agent={agent.agent}
          accounts={agent.accounts}
          review={review[a]}
          shown={shown}
          onRow={(i, row) => onReview(withAccountRow(review, a, i, row))}
        />
      ))}
      <p className="flex items-start gap-2 text-sm">
        <Info aria-hidden="true" className="mt-0.5 size-4 shrink-0 text-accent-soft-foreground" />
        <span className="min-w-0">{t("configuration.import.accounts.note")}</span>
      </p>
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
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className={CARD_CLASS}>
      <h3 id={headingId} className={`${CARD_HEADER_CLASS} text-base font-semibold text-foreground`}>{AGENT_LABEL[agent]}</h3>
      {accounts.length === 0 ? (
        <p className="px-4 py-3 text-sm text-app-muted">{t("configuration.import.accounts.none")}</p>
      ) : (
        <ul className="flex flex-col">
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
        </ul>
      )}
    </section>
  );
}
