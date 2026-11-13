import { booleanSetting, type BooleanField } from "../configuration/booleanSettings";
import { KEEP_AWAKE_WHEN, KEEP_AWAKE_WHEN_DESCRIPTION } from "../configuration/keepAwakeWhen";
import { appConfig } from "../appConfig";
import { DEVELOPMENT_CHOICES, GIT_WORKFLOW_ITEMS } from "../git/gitWorkflow";
import type { PlainMessageKey, Translate } from "../i18n/catalog";
import type { Language } from "../i18n/languages";
import type { PlatformAdapter } from "../platform";
import type { Settings } from "../protocol";
import { SESSION_ORDER_WORDS } from "../sessions/orderWording";
import { ALL_COMMANDS, clientCapabilities, isAvailable, SHORTCUT_GROUPS } from "../shortcuts/commands";
import type { SettingsSectionId } from "./sections";

/** What decides which settings a section shows: the platform's capabilities, whether the daemon's host can be held
 * awake, the daemon's settings, and what the reads of the system that some of them wait on have answered. */
export interface SettingsSearchContext {
  platform: PlatformAdapter;
  keepAwake: boolean;
  settings: Settings;
  /** What each read says of the settings that wait on it, as the section decides it (see `searchReads`): a row with
   * no capability to read from is never counted, and one with it is counted from the answer on. */
  reads: {
    /** The launch-at-login row is on screen. */
    launchAtLogin: boolean;
    /** The permission rows are on screen. */
    macosPermissions: boolean;
    /** The permissions could not be read, so the section shows an alert in place of the rows. */
    macosPermissionsUnreadable: boolean;
  };
}

/** A part of an entry's text: a message, or one worded with parameters that are not the user's to change. */
type SearchText = PlainMessageKey | ((t: Translate) => string);

/** One thing a search can find in a section: a row (`SettingRow` given the same `searchId`) or a block of the
 * section's own (`SettingsSearchBlock`). Its text is its name and every wording its description can take, together
 * with the heading of its group (the `title` of the `SettingsGroup` it sits in, which the row inherits) and its
 * section's name. A description that follows the row's value (a switch's position, a choice) is searched in all its
 * wordings, so that changing the value while searching never takes the row out from under the user. A block's
 * `description` lists the words it is found by, whether or not the block shows them. `shown` says
 * whether the section renders it in this context; one it renders in every context has none. */
export interface SettingsSearchEntry<Id extends string = string> {
  id: Id;
  label: SearchText;
  description?: readonly SearchText[];
  group?: PlainMessageKey;
  shown?: (context: SettingsSearchContext) => boolean;
}

/** The entry of a switch the daemon holds, worded as `DaemonSwitchSetting` words it: its id is the field. Its help tip's
 * text is not shown on the row, so it is not searched. */
function daemonSwitch<F extends BooleanField>(field: F, group?: PlainMessageKey, shown?: (context: SettingsSearchContext) => boolean): SettingsSearchEntry<F> {
  const { label, description } = booleanSetting(field);
  return { id: field, label, description: [description.on, description.off], group, shown };
}

/** The entry of an item of the Git workflow that is a switch, worded in either position as `GitSection` words it. */
function gitWorkflowSwitch<I extends "squash_before_delivery" | "rebase_before_delivery" | "revert_gate">(item: I, group: PlainMessageKey, shown?: (context: SettingsSearchContext) => boolean): SettingsSearchEntry<`git_workflow.${I}`> {
  const { label, description } = GIT_WORKFLOW_ITEMS[item];
  return { id: `git_workflow.${item}`, label, description: [description.on, description.off], group, shown };
}

const STARTUP: PlainMessageKey = "settings.appBehavior.group.startup";
const WINDOW: PlainMessageKey = "settings.appBehavior.group.window";
const POWER: PlainMessageKey = "settings.appBehavior.group.power";
const TERMINALS: PlainMessageKey = "settings.appBehavior.group.terminals";
const REPOSITORY: PlainMessageKey = "settings.git.group.repository";
const DEVELOPMENT: PlainMessageKey = "settings.git.group.development";
const PROTECTION: PlainMessageKey = "settings.git.group.protection";
const SORTING: PlainMessageKey = "settings.sort.group";

/** Whether sessions work on a branch of their own, without which the Git section hides the items that shape it. */
const ownBranch = ({ settings }: SettingsSearchContext) => settings.git_workflow.development !== "in_place";

/**
 * What a search can find in each section, in the order the section shows it. The rows a section renders carry the
 * same ids (`searchId`), so a section that is not rendered is still counted from here; `settingsSearch.test.tsx`
 * renders every section and holds the two to each other. A list whose entries are the user's own data (the accounts
 * of each agent, the trusted folders) is one block, found by its section's name and its own heading or lead rather
 * than by its contents.
 */
