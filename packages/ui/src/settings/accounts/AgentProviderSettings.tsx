import React, { useState } from "react";

import { idWords, providerLabel, servedProvider, switchValue } from "../../agents/providers";
import { alertAppDataFailure } from "../../components/alerts/raiseError";
import { ConfirmDialog } from "../../components/dialog/ConfirmDialog";
import { OptionSelect, type Option } from "../../components/form/OptionSelect";
import { appDataRecovery } from "../../errors/errorRecovery";
import { InlineReference } from "../../i18n/InlineReference";
import { isolate } from "../../i18n/isolate";
import { Message, useT } from "../../i18n/react";
import type { Agent, SettingDescriptor, SettingKind } from "../../protocol";
import { useDaemon, useDaemonStore, useProviders } from "../../store";
import { ProviderName } from "../../components/text/ProviderName";
import { SettingHelp, useSettingCopy } from "./providerSettingCopy";
import { SettingRow, SettingSwitch } from "../SettingRow";

type Choice = Extract<SettingKind, { type: "choice" }>["choices"][number];

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
 * the daemon carries in `settings.provider_settings`, which is the one in effect (the user's choice, else the setting's
 * default); a provider with no entry reads on unless the schema's default is off. */
function SwitchSetting({ agent, descriptor }: { agent: Agent; descriptor: SettingDescriptor }): React.ReactElement | null {
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
        isSelected={switchValue(descriptor, stored)}
        isUnavailable={busy}
        onChange={change}
      />
    </SettingRow>
  );
}

/** An agent's provider settings, one row for each the daemon serves for it (`ProviderInfo.settings`), in the agent's
 * settings dialog: each applies to every account of the agent (see `docs/product/per-agent-settings.md`).
 * A setting the provider's presentation module has no words for is not shown. */
export function AgentProviderSettings({ agent }: { agent: Agent }): React.ReactElement {
  const provider = servedProvider(useProviders(), agent);
  return (
    <>
      {provider?.settings.map((descriptor) => descriptor.kind.type === "choice"
        ? <ChoiceSetting key={descriptor.id} agent={agent} descriptor={descriptor} choices={descriptor.kind.choices} />
        : <SwitchSetting key={descriptor.id} agent={agent} descriptor={descriptor} />)}
    </>
  );
}
