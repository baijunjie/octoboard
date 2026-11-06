import React from "react";

import { useCurrentLanguage, useT } from "../i18n/react";
import type { DocumentAgentSettings } from "../protocol";
import { useProviders } from "../store";
import { AGENT_SWITCHES, type AgentBooleanField } from "./booleanSettings";
import { hasAgentTags } from "./importPlan";
import { BooleanSettingLine, CompositeLine } from "./importParts";
import { agentTagsValues, providerSettingLines, type CurrentProviderSettings } from "./importValues";
import { SettingsGroupCard } from "./SettingsGroups";

/** This machine's values of the Agent accounts settings a document carries, in the shape of the document's own, except
 * that a provider setting left unset is `null`. */
export type CurrentAgentSettings = { [F in AgentBooleanField]: boolean } & Pick<DocumentAgentSettings, "agent_tags"> & { provider_settings: CurrentProviderSettings };

/**
 * The settings of the Agent accounts section a document carries, as one card of lines: the switches, then the provider
 * settings (Claude Code's approval mode, Claude compatibility and the like) and the agent tags. With `current` (this
 * machine's values) it is the review's: each line says what applying `settings` would change, and a line made of
 * several items lists those that would change with what each is now. Without it, it is the result's: each line says
 * what `settings` were applied as, listing the values of those made of several items. `heading` titles the card.
 */
export function AgentSettingsCard({
  settings,
  current,
  heading,
}: {
  settings: DocumentAgentSettings;
  current?: CurrentAgentSettings;
  heading: string;
}): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const providers = useProviders();
  const comparing = current !== undefined;
  return (
    <SettingsGroupCard
      title={heading}
      lines={[
        ...AGENT_SWITCHES.map(({ label, field, consequence }) => (
          <BooleanSettingLine key={field} label={t(label)} wanted={settings[field]} current={current?.[field]} consequence={consequence} />
        )),
        ...providerSettingLines(t, providers, settings.provider_settings, current?.provider_settings).map(({ key, label, values }) => (
          <CompositeLine key={key} label={label} values={values} comparing={comparing} />
        )),
        ...(hasAgentTags(providers, settings.agent_tags)
          ? [<CompositeLine key="agent-tags" label={t("configuration.import.agentTags")} values={agentTagsValues(t, language, providers, settings.agent_tags, current?.agent_tags)} comparing={comparing} />]
          : []),
      ]}
    />
  );
}
