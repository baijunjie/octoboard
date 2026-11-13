import React from "react";

import { usePlatform } from "../platform/react";
import { useDaemonStore } from "../store";
import { BackgroundRunSetting } from "./BackgroundRunSetting";
import { DaemonSwitchSetting } from "./DaemonSwitchSetting";
import { KeepAwakeSetting } from "./KeepAwakeSetting";
import { LanguageSetting } from "./LanguageSetting";
import { LaunchAtLoginSetting } from "./LaunchAtLoginSetting";
import { MenuBarIconSetting } from "./MenuBarIconSetting";
import { SessionOrderSettings } from "./SessionOrderSettings";
import { SettingsGroup } from "./SettingsGroup";

/** The app-wide settings: the language, then groups: startup (whether the app opens at login, where the platform
 * offers it, and whether the interrupted sessions are resumed), the window (where the platform offers it: whether
 * closing it keeps the app running, whether the menu bar icon shows), power (where the daemon's host offers it: whether
 * the computer is kept awake, and when), the terminals (whether closing a panel closes idle terminals) and Sorting, in
 * which sessions and projects are ordered. A group the platform has nothing for is left out. */
export function GeneralSection(): React.ReactElement {
  const { backgroundRun, statusItem, launchAtLogin } = usePlatform();
  // The daemon's own host, which may be another machine than the one the window runs on.
  const keepAwake = useDaemonStore((s) => s.host?.keep_awake === true);
  return (
    <>
      {/* Wrapped so the last ungrouped row ends with the gap above the group's heading, not a second line. */}
      <div>
        <LanguageSetting />
      </div>
      <SettingsGroup title="settings.general.group.startup">
        {launchAtLogin && <LaunchAtLoginSetting />}
        <DaemonSwitchSetting field="resume_sessions_on_start" />
      </SettingsGroup>
      {(backgroundRun || statusItem) && (
        <SettingsGroup title="settings.general.group.window">
          {backgroundRun && <BackgroundRunSetting />}
          {statusItem && <MenuBarIconSetting />}
        </SettingsGroup>
      )}
      {keepAwake && (
        <SettingsGroup title="settings.general.group.power">
          <KeepAwakeSetting />
        </SettingsGroup>
      )}
      <SettingsGroup title="settings.general.group.terminals">
        <DaemonSwitchSetting field="auto_close_idle_shell_terminals" />
      </SettingsGroup>
      <SettingsGroup title="settings.sort.group">
        <SessionOrderSettings />
      </SettingsGroup>
    </>
  );
}
