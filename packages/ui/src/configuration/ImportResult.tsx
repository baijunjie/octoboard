import { CircleAlert, CircleCheck, CircleMinus, CircleSlash, Info, type LucideIcon } from "lucide-react";
import React, { useId } from "react";

import { AGENT_LABEL } from "../agents";
import { CodedFailureBody } from "../components/DaemonErrorMessage";
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

export type Imported = Extract<Event, { type: "configuration_imported" }>;

const VIA_LABEL: Record<ImportChoiceKind, PlainMessageKey> = {
  exported_path: "configuration.import.result.via.exportedPath",
  default_clone_dir: "configuration.import.result.via.defaultCloneDir",
  clone: "configuration.import.result.via.clone",
  directory: "configuration.import.result.via.directory",
};

const COLUMNS: { key: PlainMessageKey; width: string }[] = [
  { key: "configuration.import.result.column.console", width: "@xl:w-1/5" },
  { key: "configuration.import.result.column.project", width: "@xl:w-1/5" },
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

/** One line of the result: a console or one of its projects (none: the console itself), what came of
 * it, and what there is to say about that. */
interface Line {
  console: string;
  project: string | undefined;
  status: Status;
  detail: React.ReactNode;
}

/** One cell. The cell that is dropped when stacked (`empty`) is only ever the last column's, so no
 * other cell is without its value and the rest never end up under the wrong header; a column in the
 * middle that could be empty would have to keep its cell. Stacked, it carries its column's name in front of the value, as text of its own, so a
 * value is never left without what it is the value of; in a row of cells the header says it, and
 * the label is not drawn (nor read: it is `aria-hidden`, since the column header does the naming
 * for assistive technology in both forms). */
function Cell({
  label,
  empty,
  children,
}: {
  label: string;
  /** Nothing to say here: the cell is dropped when stacked, where an empty value is only noise. */
  empty?: boolean;
  children?: React.ReactNode;
}): React.ReactElement {
  return (
    <td
      role="cell"
      className={`items-baseline gap-2 py-0.5 ${empty ? "hidden" : "flex"} @xl:table-cell @xl:groove-b @xl:group-last/row:border-b-0 @xl:px-2 @xl:py-2 @xl:align-top`}
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
 * What an import came to, as the daemon reported it: a summary of the counts, then a table with a
 * line for each project (and for a console that failed or came out empty) saying what came of it
 * (created and how it was placed, left out, failed and why, not tried), and every account reference
 * that fell back to a default account under a heading of its own. The summary gives the totals and
 * the table the items, so neither repeats the other. `shortcuts` says what came of the document's
 * keyboard shortcuts, when they were applied here.
 */
export function ImportResult({ result, shortcuts }: { result: Imported; shortcuts?: { applied: number; skipped: number } }): React.ReactElement {
  const t = useT();
  const failureNode = useFailureNode();
  const failureText = useFailureText();
  const fallbacksId = useId();
  const outcome = outcomeOf(result.consoles);
  const settingLine = (label: string, on: boolean) => (
    <p>
      <Message
        id={on ? "configuration.import.result.settings.on" : "configuration.import.result.settings.off"}
        params={{ setting: <InlineReference dir="auto" wrap>{label}</InlineReference> }}
      />
    </p>
  );
  // A failed line's popover: the reason in the application's own words, its remedy, the raw text last.
  const failureBody = (failure: ImportFailure) => <CodedFailureBody failure={failure} text={failureText(failure)} />;
  // Every project in the order of the document, under its console; a console is a line of its
  // own only when something came of it that its projects do not already say: it failed, or it was
  // created with nothing in it.
  const lines: Line[] = result.consoles.flatMap((console) => {
    const own: Line[] =
      console.status === "failed"
        ? [
          {
            console: console.name,
            project: undefined,
            status: "failed",
            detail: console.error ? failureBody(console.error) : "",
          },
        ]
        : console.projects.length === 0
          ? [{ console: console.name, project: undefined, status: "created", detail: "" }]
          : [];
    const projects: Line[] = console.projects.map((project) => ({
      console: console.name,
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
    }));
    return [...own, ...projects];
  });
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
      {result.settings_applied && settingLine(t("settings.git.autoPull.label"), result.settings_applied.auto_pull_repositories)}
      {typeof result.settings_applied?.auto_delete_unresumable_archives === "boolean" &&
        settingLine(t("settings.archiveCleanup.label"), result.settings_applied.auto_delete_unresumable_archives)}
      {typeof result.settings_applied?.auto_select_account === "boolean" &&
        settingLine(t("settings.autoSelect.label"), result.settings_applied.auto_select_account)}
      {typeof result.settings_applied?.sidebar_order_by_status === "boolean" &&
        settingLine(t("settings.sidebarOrder.label"), result.settings_applied.sidebar_order_by_status)}
      {typeof result.settings_applied?.auto_start_quota_windows === "boolean" &&
        settingLine(t("settings.startQuotaWindows.label"), result.settings_applied.auto_start_quota_windows)}
      {typeof result.settings_applied?.auto_approve_session_requests === "boolean" &&
        settingLine(t("settings.autoApproveSession.label"), result.settings_applied.auto_approve_session_requests)}
      {typeof result.settings_applied?.auto_approve_resume_requests === "boolean" &&
        settingLine(t("settings.autoApproveResume.label"), result.settings_applied.auto_approve_resume_requests)}
      {typeof result.settings_applied?.auto_approve_account_switches === "boolean" &&
        settingLine(t("settings.autoApproveAccountSwitches.label"), result.settings_applied.auto_approve_account_switches)}
      {typeof result.settings_applied?.auto_approve_handoffs === "boolean" &&
        settingLine(t("settings.autoApproveHandoffs.label"), result.settings_applied.auto_approve_handoffs)}
      {result.settings_applied?.worktrees && (
        <p>
          <Message id="configuration.import.result.settings.worktrees" params={{ setting: <InlineReference dir="auto" wrap>{t("settings.section.worktrees")}</InlineReference> }} />
        </p>
      )}
      {shortcuts && (
        <p>
          <Message id={shortcuts.applied === 0 ? "configuration.import.result.shortcutsReset" : "configuration.import.result.shortcuts"} params={{ setting: <InlineReference dir="auto" wrap>{t("settings.section.shortcuts")}</InlineReference>, count: shortcuts.applied }} />
          {shortcuts.skipped > 0 && <> {t("configuration.import.result.shortcutsSkipped", { count: shortcuts.skipped })}</>}
        </p>
      )}
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
      {lines.length > 0 && (
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
            <tbody role="rowgroup" className="block @xl:table-row-group">
              {lines.map((line, index) => {
                const settled = line.status === "failed" ? undefined : STATUS[line.status];
                return (
                  <tr
                    key={index}
                    role="row"
                    className="group/row block groove-b py-2 last:border-b-0 @xl:table-row @xl:py-0"
                  >
                    <Cell label={t(COLUMNS[0].key)}>
                      <span className="font-medium text-foreground">{line.console}</span>
                    </Cell>
                    <Cell label={t(COLUMNS[1].key)}>
                      {line.project ?? <span className="text-app-muted">{t("configuration.import.result.wholeConsole")}</span>}
                    </Cell>
                    <Cell label={t(COLUMNS[2].key)}>
                      {settled === undefined ? (
                        // A failed line is a glyph and the word like the others; the pair opens the reason in a popover.
                        <StatusMark
                          label={t("configuration.import.result.failedFor", { subject: isolate(line.project ?? line.console) })}
                          text={t("configuration.import.result.failed")}
                        >
                          {line.detail === "" ? undefined : line.detail}
                        </StatusMark>
                      ) : (
                        <span className="inline-flex items-center gap-1.5 whitespace-nowrap">
                          <settled.icon aria-hidden="true" className={`size-4 shrink-0 ${settled.tone}`} />
                          {t(settled.label)}
                        </span>
                      )}
                    </Cell>
                    <Cell label={t(COLUMNS[3].key)} empty={line.status === "failed" || line.detail === ""}>
                      {line.status !== "failed" && line.detail}
                    </Cell>
                  </tr>
                );
              })}
            </tbody>
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
    </>
  );
}
