import React from "react";

import { compatibilitySettings, type Compatibility, type CompatibilityChanges } from "../agents/launchCompatibility";
import { switchValue } from "../agents/providers";
import { GroupRow, GroupRows } from "../components/form/GroupRows";
import { FollowSwitchSelect } from "../components/form/FollowSwitchSelect";
import type { PlainMessageKey } from "../i18n/catalog";
import { useT } from "../i18n/react";
import type { Agent, SettingDescriptor } from "../protocol";
import { SettingHelp, useSettingCopy } from "../settings/accounts/providerSettingCopy";
import { useDaemonStore, useProviders } from "../store";

export type CompatibilityFieldProps = {
  agent: Agent;
  value: CompatibilityChanges;
  onChange: (value: CompatibilityChanges) => void;
  inherit?: Compatibility;
  followLabel?: PlainMessageKey;
  isDisabled?: boolean;
};

/** A compatibility switch without its visible heading, for a standalone row or a launch matrix cell. */
export function CompatibilityPicker({ agent, descriptor, value, inherit, followLabel = "dialog.agentSettings.sameAsAgent", onChange, isDisabled, labelledBy, describedBy }: {
  agent: Agent;
  descriptor: SettingDescriptor;
  value: boolean | null;
  inherit?: boolean;
  followLabel?: PlainMessageKey;
  onChange: (value: boolean | null) => void;
  isDisabled?: boolean;
  labelledBy?: string;
  describedBy?: string;
}): React.ReactElement | null {
  const t = useT();
  const copy = useSettingCopy(agent, descriptor.id);
  const stored = useDaemonStore((s) => s.settings.provider_settings[agent]?.[descriptor.id]);
  const heading = copy.text("heading");
  if (!heading) return null;
  return <FollowSwitchSelect label={copy.text("label") ?? heading} labelledBy={labelledBy} describedBy={describedBy}
    value={value} inherit={inherit ?? switchValue(descriptor, stored)} followLabel={t(followLabel)}
    onLabel={t("dialog.agentSettings.compatibilityOn")} offLabel={t("dialog.agentSettings.compatibilityOff")}
    onChange={onChange} isDisabled={isDisabled} />;
}

function CompatibilityRow(props: React.ComponentProps<typeof CompatibilityPicker>): React.ReactElement | null {
  const copy = useSettingCopy(props.agent, props.descriptor.id);
  const heading = copy.text("heading");
  if (!heading) return null;
  return (
    <GroupRow label={heading} help={<SettingHelp copy={copy} />}>
      <CompatibilityPicker {...props} />
    </GroupRow>
  );
}

/** The selected agent's switches, in the provider's declared order. */
export function AgentCompatibilityField({ agent, value, onChange, inherit, followLabel = "dialog.agentSettings.sameAsAgent", isDisabled }: CompatibilityFieldProps): React.ReactElement | null {
  const settings = compatibilitySettings(useProviders(), agent);
  if (settings.length === 0) return null;
  return (
    <GroupRows stacked>
      {settings.map((descriptor) => <CompatibilityRow key={descriptor.id} agent={agent} descriptor={descriptor}
        value={value[descriptor.id] ?? null} inherit={inherit?.[descriptor.id]} followLabel={followLabel}
        onChange={(held) => onChange({ ...value, [descriptor.id]: held })} isDisabled={isDisabled} />)}
    </GroupRows>
  );
}
