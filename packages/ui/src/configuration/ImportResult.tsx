import { CircleAlert, CircleCheck, CircleMinus, CircleSlash, CircleX, Info, type LucideIcon } from "lucide-react";
import React, { useId } from "react";

import { AGENT_LABEL } from "../agents";
import { CodeToneContext } from "../components/codeTone";
import type { PlainMessageKey } from "../i18n/catalog";
import { Message, useT } from "../i18n/react";
import type { Event, ImportChoiceKind, ImportedProject } from "../protocol";
import { useFailureNode } from "./failureText";
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

/** What each outcome is called, drawn and coloured. The colour only strengthens the word and the
 * shape, which differ between all four. */
const STATUS: Record<Status, { label: PlainMessageKey; icon: LucideIcon; tone: string }> = {
  created: { label: "configuration.import.result.created", icon: CircleCheck, tone: "text-success-glyph" },
  left_out: { label: "configuration.import.result.leftOut", icon: CircleMinus, tone: "text-app-muted" },
  failed: { label: "configuration.import.result.failed", icon: CircleX, tone: "text-app-danger" },
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
      className={`items-baseline gap-2 py-0.5 ${empty ? "hidden" : "flex"} @xl:table-cell @xl:px-2 @xl:py-2 @xl:align-top`}
    >
      <span aria-hidden="true" className="w-20 shrink-0 text-app-muted [overflow-wrap:anywhere] @xl:hidden">
        {label}
      </span>
      <span className="min-w-0 [overflow-wrap:anywhere]" dir="auto">
        {children}
      </span>
    </td>
  );
}

/**
 * What an import came to, as the daemon reported it: a summary of the counts, then a table with a
 * line for each project (and for a console that failed or came out empty) saying what came of it
 * (created and how it was placed, left out, failed and why, not tried), and every account reference
 * that fell back to a default account under a heading of its own. The summary gives the totals and
 * the table the items, so neither repeats the other.
 */
export function ImportResult({ result }: { result: Imported }): React.ReactElement {
  const t = useT();
  const failureNode = useFailureNode();
  const fallbacksId = useId();
  const outcome = outcomeOf(result.consoles);
  const setting = t("settings.git.autoPull.label");
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
            detail: console.error ? failureNode(console.error) : "",
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
              ? failureNode(project.error)
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
      {result.settings_applied && (
        <p>
          {t(
            result.settings_applied.auto_pull_repositories
              ? "configuration.import.result.settings.on"
              : "configuration.import.result.settings.off",
            { setting },
          )}
        </p>
      )}
      {result.settings_error && (
        <CodeToneContext.Provider value="danger">
          <p className="flex items-start gap-2 text-app-danger">
            <CircleX aria-hidden="true" className="mt-0.5 size-4 shrink-0" />
            <span className="min-w-0">
              <Message id="configuration.import.result.settings.failed" params={{ reason: failureNode(result.settings_error) }} />
            </span>
          </p>
        </CodeToneContext.Provider>
      )}
      {lines.length > 0 && (
        <div className="@container">
          {/* Narrower than `@xl` of its own width, a line becomes a block of labelled values rather than a
              row of cells, and nothing scrolls sideways. The table's container, not the window, is what
              tells: the dialog narrows with the window, and a window under the `docked` breakpoint can
              still hold it at full width. */}
          {/* Explicit roles on every part: stacked, the parts are `display: block`, which some
              browsers take as leaving the table, and the roles keep it one. */}
          <table
            role="table"
            aria-label={t("configuration.import.result.table.label")}
            className="block w-full text-start @xl:table @xl:table-fixed"
          >
            <thead role="rowgroup" className="sr-only @xl:not-sr-only">
              <tr role="row" className="@xl:border-b @xl:border-separator">
                {COLUMNS.map(({ key, width }) => (
                  <th
                    key={key}
                    role="columnheader"
                    scope="col"
                    className={`px-2 pb-2 text-start font-medium text-app-muted ${width}`}
                  >
                    {t(key)}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody role="rowgroup" className="block @xl:table-row-group">
              {lines.map((line, index) => {
                const { label, icon: Icon, tone } = STATUS[line.status];
                return (
                  <tr
                    key={index}
                    role="row"
                    className="block border-b border-separator py-2 last:border-b-0 @xl:table-row @xl:py-0"
                  >
                    <Cell label={t(COLUMNS[0].key)}>
                      <span className="font-medium text-foreground">{line.console}</span>
                    </Cell>
                    <Cell label={t(COLUMNS[1].key)}>
                      {line.project ?? <span className="text-app-muted">{t("configuration.import.result.wholeConsole")}</span>}
                    </Cell>
                    <Cell label={t(COLUMNS[2].key)}>
                      <span className="inline-flex items-center gap-1.5 whitespace-nowrap">
                        <Icon aria-hidden="true" className={`size-4 shrink-0 ${tone}`} />
                        {t(label)}
                      </span>
                    </Cell>
                    <Cell label={t(COLUMNS[3].key)} empty={line.detail === ""}>
                      {line.detail}
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
            <CircleAlert aria-hidden="true" className="size-4 shrink-0 text-warning-glyph" />
            {t("configuration.import.result.fallbacks")}
          </h3>
          <p className="flex items-start gap-2 text-sm">
            <Info aria-hidden="true" className="mt-0.5 size-4 shrink-0 text-warning-glyph" />
            <span className="min-w-0">{t("configuration.import.result.fallbacks.intro")}</span>
          </p>
          <ul className="flex flex-col gap-1">
            {outcome.fallbacks.map((fallback, index) => {
              const agent = AGENT_LABEL[fallback.agent];
              return (
                <li key={index} className="[overflow-wrap:anywhere]" dir="auto">
                  {fallback.project === undefined
                    ? t("configuration.import.result.fallback.console", {
                      agent,
                      name: fallback.name,
                      console: fallback.console,
                    })
                    : t("configuration.import.result.fallback.project", {
                      agent,
                      name: fallback.name,
                      project: fallback.project,
                      console: fallback.console,
                    })}
                </li>
              );
            })}
          </ul>
        </section>
      )}
    </>
  );
}
