import { CircleAlert, CircleMinus, CircleSlash, Folder, Info, type LucideIcon } from "lucide-react";
import React from "react";

import { FlatCard } from "../components/card/FlatCard";
import { Checkbox } from "../primitives";
import { DetailsButton } from "../components/alerts/DetailsButton";
import { IndicatorTooltip } from "../components/hint/IndicatorTooltip";
import { InlineReference } from "../i18n/InlineReference";
import { TILDE_PARAMS } from "../components/text/literalParams";
import { MarkLineBox } from "../components/status/MarkLineBox";
import { StatusMark } from "../components/status/StatusMark";
import { StatusBadge, type StatusBadgeColor } from "../components/status/StatusBadge";
import { useSubmitValidation } from "../components/dialog/dialogHooks";
import { useDirectoryBrowse } from "../components/dialog/DirectoryPicker";
import { TextInput } from "../components/form/TextInput";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import type { AccountFallback, ImportFailure } from "../protocol";
import type { PlainMessageKey } from "../i18n/catalog";
import { useFailureNode } from "./failureText";
import { type DirectoryProblem } from "./importPlan";
import type { ClientSettingValue } from "./clientSettings";
import type { SettingValue } from "./importValues";
import { ProviderName } from "../components/text/ProviderName";

/** A flat card's heading inside a header row of its own: the name, and after it the warning mark of the account
 * references that fell back to a default account. `className` lays it out in the row. */
export function CardTitle({ name, fallbacks, className }: { name: string; fallbacks: AccountFallback[]; className: string }): React.ReactElement {
  return (
    <div className={`flex items-start gap-1 ${className}`}>
      <FlatCard.Heading className="min-w-0 [overflow-wrap:anywhere]" dir="auto">
        {name}
      </FlatCard.Heading>
      <FallbackMark subject={name} fallbacks={fallbacks} size="text-base" />
    </div>
  );
}

