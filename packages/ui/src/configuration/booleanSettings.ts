import type { PlainMessageKey } from "../i18n/catalog";
import type { DocumentAgentSettings, DocumentSettings } from "../protocol";
import type { ClientSettings } from "./clientSettings";

/** The fields of `T` that are a switch, picked by their type so a setting of another kind cannot be listed below. A
 * field a document from an older build may lack is optional and still a switch; the ones in `Held` are not the
 * daemon's. */
type SwitchFields<T, Held extends PropertyKey = never> = { [K in keyof T]-?: K extends Held ? never : NonNullable<T[K]> extends boolean ? K : never }[keyof T];

/** The application settings of a document that are a switch the daemon holds: those only the client holds
 * (`ClientSettings`) are carried beside them and not listed here. */
export type ApplicationBooleanField = SwitchFields<DocumentSettings, keyof ClientSettings>;

/** The settings of the Agent accounts section, which travel with the accounts, that are a switch. */
export type AgentBooleanField = SwitchFields<DocumentAgentSettings>;

export type BooleanField = ApplicationBooleanField | AgentBooleanField;

/** The Settings dialog sections a switch is shown in: the five of the application settings, and the Agent accounts
 * section, whose settings travel with the accounts. */
export type SwitchSection = "general" | "appBehavior" | "git" | "agentBehavior" | "cleanup" | "agentAccounts";

/** One daemon setting that is a switch: its field (see `Settings.<field>` in protocol.generated.ts for what it
 * does), the Settings dialog section it is shown in and its place among that section's lines, the name and the two
 * descriptions the settings page gives it, the help tip some carry and what turning it on means in the import cards.
 * The keys are listed, not derived: their prefixes are irregular. */
export interface BooleanSetting<F extends BooleanField = BooleanField> {
  field: F;
  section: SwitchSection;
  /** The place among the switches of its section, from 1. */
  order: number;
  label: PlainMessageKey;
  /** `onMacos`, where given, is the description of the on state for a macOS desktop app, which says what
   * only it does. */
  description: { on: PlainMessageKey; off: PlainMessageKey; onMacos?: PlainMessageKey };
  /** The help tip that ends the label's line: its accessible name and its text. */
  help?: { label: PlainMessageKey; text: PlainMessageKey };
  /** What turning it on means in the import cards; `null` for a setting whose line says no more than "turns on". */
  consequence: PlainMessageKey | null;
}

/** The entry of one switch, whose section is fixed by what kind of setting it is: an Agent accounts setting is shown
 * in that section, an application setting in one of the others. */
type Copy<F extends BooleanField> = Omit<BooleanSetting<F>, "field" | "section"> & {
  section: F extends AgentBooleanField ? "agentAccounts" : Exclude<SwitchSection, "agentAccounts">;
};

/** The copy and the place of each switch, complete by type: a boolean setting added to `DocumentSettings` or
 * `DocumentAgentSettings` fails to compile until it has an entry, which gives it its section and so its group below. */
