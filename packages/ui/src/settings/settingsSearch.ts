import { booleanSetting, type BooleanField } from "../configuration/booleanSettings";
import { KEEP_AWAKE_WHEN, KEEP_AWAKE_WHEN_DESCRIPTION } from "../configuration/keepAwakeWhen";
import { appConfig } from "../appConfig";
import { DEVELOPMENT_CHOICES, GIT_WORKFLOW_ITEMS } from "../git/gitWorkflow";
import type { PlainMessageKey, Translate } from "../i18n/catalog";
import { daemonOs } from "../i18n/platformTerms";
import type { Language } from "../i18n/languages";
import { windowKind, type PlatformAdapter } from "../platform";
import type { RemoteAccess, Settings } from "../protocol";
import { SESSION_ORDER_WORDS } from "../sessions/orderWording";
import { ALL_COMMANDS, clientCapabilities, isAvailable, SHORTCUT_GROUPS } from "../shortcuts/commands";
import { anyServing, browserPortFailed, hasBrowserPort, servedBrowserAddress, servedConnection, servedForwardCommand } from "./accessState";
import { COMMAND_LINE_TOOL } from "./commandLineTool";
import type { SettingsSectionId } from "./sections";
import { sshServerNames, sshServerWords } from "./sshServerWords";

/** What decides which settings a section shows: the platform's capabilities, whether the daemon's host can be held
 * awake, the daemon's settings, and what the reads of the system that some of them wait on have answered. */
