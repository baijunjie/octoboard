import React, { useState } from "react";

import { providerLabel } from "../agents/providers";
import { ErrorAlert } from "../components/alerts/ErrorAlert";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/announce/StatusAnnouncer";
import type { PlainMessageKey } from "../i18n/catalog";
import { Message, useT } from "../i18n/react";
import type { ReplyOf } from "../requests";
import type { Agent, DirectoryChoiceKind, ImportedAccounts } from "../protocol";
import type { Overrides } from "../shortcuts/store";
import { AgentCardSections, AgentSettingsCard, useAgentSettingLines } from "./AgentSettingsCard";
import type { ClientSettingValue } from "./clientSettings";
import { useFailureNode } from "./failureText";
import { accountsOutcomeOf, cardAgents, outcomeOf } from "./importPlan";
import { FlatCard } from "../components/card/FlatCard";
import { CompositeLine, EmptyLine, NamedValue, SettingLine, ToneLine } from "./importParts";
import { shortcutValues, type SettingValue } from "./importValues";
import { RegionTabs, type Region, type RegionPanel } from "./RegionTabs";
import { Outcome, ResultCard, ResultRow } from "./ResultCards";
import { SettingsGroupCard, SettingsGroups } from "./SettingsGroups";
import { useProviders } from "../store";

export type Imported = ReplyOf<"import_configuration">;

const VIA_LABEL: Record<DirectoryChoiceKind, PlainMessageKey> = {
  exported_path: "configuration.import.result.via.exportedPath",
  default_clone_dir: "configuration.import.result.via.defaultCloneDir",
  clone: "configuration.import.result.via.clone",
  directory: "configuration.import.result.via.directory",
};

/** One region's totals on a line: each item reads on its own, the dot between them is only for the eye, and the line
 * wraps when it runs out of room. */
function RegionTotals({ label, items }: { label: string; items: string[] }): React.ReactElement | null {
  if (items.length === 0) return null;
  return (
    <ul aria-label={label} className="flex flex-wrap gap-x-2 text-sm">
      {items.map((item, index) => (
        <li key={item}>
          {item}
          {index < items.length - 1 && <span aria-hidden="true" className="ms-2">·</span>}
        </li>
      ))}
    </ul>
  );
}

/** A directory on a line of its own in a settings card's line, as plain monospace text in the line's muted style. */
function DirectoryValue({ children }: { children: string }): React.ReactElement {
  return <div className="align-match-parent font-mono [overflow-wrap:anywhere]" dir="ltr">{children}</div>;
}

/** One agent's result card: that it was added to the agent list, its default account's changes, the provider settings
 * and tags applied to it and its accounts, each with what came of it, as `AgentCardSections` lays them out; nothing
 * where there is none of them. `listed` is the agent's entry of the accounts reported, absent for an agent only the
 * settings applied or its being added name. */
function AgentResult({
  agent,
  listed,
  settings,
  added,
}: {
  agent: Agent;
  listed: ImportedAccounts["agents"][number] | undefined;
  settings: ImportedAccounts["agent_settings_applied"];
  /** Whether the import added the agent to the agent list. */
  added: boolean;
}): React.ReactElement | null {
  const t = useT();
  const providers = useProviders();
  const lines = useAgentSettingLines(agent, settings);
  const defaults: React.ReactNode[] = [];
  if (added) {
    defaults.push(
      <p key="added" className="text-sm">
        <Message id="configuration.import.result.accounts.agentWasAdded" />
      </p>,
    );
  }
  if (listed?.default_name_applied) {
    defaults.push(
      <p key="named" className="text-sm">
        <Message id="configuration.import.result.accounts.defaultAccountNamed" />
      </p>,
    );
  }
  if (listed && listed.default_disabled_applied !== null) {
    defaults.push(
      <p key="switched" className="text-sm">
        <Message
          id={listed.default_disabled_applied ? "configuration.import.result.accounts.defaultAccountDisabled" : "configuration.import.result.accounts.defaultAccountEnabled"}
        />
      </p>,
    );
  }
  const rows = (listed?.accounts ?? []).map((account, i) => (
    <ResultRow key={i} name={account.name} status={<Outcome status={account.status} />} failure={account.status === "failed" ? account.error : undefined} />
  ));
  if (defaults.length === 0 && lines.length === 0 && rows.length === 0) return null;
  return (
    <ResultCard name={providerLabel(t, providers, agent)}>
      <AgentCardSections defaults={defaults} lines={lines} accounts={rows} />
    </ResultCard>
  );
}

