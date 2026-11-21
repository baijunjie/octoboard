import React from "react";

import { defaultAccountName } from "../agents/agents";
import { compatibilitySettings } from "../agents/launchCompatibility";
import { providerLabel, providerModelDisplay } from "../agents/providers";
import { useT } from "../i18n/react";
import type { Agent, ScheduledTaskLaunch } from "../protocol";
import { sessionModelValues } from "../sessions/sessionModel";
import { sessionDisplayTitle } from "../sessions/sessionLabel";
import { settingText } from "../settings/accounts/providerSettingCopy";
import { useDaemonStore, useProviders } from "../store";

/** An id the window cannot name, as plain code: it stands alone in its row, which takes no chip. */
function Id({ children }: { children: string }): React.ReactElement {
  return <span dir="ltr" className="font-mono">{children}</span>;
}

/** Whether `launch` has anything for `ScheduledTaskLaunchRows` to list. */
export function hasLaunchRows(launch: ScheduledTaskLaunch, projectTask: boolean): boolean {
  return launch.agent !== undefined || launch.account !== undefined || launch.model !== undefined || launch.effort !== undefined
    || (launch.bound_to !== undefined && projectTask) || Object.keys(launch.compatibility).length > 0;
}

/** The launch settings a task names, as the rows of a list of labels and values, each as the session dialogs and the task
 * form show it: the agent, the account (the agent's default by its name where the task names the agent), the model as
 * a session's agent mark names it and the reasoning effort (`sessionModelValues`, the default for an explicit null),
 * the session a project task reports to, and each compatibility switch it holds. A setting left open is left out. A value
 * stands alone, so it takes no chip; an id the window cannot name is plain code. */
export function ScheduledTaskLaunchRows({ launch, projectTask }: { launch: ScheduledTaskLaunch; projectTask: boolean }): React.ReactElement {
  const t = useT();
  const providers = useProviders();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const defaultNames = useDaemonStore((s) => s.settings.default_account_names);
  const boundTo = useDaemonStore((s) => (launch.bound_to ? s.sessions.get(launch.bound_to) : undefined));
  const { agent } = launch;
  const rows: { label: string; value: React.ReactNode }[] = [];
  if (agent !== undefined) rows.push({ label: t("dialog.session.agent"), value: <span dir="auto">{providerLabel(t, providers, agent)}</span> });
  if (launch.account !== undefined) {
    const account = launch.account === null ? undefined : accounts.find((item) => item.id === launch.account)?.name;
    let value: React.ReactNode;
    // The default account is the agent's, so without an agent it is only called the default.
    if (launch.account === null) value = <span dir="auto">{agent === undefined ? t("session.modelEffort.unknownItem") : defaultAccountName(t, defaultNames, agent)}</span>;
    else value = account === undefined ? <Id>{launch.account}</Id> : <span dir="auto">{account}</span>;
    rows.push({ label: t("session.agentTooltip.account"), value });
  }
  const values = sessionModelValues(t, agent === undefined ? undefined : providerModelDisplay(providers, agent), { model: launch.model ?? null, effort: launch.effort ?? null, compatibility: {} });
  if (launch.model !== undefined) rows.push({ label: t("session.agentTooltip.model"), value: <span dir="auto">{values.model}</span> });
  if (launch.effort !== undefined) rows.push({ label: t("dialog.agentSettings.effort"), value: <span dir="auto">{values.effort}</span> });
  // A console task has no session to report to.
  if (launch.bound_to !== undefined && projectTask) rows.push({ label: t("scheduledTasks.form.reportTo"), value: boundTo ? <span dir="auto">{sessionDisplayTitle(t, boundTo)}</span> : <Id>{launch.bound_to}</Id> });
  // The provider's declared order, as the task form lists them, then any the provider does not declare.
  const held = Object.entries(launch.compatibility);
  const declared = agent === undefined ? [] : compatibilitySettings(providers, agent).map((descriptor) => descriptor.id);
  const switches = [...declared.flatMap((id) => held.filter(([setting]) => setting === id)), ...held.filter(([setting]) => !declared.includes(setting))];
  return (
    <>
      {rows.map(({ label, value }) => (
        <React.Fragment key={label}>
          <dt className="text-app-muted">{label}</dt>
          <dd className="min-w-0">{value}</dd>
        </React.Fragment>
      ))}
      {switches.map(([setting, on]) => <CompatibilityRow key={setting} agent={declared.includes(setting) ? agent : undefined} setting={setting} on={on} />)}
    </>
  );
}

/** One compatibility switch the task's launch holds, named as the session dialog and the task form name it (the
 * provider's own copy for the setting of `agent`) and valued On or Off; its id, as plain code, where `agent` is not given
 * because the provider does not declare it. */
function CompatibilityRow({ agent, setting, on }: { agent: Agent | undefined; setting: string; on: boolean }): React.ReactElement {
  const t = useT();
  const providers = useProviders();
  const label = agent === undefined ? undefined : settingText(t, providers, agent, setting, "label") ?? settingText(t, providers, agent, setting, "heading");
  return (
    <>
      <dt className="text-app-muted">{label ?? <Id>{setting}</Id>}</dt>
      <dd className="min-w-0">{t(on ? "dialog.agentSettings.compatibilityOn" : "dialog.agentSettings.compatibilityOff")}</dd>
    </>
  );
}
