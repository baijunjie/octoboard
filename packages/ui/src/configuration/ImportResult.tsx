import React, { useState } from "react";

import { providerLabel } from "../agents/providers";
import { ErrorAlert } from "../components/alerts/ErrorAlert";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/announce/StatusAnnouncer";
import type { PlainMessageKey } from "../i18n/catalog";
import { Message, useT } from "../i18n/react";
import type { ReplyOf } from "../requests";
import type { DirectoryChoiceKind } from "../protocol";
import type { Overrides } from "../shortcuts/store";
import { AgentSettingsCard } from "./AgentSettingsCard";
import type { ClientSettingValue } from "./clientSettings";
import { useFailureNode } from "./failureText";
import { accountsOutcomeOf, outcomeOf } from "./importPlan";
import { AgentList, CardRows, EmptyLine, NamedValue, SettingLine, ToneLine } from "./importParts";
import { shortcutValues } from "./importValues";
import { RegionTabs, type Region, type RegionPanel } from "./RegionTabs";
import { Outcome, ResultCard, ResultRow } from "./ResultCards";
import { SettingsGroupCard, SettingsGroups } from "./SettingsGroups";
import { useProviders } from "../store";
import { ProviderName } from "../components/text/ProviderName";

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

/**
 * What an import came to, as the daemon reported it: the regions the document carried as tabs (a single one stands
 * without a tab bar; see `RegionTabs`), each with a warning glyph on its tab when it holds a failure or a warning, and
 * the first such tab forward. Consoles and projects: a line of their totals, then a card for every console, as in the
 * review: its name and always its own outcome (created, or failed and why), a row for each of its projects (what came
 * of it: created and how it was placed, left out, failed, not tried; the reason of a failure written under the row),
 * and a warning mark after the name of every console or project whose account references fell back to a default
 * account. Accounts: a line of their totals, then a card for every agent with a row for each of its accounts (created,
 * already existing, left out, or failed; the reason written under the row), the agents added, the default accounts
 * named and disabled or enabled, an alert when the agent account settings could not be applied or else a card of what
 * was applied (the provider settings and agent tags listing the values they were set to), and, when accounts were
 * created, a note that they are not logged in. Application settings: an alert when they could not be applied, beside
 * it a card of the default clone directory when that had been written already, else cards of what was applied, grouped
 * and titled as in the review (see `SettingsGroups`), where the git workflow and keyboard shortcuts also list the
 * values they were set to. The totals lines give the counts and the cards the items, so neither repeats the other.
 * `shortcuts` says what came of the document's keyboard shortcuts, when they were applied here: how many, how many
 * were skipped, and the overrides that were set. `clientSettings` are the settings only this client holds that were
 * applied here, worded for the cards.
 */
export function ImportResult({
  result,
  shortcuts,
  clientSettings = [],
}: {
  result: Imported;
  shortcuts?: { applied: number; skipped: number; overrides: Overrides };
  clientSettings?: ClientSettingValue[];
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
            <CardRows>
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
            </CardRows>
          )}
        </ResultCard>
      ))}
    </>
  );
  const accountsContent = accounts && (
    <>
      <RegionTotals label={t("configuration.import.result.totals.accounts")} items={accountsTotals} />
      {accounts.agents.filter((agent) => agent.accounts.length > 0).map((agent) => (
        <ResultCard key={agent.agent} name={providerLabel(t, providers, agent.agent)}>
          <CardRows>
            {agent.accounts.map((account, i) => (
              <ResultRow
                key={i}
                name={account.name}
                status={<Outcome status={account.status} />}
                failure={account.status === "failed" ? account.error : undefined}
              />
            ))}
          </CardRows>
        </ResultCard>
      ))}
      {accounts.agents_added.length > 0 && (
        <AgentList lead={t("configuration.import.result.accounts.added", { count: accounts.agents_added.length })} agents={accounts.agents_added} />
      )}
      {accounts.agents.filter((agent) => agent.default_name_applied).map((agent) => (
        <p key={agent.agent} className="text-sm">
          <Message id="configuration.import.result.accounts.defaultNamed" params={{ agent: <ProviderName agent={agent.agent} /> }} />
        </p>
      ))}
      {accounts.agents.filter((agent) => agent.default_disabled_applied !== null).map((agent) => (
        <p key={agent.agent} className="text-sm">
          <Message
            id={agent.default_disabled_applied ? "configuration.import.result.accounts.defaultDisabled" : "configuration.import.result.accounts.defaultEnabled"}
            params={{ agent: <ProviderName agent={agent.agent} /> }}
          />
        </p>
      ))}
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
      {accounts.agent_settings_applied && (
        <AgentSettingsCard settings={accounts.agent_settings_applied} heading={t("configuration.import.agentSettings.heading")} />
      )}
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
          cloneDir={
            applied.default_clone_dir !== null ? (
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
  // A region is there when the document carried it: its consoles, the accounts asked for, the settings that came to
  // something (applied or failed). An import of a document with no console at all has no consoles region.
  const regions: RegionPanel[] = [];
  if (result.consoles.length > 0) {
    regions.push({
      id: "consoles",
      label: t("configuration.import.tab.consoles"),
      attention: outcome.consolesFailed + outcome.projectsFailed + outcome.projectsNotAttempted > 0,
      content: consolesContent,
    });
  }
  if (accounts) {
    regions.push({
      id: "accounts",
      label: t("configuration.import.tab.accounts"),
      attention: (accountsOutcome?.failed ?? 0) > 0 || !!accounts.agent_settings_error,
      content: accountsContent,
    });
  }
  if (applied || result.settings_error || result.clone_dir_applied) {
    regions.push({ id: "settings", label: t("configuration.import.tab.settings"), attention: !!result.settings_error, content: settingsContent });
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
