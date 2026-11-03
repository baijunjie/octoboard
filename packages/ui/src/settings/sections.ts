/** The settings dialog's sections, in the order its tabs list them. */
export const SETTINGS_SECTION_IDS = [
  "general",
  "git",
  "accounts",
  "agent-permissions",
  "trusted-folders",
  "permissions",
  "notifications",
  "shortcuts",
  "configuration",
  "about",
] as const;

/** A section of the settings dialog, by the key of its tab. */
export type SettingsSectionId = (typeof SETTINGS_SECTION_IDS)[number];

/** The extras a request to open Settings can carry besides the section: the Add agent dialog in front when `addAgent`
 * is set, or, for `account` (an account ID), that account's agent tab with its sharing switch scrolled to and focused. */
export interface SettingsRequestOptions {
  addAgent?: boolean;
  account?: string;
}

/** What the settings dialog is asked to show: the section (none for its first), whether to put its Add agent dialog in
 * front, and the account (if any) whose sharing switch to focus. `nonce` changes with every request, so an open dialog
 * can follow a later one. */
export interface SettingsNavigation {
  nonce: number;
  section: SettingsSectionId | undefined;
  addAgent: boolean;
  account?: string;
}
