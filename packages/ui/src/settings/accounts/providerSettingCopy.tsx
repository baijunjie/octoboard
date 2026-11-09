import React from "react";

import { providerLabel, settingMessageKey } from "../../agents/providers";
import { HelpTip } from "../../components/hint/HelpTip";
import { ProviderName } from "../../components/text/ProviderName";
import type { MessageKey, Translate } from "../../i18n/catalog";
import { InlineReference } from "../../i18n/InlineReference";
import { isolate } from "../../i18n/isolate";
import { Message, useT } from "../../i18n/react";
import type { Agent, ProviderInfo } from "../../protocol";
import { settingReferences } from "../../providers/registry";
import { useProviders } from "../../store";

/** A provider setting's plain copy, substituting the agent name where its message asks for it. */
export function settingText(t: Translate, providers: readonly ProviderInfo[], agent: Agent, setting: string, name: string): string | undefined {
  const key = settingMessageKey(agent, setting, name);
  return key && (t as (key: MessageKey, params?: Record<string, string>) => string)(key, { agent: isolate(providerLabel(t, providers, agent)) });
}

/** What a setting's copy words it with: the provider's own messages for it (see `settingMessageKey`), and the provider's
 * name as `{agent}` in each, with the literals its description names set apart as references. */
export function useSettingCopy(agent: Agent, setting: string) {
  const t = useT();
  const providers = useProviders();
  const key = (name: string) => settingMessageKey(agent, setting, name);
  const agentName = <ProviderName agent={agent} />;
  const references = Object.fromEntries(Object.entries(settingReferences(agent, setting)).map(([placeholder, text]) => [
    placeholder,
    <InlineReference key={placeholder} dir="ltr" monospace wrap>{text}</InlineReference>,
  ]));
  return {
    key,
    text: (name: string) => settingText(t, providers, agent, setting, name),
    node: (name: string) => {
      const found = key(name);
      return found && <Message id={found} params={{ agent: agentName, ...references } as never} />;
    },
    saveFailedTitle: () => settingText(t, providers, agent, setting, "saveFailedTitle") ?? t("settings.accounts.setting.saveFailedTitle"),
  };
}

/** The setting row's help tip, saying what the setting does in the provider's words. */
export function SettingHelp({ copy }: { copy: ReturnType<typeof useSettingCopy> }): React.ReactElement | null {
  const label = copy.text("helpLabel");
  const description = copy.node("helpTiming") ?? copy.node("description");
  const custom = copy.node("helpCustom");
  return label && description ? (
    <HelpTip label={label}>
      {custom ? (
        <div className="flex flex-col gap-2">
          <p>{description}</p>
          <p>{custom}</p>
        </div>
      ) : description}
    </HelpTip>
  ) : null;
}

