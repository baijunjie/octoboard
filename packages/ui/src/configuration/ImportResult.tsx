import { CircleAlert, CircleCheck, CircleMinus, CircleSlash, Info, type LucideIcon } from "lucide-react";
import React, { useId } from "react";

import { AGENT_LABEL } from "../agents";
import { CodedFailureBody } from "../components/errorParts";
import { InlineReference } from "../components/InlineReference";
import { ErrorAlert } from "../components/ErrorAlert";
import { StatusMark } from "../components/StatusMark";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/StatusAnnouncer";
import type { PlainMessageKey } from "../i18n/catalog";
import { Message, useT } from "../i18n/react";
import type { Event, ImportChoiceKind, ImportedProject, ImportFailure } from "../protocol";
import { isolate } from "../i18n/isolate";
import { useFailureNode, useFailureText } from "./failureText";
import { outcomeOf } from "./importPlan";
import { BOOLEAN_SETTINGS, CARD_CLASS, SettingLine } from "./importParts";

export type Imported = Extract<Event, { type: "configuration_imported" }>;

const VIA_LABEL: Record<ImportChoiceKind, PlainMessageKey> = {
  exported_path: "configuration.import.result.via.exportedPath",
  default_clone_dir: "configuration.import.result.via.defaultCloneDir",
  clone: "configuration.import.result.via.clone",
  directory: "configuration.import.result.via.directory",
};

const COLUMNS: { key: PlainMessageKey; width: string }[] = [
  { key: "configuration.import.result.column.project", width: "@xl:w-1/3" },
  { key: "configuration.import.result.column.result", width: "@xl:w-36" },
  { key: "configuration.import.result.column.detail", width: "" },
];

type Status = ImportedProject["status"];

/** What each outcome that is not a failure is called, drawn and coloured. The colour only strengthens
 * the word and the shape, which differ between the outcomes; a failure is a `StatusMark`. */
const STATUS: Record<Exclude<Status, "failed">, { label: PlainMessageKey; icon: LucideIcon; tone: string }> = {
  created: { label: "configuration.import.result.created", icon: CircleCheck, tone: "text-success-soft-foreground" },
  left_out: { label: "configuration.import.result.leftOut", icon: CircleMinus, tone: "text-app-muted" },
  not_attempted: { label: "configuration.import.result.status.notAttempted", icon: CircleSlash, tone: "text-app-muted" },
};

/** What came of a console or one of its projects, and what there is to say about that. */
interface Line {
  status: Status;
  detail: React.ReactNode;
}

/** A console and the lines under it. The console has a line of its own only when something came of
 * it that its projects do not already say: it failed, or it was created with nothing in it. */
interface Group {
  console: string;
  own: Line | undefined;
  projects: (Line & { project: string })[];
}

/** One cell. The cell that is dropped when stacked (`empty`) is only ever the last column's, so no
 * other cell is without its value and the rest never end up under the wrong header; a column in
 * the middle that could be empty would have to keep its cell. Stacked, it carries its column's
 * name in front of the value, as text of its own, so a value is never left without what it is the
 * value of; in a row of cells the header says it, and the label is not drawn (nor read: it is
 * `aria-hidden`, since the column header does the naming for assistive technology in both forms). */
function Cell({
  label,
  empty,
  indent,
  head,
  children,
}: {
  label: string;
  /** The first cell of a project's row, set in from its console's name above it. */
  indent?: boolean;
  /** A cell of a console's own line, which heads its group: spaced like the name, with no groove under it. */
  head?: boolean;
  /** Nothing to say here: the cell is dropped when stacked, where an empty value is only noise. */
  empty?: boolean;
  children?: React.ReactNode;
}): React.ReactElement {
  return (
    <td
      role="cell"
      className={`items-baseline gap-2 py-0.5 ${empty ? "hidden" : "flex"} @xl:table-cell @xl:px-2 @xl:align-top ${head ? "@xl:pt-4 @xl:pb-2" : "@xl:groove-b @xl:group-last/row:border-b-0 @xl:py-2"} ${indent ? "@xl:ps-6" : ""}`}
    >
      <span aria-hidden="true" className="w-20 shrink-0 text-app-muted [overflow-wrap:anywhere] @xl:hidden">
        {label}
      </span>
      <div className="min-w-0 [overflow-wrap:anywhere]" dir="auto">
        {children}
      </div>
    </td>
  );
}

/**
 * What an import came to, as the daemon reported it: a summary of the counts, an alert when the
 * application settings could not be applied, a table with the projects grouped under their console's
 * name (a console that failed or came out empty has its own result on its header row; a project says
 * what came of it: created and how it was placed, left out, failed and why, not tried), every account
 * reference that fell back to a default account under a heading of its own, and, at the end, a card
 * of the application settings that were applied. The summary gives the totals and the table the
 * items, so neither repeats the other. `shortcuts` says what came of the document's keyboard
 * shortcuts, when they were applied here.
 */
