import React from "react";

import { AGENT_LABEL } from "../agents/agents";
import { DetailsButton } from "../components/alerts/DetailsButton";
import { InlineReference } from "../i18n/InlineReference";
import { TILDE_PARAMS } from "../components/text/literalParams";
import { MarkLineBox } from "../components/status/MarkLineBox";
import { StatusMark } from "../components/status/StatusMark";
import { StatusBadge, type StatusBadgeColor } from "../components/status/StatusBadge";
import { useSubmitValidation } from "../components/dialog/dialogHooks";
import { useDirectoryBrowse } from "../components/dialog/DirectoryPicker";
import { TextInput } from "../components/form/TextInput";
import { isolate } from "../i18n/isolate";
import { joinNodes } from "../i18n/joinPhrases";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import type { AccountFallback, Agent, ImportFailure } from "../protocol";
import { useFailureNode } from "./failureText";
import { type DirectoryProblem } from "./importPlan";
import type { SettingValue } from "./importValues";

/**
 * The look of one card of the import dialog: a console's projects, an agent's accounts, the applied or applicable
 * settings. Its edge and the grooves between rows already frame it as a table, so the rows take the dialog's own
 * background and only the header is filled.
 */
export const CARD_CLASS = "flex flex-col rounded-xl panel-edge";
/** A card's header row, filled like the fields in the rows so it reads apart from the rows under it. */
export const CARD_HEADER_CLASS = "rounded-t-xl bg-(--control-fill) groove-b px-4 py-3 last:rounded-b-xl last:border-b-0";

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
  const directoryBrowse = useDirectoryBrowse(value, onChange);
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
        trailing={directoryBrowse.browseButton()}
      />
      {directoryBrowse.picker}
    </>
  );
}

/** One line of a settings card: the setting's name, a badge for what came or would come of it, where that is worth
 * knowing a muted note under it and, for a setting made of several items, a muted list of the `values` the badge
 * speaks of. */
export function SettingLine({ label, badge, tone = "default", values, children }: { label: string; badge: string; tone?: StatusBadgeColor; values?: SettingValue[]; children?: React.ReactNode }): React.ReactElement {
  return (
    <li className="flex flex-col gap-0.5 py-2">
      <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
        <span className="font-medium text-foreground" dir="auto">{label}</span>
        <StatusBadge color={tone} variant="soft">{badge}</StatusBadge>
      </span>
      {children && <span className="text-sm text-app-muted">{children}</span>}
      {values && values.length > 0 && (
        // Lines 24px tall, so a row with tags or key caps keeps the rhythm of a plain row and wrapped lines of
        // chips do not touch. A row that wraps hangs its later lines, so they do not read as rows of their own; the
        // chips inside (blocks that would inherit the indent) take none.
        <ul className="flex flex-col text-sm leading-6 text-app-muted">
          {values.map(({ item, value }) => (
            <li key={item} className="ps-4 -indent-4 [&_*]:indent-0">
              <Message id="configuration.import.settingValue" params={{ item, value }} />
            </li>
          ))}
        </ul>
      )}
    </li>
  );
}

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
      {details && <DetailsButton>{details}</DetailsButton>}
    </div>
  );
}