/**
 * What an import came to, as the daemon reported it: the regions the document carried as tabs (a single one stands
 * without a tab bar; see `RegionTabs`), each with a warning glyph on its tab when it holds a failure or a warning, and
 * the first such tab forward. Consoles and projects: a line of their totals, then a card for every console, as in the
 * review: its name and always its own outcome (created, or failed and why), a row for each of its projects (what came
 * of it: created and how it was placed, left out, failed, not tried; the reason of a failure written under the row),
 * and a warning mark after the name of every console or project whose account references fell back to a default
 * account. Settings: an alert when they could not be applied, beside it a card of the default clone directory when that
 * had been written already, else cards of what was applied, grouped and titled as in the review (see `SettingsGroups`),
 * where the git workflow and keyboard shortcuts also list the values they were set to. Accounts: a line of their
 * totals, then an alert when the agent account settings could not be applied, the card of the switches that were
 * applied, a card for every agent that has anything (that it was added to the agent list, the default account named and
 * disabled or enabled, the provider settings and tag applied, listing the values they were set to, and a row for each
 * of its accounts: created, already existing, left out, or failed; the reason written under the row), and, when
 * accounts were created, a note that they are not logged in. The totals lines give the counts and the cards the items,
 * so neither repeats the other. `shortcuts` says what came of the document's keyboard shortcuts, when they were applied
 * here: how many, how many were skipped, and the overrides that were set. `clientSettings` are the settings only this
 * client holds that were applied here, worded for the cards, and `sorting` the items of the session order that were
 * applied.
 */
