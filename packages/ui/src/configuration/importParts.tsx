import React from "react";

import { AGENT_LABEL } from "../agents";
import { InlineReference } from "../components/InlineReference";
import { StatusMark } from "../components/StatusMark";
import { StatusBadge, type StatusBadgeColor } from "../components/StatusBadge";
import type { PlainMessageKey } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import type { AccountFallback, DocumentSettings } from "../protocol";

/** The look of one card of the import dialog: a console's projects, the applied or applicable settings. */
export const CARD_CLASS = "flex flex-col rounded-xl border border-transparent shadow-(--panel-shadow)";

/** One line of a settings card: the setting's name, a badge for what came or would come of it and,
 * where that is worth knowing, a muted note under it. */
export function SettingLine({ label, badge, tone = "default", children }: { label: string; badge: string; tone?: StatusBadgeColor; children?: React.ReactNode }): React.ReactElement {
  return (
    <li className="flex flex-col gap-0.5 py-2">
      <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
        <span className="font-medium text-foreground" dir="auto">{label}</span>
        <StatusBadge color={tone} variant="soft">{badge}</StatusBadge>
      </span>
      {children && <span className="text-sm text-app-muted">{children}</span>}
    </li>
  );
}

/** The document's settings that are a switch, picked by their type so a setting of another kind
 * cannot be listed below. */
type BooleanField = { [K in keyof DocumentSettings]-?: NonNullable<DocumentSettings[K]> extends boolean ? K : never }[keyof DocumentSettings];

/** The application settings that are a switch, in the order the cards list them: the name the
 * settings page gives each, its field in the document and what turning it on means. */
export const BOOLEAN_SETTINGS: {
  label: PlainMessageKey;
  field: BooleanField;
  consequence: PlainMessageKey;
}[] = [
  { label: "settings.git.autoPull.label", field: "auto_pull_repositories", consequence: "configuration.import.settings.consequence.autoPull" },
  { label: "settings.archiveCleanup.label", field: "auto_delete_unresumable_archives", consequence: "configuration.import.settings.consequence.archiveCleanup" },
  { label: "settings.autoSelect.label", field: "auto_select_account", consequence: "configuration.import.settings.consequence.autoSelectAccount" },
  { label: "settings.sidebarOrder.label", field: "sidebar_order_by_status", consequence: "configuration.import.settings.consequence.sidebarOrder" },
  { label: "settings.startQuotaWindows.label", field: "auto_start_quota_windows", consequence: "configuration.import.settings.consequence.autoStartQuotaWindows" },
  { label: "settings.autoApproveSession.label", field: "auto_approve_session_requests", consequence: "configuration.import.settings.consequence.autoApproveSession" },
  { label: "settings.autoApproveResume.label", field: "auto_approve_resume_requests", consequence: "configuration.import.settings.consequence.autoApproveResume" },
  { label: "settings.autoApproveAccountSwitches.label", field: "auto_approve_account_switches", consequence: "configuration.import.settings.consequence.autoApproveAccountSwitches" },
  { label: "settings.autoApproveHandoffs.label", field: "auto_approve_handoffs", consequence: "configuration.import.settings.consequence.autoApproveHandoffs" },
  { label: "settings.askQuestionsAsChoices.label", field: "ask_questions_as_choices", consequence: "configuration.import.settings.consequence.askQuestionsAsChoices" },
];

/** One paragraph per account reference that names no account of this machine: the agent's default account
 * is used instead. */
export function FallbackLines({ fallbacks }: { fallbacks: AccountFallback[] }): React.ReactElement {
  return (
    <>
      {fallbacks.map((fallback) => (
        <p key={`${fallback.agent}:${fallback.name}`} dir="auto">
          <Message
            id="configuration.import.fallback"
            params={{ agent: <InlineReference dir="auto" wrap>{AGENT_LABEL[fallback.agent]}</InlineReference>, name: <InlineReference dir="auto" wrap>{fallback.name}</InlineReference> }}
          />
        </p>
      ))}
    </>
  );
}

/** The warning mark after the name of a console or a project whose account references name no account of
 * this machine: its popover says, per reference, that the agent's default account is used instead. Nothing
 * when there are none. */
export function FallbackMark({ subject, fallbacks }: { subject: string; fallbacks: AccountFallback[] }): React.ReactElement | null {
  const t = useT();
  if (fallbacks.length === 0) return null;
  return (
    <StatusMark tone="warning" label={t("configuration.import.fallbacks.label", { subject: isolate(subject) })}>
      <FallbackLines fallbacks={fallbacks} />
    </StatusMark>
  );
}