const COPY: { [F in BooleanField]: Copy<F> } = {
  auto_close_idle_shell_terminals: {
    section: "appBehavior",
    order: 3,
    label: "settings.closeIdleTerminals.label",
    description: { on: "settings.closeIdleTerminals.description.on", off: "settings.closeIdleTerminals.description.off" },
    consequence: "configuration.import.settings.consequence.closeIdleTerminals",
  },
  keep_awake: {
    section: "appBehavior",
    order: 2,
    label: "settings.keepAwake.label",
    description: { on: "settings.keepAwake.description.on", off: "settings.keepAwake.description.off" },
    consequence: null,
  },
  auto_pull_repositories: {
    section: "git",
    order: 1,
    label: "settings.git.autoPull.label",
    description: { on: "settings.git.autoPull.description.on", off: "settings.git.autoPull.description.off" },
    help: { label: "settings.git.autoPull.help", text: "settings.git.autoPull.remoteCheck" },
    consequence: "configuration.import.settings.consequence.autoPull",
  },
  resume_sessions_on_start: {
    section: "appBehavior",
    order: 1,
    label: "settings.resumeOnStart.label",
    description: { on: "settings.resumeOnStart.description.on", off: "settings.resumeOnStart.description.off", onMacos: "settings.resumeOnStart.description.onMacos" },
    consequence: "configuration.import.settings.consequence.resumeOnStart",
  },
  auto_delete_unresumable_archives: {
    section: "cleanup",
    order: 1,
    label: "settings.archiveCleanup.label",
    description: { on: "settings.archiveCleanup.description.on", off: "settings.archiveCleanup.description.off" },
    consequence: "configuration.import.settings.consequence.archiveCleanup",
  },
  auto_select_account: {
    section: "agentAccounts",
    order: 1,
    label: "settings.autoSelect.label",
    description: { on: "settings.autoSelect.description.on", off: "settings.autoSelect.description.off" },
    consequence: "configuration.import.settings.consequence.autoSelectAccount",
  },
  auto_switch_on_usage_limit: {
    section: "agentAccounts",
    order: 2,
    label: "settings.autoSwitch.label",
    description: { on: "settings.autoSwitch.description.on", off: "settings.autoSwitch.description.off" },
    consequence: "configuration.import.settings.consequence.autoSwitchOnUsageLimit",
  },
  auto_start_quota_windows: {
    section: "agentAccounts",
    order: 3,
    label: "settings.startQuotaWindows.label",
    description: { on: "settings.startQuotaWindows.description.on", off: "settings.startQuotaWindows.description.off" },
    help: { label: "settings.startQuotaWindows.help", text: "settings.startQuotaWindows.helpText" },
    consequence: "configuration.import.settings.consequence.autoStartQuotaWindows",
  },
  auto_approve_session_requests: {
    section: "agentBehavior",
    order: 1,
    label: "settings.autoApproveSession.label",
    description: { on: "settings.autoApproveSession.approvalDescription.on", off: "settings.autoApproveSession.approvalDescription.off" },
    consequence: "configuration.import.settings.consequence.autoApproveSession",
  },
  auto_approve_resume_requests: {
    section: "agentBehavior",
    order: 2,
    label: "settings.autoApproveResume.label",
    description: { on: "settings.autoApproveResume.approvalDescription.on", off: "settings.autoApproveResume.approvalDescription.off" },
    consequence: "configuration.import.settings.consequence.autoApproveResume",
  },
  auto_approve_account_switches: {
    section: "agentBehavior",
    order: 3,
    label: "settings.autoApproveAccountSwitches.label",
    description: { on: "settings.autoApproveAccountSwitches.description.on", off: "settings.autoApproveAccountSwitches.description.off" },
    consequence: "configuration.import.settings.consequence.autoApproveAccountSwitches",
  },
  auto_approve_handoffs: {
    section: "agentBehavior",
    order: 4,
    label: "settings.autoApproveHandoffs.label",
    description: { on: "settings.autoApproveHandoffs.description.on", off: "settings.autoApproveHandoffs.description.off" },
    consequence: "configuration.import.settings.consequence.autoApproveHandoffs",
  },
  ask_questions_as_choices: {
    section: "agentBehavior",
    order: 5,
    label: "settings.askQuestionsAsChoices.label",
    description: { on: "settings.askQuestionsAsChoices.description.on", off: "settings.askQuestionsAsChoices.description.off" },
    consequence: "configuration.import.settings.consequence.askQuestionsAsChoices",
  },
};

/** The entry of `field`. */
export function booleanSetting<F extends BooleanField>(field: F): BooleanSetting<F> {
  return { field, ...COPY[field] } as BooleanSetting<F>;
}

/** The switches shown in `section`, in the order of that section's lines. */
function switchesIn<F extends BooleanField>(section: SwitchSection): readonly BooleanSetting<F>[] {
  return (Object.keys(COPY) as F[])
    .map(booleanSetting)
    .filter((setting) => setting.section === section)
    .sort((a, b) => a.order - b.order);
}

/** The switches of the Agent accounts section, in the order the import cards list them. */
export const AGENT_SWITCHES = switchesIn<AgentBooleanField>("agentAccounts");

/** The switches the Settings dialog's General section shows, in its order (the settings of it a client holds sit
 * around them in the import cards). */
export const GENERAL_SWITCHES = switchesIn<ApplicationBooleanField>("general");

/** The switches of the App behavior section, in its order (the settings of it a client holds sit between the
 * first and the second in the import cards). */
export const APP_BEHAVIOR_SWITCHES = switchesIn<ApplicationBooleanField>("appBehavior");

/** The switches of the Git section, in its order. */
export const GIT_SWITCHES = switchesIn<ApplicationBooleanField>("git");

/** The switches of the Agent behavior section, in its order. */
export const AGENT_BEHAVIOR_SWITCHES = switchesIn<ApplicationBooleanField>("agentBehavior");

/** The switches of the Cleanup section, in its order. */
export const CLEANUP_SWITCHES = switchesIn<ApplicationBooleanField>("cleanup");

/** The application settings that are a switch: those of the five groups above. */
export const APPLICATION_SWITCHES: readonly BooleanSetting<ApplicationBooleanField>[] = [
  ...GENERAL_SWITCHES,
  ...APP_BEHAVIOR_SWITCHES,
  ...GIT_SWITCHES,
  ...AGENT_BEHAVIOR_SWITCHES,
  ...CLEANUP_SWITCHES,
];
