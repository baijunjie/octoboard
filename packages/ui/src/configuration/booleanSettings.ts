import type { PlainMessageKey } from "../i18n/catalog";
import type { DocumentSettings } from "../protocol";

/** The document's settings that are a switch, picked by their type so a setting of another kind
 * cannot be listed below. */
export type BooleanField = { [K in keyof DocumentSettings]-?: DocumentSettings[K] extends boolean ? K : never }[keyof DocumentSettings];

/** One application setting that is a switch: its field (see `Settings.<field>` in protocol.generated.ts for what it
 * does), the name and the two descriptions the settings page gives it, the help tip some carry and what
 * turning it on means in the import cards. The keys are listed, not derived: their prefixes are irregular. */
export interface BooleanSetting {
  field: BooleanField;
  label: PlainMessageKey;
  description: { on: PlainMessageKey; off: PlainMessageKey };
  /** The help tip that ends the label's line: its accessible name and its text. */
  help?: { label: PlainMessageKey; text: PlainMessageKey };
  consequence: PlainMessageKey;
}

/** The copy of each switch, complete by type: a boolean setting added to `DocumentSettings` fails to compile
 * until it has an entry. Its order is the order of the import cards. */
const COPY: Record<BooleanField, Omit<BooleanSetting, "field">> = {
  auto_pull_repositories: {
    label: "settings.git.autoPull.label",
    description: { on: "settings.git.autoPull.description.on", off: "settings.git.autoPull.description.off" },
    help: { label: "settings.git.autoPull.help", text: "settings.git.autoPull.remoteCheck" },
    consequence: "configuration.import.settings.consequence.autoPull",
  },
  auto_delete_unresumable_archives: {
    label: "settings.archiveCleanup.label",
    description: { on: "settings.archiveCleanup.description.on", off: "settings.archiveCleanup.description.off" },
    consequence: "configuration.import.settings.consequence.archiveCleanup",
  },
  auto_select_account: {
    label: "settings.autoSelect.label",
    description: { on: "settings.autoSelect.description.on", off: "settings.autoSelect.description.off" },
    consequence: "configuration.import.settings.consequence.autoSelectAccount",
  },
  auto_switch_on_usage_limit: {
    label: "settings.autoSwitch.label",
    description: { on: "settings.autoSwitch.description.on", off: "settings.autoSwitch.description.off" },
    consequence: "configuration.import.settings.consequence.autoSwitchOnUsageLimit",
  },
  sidebar_order_by_status: {
    label: "settings.sidebarOrder.label",
    description: { on: "settings.sidebarOrder.description.on", off: "settings.sidebarOrder.description.off" },
    consequence: "configuration.import.settings.consequence.sidebarOrder",
  },
  auto_start_quota_windows: {
    label: "settings.startQuotaWindows.label",
    description: { on: "settings.startQuotaWindows.description.on", off: "settings.startQuotaWindows.description.off" },
    help: { label: "settings.startQuotaWindows.help", text: "settings.startQuotaWindows.helpText" },
    consequence: "configuration.import.settings.consequence.autoStartQuotaWindows",
  },
  auto_approve_session_requests: {
    label: "settings.autoApproveSession.label",
    description: { on: "settings.autoApproveSession.description.on", off: "settings.autoApproveSession.description.off" },
    consequence: "configuration.import.settings.consequence.autoApproveSession",
  },
  auto_approve_resume_requests: {
    label: "settings.autoApproveResume.label",
    description: { on: "settings.autoApproveResume.description.on", off: "settings.autoApproveResume.description.off" },
    consequence: "configuration.import.settings.consequence.autoApproveResume",
  },
  auto_approve_account_switches: {
    label: "settings.autoApproveAccountSwitches.label",
    description: { on: "settings.autoApproveAccountSwitches.description.on", off: "settings.autoApproveAccountSwitches.description.off" },
    consequence: "configuration.import.settings.consequence.autoApproveAccountSwitches",
  },
  auto_approve_handoffs: {
    label: "settings.autoApproveHandoffs.label",
    description: { on: "settings.autoApproveHandoffs.description.on", off: "settings.autoApproveHandoffs.description.off" },
    consequence: "configuration.import.settings.consequence.autoApproveHandoffs",
  },
  ask_questions_as_choices: {
    label: "settings.askQuestionsAsChoices.label",
    description: { on: "settings.askQuestionsAsChoices.description.on", off: "settings.askQuestionsAsChoices.description.off" },
    consequence: "configuration.import.settings.consequence.askQuestionsAsChoices",
  },
};

/** The entry of `field`. */
export function booleanSetting(field: BooleanField): BooleanSetting {
  return { field, ...COPY[field] };
}

/** The application settings that are a switch, in the order the import cards list them: that of `COPY`. */
export const BOOLEAN_SETTINGS: readonly BooleanSetting[] = (Object.keys(COPY) as BooleanField[]).map(booleanSetting);