export function ImportResult({ result, shortcuts }: { result: Imported; shortcuts?: { applied: number; skipped: number } }): React.ReactElement {
  const t = useT();
  const failureNode = useFailureNode();
  const failureText = useFailureText();
  const fallbacksId = useId();
  const outcome = outcomeOf(result.consoles);
  const applied = result.settings_applied;
  const settingsHeadingId = useId();
  // A failed line's popover: the reason in the application's own words, its remedy, the raw text last.
  const failureBody = (failure: ImportFailure) => <CodedFailureBody failure={failure} text={failureText(failure)} />;
  // Every project in the order of the document, under its console.
  const groups: Group[] = result.consoles.map((console) => ({
    console: console.name,
    own:
      console.status === "failed"
        ? { status: "failed", detail: console.error ? failureBody(console.error) : "" }
        : console.projects.length === 0
          ? { status: "created", detail: "" }
          : undefined,
    projects: console.projects.map((project) => ({
      project: project.name,
      status: project.status,
      detail:
        project.status === "created"
          ? project.via
            ? t(VIA_LABEL[project.via])
            : ""
          : project.status === "failed"
            ? project.error
              ? failureBody(project.error)
              : ""
            : project.status === "not_attempted"
              ? t("configuration.import.result.detail.consoleFailed")
              : "",
    })),
  }));
  // A failed line is a glyph and the word like the others; the pair opens the reason in a popover.
  const outcomeCell = (line: Line, subject: string) => {
    const settled = line.status === "failed" ? undefined : STATUS[line.status];
    return settled === undefined ? (
      <StatusMark
        label={t("configuration.import.result.failedFor", { subject: isolate(subject) })}
        text={t("configuration.import.result.failed")}
      >
        {line.detail === "" ? undefined : line.detail}
      </StatusMark>
    ) : (
      <span className="inline-flex items-center gap-1.5 whitespace-nowrap">
        <settled.icon aria-hidden="true" className={`size-4 shrink-0 ${settled.tone}`} />
        {t(settled.label)}
      </span>
    );
  };
  const failures = outcome.consolesFailed + outcome.projectsFailed;
  // One sentence per thing that happened, so a count and what it counts stay together; a thing
  // that did not happen has no sentence, and "nothing failed" is said once instead of by two zeros.
  const summary = [
    outcome.consolesCreated > 0 && t("configuration.import.result.consolesCreated", { count: outcome.consolesCreated }),
    outcome.projectsCreated > 0 && t("configuration.import.result.projectsCreated", { count: outcome.projectsCreated }),
    outcome.projectsLeftOut > 0 && t("configuration.import.result.projectsLeftOut", { count: outcome.projectsLeftOut }),
    failures > 0 && t("configuration.import.result.failedCount", { count: failures }),
    outcome.projectsNotAttempted > 0 &&
      t("configuration.import.result.notAttempted", { count: outcome.projectsNotAttempted }),
    // The settings are told of on their own line, and a failure there is a failure too.
    failures === 0 &&
      outcome.projectsNotAttempted === 0 &&
      !result.settings_error &&
      t("configuration.import.result.noFailures"),
  ].filter((sentence) => sentence !== false);

  return (
    <>
      <p>{summary.join(t("configuration.import.result.summary.join"))}</p>
      {result.settings_error && (
        <ErrorAlert
          announce
          announceDelayMs={AFTER_FOCUS_MOVE_DELAY_MS}
          title={t("configuration.import.result.settings.failedTitle")}
          remedy={failureNode(result.settings_error, "remedy")}
          details={failureNode(result.settings_error, "details")}>
          <Message id="configuration.import.result.settings.failed" params={{ reason: failureNode(result.settings_error, "summary") }} />
        </ErrorAlert>
      )}
      {groups.length > 0 && (
        <div className="@container">
          {/* Narrower than `@xl` of its own width, a line becomes a block of labelled values rather than a
              row of cells, and nothing scrolls sideways. The table's container, not the window, is what
              tells: the dialog narrows with the window, and a window under the `docked` breakpoint can
              still hold it at full width. */}
          {/* The table is `border-separate` at `@xl` and the groove sits on its cells: in collapse mode
              `border-image` does not apply to rows or cells, and in separate mode a `tr` draws no border. */}
          {/* Explicit roles on every part: stacked, the parts are `display: block`, which some
              browsers take as leaving the table, and the roles keep it one. */}
          <table
            role="table"
            aria-label={t("configuration.import.result.table.label")}
            className="block w-full text-start @xl:table @xl:table-fixed @xl:border-separate @xl:border-spacing-0"
          >
            <thead role="rowgroup" className="sr-only @xl:not-sr-only">
              <tr role="row">
                {COLUMNS.map(({ key, width }) => (
                  <th
                    key={key}
                    role="columnheader"
                    scope="col"
                    className={`px-2 pb-2 text-start font-medium text-app-muted @xl:groove-b ${width}`}
                  >
                    {t(key)}
                  </th>
                ))}
              </tr>
            </thead>
            {/* A console heads a group of rows, so its name is said once rather than on every project's line. */}
            {groups.map((group, index) => (
              <tbody key={index} role="rowgroup" className="block @xl:table-row-group">
                <tr role="row" className="group/row block pt-3 @xl:table-row @xl:pt-0">
                  <th
                    role="rowheader"
                    scope="rowgroup"
                    colSpan={group.own ? undefined : COLUMNS.length}
                    className="block py-0.5 text-start font-semibold text-foreground [overflow-wrap:anywhere] @xl:table-cell @xl:px-2 @xl:pt-4 @xl:pb-2 @xl:align-top"
                    dir="auto"
                  >
                    {group.console}
                  </th>
                  {group.own && (
                    <>
                      <Cell label={t(COLUMNS[1].key)} head>{outcomeCell(group.own, group.console)}</Cell>
                      <Cell label={t(COLUMNS[2].key)} head empty={group.own.status === "failed" || group.own.detail === ""}>
                        {group.own.status !== "failed" && group.own.detail}
                      </Cell>
                    </>
                  )}
                </tr>
                {group.projects.map((line, p) => (
                  <tr key={p} role="row" className="group/row block groove-b py-2 ps-4 last:border-b-0 @xl:table-row @xl:py-0 @xl:ps-0">
                    <Cell label={t(COLUMNS[0].key)} indent>{line.project}</Cell>
                    <Cell label={t(COLUMNS[1].key)}>{outcomeCell(line, line.project)}</Cell>
                    <Cell label={t(COLUMNS[2].key)} empty={line.status === "failed" || line.detail === ""}>
                      {line.status !== "failed" && line.detail}
                    </Cell>
                  </tr>
                ))}
              </tbody>
            ))}
          </table>
        </div>
      )}
      {outcome.fallbacks.length > 0 && (
        <section aria-labelledby={fallbacksId} className="flex flex-col gap-2">
          <h3 id={fallbacksId} className="flex items-center gap-2 text-base font-semibold text-foreground">
            <CircleAlert aria-hidden="true" className="size-4 shrink-0 text-warning-soft-foreground" />
            {t("configuration.import.result.fallbacks")}
          </h3>
          <p className="flex items-start gap-2 text-sm">
            <Info aria-hidden="true" className="mt-0.5 size-4 shrink-0 text-warning-soft-foreground" />
            <span className="min-w-0"><Message id="configuration.import.result.fallbacks.intro" /></span>
          </p>
          <ul className="flex flex-col gap-1">
            {outcome.fallbacks.map((fallback, index) => {
              const agent = <InlineReference dir="auto" wrap>{AGENT_LABEL[fallback.agent]}</InlineReference>;
              return (
                <li key={index} className="[overflow-wrap:anywhere]" dir="auto">
                  {fallback.project === undefined
                    ? <Message id="configuration.import.result.fallback.console" params={{
                      agent,
                      name: <InlineReference dir="auto" wrap>{fallback.name}</InlineReference>,
                      console: <InlineReference dir="auto" wrap>{fallback.console}</InlineReference>,
                    }} />
                    : <Message id="configuration.import.result.fallback.project" params={{
                      agent,
                      name: <InlineReference dir="auto" wrap>{fallback.name}</InlineReference>,
                      project: <InlineReference dir="auto" wrap>{fallback.project}</InlineReference>,
                      console: <InlineReference dir="auto" wrap>{fallback.console}</InlineReference>,
                    }} />}
                </li>
              );
            })}
          </ul>
        </section>
      )}
      {applied && (
        <section aria-labelledby={settingsHeadingId} className={CARD_CLASS}>
          <h3 id={settingsHeadingId} className="groove-b px-4 py-3 text-base font-semibold text-foreground">{t("configuration.import.result.settings.heading")}</h3>
          <ul className="flex flex-col px-4 py-1 [&>li]:groove-b [&>li:last-child]:border-b-0">
            {BOOLEAN_SETTINGS.map(({ label, field }) => {
              const on = applied[field];
              return typeof on === "boolean" && <SettingLine key={field} label={t(label)} badge={t(on ? "configuration.import.result.badge.on" : "configuration.import.result.badge.off")} />;
            })}
            {applied.git_workflow && <SettingLine label={t("configuration.import.gitWorkflow")} badge={t("configuration.import.result.badge.matches")} />}
            {shortcuts && (
              <SettingLine
                label={t("settings.section.shortcuts")}
                badge={shortcuts.applied === 0 ? t("configuration.import.result.badge.defaults") : t("configuration.import.result.badge.shortcuts", { count: shortcuts.applied })}
              >
                {shortcuts.skipped > 0 && t("configuration.import.result.shortcutsSkipped", { count: shortcuts.skipped })}
              </SettingLine>
            )}
          </ul>
        </section>
      )}
    </>
  );
}
