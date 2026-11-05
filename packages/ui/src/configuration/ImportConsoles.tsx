import { CircleCheck, CloudDownload } from "lucide-react";
import React from "react";

import { MarkLineBox } from "../components/status/MarkLineBox";
import { StatusMark } from "../components/status/StatusMark";
import { LabelledGroup } from "../components/form/LabelledGroup";
import { OptionSelect } from "../components/form/OptionSelect";
import type { PlainMessageKey } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import type { InspectedConsole, InspectedProject } from "../protocol";
import { useHostPathsOrEmpty } from "../components/hooks/useHostPaths";
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
import { Card, CARD_HEADER_CLASS, CardRows, CardTitle, DirectoryChoiceField, EmptyLine, FallbackLines, FallbackMark, Field, FieldList, ItemName, ReviewRow, type Shown, STATUS_TONE, type StatusEntry, StatusGlyph, ToneLine } from "./importParts";
import { Button } from "../primitives";

const CHOICE_LABEL: Record<RowState["choice"], PlainMessageKey> = {
  exported_path: "configuration.import.choice.exportedPath",
  default_clone_dir: "configuration.import.choice.defaultCloneDir",
  clone: "configuration.import.choice.clone",
  directory: "configuration.import.choice.directory",
  skip: "configuration.import.choice.skip",
};

/** Where the daemon found the project on its own host, which is what its preselected choice follows. A project
 * found nowhere has no line: its name carries a warning mark instead. */
const STATE: Record<OfferedChoice, StatusEntry> = {
  exported_path: { label: "configuration.import.project.state.exportedPath", icon: CircleCheck, tone: STATUS_TONE.success },
  default_clone_dir: { label: "configuration.import.project.state.defaultCloneDir", icon: CircleCheck, tone: STATUS_TONE.success },
  clone: { label: "configuration.import.project.state.clone", icon: CloudDownload, tone: STATUS_TONE.accent },
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
  // A path written on another operating system says so once: in the not-found mark where the project has no way in,
  // and otherwise in a note after the exported path, with no remedy, since the way it has needs no directory.
  const foreignNote = project.foreign_path && state !== undefined && (
    <StatusMark inline tone="note" label={t("configuration.import.project.state.foreignPathMark", { project: isolate(project.name) })}>
      <p dir="auto">{t("configuration.import.project.state.foreignPath")}</p>
    </StatusMark>
  );
  // Placed where it was exported, the 'Placed in' line alone says both.
  const fields: [string, string, React.ReactNode?][] = [];
  if (recordedPath && !inPlace) fields.push([t("configuration.import.project.field.exportedPath"), project.path, foreignNote]);
  if (row.choice === "clone") {
    fields.push([t("configuration.import.project.field.cloneFrom"), project.remote_url ?? ""], [t("configuration.import.project.field.cloneInto"), target ?? ""]);
  } else if (row.choice !== "skip" && row.choice !== "directory") {
    fields.push([t("configuration.import.project.field.place"), target ?? ""]);
  }
  return (
    <ReviewRow
      lead={
        <>
          <span className="flex items-start gap-1">
            <ItemName>{project.name}</ItemName>
            {!state && (
              // A project not found that also has account fallbacks is still one point: one mark, naming both.
              <MarkLineBox size="text-sm">
                <StatusMark
                  tone="warning"
                  label={t(project.account_fallbacks.length > 0 ? "configuration.import.project.state.noneAndFallbacksMark" : "configuration.import.project.state.noneMark", { project: isolate(project.name) })}
                >
                  <p dir="auto">{project.foreign_path ? t("configuration.import.project.state.foreignPath") : <Message id="configuration.import.project.state.folderNotFound" />}</p>
                  <p dir="auto">{t("configuration.import.project.state.noneRemedy")}</p>
                  <FallbackLines fallbacks={project.account_fallbacks} />
                </StatusMark>
              </MarkLineBox>
            )}
            {state && <FallbackMark subject={project.name} fallbacks={project.account_fallbacks} size="text-sm" />}
          </span>
          {state && (
            <span className="flex items-center gap-1.5 text-sm text-app-muted">
              <StatusGlyph entry={state} size="size-3.5" />
              <Message id={state.label} />
            </span>
          )}
        </>
      }
      control={
        <OptionSelect
          hideLabel
          label={t("configuration.import.project.choice", { project: isolate(project.name) })}
          options={choicesFor(project).map((choice) => ({ value: choice, label: t(CHOICE_LABEL[choice]) }))}
          value={row.choice}
          onChange={(choice) => onChange({ ...row, choice })}
        />
      }
    >
      {fields.length > 0 && (
        <FieldList>
          {fields.map(([label, value, mark]) => (
            <Field key={label} label={label} value={value} mark={mark} />
          ))}
        </FieldList>
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
    </ReviewRow>
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
  const { style } = useHostPathsOrEmpty();
  return (
    <Card
      header={(headingId) => (
        <div className={`flex flex-col gap-1.5 ${CARD_HEADER_CLASS}`}>
          <div className="flex flex-wrap items-center gap-x-2">
            <CardTitle id={headingId} name={console.name} fallbacks={console.account_fallbacks} className="min-w-0" />
            <span className="text-sm text-app-muted">{t("configuration.import.console.projects", { count: console.projects.length })}</span>
          </div>
        </div>
      )}
    >
      {console.projects.length === 0 ? (
        <EmptyLine>{t("configuration.import.console.noProjects")}</EmptyLine>
      ) : (
        <CardRows>
          {console.projects.map((project, p) => {
            const row = review[index][p];
            return (
              <ProjectRow
                key={p}
                project={project}
                row={row}
                problem={shown(directoryProblem(row, style))}
                onChange={(next) => onReview(withRow(review, index, p, next))}
              />
            );
          })}
        </CardRows>
      )}
    </Card>
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
                disabled={count === 0}
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
        <ToneLine>{t("configuration.import.review.cloneNote")}</ToneLine>
      )}
    </>
  );
}