/** A note: a line marked by the circled `i`. */
export function ToneLine({ children }: { children: React.ReactNode }): React.ReactElement {
  return (
    <p className="flex items-start gap-2 text-sm">
      <Info aria-hidden="true" className="mt-0.5 size-4 shrink-0 text-accent-soft-foreground" />
      <span className="min-w-0">{children}</span>
    </p>
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

/**
 * One row of the review: `lead` (the name and what is known of the item) at the start, the `control` that decides
 * what to do with it, when it has one, at the end (under the lead where the row is narrow), and what the choice calls
 * for under both as `children`.
 */
export function ReviewRow({ lead, control, children }: { lead: React.ReactNode; control?: React.ReactNode; children?: React.ReactNode }): React.ReactElement {
  return (
    <li className="@container flex flex-col gap-2 py-3">
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

/** The statuses a finished import and a running one share: an item that failed, and one never tried because the console
 * it belongs to failed. */
export const SHARED_STATUS = {
  failed: { label: "configuration.import.result.failed", icon: CircleAlert, tone: STATUS_TONE.danger },
  not_attempted: { label: "configuration.import.result.status.notAttempted", icon: CircleSlash, tone: STATUS_TONE.muted },
} satisfies Record<string, StatusEntry>;

/** An item or card the user left out of the import, as a result and a card left unchecked show it. */
export const LEFT_OUT_STATUS: StatusEntry = { label: "configuration.import.result.leftOut", icon: CircleMinus, tone: STATUS_TONE.muted };

/** The glyph of a status, `size` being its size (and margin) classes. */
export function StatusGlyph({ entry, size }: { entry: StatusEntry; size: string }): React.ReactElement {
  const Icon = entry.icon;
  return <Icon aria-hidden="true" className={`${size} shrink-0 ${entry.tone}`} />;
}

/** A status as a line: `children` is its glyph (`StatusGlyph`, or a spinner while it is under way) and `label` its word. */
export function StatusLine({ label, children }: { label: PlainMessageKey; children: React.ReactNode }): React.ReactElement {
  const t = useT();
  return (
    <span className="inline-flex items-center gap-1.5 whitespace-nowrap text-sm">
      {children}
      {t(label)}
    </span>
  );
}

/** A status entry as its glyph and its word. */
export function EntryStatus({ entry }: { entry: StatusEntry }): React.ReactElement {
  return (
    <StatusLine label={entry.label}>
      <StatusGlyph entry={entry} size="size-4" />
    </StatusLine>
  );
}

/** `useSubmitValidation`'s `shown`: holds a field's message back until the user has tried to import. */
export type Shown = ReturnType<typeof useSubmitValidation>["shown"];

/** A labelled path or URL on a line of its own, shown as plain monospace text (see the "References in copy use
 * `InlineReference`" section of docs/ui-standard.md). It is a pair of cells of a `dl` laid out as a two-column
 * grid. */
export function Field({ label, value, mark }: {
  label: string;
  value: string;
  /** A mark about the value, after it: a note of what is wrong with it. */
  mark?: React.ReactNode;
}): React.ReactElement {
  return (
    <>
      <dt className="text-app-muted">{label}</dt>
      <dd className="min-w-0 font-mono [overflow-wrap:anywhere]" dir="ltr">
        {value}
        {mark && <span className="ms-1 inline-flex align-middle font-sans" dir="auto">{mark}</span>}
      </dd>
    </>
  );
}

/** A directory on a line of its own, led by a folder icon in place of a label repeated on every row. The icon is a
 * mark that nothing else on screen names, so it is a named image (`label`) with the same words as its tooltip. Kept
 * shortfall, marked here as "A tooltip on a mark that is not a control" in docs/ui-standard.md asks: a sighted user
 * working from the keyboard reaches no tooltip, so the folder's name reaches them nowhere. */
export function DirectoryLine({ label, value }: { label: string; value: string }): React.ReactElement {
  return (
    <div className="flex items-start gap-1.5 text-sm">
      <IndicatorTooltip tooltip={label} label={label} className="mt-0.5 shrink-0 text-app-muted">
        <Folder aria-hidden="true" className="size-3.5" />
      </IndicatorTooltip>
      <span className="min-w-0 font-mono [overflow-wrap:anywhere]" dir="ltr">{value}</span>
    </div>
  );
}

/** The `Field`s of a row, as the `dl` that lays them out in two columns. */
export function FieldList({ children }: { children: React.ReactNode }): React.ReactElement {
  return <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1 text-sm">{children}</dl>;
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
export function SettingLine({ label, badge, tone = "default", values, children }: { label: React.ReactNode; badge: React.ReactNode; tone?: StatusBadgeColor; values?: SettingValue[]; children?: React.ReactNode }): React.ReactElement {
  return (
    <li className="flex flex-col gap-0.5 py-2">
      <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
        <span className="font-medium text-foreground" dir={typeof label === "string" ? "auto" : undefined}>{label}</span>
        <StatusBadge color={tone} variant="soft">{badge}</StatusBadge>
      </span>
      {children && <div className="text-sm text-app-muted">{children}</div>}
      {values && values.length > 0 && (
        // Lines 24px tall, so a row with tags or key caps keeps the rhythm of a plain row and wrapped lines of
        // chips do not touch. A row that wraps hangs its later lines, so they do not read as rows of their own; the
        // chips inside (blocks that would inherit the indent) take none.
        <ul className="flex flex-col text-sm leading-6 text-app-muted">
          {values.map(({ item, value }) => (
            <li key={item ?? "value"} className="ps-4 -indent-4 [&_*]:indent-0">
              {item === undefined ? value : <Message id="configuration.import.settingValue" params={{ item, value }} />}
            </li>
          ))}
        </ul>
      )}
    </li>
  );
}

/** `text` as a name in the language `lang` (a language's own name, a style's English one), so a reader or a speech
 * engine takes it for that language. */
export function NamedValue({ text, lang }: { text: string; lang?: string }): React.ReactNode {
  return lang ? <span lang={lang}>{text}</span> : text;
}

/** A boolean application setting: what applying it would do against `current` (turns it on, off, or leaves it as it
 * is), or, with no `current`, what it was set to. */
export function BooleanSettingLine({ label, wanted, current, consequence }: { label: string; wanted: boolean; current: boolean | undefined; consequence: PlainMessageKey | null }): React.ReactElement {
  const t = useT();
  if (current === undefined) return <SettingLine label={label} badge={t(wanted ? "configuration.import.result.badge.on" : "configuration.import.result.badge.off")} />;
  if (wanted === current) return <SettingLine label={label} badge={t("configuration.import.settings.badge.same")} />;
  return wanted ? (
    <SettingLine label={label} badge={t("configuration.import.settings.badge.turnsOn")} tone="accent">
      {consequence && <Message id={consequence} />}
    </SettingLine>
  ) : (
    <SettingLine label={label} badge={t("configuration.import.settings.badge.turnsOff")} />
  );
}

/** A setting made of several items: against `current`, listing those that would change or "No change" where none
 * would; with none, listing the values it was set to. */
export function CompositeLine({ label, values, comparing }: { label: React.ReactNode; values: SettingValue[]; comparing: boolean }): React.ReactElement {
  const t = useT();
  if (!comparing) return <SettingLine label={label} badge={t("configuration.import.result.badge.matches")} values={values} />;
  return values.length === 0
    ? <SettingLine label={label} badge={t("configuration.import.settings.badge.same")} />
    : <SettingLine label={label} badge={t("configuration.import.settings.badge.changes")} tone="accent" values={values} />;
}

/** A client setting, or the language the daemon holds: the value in the file, with the value this client has now where
 * they differ. */
export function ClientSettingLine({ setting: { label, value, current, lang, currentLang } }: { setting: ClientSettingValue }): React.ReactElement {
  const t = useT();
  if (current === undefined || current === value) return <SettingLine label={t(label)} badge={t("configuration.import.settings.badge.same")} />;
  return (
    <SettingLine label={t(label)} badge={t("configuration.import.settings.badge.changes")} tone="accent">
      <Message id="configuration.import.settingValueChange" params={{ value: <NamedValue text={value} lang={lang} />, current: <NamedValue text={current} lang={currentLang} /> }} />
    </SettingLine>
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
 * a Details button, so the raw text never stands in the line itself. */
export function FailureReason({ failure }: { failure: ImportFailure }): React.ReactElement {
  const failureNode = useFailureNode();
  const remedy = failureNode(failure, "remedy");
  const details = failureNode(failure, "details");
  return (
    <div className="flex flex-col items-start gap-1.5 text-sm">
      <p className="min-w-0 [overflow-wrap:anywhere]" dir="auto">{failureNode(failure, "summary")}</p>
      {remedy && <p className="min-w-0 text-app-muted [overflow-wrap:anywhere]" dir="auto">{remedy}</p>}
      {details && <DetailsButton>{details}</DetailsButton>}
    </div>
  );
}

/** Whether one thing of the review (a Settings section or an agent's card) is imported, and the change of that. */
export interface ImportChoice {
  on: boolean;
  onChange: (on: boolean) => void;
}

/** The header row of a card with a `choice` (the import checkbox of a Settings section card or an agent card): `title` as
 * the heading holding a checkbox named by it, and, while the choice is off, the mark that the card is left out. */
export function choiceHeader(title: string, choice: ImportChoice): React.ReactElement {
  return (
    <FlatCard.Header className="flex flex-wrap items-center justify-between gap-x-4 gap-y-1">
      <FlatCard.Heading className="min-w-0">
        {/* The default variant, not `secondary`: the secondary box is filled with the page's field colour, which vanishes on the
            card header's own fill. The box is centred on the first line of the content row's own line height (`Checkbox.css`), so the
            row takes the title's size (`text-base`, a 24px line) rather than the `text-sm` of HeroUI's, which put it 2px high. */}
        <Checkbox selected={choice.on} onSelectedChange={choice.onChange} contentClassName="min-h-6 text-base" labelClassName="text-base font-semibold text-foreground">
          {title}
        </Checkbox>
      </FlatCard.Heading>
      {!choice.on && <EntryStatus entry={LEFT_OUT_STATUS} />}
    </FlatCard.Header>
  );
}
