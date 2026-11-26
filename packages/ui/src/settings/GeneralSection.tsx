import React from "react";

import { usePlatform } from "../platform/react";
import { useDaemonStore } from "../store";
import { BackgroundRunSetting } from "./BackgroundRunSetting";
import { CommandLineSetting } from "./CommandLineSetting";
import { DaemonSwitchSetting } from "./DaemonSwitchSetting";
import { KeepAwakeSetting } from "./KeepAwakeSetting";
import { LanguageSetting } from "./LanguageSetting";
import { LaunchAtLoginSetting } from "./LaunchAtLoginSetting";
import { MenuBarIconSetting } from "./MenuBarIconSetting";
import { SessionComposerSetting } from "./SessionComposerSetting";
import { SessionOrderSettings } from "./SessionOrderSettings";
import { SettingsGroup } from "./SettingsGroup";

/** The app-wide settings: first the language, a row of no group since it governs the whole interface, then groups
 * ordered by how often each is used, most frequent first: Sorting, in which sessions and projects are ordered; the
 * terminals (whether this client shows the message composer below a session's terminal, and whether closing a panel
 * closes idle terminals); power (where the daemon's host offers it: whether the
 * computer is kept awake, and when); the window (where the platform offers it: whether closing it keeps the app
 * running, whether the menu bar icon shows); startup (whether the app opens at login, where the platform offers it, and
 * whether the interrupted sessions are resumed); and the command-line tool (where the application can link it into the
 * PATH). A group the platform has nothing for is left out. */
export function GeneralSection(): React.ReactElement {
  const { backgroundRun, statusItem, launchAtLogin, commandLine } = usePlatform();
  // The switch for the menu bar icons is the application's, which another daemon's window does not offer.
  const menuBarIcon = statusItem?.setVisible !== undefined;
  // The daemon's own host, which may be another machine than the one the window runs on.
  const keepAwake = useDaemonStore((s) => s.host?.keep_awake === true);
  return (
    <>
      {/* Wrapped so the ungrouped row ends with the gap above the first group's heading, not a second line. */}
      <div>
        <LanguageSetting />
      </div>
      <SettingsGroup title="settings.sort.group">
        <SessionOrderSettings />
      </SettingsGroup>
      <SettingsGroup title="settings.general.group.terminals">
        <SessionComposerSetting />
        <DaemonSwitchSetting field="auto_close_idle_shell_terminals" />
      </SettingsGroup>
      {keepAwake && (
        <SettingsGroup title="settings.general.group.power">
          <KeepAwakeSetting />
        </SettingsGroup>
      )}
      {(backgroundRun || menuBarIcon) && (
        <SettingsGroup title="settings.general.group.window">
          {backgroundRun && <BackgroundRunSetting />}
          {menuBarIcon && <MenuBarIconSetting />}
        </SettingsGroup>
      )}
      <SettingsGroup title="settings.general.group.startup">
        {launchAtLogin && <LaunchAtLoginSetting />}
        <DaemonSwitchSetting field="resume_sessions_on_start" />
      </SettingsGroup>
      {commandLine && (
        <SettingsGroup title="settings.general.group.commandLine">
          <CommandLineSetting />
        </SettingsGroup>
      )}
    </>
  );
}
