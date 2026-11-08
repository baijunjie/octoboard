import React from "react";

import { useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import { BackgroundRunSetting } from "./BackgroundRunSetting";
import { DaemonSwitchSetting } from "./DaemonSwitchSetting";
import { KeepAwakeSetting } from "./KeepAwakeSetting";
import { LaunchAtLoginSetting } from "./LaunchAtLoginSetting";
import { MenuBarIconSetting } from "./MenuBarIconSetting";
import { SettingsGroup } from "./SettingsGroup";

/** How the application behaves around the window, in groups: startup (whether the app opens at login, where the
 * platform offers it, and whether the sessions the previous run left running are resumed), the window (where the
 * platform offers it: whether closing it keeps the app running, whether the menu bar icon shows), power (where the
 * platform offers it: whether the computer is kept awake, and when) and the terminals (whether closing a panel closes
 * idle terminals). A group the platform has nothing for is left out. */
export function AppBehaviorSection(): React.ReactElement {
  const t = useT();
  const { backgroundRun, statusItem, keepAwake, launchAtLogin } = usePlatform();
  return (
    <>
      <SettingsGroup title={t("settings.appBehavior.group.startup")}>
        {launchAtLogin && <LaunchAtLoginSetting />}
        <DaemonSwitchSetting field="resume_sessions_on_start" />
      </SettingsGroup>
      {(backgroundRun || statusItem) && (
        <SettingsGroup title={t("settings.appBehavior.group.window")}>
          {backgroundRun && <BackgroundRunSetting />}
          {statusItem && <MenuBarIconSetting />}
        </SettingsGroup>
      )}
      {keepAwake && (
        <SettingsGroup title={t("settings.appBehavior.group.power")}>
          <KeepAwakeSetting />
        </SettingsGroup>
      )}
      <SettingsGroup title={t("settings.appBehavior.group.terminals")}>
        <DaemonSwitchSetting field="auto_close_idle_shell_terminals" />
      </SettingsGroup>
    </>
  );
}
