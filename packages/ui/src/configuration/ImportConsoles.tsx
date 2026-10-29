import { Info, CircleCheck, CloudDownload, type LucideIcon } from "lucide-react";
import React, { useId } from "react";

import { Button } from "../components/Button";
import { MarkLineBox } from "../components/MarkLineBox";
import { StatusMark } from "../components/StatusMark";
import { LabelledGroup } from "../dialogs/LabelledGroup";
import { OptionSelect } from "../dialogs/OptionSelect";
import type { PlainMessageKey } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import type { InspectedConsole, InspectedProject } from "../protocol";
import {
  applyToAll,
  availableFor,
  choicesFor,
  directoryProblem,
  hasRecordedPath,
  targetOf,
  withRow,
  type DirectoryProblem,
  type OfferedChoice,
  type ReviewState,
  type RowState,
} from "./importPlan";
import { CARD_CLASS, CARD_HEADER_CLASS, DirectoryChoiceField, FallbackLines, FallbackMark, Field, type Shown } from "./importParts";

const CHOICE_LABEL: Record<RowState["choice"], PlainMessageKey> = {
  exported_path: "configuration.import.choice.exportedPath",
  default_clone_dir: "configuration.import.choice.defaultCloneDir",
  clone: "configuration.import.choice.clone",
  directory: "configuration.import.choice.directory",
  skip: "configuration.import.choice.skip",
};

/** Where the daemon found the project on its own host, which is what its preselected choice follows. A project
 * found nowhere has no line: its name carries a warning mark instead. */
const STATE: Record<OfferedChoice, { label: PlainMessageKey; icon: LucideIcon; tone: string }> = {
  exported_path: { label: "configuration.import.project.state.exportedPath", icon: CircleCheck, tone: "text-success-soft-foreground" },
  default_clone_dir: { label: "configuration.import.project.state.defaultCloneDir", icon: CircleCheck, tone: "text-success-soft-foreground" },
  clone: { label: "configuration.import.project.state.clone", icon: CloudDownload, tone: "text-accent-soft-foreground" },
};

const APPLY_ALL: {
  choice: OfferedChoice | "skip";
  label:
    | "configuration.import.all.exportedPath"
    | "configuration.import.all.defaultCloneDir"
    | "configuration.import.all.clone"
    | "configuration.import.all.skip";
}[] = [
  { choice: "exported_path", label: "configuration.import.all.exportedPath" },
  { choice: "default_clone_dir", label: "configuration.import.all.defaultCloneDir" },
  { choice: "clone", label: "configuration.import.all.clone" },
  { choice: "skip", label: "configuration.import.all.skip" },
];

function ProjectRow({
  project,
  row,
  problem,
  onChange,
}: {
  project: InspectedProject;
  row: RowState;
  /** What is wrong with the directory typed for this project, once the user has tried to import. */
  problem: DirectoryProblem | undefined;
  onChange: (row: RowState) => void;
}): React.ReactElement {
  const t = useT();
  const target = targetOf(project, row);
  const inPlace = row.choice === "exported_path" && target === project.path;
  const recordedPath = hasRecordedPath(project.path);
  const state = project.default ? STATE[project.default] : undefined;
  // Placed where it was exported, the 'Placed in' line alone says both.
  const fields: [string, string][] = [];
  if (recordedPath && !inPlace) fields.push([t("configuration.import.project.field.exportedPath"), project.path]);
  if (row.choice === "clone") {
    fields.push([t("configuration.import.project.field.cloneFrom"), project.remote_url ?? ""], [t("configuration.import.project.field.cloneInto"), target ?? ""]);
  } else if (row.choice !== "skip" && row.choice !== "directory") {
    fields.push([t("configuration.import.project.field.place"), target ?? ""]);
  }
  return (
    <li className="@container flex flex-col gap-2 groove-b px-4 py-3 last:border-b-0">
      <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-1.5">
        <div className="flex min-w-40 flex-1 flex-col gap-0.5">
          <span className="flex items-start gap-1">
            <span className="min-w-0 font-medium text-foreground [overflow-wrap:anywhere]" dir="auto">
              {project.name}
            </span>
            {!state && (
              // A project not found that also has account fallbacks is still one point: one mark, naming both.
              <MarkLineBox size="text-sm">
                <StatusMark
                  tone="warning"
                  label={t(project.account_fallbacks.length > 0 ? "configuration.import.project.state.noneAndFallbacksMark" : "configuration.import.project.state.noneMark", { project: isolate(project.name) })}
                >
                  <p dir="auto"><Message id="configuration.import.project.state.none" /></p>
                  <p dir="auto">{t("configuration.import.project.state.noneRemedy")}</p>
                  <FallbackLines fallbacks={project.account_fallbacks} />
                </StatusMark>
              </MarkLineBox>
            )}
            {state && <FallbackMark subject={project.name} fallbacks={project.account_fallbacks} size="text-sm" />}
          </span>
          {state && (
            <span className="flex items-center gap-1.5 text-sm text-app-muted">
              <state.icon aria-hidden="true" className={`size-3.5 shrink-0 ${state.tone}`} />
              <Message id={state.label} />
            </span>
          )}
        </div>
        <div className="w-full @md:w-56">
          <OptionSelect
            hideLabel
            label={t("configuration.import.project.choice", { project: isolate(project.name) })}
            options={choicesFor(project).map((choice) => ({ value: choice, label: t(CHOICE_LABEL[choice]) }))}
            value={row.choice}
            onChange={(choice) => onChange({ ...row, choice })}
          />
        </div>
      </div>
      {fields.length > 0 && (
        <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1 text-sm">
          {fields.map(([label, value]) => (
            <Field key={label} label={label} value={value} />
          ))}
        </dl>
      )}
      {row.choice === "directory" && (
        <DirectoryChoiceField
          label={t("configuration.import.directory.label", { project: isolate(project.name) })}
          placeholder={t("configuration.import.directory.placeholder")}
          requiredMessage={t("configuration.import.directory.required")}
          value={row.directory}
          onChange={(directory) => onChange({ ...row, directory })}
          problem={problem}
        />
      )}
    </li>
  );
}

