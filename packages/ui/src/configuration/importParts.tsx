import { Info, TriangleAlert, type LucideIcon } from "lucide-react";
import React, { useId } from "react";

import { providerLabel } from "../agents/providers";
import { DetailsButton } from "../components/alerts/DetailsButton";
import { InlineReference } from "../i18n/InlineReference";
import { TILDE_PARAMS } from "../components/text/literalParams";
import { AgentNameList } from "../components/text/AgentNameList";
import { MarkLineBox } from "../components/status/MarkLineBox";
import { StatusMark } from "../components/status/StatusMark";
import { StatusBadge, type StatusBadgeColor } from "../components/status/StatusBadge";
import { useSubmitValidation } from "../components/dialog/dialogHooks";
import { useDirectoryBrowse } from "../components/dialog/DirectoryPicker";
import { TextInput } from "../components/form/TextInput";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import type { AccountFallback, Agent, ImportFailure } from "../protocol";
import type { PlainMessageKey } from "../i18n/catalog";
import { useFailureNode } from "./failureText";
import { type DirectoryProblem } from "./importPlan";
import type { SettingValue } from "./importValues";
import { useProviders } from "../store";
import { ProviderName } from "../components/text/ProviderName";

/**
 * The look of one card of the import dialog: a console's projects, an agent's accounts, the applied or applicable
 * settings. Its edge and the grooves between rows already frame it as a table, so the rows take the dialog's own
 * background and only the header is filled.
 */
export const CARD_CLASS = "flex flex-col rounded-xl panel-edge";
/** A card's header row, filled like the fields in the rows so it reads apart from the rows under it. */
export const CARD_HEADER_CLASS = "rounded-t-xl bg-(--control-fill) groove-b px-4 py-3 last:rounded-b-xl last:border-b-0";

/**
 * A card: a section labelled by its heading, with the header row `header` draws for the id of that heading (the heading
 * is `CardHeading` where it is the header itself, else `CardTitle` within a header of its own), then the rows.
 */
export function Card({ header, children }: { header: (headingId: string) => React.ReactNode; children: React.ReactNode }): React.ReactElement {
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className={CARD_CLASS}>
      {header(headingId)}
      {children}
    </section>
  );
}

/** A card's heading as its whole header row. */
export function CardHeading({ id, children }: { id: string; children: React.ReactNode }): React.ReactElement {
  return <h3 id={id} className={`${CARD_HEADER_CLASS} text-base font-semibold text-foreground`}>{children}</h3>;
}

/** A card's heading inside a header row of its own: the name, and after it the warning mark of the account
 * references that fell back to a default account. `className` lays it out in the row. */
export function CardTitle({ id, name, fallbacks, className }: { id: string; name: string; fallbacks: AccountFallback[]; className: string }): React.ReactElement {
  return (
    <div className={`flex items-start gap-1 ${className}`}>
      <h3 id={id} className="min-w-0 text-base font-semibold text-foreground [overflow-wrap:anywhere]" dir="auto">
        {name}
      </h3>
      <FallbackMark subject={name} fallbacks={fallbacks} size="text-base" />
    </div>
  );
}

/** A line marked by a glyph that gives its tone: a note (information, the circled `i`) by default, or a warning (the
 * triangle). `stacked` lays its body out as a column of blocks (a sentence and what goes with it) instead of running
 * text. */
export function ToneLine({ tone = "info", stacked = false, children }: { tone?: "info" | "warning"; stacked?: boolean; children: React.ReactNode }): React.ReactElement {
  const Glyph = tone === "info" ? Info : TriangleAlert;
  const glyph = <Glyph aria-hidden="true" className={`mt-0.5 size-4 shrink-0 ${tone === "info" ? "text-accent-soft-foreground" : "text-warning-soft-foreground"}`} />;
  return (
    // A line of running text is a paragraph; a stacked one holds blocks, which a paragraph cannot.
    stacked ? (
      <div className="flex items-start gap-2 text-sm">
        {glyph}
        <div className="flex min-w-0 flex-col gap-1.5">{children}</div>
      </div>
    ) : (
      <p className="flex items-start gap-2 text-sm">
        {glyph}
        <span className="min-w-0">{children}</span>
      </p>
    )
  );
}

/** The line a card shows in place of its rows when it has none. */
export function EmptyLine({ children }: { children: React.ReactNode }): React.ReactElement {
  return <p className="px-4 py-3 text-sm text-app-muted">{children}</p>;
}

/** The name of a project or an account in a row, wrapping anywhere so a long one cannot widen the row. */
export function ItemName({ children }: { children: React.ReactNode }): React.ReactElement {
  return <span className="min-w-0 font-medium text-foreground [overflow-wrap:anywhere]" dir="auto">{children}</span>;
}

/** The rows of a card, each divided from the next. */
export function CardRows({ children }: { children: React.ReactNode }): React.ReactElement {
  return <ul className="flex flex-col">{children}</ul>;
}

/**
 * One row of the review: `lead` (the name and what is known of the item) at the start, the `control` that decides
 * what to do with it, when it has one, at the end (under the lead where the row is narrow), and what the choice calls
 * for under both as `children`.
 */
export function ReviewRow({ lead, control, children }: { lead: React.ReactNode; control?: React.ReactNode; children?: React.ReactNode }): React.ReactElement {
  return (
    <li className="@container flex flex-col gap-2 groove-b px-4 py-3 last:border-b-0">
      <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-1.5">
        <div className="flex min-w-40 flex-1 flex-col gap-0.5">{lead}</div>
        {control && <div className="w-full @md:w-56">{control}</div>}
      </div>
      {children}
    </li>
  );
}

/** The colours of a status glyph. The colour only strengthens the word and the shape, which differ between statuses. */
export const STATUS_TONE = {
  success: "text-success-soft-foreground",
  accent: "text-accent-soft-foreground",
  danger: "text-danger-soft-foreground",
  muted: "text-app-muted",
} as const;

/** One status of a table (an outcome, a state): the word that names it, its glyph and the glyph's colour. */
export interface StatusEntry {
  label: PlainMessageKey;
  icon: LucideIcon;
  tone: (typeof STATUS_TONE)[keyof typeof STATUS_TONE];
}

/** The glyph of a status, `size` being its size (and margin) classes. */
export function StatusGlyph({ entry, size }: { entry: StatusEntry; size: string }): React.ReactElement {
  const Icon = entry.icon;
  return <Icon aria-hidden="true" className={`${size} shrink-0 ${entry.tone}`} />;
}

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

/** The `Field`s of a row, as the `dl` that lays them out in two columns. */
export function FieldList({ children }: { children: React.ReactNode }): React.ReactElement {
  return <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1 text-sm">{children}</dl>;
}

/** The sentence `lead` introducing agents, and the agents under it as a list, one agent to an item. */
export function AgentList({ lead, agents }: { lead: React.ReactNode; agents: Agent[] }): React.ReactElement {
  const t = useT();
  const providers = useProviders();
  return <p className="text-sm"><AgentNameList lead={lead} labels={agents.map((agent) => providerLabel(t, providers, agent))} /></p>;
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
            params={{ agent: <ProviderName agent={fallback.agent} />, name: <InlineReference dir="auto" wrap>{fallback.name}</InlineReference> }}
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