export interface SettingsSearchContext {
  platform: PlatformAdapter;
  keepAwake: boolean;
  settings: Settings;
  /** Where remote access and browser access stand, which decides what the Access section shows
   * beyond its switches (null where the daemon offers none, when the section is not listed at all). */
  remoteAccess: RemoteAccess | null;
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

const STARTUP: PlainMessageKey = "settings.general.group.startup";
const WINDOW: PlainMessageKey = "settings.general.group.window";
const COMMAND_LINE: PlainMessageKey = "settings.general.group.commandLine";
const POWER: PlainMessageKey = "settings.general.group.power";
const TERMINALS: PlainMessageKey = "settings.general.group.terminals";
const REPOSITORY: PlainMessageKey = "settings.git.group.repository";
const DEVELOPMENT: PlainMessageKey = "settings.git.group.development";
const PROTECTION: PlainMessageKey = "settings.git.group.protection";
const SORTING: PlainMessageKey = "settings.sort.group";
const BROWSER: PlainMessageKey = "settings.remoteAccess.group.browser";
const OTHER_COMPUTERS: PlainMessageKey = "settings.remoteAccess.group.others";
const KEY: PlainMessageKey = "settings.remoteAccess.group.key";

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
    { id: "language", label: "settings.language.label", description: ["settings.language.sharedDescription"] },
    ...(["sessions", "activeProjects", "inactiveProjects"] as const).map((part): SettingsSearchEntry<`session-order.${typeof part}`> => ({
      id: `session-order.${part}`,
      label: SESSION_ORDER_WORDS[part].label,
      description: [SESSION_ORDER_WORDS[part].description],
      group: SORTING,
    })),
    daemonSwitch("auto_close_idle_shell_terminals", TERMINALS),
    daemonSwitch("keep_awake", POWER, ({ keepAwake }) => keepAwake),
    {
      id: "keep_awake_when",
      label: "settings.keepAwake.when.label",
      description: KEEP_AWAKE_WHEN.map((when) => KEEP_AWAKE_WHEN_DESCRIPTION[when]),
      group: POWER,
      shown: ({ keepAwake, settings }) => keepAwake && settings.keep_awake,
    },
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
      shown: ({ platform }) => platform.statusItem?.setVisible !== undefined,
    },
    {
      id: "launch-at-login",
      label: "settings.launchAtLogin.label",
      description: ["settings.launchAtLogin.description.on", "settings.launchAtLogin.description.off", "settings.launchAtLogin.description.requiresApproval"],
      group: STARTUP,
      shown: ({ platform, reads }) => platform.launchAtLogin !== undefined && reads.launchAtLogin,
    },
    daemonSwitch("resume_sessions_on_start", STARTUP),
    {
      id: "command-line",
      label: "settings.commandLine.label",
      description: [(t) => t("settings.commandLine.description", { command: COMMAND_LINE_TOOL })],
      group: COMMAND_LINE,
      shown: ({ platform }) => platform.commandLine !== undefined,
    },
  ],
  appearance: [
    { id: "theme", label: "settings.theme.label", description: ["settings.theme.description"] },
    { id: "markdown-style", label: "settings.markdownStyle.label", description: ["settings.markdownStyle.description"] },
    { id: "terminal-font-size", label: "settings.terminalFontSize.label", description: ["settings.terminalFontSize.description"] },
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
  // Listed only in the application's window for a daemon that offers it (`remoteAccessOffered`), so the section's own
  // entries need no `shown` for that; the browser port decides the This computer's browser group and the Open in a
  // browser block, and what the daemon is doing the rest.
  "remote-access": [
    { id: "remote-access.browser-allow", label: "settings.remoteAccess.browserAccess.label", description: ["settings.remoteAccess.browserAccess.on", "settings.remoteAccess.browserAccess.off"], group: BROWSER, shown: ({ remoteAccess }) => hasBrowserPort(remoteAccess) },
    // What the section says in place of the address when the browser port could not be served, found by its reasons.
    {
      id: "remote-access.browser-failed",
      label: "settings.remoteAccess.failed.titleBrowser",
      description: [
        "settings.remoteAccess.failed.noUi",
        "settings.remoteAccess.failed.ownerKeyFailed",
        "settings.remoteAccess.failed.portInUse",
        "settings.remoteAccess.failed.portIsDaemons",
        "settings.remoteAccess.failed.portFailed",
      ],
      group: BROWSER,
      shown: ({ remoteAccess }) => remoteAccess?.browser_failure != null,
    },
    // The current port is the row's value, not searched.
    { id: "remote-access.browser-port", label: "settings.remoteAccess.port.label", description: ["settings.browserAccess.port.description"], group: BROWSER, shown: ({ remoteAccess }) => hasBrowserPort(remoteAccess) },
    // The address for a browser on this computer, shown where the daemon serves the browser at its own port.
    {
      id: "remote-access.browser-local",
      label: "settings.remoteAccess.local.label",
      description: ["settings.remoteAccess.local.description"],
      group: BROWSER,
      shown: ({ remoteAccess }) => servedBrowserAddress(remoteAccess) !== null,
    },
    // Worded by the system's name for its SSH server; the daemon whose section it is runs on this computer.
    {
      id: "remote-access.remote-login",
      label: (t) => t(sshServerWords(daemonOs()).label),
      // Said for the application alone, or for the application and a browser where the daemon has a browser port.
      description: [
        (t) => t(sshServerWords(daemonOs()).description, sshServerNames((key) => t(key))),
        (t) => t(sshServerWords(daemonOs()).descriptionBoth, sshServerNames((key) => t(key))),
      ],
      group: OTHER_COMPUTERS,
    },
    { id: "remote-access.allow", label: "settings.remoteAccess.allow.label", description: ["settings.remoteAccess.allow.ways.on", "settings.remoteAccess.allow.ways.off"], group: OTHER_COMPUTERS },
    // What the section says in place of the connection details when remote access could not serve, found by its reasons.
    {
      id: "remote-access.failed",
      label: "settings.remoteAccess.failed.titleWays",
      description: [
        "settings.remoteAccess.failed.noUi",
        "settings.remoteAccess.failed.notPrivateApp",
        "settings.remoteAccess.failed.socketFailedApp",
        "settings.remoteAccess.failed.ownerKeyFailedApp",
        "settings.remoteAccess.failed.pipeFailed",
      ],
      group: OTHER_COMPUTERS,
      shown: ({ remoteAccess }) => remoteAccess?.failure != null,
    },
    {
      id: "remote-access.connection",
      label: "settings.remoteAccess.connection.label",
      description: [(t) => t("settings.remoteAccess.connection.description", { addRemoteDaemon: t("remoteDaemons.add"), connectionDetails: t("remoteDaemons.form.details") })],
      group: OTHER_COMPUTERS,
      shown: ({ remoteAccess }) => servedConnection(remoteAccess) !== null,
    },
    {
      id: "remote-access.browser-forward",
      label: "settings.remoteAccess.forward.label",
      // Said with no address token where the daemon lists no address of this computer, with it otherwise.
      description: [
        (t) => t("settings.remoteAccess.forward.description", { server: "<server>" }),
        (t) => t("settings.remoteAccess.forward.descriptionPick", { manual: t("settings.remoteAccess.forward.address.manual"), server: "<server>" }),
      ],
      group: OTHER_COMPUTERS,
      shown: ({ remoteAccess }) => servedForwardCommand(remoteAccess) !== null,
    },
    // Step 2 points at the local address above rather than repeating it.
    {
      id: "remote-access.browser-address",
      label: "settings.remoteAccess.address.labelAbove",
      description: [(t) => t("settings.remoteAccess.address.descriptionAbove", { localAddress: t("settings.remoteAccess.local.label") })],
      group: OTHER_COMPUTERS,
      shown: ({ remoteAccess }) => servedBrowserAddress(remoteAccess) !== null,
    },
    // What the Browser tab says in place of the steps while browser access is on but its port is not open.
    {
      id: "remote-access.browser-closed",
      label: "settings.remoteAccess.browserClosed.label",
      description: [(t) => t("settings.remoteAccess.browserClosed.description", { allowBrowserAccess: t("settings.remoteAccess.browserAccess.label") })],
      group: OTHER_COMPUTERS,
      shown: ({ remoteAccess }) => browserPortFailed(remoteAccess),
    },
    // What the block says in place of the steps while browser access is off.
    {
      id: "remote-access.browser-off",
      label: "settings.remoteAccess.openInBrowser.label",
      description: [(t) => t("settings.remoteAccess.openInBrowser.off", { allowBrowserAccess: t("settings.remoteAccess.browserAccess.label") })],
      group: OTHER_COMPUTERS,
      shown: ({ remoteAccess }) => hasBrowserPort(remoteAccess) && remoteAccess?.browser_allowed === false,
    },
    // The owner key is shared by the two ways in, so the row is there while either serves.
    {
      id: "remote-access.rotate",
      label: "settings.remoteAccess.rotate.label",
      // Said for another computer's app alone where the daemon has no browser port.
      description: ["settings.remoteAccess.rotate.descriptionShared", "settings.remoteAccess.rotate.descriptionApp"],
      group: KEY,
      shown: ({ remoteAccess }) => anyServing(remoteAccess),
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
  memory: [
    daemonSwitch("memory_enabled"),
    { id: "memory.leftBehind", label: "settings.memory.leftBehind.title", description: ["settings.memory.leftBehind.description"] },
  ],
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
  // Listed only where something updates (`SettingsDialog`). A remote window's section has only the daemon's own two
  // settings of its updates, the automatic check worded for the daemon.
  updates: [
    // Its description is where updates stand, not what the row does, so it is not searched.
    { id: "updates.version", label: "settings.updates.status.label", shown: () => windowKind() !== "remote" },
    {
      id: "updates.autoCheck",
      label: "settings.updates.autoCheck.label",
      description: [
        (t) => t(windowKind() === "remote" ? "settings.updates.autoCheck.description.remote.on" : "settings.updates.autoCheck.description.on"),
        (t) => t(windowKind() === "remote" ? "settings.updates.autoCheck.description.remote.off" : "settings.updates.autoCheck.description.off"),
      ],
    },
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
