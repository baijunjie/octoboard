import React, { useState } from "react";

import { AGENT_LABEL } from "../agents";
import { Button } from "../components/Button";
import { DetailsButton } from "../components/DetailsButton";
import { InlineReference } from "../components/InlineReference";
import { TILDE_PARAMS } from "../components/literalParams";
import { MarkLineBox } from "../components/MarkLineBox";
import { StatusMark } from "../components/StatusMark";
import { StatusBadge, type StatusBadgeColor } from "../components/StatusBadge";
import { useSubmitValidation } from "../dialogs/Dialog";
import { DirectoryPicker } from "../dialogs/DirectoryPicker";
import { TextInput } from "../dialogs/TextInput";
import type { PlainMessageKey } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import { joinNodes } from "../i18n/joinPhrases";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import type { AccountFallback, Agent, DocumentSettings, ImportFailure } from "../protocol";
import { useFailureNode } from "./failureText";
import { isUsableDirectory, type DirectoryProblem } from "./importPlan";

/** The look of one card of the import dialog: a console's projects, an agent's accounts, the applied or applicable settings. */
export const CARD_CLASS = "flex flex-col rounded-xl bg-surface-secondary panel-edge shadow-(--panel-shadow)";
/** A card's header row: one step darker than the card, so the header reads apart from the rows under it. */
export const CARD_HEADER_CLASS = "rounded-t-xl bg-[color-mix(in_oklab,var(--surface-secondary),var(--foreground)_6%)] groove-b px-4 py-3 last:rounded-b-xl last:border-b-0";

/** `useSubmitValidation`'s `shown`: holds a field's message back until the user has tried to import. */
export type Shown = ReturnType<typeof useSubmitValidation>["shown"];

/** A labelled path or URL on a line of its own, shown as plain monospace text (see the "References in copy use
 * `InlineReference`" section of docs/ui-accessibility.md). It is a pair of cells of a `dl` laid out as a two-column
 * grid. */
export function Field({ label, value }: { label: string; value: string }): React.ReactElement {
  return (
    <>
      <dt className="text-app-muted">{label}</dt>
      <dd className="min-w-0 font-mono [overflow-wrap:anywhere]" dir="ltr">{value}</dd>
    </>
  );
}

/** The agents of a list as references in a sentence, joined as the language joins a list. */
export function AgentList({ agents }: { agents: Agent[] }): React.ReactNode {
  const language = useCurrentLanguage();
  return joinNodes(language, agents.map((agent) => AGENT_LABEL[agent]), (label, key) => <InlineReference key={key} dir="auto" wrap>{label}</InlineReference>);
}

/**
 * The field a project or an account is given a directory of the user's own in, with a Browse button for the directory
 * picker. `problem` is what is wrong with the typed value, already held back until the user has tried to import, and
 * `requiredMessage` is the sentence for an empty one.
 */
export function DirectoryChoiceField({
  label,
  placeholder,
  requiredMessage,
  value,
  onChange,
  problem,
}: {
  /** The field's accessible name, naming what the directory is for. */
  label: string;
  placeholder: string;
  requiredMessage: string;
  value: string;
  onChange: (directory: string) => void;
  problem: DirectoryProblem | undefined;
}): React.ReactElement {
  const t = useT();
  const [picking, setPicking] = useState(false);
  return (
    <>
      <TextInput
        label={label}
        labelContent={t("configuration.import.directory.visibleLabel")}
        value={value}
        onChange={onChange}
        dir="ltr"
        placeholder={placeholder}
        errorMessage={problem === "empty" ? requiredMessage : problem === "not_absolute" ? <Message id="configuration.import.directory.notAbsolute" params={TILDE_PARAMS} /> : undefined}
        trailing={
          <Button type="button" variant="secondary" onPress={() => setPicking(true)}>
            {t("common.browse")}
          </Button>
        }
      />
      {picking && (
        <DirectoryPicker
          title={t("dialog.chooseDirectory")}
          initialPath={isUsableDirectory(value) ? value : "~"}
          onPick={(picked) => {
            onChange(picked);
            setPicking(false);
          }}
          onClose={() => setPicking(false)}
        />
      )}
    </>
  );
}

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
  { label: "settings.autoSwitch.label", field: "auto_switch_on_usage_limit", consequence: "configuration.import.settings.consequence.autoSwitchOnUsageLimit" },
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
 * when there are none. `size` is the name's text size class. */
export function FallbackMark({ subject, fallbacks, size }: { subject: string; fallbacks: AccountFallback[]; size: string }): React.ReactElement | null {
  const t = useT();
  if (fallbacks.length === 0) return null;
  return (
    <MarkLineBox size={size}>
      <StatusMark tone="warning" label={t("configuration.import.fallbacks.label", { subject: isolate(subject) })}>
        <FallbackLines fallbacks={fallbacks} />
      </StatusMark>
    </MarkLineBox>
  );
}

/** What the daemon reported of a failure, written out where it happened: the reason in the application's own words,
 * the one main thing to do about it under it, and, where there is more, the further explanation and the raw text behind
 * a Details button, so the raw text never stands in the line itself. Under a sentence that already says what
 * happened, `withoutSummary` leaves the reason out and keeps the remedy and Details. */
export function FailureReason({ failure, withoutSummary = false }: { failure: ImportFailure; withoutSummary?: boolean }): React.ReactElement {
  const failureNode = useFailureNode();
  const remedy = failureNode(failure, "remedy");
  const details = failureNode(failure, "details");
  return (
    <div className="flex flex-col items-start gap-1.5 text-sm">
      {!withoutSummary && <p className="min-w-0 [overflow-wrap:anywhere]" dir="auto">{failureNode(failure, "summary")}</p>}
      {remedy && <p className="min-w-0 text-app-muted [overflow-wrap:anywhere]" dir="auto">{remedy}</p>}
      {details && <DetailsButton placement="bottom">{details}</DetailsButton>}
    </div>
  );
}
