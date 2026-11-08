import type { PlainMessageKey } from "../i18n/catalog";
import type { SettingsSection } from "../protocol";

/** The Settings sections a configuration document carries settings of, in the order Settings lists them. */
export const SETTINGS_SECTIONS: readonly SettingsSection[] = ["general", "app_behavior", "git", "agent_behavior", "keyboard_shortcuts", "cleanup"];

/** Each section's name, as Settings names it. */
export const SECTION_LABEL: Record<SettingsSection, PlainMessageKey> = {
  general: "settings.section.general",
  app_behavior: "settings.section.appBehavior",
  git: "settings.section.git",
  agent_behavior: "settings.section.agentPermissions",
  keyboard_shortcuts: "settings.section.shortcuts",
  cleanup: "settings.section.cleanup",
};
