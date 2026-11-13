import type { PlainMessageKey } from "../i18n/catalog";
import type { SettingsSection } from "../protocol";

/** The Settings sections a configuration document carries settings of, in the order Settings lists them. */
export const SETTINGS_SECTIONS: readonly SettingsSection[] = ["general", "appearance", "agent_behavior", "git", "keyboard_shortcuts", "cleanup"];

/** Each section's name, as Settings names it. */
export const SECTION_LABEL: Record<SettingsSection, PlainMessageKey> = {
  general: "settings.section.general",
  appearance: "settings.section.appearance",
  git: "settings.section.git",
  agent_behavior: "settings.section.agentPermissions",
  keyboard_shortcuts: "settings.section.shortcuts",
  cleanup: "settings.section.cleanup",
};
