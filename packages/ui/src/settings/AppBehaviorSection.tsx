import React from "react";

import { usePlatform } from "../platform/react";
import { useDaemonStore } from "../store";
import { BackgroundRunSetting } from "./BackgroundRunSetting";
import { DaemonSwitchSetting } from "./DaemonSwitchSetting";
import { KeepAwakeSetting } from "./KeepAwakeSetting";
import { LaunchAtLoginSetting } from "./LaunchAtLoginSetting";
import { MenuBarIconSetting } from "./MenuBarIconSetting";
import { SettingsGroup } from "./SettingsGroup";

/** How the application behaves around the window, in groups: startup (whether the app opens at login, where the
 * platform offers it, and whether the interrupted sessions are resumed), the window (where the platform offers it:
 * whether closing it keeps the app running, whether the menu bar icon shows), power (where the daemon's host offers it:
 * whether the computer is kept awake, and when) and the terminals (whether closing a panel closes idle terminals). A
 * group the platform has nothing for is left out. */
export function AppBehaviorSection(): React.ReactElement {
  const { backgroundRun, statusItem, launchAtLogin } = usePlatform();
  // The daemon's own host, which may be another machine than the one the window runs on.
  const keepAwake = useDaemonStore((s) => s.host?.keep_awake === true);
  return (
    <>
      <SettingsGroup title="settings.appBehavior.group.startup">
        {launchAtLogin && <LaunchAtLoginSetting />}
        <DaemonSwitchSetting field="resume_sessions_on_start" />
      </SettingsGroup>
      {(backgroundRun || statusItem) && (
        <SettingsGroup title="settings.appBehavior.group.window">
          {backgroundRun && <BackgroundRunSetting />}
          {statusItem && <MenuBarIconSetting />}
        </SettingsGroup>
      )}
      {keepAwake && (
        <SettingsGroup title="settings.appBehavior.group.power">
          <KeepAwakeSetting />
        </SettingsGroup>
      )}
      <SettingsGroup title="settings.appBehavior.group.terminals">
        <DaemonSwitchSetting field="auto_close_idle_shell_terminals" />
      </SettingsGroup>
    </>
  );
}
