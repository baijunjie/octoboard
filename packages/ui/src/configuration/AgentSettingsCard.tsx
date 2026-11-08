import React from "react";

import { useCurrentLanguage, useT } from "../i18n/react";
import type { Agent, DocumentAgentSettings } from "../protocol";
import { useProviders } from "../store";
import { AGENT_SWITCHES, type AgentBooleanField } from "./booleanSettings";
import { FlatCard } from "../components/card/FlatCard";
import { BooleanSettingLine, CompositeLine } from "./importParts";
import { agentTagsValues, providerSettingLines, type CurrentProviderSettings } from "./importValues";
import { SettingsGroupCard } from "./SettingsGroups";

/** This machine's values of the Agent accounts settings a document carries, in the shape of the document's own, except
 * that a provider setting left unset is `null`. */
export type CurrentAgentSettings = { [F in AgentBooleanField]: boolean } & Pick<DocumentAgentSettings, "agent_tags"> & { provider_settings: CurrentProviderSettings };

/**
 * The settings of the Agent accounts section that are not any one agent's, a document's three switches, as one card of
 * lines. With `current` (this machine's values) it is the review's: each line says what applying `settings` would
 * change. Without it, it is the result's: each line says what `settings` were applied as. `heading` titles the card.
 * Without `settings` there is no card. The provider settings and tags are each agent's, in `useAgentSettingLines`.
 */
export function AgentSettingsCard({
  settings,
  current,
  heading,
}: {
  settings?: DocumentAgentSettings | null;
  current?: CurrentAgentSettings;
  heading: string;
}): React.ReactElement | null {
  const t = useT();
  return (
    <SettingsGroupCard
      title={heading}
      lines={AGENT_SWITCHES.flatMap(({ label, field, consequence }) =>
        settings ? [<BooleanSettingLine key={field} label={t(label)} wanted={settings[field]} current={current?.[field]} consequence={consequence} />] : [],
      )}
    />
  );
}

/**
 * The lines of one agent's provider settings (Claude Code's approval mode, Claude compatibility and the like), then its
 * tags, each named by the setting alone and holding the agent's value, as lines of its card. With `current` (this
 * machine's values) they are the review's: each line says what applying `settings` would change and lists the value that
 * would change with what it is now. Without it, they are the result's: each line says what `settings` were applied as,
 * listing the value. No line for what `settings` leaves without a value for the agent.
 */
export function useAgentSettingLines(
  agent: Agent,
  settings: Pick<DocumentAgentSettings, "provider_settings" | "agent_tags"> | null | undefined,
  current?: Pick<CurrentAgentSettings, "provider_settings" | "agent_tags">,
): React.ReactElement[] {
  const t = useT();
  const language = useCurrentLanguage();
  const providers = useProviders();
  if (!settings) return [];
  const comparing = current !== undefined;
  return [
    ...providerSettingLines(t, providers, agent, settings.provider_settings[agent], current?.provider_settings).map(({ key, label, values }) => (
      <CompositeLine key={key} label={label} values={values} comparing={comparing} />
    )),
    ...(settings.agent_tags[agent]
      ? [<CompositeLine key="agent-tags" label={t("settings.accounts.tags.heading")} values={agentTagsValues(t, language, agent, settings.agent_tags, current?.agent_tags)} comparing={comparing} />]
      : []),
  ];
}

/**
 * What an agent's card holds under its heading, in this order: `defaults`, the lines about its default account
 * (paragraphs), `lines` (see `useAgentSettingLines`), and `accounts`, its account rows, under an "Accounts"
 * subheading (a `FlatCard.Band`) where something stands above them to be told apart from. Each part is left out when
 * empty.
 */
export function AgentCardSections({ defaults, lines, accounts }: { defaults: React.ReactNode[]; lines: React.ReactNode[]; accounts: React.ReactNode[] }): React.ReactElement {
  const t = useT();
  const settled = defaults.length > 0 || lines.length > 0;
  return (
    <>
      {settled && (
        <FlatCard.Blocks>
          {defaults.length > 0 && <div className="flex flex-col gap-1 py-3">{defaults}</div>}
          {lines.length > 0 && <FlatCard.Rows padded>{lines}</FlatCard.Rows>}
        </FlatCard.Blocks>
      )}
      {accounts.length > 0 && (
        <>
          {settled && <FlatCard.Band>{t("configuration.import.accounts.heading")}</FlatCard.Band>}
          <FlatCard.Rows>{accounts}</FlatCard.Rows>
        </>
      )}
    </>
  );
}