const INDEX = {
  general: [
    { id: "appearance", label: "settings.appearance.label", description: ["settings.appearance.description"] },
    { id: "language", label: "settings.language.label", description: ["settings.language.sharedDescription"] },
    { id: "markdown-style", label: "settings.markdownStyle.label", description: ["settings.markdownStyle.description"] },
    ...(["sessions", "activeProjects", "inactiveProjects"] as const).map((part): SettingsSearchEntry<`session-order.${typeof part}`> => ({
      id: `session-order.${part}`,
      label: SESSION_ORDER_WORDS[part].label,
      description: [SESSION_ORDER_WORDS[part].description],
      group: SORTING,
    })),
  ],
  "app-behavior": [
    {
      id: "launch-at-login",
      label: "settings.launchAtLogin.label",
      description: ["settings.launchAtLogin.description.on", "settings.launchAtLogin.description.off", "settings.launchAtLogin.description.requiresApproval"],
      group: STARTUP,
      shown: ({ platform, reads }) => platform.launchAtLogin !== undefined && reads.launchAtLogin,
    },
    daemonSwitch("resume_sessions_on_start", STARTUP),
    {
      id: "run-in-background",
      label: "settings.background.label",
      description: ["settings.background.description.on", "settings.background.description.off", "settings.background.description.onIconHidden", "settings.background.description.offIconHidden"],
      group: WINDOW,
      shown: ({ platform }) => platform.backgroundRun !== undefined,
    },
    {
      id: "menu-bar-icon",
      label: "settings.menuBarIcon.label",
      description: ["settings.menuBarIcon.description.on", "settings.menuBarIcon.description.off", "settings.menuBarIcon.description.offInBackground"],
      group: WINDOW,
      shown: ({ platform }) => platform.statusItem !== undefined,
    },
    daemonSwitch("keep_awake", POWER, ({ keepAwake }) => keepAwake),
    {
      id: "keep_awake_when",
      label: "settings.keepAwake.when.label",
      description: KEEP_AWAKE_WHEN.map((when) => KEEP_AWAKE_WHEN_DESCRIPTION[when]),
      group: POWER,
      shown: ({ keepAwake, settings }) => keepAwake && settings.keep_awake,
    },
    daemonSwitch("auto_close_idle_shell_terminals", TERMINALS),
  ],
  git: [
    { id: "clone-dir", label: "settings.cloneDir.label", description: ["settings.cloneDir.description.gitUrl"], group: REPOSITORY },
    daemonSwitch("auto_pull_repositories", REPOSITORY),
    {
      id: "git_workflow.development",
      label: GIT_WORKFLOW_ITEMS.development.label,
      description: DEVELOPMENT_CHOICES.map((choice) => choice.description),
      group: DEVELOPMENT,
    },
    gitWorkflowSwitch("squash_before_delivery", DEVELOPMENT, ownBranch),
    gitWorkflowSwitch("rebase_before_delivery", DEVELOPMENT, ownBranch),
    { id: "git_workflow.delivery", label: GIT_WORKFLOW_ITEMS.delivery.label, description: [GIT_WORKFLOW_ITEMS.delivery.description], group: DEVELOPMENT, shown: ownBranch },
    gitWorkflowSwitch("revert_gate", PROTECTION),
  ],
  accounts: [
    daemonSwitch("auto_select_account"),
    daemonSwitch("auto_switch_on_usage_limit"),
    daemonSwitch("auto_start_quota_windows"),
    // The agents' tabs and each one's accounts: the user's own, so found by name only as a whole.
    { id: "accounts.agents", label: "settings.accounts.agents" },
  ],
  "agent-permissions": [
    daemonSwitch("auto_approve_session_requests"),
    daemonSwitch("auto_approve_resume_requests"),
    daemonSwitch("auto_approve_account_switches"),
    daemonSwitch("auto_approve_handoffs"),
    daemonSwitch("ask_questions_as_choices"),
    daemonSwitch("keep_sessions_in_own_project"),
  ],
  // The folders are the user's own, so the list is found as a whole, by the section's name and its lead.
  "trusted-folders": [{ id: "trusted-folders.list", label: "settings.section.trustedFolders", description: ["settings.trusted.description"] }],
  permissions: [
    {
      id: "permissions.screenRecording",
      label: "settings.permissions.screenRecording.label",
      description: ["settings.permissions.screenRecording.description"],
      shown: ({ platform, reads }) => platform.macosPermissions !== undefined && reads.macosPermissions,
    },
    {
      id: "permissions.accessibility",
      label: "settings.permissions.accessibility.label",
      description: ["settings.permissions.accessibility.description"],
      shown: ({ platform, reads }) => platform.macosPermissions !== undefined && reads.macosPermissions,
    },
    // What the section shows in place of the rows when the read failed, found by what the rows would have been.
    {
      id: "permissions.unreadable",
      label: "settings.permissions.unreadable.title",
      description: [
        "settings.permissions.unreadable",
        "settings.permissions.screenRecording.label",
        "settings.permissions.screenRecording.description",
        "settings.permissions.accessibility.label",
        "settings.permissions.accessibility.description",
      ],
      shown: ({ platform, reads }) => platform.macosPermissions !== undefined && reads.macosPermissionsUnreadable,
    },
  ],
  // Its description is where notifications stand, not what the setting does, so it is not searched.
  notifications: [{ id: "notifications", label: "settings.notifications.label" }],
  // One entry per command the client has, in the list's order by group, found by its name and its group's.
  shortcuts: SHORTCUT_GROUPS.flatMap((group) => ALL_COMMANDS.filter((command) => command.group === group).map((command): SettingsSearchEntry<`shortcut.${typeof command.id}`> => ({
    id: `shortcut.${command.id}`,
    label: command.label,
    group: `settings.shortcuts.group.${group}`,
    shown: ({ platform }) => isAvailable(command.availability, clientCapabilities(platform)),
  }))),
  cleanup: [daemonSwitch("auto_delete_unresumable_archives")],
  configuration: [
    {
      id: "configuration.export",
      label: "settings.configuration.export.label",
      description: ["settings.configuration.export.description"],
      shown: ({ platform }) => platform.configurationFile !== undefined,
    },
    {
      id: "configuration.import",
      label: "settings.configuration.import.label",
      description: ["settings.configuration.import.description"],
      shown: ({ platform }) => platform.configurationFile !== undefined,
    },
  ],
  // Listed only where something updates (`SettingsDialog`), so its entries need no `shown`.
  updates: [
    // Its description is where updates stand, not what the row does, so it is not searched.
    { id: "updates.version", label: "settings.updates.status.label" },
    { id: "updates.autoCheck", label: "settings.updates.autoCheck.label", description: ["settings.updates.autoCheck.description.on", "settings.updates.autoCheck.description.off"] },
    { id: "updates.channel", label: "settings.updates.channel.label", description: ["settings.updates.channel.description"] },
  ],
  about: [
    { id: "about.website", label: "settings.about.website.label" },
    { id: "about.repository", label: "settings.about.repository.label" },
    { id: "about.issues", label: "settings.about.issues.label" },
    { id: "about.license", label: "settings.about.license.label", description: [(t) => t("settings.about.license.description", { license: appConfig.license })] },
    { id: "about.third-party", label: "settings.about.thirdParty.label", description: ["settings.about.thirdParty.description"] },
    {
      id: "about.copyright",
      label: "settings.about.copyright.label",
      description: [(t) => t("settings.about.copyright.description", { year: String(appConfig.copyrightYear), author: appConfig.author.name })],
    },
  ],
} as const satisfies Record<SettingsSectionId, readonly SettingsSearchEntry[]>;