function ConsoleGroup({
  console,
  index,
  review,
  shown,
  onReview,
}: {
  console: InspectedConsole;
  index: number;
  review: ReviewState;
  shown: Shown;
  onReview: (review: ReviewState) => void;
}): React.ReactElement {
  const t = useT();
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className={CARD_CLASS}>
      <div className={`flex flex-col gap-1.5 ${CARD_HEADER_CLASS}`}>
        <div className="flex flex-wrap items-center gap-x-2">
          <div className="flex min-w-0 items-start gap-1">
            <h3 id={headingId} className="min-w-0 text-base font-semibold text-foreground [overflow-wrap:anywhere]" dir="auto">
              {console.name}
            </h3>
            <FallbackMark subject={console.name} fallbacks={console.account_fallbacks} size="text-base" />
          </div>
          <span className="text-sm text-app-muted">{t("configuration.import.console.projects", { count: console.projects.length })}</span>
        </div>
      </div>
      {console.projects.length === 0 ? (
        <p className="px-4 py-3 text-sm text-app-muted">{t("configuration.import.console.noProjects")}</p>
      ) : (
        <ul className="flex flex-col">
          {console.projects.map((project, p) => {
            const row = review[index][p];
            return (
              <ProjectRow
                key={p}
                project={project}
                row={row}
                problem={shown(directoryProblem(row))}
                onChange={(next) => onReview(withRow(review, index, p, next))}
              />
            );
          })}
        </ul>
      )}
    </section>
  );
}

/**
 * The consoles and projects of the document, the panel of the review's Consoles tab: a note that each console is
 * created as a new one (when there is a console), the choice that can be applied to every project it is available for
 * at once (said through `onAnnounce`, a live region the caller keeps mounted), a card for every console with a row for
 * each of its projects (the choices that project's state allows and the directory the chosen one would use), and,
 * while any project is cloned, a note on how long that takes. `consoles` carry the account references that fall back
 * (see `withEffectiveFallbacks`). The state of every choice is the caller's.
 */
export function ImportConsoles({
  consoles,
  review,
  onReview,
  onAnnounce,
  shown,
}: {
  consoles: InspectedConsole[];
  review: ReviewState;
  onReview: (review: ReviewState) => void;
  onAnnounce: (text: string) => void;
  shown: Shown;
}): React.ReactElement {
  const t = useT();
  const cloning = review.some((rows) => rows.some((row) => row.choice === "clone"));
  return (
    <>
      {consoles.length > 0 && <p className="text-sm">{t("configuration.import.consoles.note")}</p>}
      <LabelledGroup label={t("configuration.import.all.label")}>
        <div className="flex flex-wrap gap-2">
          {APPLY_ALL.map(({ choice, label }) => {
            const count = availableFor(consoles, choice);
            return (
              <Button
                key={choice}
                type="button"
                size="sm"
                variant="secondary"
                isDisabled={count === 0}
                onPress={() => {
                  onReview(applyToAll(consoles, review, choice));
                  onAnnounce(t("configuration.import.all.applied", { choice: t(CHOICE_LABEL[choice]), count }));
                }}
              >
                {t(label, { count })}
              </Button>
            );
          })}
        </div>
      </LabelledGroup>
      {consoles.map((console, index) => (
        <ConsoleGroup key={index} console={console} index={index} review={review} shown={shown} onReview={onReview} />
      ))}
      {cloning && (
        <p className="flex items-start gap-2 text-sm">
          <Info aria-hidden="true" className="mt-0.5 size-4 shrink-0 text-accent-soft-foreground" />
          <span className="min-w-0">{t("configuration.import.review.cloneNote")}</span>
        </p>
      )}
    </>
  );
}
