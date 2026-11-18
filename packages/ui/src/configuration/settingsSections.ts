import type { PlainMessageKey } from "../i18n/catalog";
import type { SettingsSection } from "../protocol";
import { knownValues } from "../values/knownValue";

/** The Settings sections a configuration document carries settings of, in the order Settings lists them. */
export const SETTINGS_SECTIONS: readonly SettingsSection[] = ["general", "appearance", "agent_behavior", "git", "keyboard_shortcuts", "cleanup"];

/** Whether `section` is a Settings section this build knows; a newer daemon may list one it does not, which no card
 * draws and no import applies. */
export const isSettingsSection = knownValues<SettingsSection>({ general: true, appearance: true, agent_behavior: true, git: true, keyboard_shortcuts: true, cleanup: true });

/** Each section's name, as Settings names it. */
export const SECTION_LABEL: Record<SettingsSection, PlainMessageKey> = {
  general: "settings.section.general",
  appearance: "settings.section.appearance",
  git: "settings.section.git",
  agent_behavior: "settings.section.agentPermissions",
  keyboard_shortcuts: "settings.section.shortcuts",
  cleanup: "settings.section.cleanup",
};