/** The id of any entry of the index: a row's `searchId` or a block's `id`, so that one naming no entry does not compile. */
export type SettingsSearchId = (typeof INDEX)[SettingsSectionId][number]["id"];

/** `INDEX`, each entry read as a whole entry, whatever fields its own literal leaves out. */
export const SETTINGS_SEARCH_INDEX: Record<SettingsSectionId, readonly SettingsSearchEntry<SettingsSearchId>[]> = INDEX;

/** What a section has that matches a query: the ids of its entries that do, and the headings of the groups they sit in. */
export interface SectionMatches {
  ids: ReadonlySet<SettingsSearchId>;
  groups: ReadonlySet<PlainMessageKey>;
}

/** The words of a query, folded for comparison: every one must be found in an entry's text for it to match. */
export function queryWords(query: string, language: Language): string[] {
  return query.trim().toLocaleLowerCase(language).split(/\s+/).filter((word) => word !== "");
}

/** The entries of `section` that are shown in `context` and match every one of `words` (see `queryWords`), with the
 * groups they sit in. The text of an entry is its section's name (`sectionLabel`), its group's heading, its name and its
 * description's wordings, in the current language of `t`, folded for `language`. */
export function sectionMatches(section: SettingsSectionId, sectionLabel: PlainMessageKey, words: readonly string[], t: Translate, language: Language, context: SettingsSearchContext): SectionMatches {
  const word = (text: SearchText) => (typeof text === "function" ? text(t) : t(text));
  const ids = new Set<SettingsSearchId>();
  const groups = new Set<PlainMessageKey>();
  for (const entry of SETTINGS_SEARCH_INDEX[section]) {
    if (entry.shown && !entry.shown(context)) continue;
    const parts = [t(sectionLabel), entry.group ? t(entry.group) : "", word(entry.label), ...(entry.description ?? []).map(word)];
    const text = parts.join("\n").toLocaleLowerCase(language);
    if (!words.every((query) => text.includes(query))) continue;
    ids.add(entry.id);
    if (entry.group) groups.add(entry.group);
  }
  return { ids, groups };
}
