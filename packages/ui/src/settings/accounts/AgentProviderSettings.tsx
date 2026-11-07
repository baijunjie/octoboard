import React, { useState } from "react";

import { idWords, providerLabel, servedProvider, settingMessageKey } from "../../agents/providers";
import { alertAppDataFailure } from "../../components/alerts/raiseError";
import { ConfirmDialog } from "../../components/dialog/ConfirmDialog";
import { OptionSelect, type Option } from "../../components/form/OptionSelect";
import { HelpTip } from "../../components/hint/HelpTip";
import { appDataRecovery } from "../../errors/errorRecovery";
import type { MessageKey } from "../../i18n/catalog";
import { InlineReference } from "../../i18n/InlineReference";
import { isolate } from "../../i18n/isolate";
import { Message, useT } from "../../i18n/react";
import type { Agent, SettingDescriptor, SettingKind } from "../../protocol";
import { settingReferences } from "../../providers/registry";
import { useDaemon, useDaemonStore, useProviders } from "../../store";
import { ProviderName } from "../../components/text/ProviderName";
import { SettingRow, SettingSwitch } from "../SettingRow";

type Choice = Extract<SettingKind, { type: "choice" }>["choices"][number];

/** What a setting's copy words it with: the provider's own messages for it (see `settingMessageKey`), and the provider's
 * name as `{agent}` in each, with the literals its description names set apart as references. */
function useSettingCopy(agent: Agent, setting: string) {
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
    text: (name: string) => {
      const found = key(name);
      return found && (t as (key: MessageKey, params?: Record<string, string>) => string)(found, { agent: isolate(providerLabel(t, providers, agent)) });
    },
    node: (name: string) => {
      const found = key(name);
      return found && <Message id={found} params={{ agent: agentName, ...references } as never} />;
    },
    saveFailedTitle: () => {
      const found = key("saveFailedTitle");
      return found ? (t as (key: MessageKey) => string)(found) : t("settings.accounts.setting.saveFailedTitle");
    },
  };
}