export function ImportResult({
  result,
  shortcuts,
  clientSettings = [],
  sorting,
}: {
  result: Imported;
  shortcuts?: { applied: number; skipped: number; overrides: Overrides };
  clientSettings?: ClientSettingValue[];
  sorting?: SettingValue[];
}): React.ReactElement {
  const t = useT();
  const providers = useProviders();
  const failureNode = useFailureNode();
  const outcome = outcomeOf(result.consoles);
  const accounts = result.accounts;
  const accountsOutcome = accounts ? accountsOutcomeOf(accounts) : undefined;
  const applied = result.settings_applied;
  const [tab, setTab] = useState<Region>();
  // One item per thing that happened, so a count and what it counts stay together; a thing that did not happen has
  // none, and "nothing failed" is said once instead of by a zero. Each region's totals depend on that region's own
  // failures alone: the settings are told of on their own, and a failure there is not one of the consoles or accounts.
  const consolesFailures = outcome.consolesFailed + outcome.projectsFailed;
  const consolesTotals = [
    outcome.consolesCreated > 0 && t("configuration.import.result.consolesCreated", { count: outcome.consolesCreated }),
    outcome.projectsCreated > 0 && t("configuration.import.result.projectsCreated", { count: outcome.projectsCreated }),
    outcome.projectsLeftOut > 0 && t("configuration.import.result.projectsLeftOut", { count: outcome.projectsLeftOut }),
    consolesFailures > 0 && t("configuration.import.result.failedCount", { count: consolesFailures }),
    outcome.projectsNotAttempted > 0 && t("configuration.import.result.notAttempted", { count: outcome.projectsNotAttempted }),
    consolesFailures === 0 && outcome.projectsNotAttempted === 0 && t("configuration.import.result.noFailures"),
  ].filter((item) => typeof item === "string");
  const accountsTotals = accountsOutcome
    ? [
      accountsOutcome.created > 0 && t("configuration.import.result.accountsCreated", { count: accountsOutcome.created }),
      accountsOutcome.existing > 0 && t("configuration.import.result.accountsExisting", { count: accountsOutcome.existing }),
      accountsOutcome.leftOut > 0 && t("configuration.import.result.accountsLeftOut", { count: accountsOutcome.leftOut }),
      accountsOutcome.failed > 0 && t("configuration.import.result.failedCount", { count: accountsOutcome.failed }),
      accountsOutcome.failed === 0 && !accounts?.agent_settings_error && t("configuration.import.result.noFailures"),
    ].filter((item) => typeof item === "string")
    : [];

  const consolesContent = (
    <>
      <RegionTotals label={t("configuration.import.result.totals.consoles")} items={consolesTotals} />
      {result.consoles.map((console, index) => (
        <ResultCard key={index} name={console.name} fallbacks={console.account_fallbacks} status={<Outcome status={console.status} />} failure={console.error}>
          {console.projects.length === 0 ? (
            <EmptyLine>{t("configuration.import.console.noProjects")}</EmptyLine>
          ) : (
            <FlatCard.Rows>
              {console.projects.map((project, p) => (
                <ResultRow
                  key={p}
                  name={project.name}
                  fallbacks={project.account_fallbacks}
                  status={<Outcome status={project.status} />}
                  detail={
                    project.status === "created" && project.via
                      ? t(VIA_LABEL[project.via])
                      : project.status === "not_attempted"
                        ? t("configuration.import.result.detail.consoleFailed")
                        : undefined
                  }
                  failure={project.status === "failed" ? project.error : undefined}
                />
              ))}
            </FlatCard.Rows>
          )}
        </ResultCard>
      ))}
    </>
  );
  const accountsContent = accounts && (
    <>
      <RegionTotals label={t("configuration.import.result.totals.accounts")} items={accountsTotals} />
      {accounts.agent_settings_error && (
        <ErrorAlert
          announce
          announceDelayMs={AFTER_FOCUS_MOVE_DELAY_MS}
          title={t("configuration.import.result.agentSettings.failedTitle")}
          remedy={failureNode(accounts.agent_settings_error, "remedy")}
          details={failureNode(accounts.agent_settings_error, "details")}>
          <Message id="configuration.import.result.agentSettings.failed" params={{ reason: failureNode(accounts.agent_settings_error, "summary") }} />
        </ErrorAlert>
      )}
      <AgentSettingsCard
        settings={accounts.agent_settings_applied}
        heading={t("configuration.import.agentSettings.heading")}
      />
      {cardAgents(providers, accounts.agent_settings_applied, accounts.agents.map(({ agent }) => agent), accounts.agents_added).map((agent) => (
        <AgentResult key={agent} agent={agent} listed={accounts.agents.find((applied) => applied.agent === agent)} settings={accounts.agent_settings_applied} added={accounts.agents_added.includes(agent)} />
      ))}
      {accountsOutcome && accountsOutcome.created > 0 && (
        <ToneLine>{t("configuration.import.result.accounts.loginNote")}</ToneLine>
      )}
    </>
  );
  const settingsContent = (
    <>
      {result.settings_error && (
        <ErrorAlert
          announce
          announceDelayMs={AFTER_FOCUS_MOVE_DELAY_MS}
          title={t("configuration.import.result.settings.failedTitle")}
          remedy={failureNode(result.settings_error, "remedy")}
          details={failureNode(result.settings_error, "details")}>
          <Message id="configuration.import.result.settings.failed" params={{ reason: failureNode(result.settings_error, "summary") }} />
        </ErrorAlert>
      )}
      {/* The directory is written before the rest, so it stays when the rest could not be applied. */}
      {!applied && result.clone_dir_applied && (
        <SettingsGroupCard
          title={t("settings.section.git")}
          lines={[
            <SettingLine key="clone-dir" label={t("settings.cloneDir.label")} badge={t("configuration.import.result.badge.matches")}>
              <div>{t("configuration.import.result.cloneDir.beforeFailure")}</div>
              <DirectoryValue>{result.clone_dir_applied}</DirectoryValue>
            </SettingLine>,
          ]}
        />
      )}
      {applied && (
        <SettingsGroups
          settings={applied}
          clientSettings={clientSettings}
          clientLine={({ label, value, lang }) => <SettingLine label={t(label)} badge={<NamedValue text={value} lang={lang} />} />}
          sorting={sorting && <CompositeLine label={t("settings.sort.group")} values={sorting} comparing={false} />}
          cloneDir={
            applied.default_clone_dir != null ? (
              <SettingLine label={t("settings.cloneDir.label")} badge={t("configuration.import.result.badge.matches")}>
                <DirectoryValue>{applied.default_clone_dir}</DirectoryValue>
              </SettingLine>
            ) : undefined
          }
          shortcuts={
            shortcuts && (
              <SettingLine
                label={t("configuration.import.settings.shortcutsLine")}
                values={shortcutValues(t, shortcuts.overrides)}
                badge={shortcuts.applied === 0 ? t("configuration.import.result.badge.defaults") : t("configuration.import.result.badge.shortcuts", { count: shortcuts.applied })}
              >
                {shortcuts.skipped > 0 && t("configuration.import.result.shortcutsSkipped", { count: shortcuts.skipped })}
              </SettingLine>
            )
          }
        />
      )}
    </>
  );
  // A region is there when the document carried it: its consoles, the settings that came to something (applied or
  // failed), the accounts asked for. An import of a document with no console at all has no consoles region.
  const regions: RegionPanel[] = [];
  if (result.consoles.length > 0) {
    regions.push({
      id: "consoles",
      label: t("configuration.import.tab.consoles"),
      attention: outcome.consolesFailed + outcome.projectsFailed + outcome.projectsNotAttempted > 0,
      content: consolesContent,
    });
  }
  if (applied || result.settings_error || result.clone_dir_applied) {
    regions.push({ id: "settings", label: t("settings.title"), attention: !!result.settings_error, content: settingsContent });
  }
  if (accounts) {
    regions.push({
      id: "accounts",
      label: t("settings.section.accounts"),
      attention: (accountsOutcome?.failed ?? 0) > 0 || !!accounts.agent_settings_error,
      content: accountsContent,
    });
  }

  return (
    <RegionTabs
      label={t("configuration.import.tabs.result")}
      regions={regions}
      selected={tab ?? regions.find((region) => region.attention)?.id}
      onSelect={setTab}
    />
  );
}