/** The setting row's help tip, saying what the setting does in the provider's words. */
function SettingHelp({ copy }: { copy: ReturnType<typeof useSettingCopy> }): React.ReactElement | null {
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

/** A choice setting: inherited native behavior plus the setting's declared choices, saved when selected. A choice
 * the schema marks `confirm` is saved only once its confirmation dialog is accepted; cancelling leaves the stored value
 * as it was. */
function ChoiceSetting({ agent, descriptor, choices }: { agent: Agent; descriptor: SettingDescriptor; choices: Choice[] }): React.ReactElement | null {
  const t = useT();
  const providers = useProviders();
  const { request } = useDaemon();
  const copy = useSettingCopy(agent, descriptor.id);
  const stored = useDaemonStore((s) => s.settings.provider_settings[agent]?.[descriptor.id]);
  const [busy, setBusy] = useState(false);
  const [confirming, setConfirming] = useState<string>();
  const heading = copy.text("heading");
  if (!heading) return null;
  const save = (value: string | null) => request({ type: "set_provider_setting", agent, setting: descriptor.id, value });
  const choiceLabel = (value: string) => copy.text(`choice.${value}`) ?? idWords(value);
  const option = (value: string, label: string): Option<string> => {
    const description = copy.text(value === "inherit" ? "inheritSummary" : `choice.${value}.summary`)
      ?? copy.text(value === "inherit" ? "inheritDescription" : `choice.${value}.description`);
    const base = { value, label, isDisabled: busy };
    return description ? { ...base, detail: description, reason: { state: description } } : base;
  };
  const choose = (picked: string) => {
    if (picked !== "inherit" && choices.find((choice) => choice.value === picked)?.confirm) {
      setConfirming(picked);
      return;
    }
    setBusy(true);
    void save(picked === "inherit" ? null : picked)
      .catch(alertAppDataFailure(copy.saveFailedTitle()))
      .finally(() => setBusy(false));
  };
  // The trigger must stay focusable while the popover returns keyboard focus after selection.
  return (
    <SettingRow
      fill
      label={heading}
      help={<SettingHelp copy={copy} />}
      description={t("settings.accounts.setting.description", { agent: isolate(providerLabel(t, providers, agent)) })}
    >
      <div className="w-64 max-w-full">
        <OptionSelect<string>
          hideLabel
          label={copy.text("label") ?? heading}
          value={typeof stored === "string" ? stored : "inherit"}
          onChange={choose}
          options={[
            option("inherit", copy.text("inherit") ?? t("settings.accounts.setting.inherit")),
            ...choices.map((choice) => option(choice.value, choiceLabel(choice.value))),
          ]}
        />
        {confirming !== undefined && (
          <ConfirmDialog
            title={copy.text(`confirm.${confirming}.title`) ?? choiceLabel(confirming)}
            message={<p>{copy.node(`confirm.${confirming}.consequence`) ?? copy.node(`confirm.${confirming}.message`) ?? <Message id="settings.accounts.setting.confirmMessage" params={{ setting: <InlineReference dir="auto" wrap>{heading}</InlineReference>, choice: <InlineReference dir="auto" wrap>{choiceLabel(confirming)}</InlineReference>, agent: <ProviderName agent={agent} /> }} />}</p>}
            confirmLabel={choiceLabel(confirming)}
            destructive
            errorTitle={copy.saveFailedTitle()}
            errorRecovery={appDataRecovery}
            onConfirm={async () => {
              await save(confirming);
              setConfirming(undefined);
            }}
            onCancel={() => setConfirming(undefined)}
          />
        )}
      </div>
    </SettingRow>
  );
}

/** A switch setting, saved when toggled; the switch stays unavailable while the save is unanswered. It shows the value
 * the daemon carries in `settings.provider_settings`, which is the one in effect (a default that follows the added
 * providers included); a provider with no entry reads on unless the schema's default is off. */
function SwitchSetting({ agent, descriptor, fallback }: { agent: Agent; descriptor: SettingDescriptor; fallback: boolean }): React.ReactElement | null {
  const { request } = useDaemon();
  const t = useT();
  const providers = useProviders();
  const copy = useSettingCopy(agent, descriptor.id);
  const stored = useDaemonStore((s) => s.settings.provider_settings[agent]?.[descriptor.id]);
  const [busy, setBusy] = useState(false);
  const heading = copy.text("heading");
  if (!heading) return null;
  const change = (value: boolean) => {
    setBusy(true);
    void request({ type: "set_provider_setting", agent, setting: descriptor.id, value })
      .catch(alertAppDataFailure(copy.saveFailedTitle()))
      .finally(() => setBusy(false));
  };
  return (
    <SettingRow
      label={heading}
      help={<SettingHelp copy={copy} />}
      description={t("settings.accounts.setting.description", { agent: isolate(providerLabel(t, providers, agent)) })}
      compact
    >
      <SettingSwitch
        label={copy.text("label") ?? heading}
        isSelected={typeof stored === "boolean" ? stored : fallback}
        isUnavailable={busy}
        onChange={change}
      />
    </SettingRow>
  );
}

/** An agent's provider settings, one row for each the daemon serves for it (`ProviderInfo.settings`), above its accounts
 * table: each applies to every account of the agent (see `docs/product/per-agent-settings.md`).
 * A setting the provider's presentation module has no words for is not shown. */
export function AgentProviderSettings({ agent }: { agent: Agent }): React.ReactElement {
  const provider = servedProvider(useProviders(), agent);
  return (
    <>
      {provider?.settings.map((descriptor) => descriptor.kind.type === "choice"
        ? <ChoiceSetting key={descriptor.id} agent={agent} descriptor={descriptor} choices={descriptor.kind.choices} />
        : <SwitchSetting key={descriptor.id} agent={agent} descriptor={descriptor} fallback={descriptor.kind.default !== "off"} />)}
    </>
  );
}
