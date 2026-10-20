import { Checkbox, Description, Label } from "@heroui/react";
import { Info } from "lucide-react";
import React, { useId, useState } from "react";

import { Button } from "../components/Button";
import { AGENT_LABEL } from "../agents";
import { InlineChip } from "../components/InlineChip";
import { TILDE_PARAMS } from "../components/literalParams";
import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { DirectoryPicker } from "../dialogs/DirectoryPicker";
import { LabelledGroup } from "../dialogs/LabelledGroup";
import { OptionSelect } from "../dialogs/OptionSelect";
import { TextInput } from "../dialogs/TextInput";
import type { PlainMessageKey } from "../i18n/catalog";
import { Message, useT } from "../i18n/react";
import type { AccountFallback, Event, InspectedConsole, InspectedProject } from "../protocol";
import {
  applyToAll,
  availableFor,
  choicesFor,
  directoryProblem,
  isUsableDirectory,
  targetOf,
  withRow,
  type OfferedChoice,
  type ReviewState,
  type RowState,
} from "./importPlan";

export type Inspection = Extract<Event, { type: "configuration_inspected" }>;

const CHOICE_LABEL: Record<RowState["choice"], PlainMessageKey> = {
  exported_path: "configuration.import.choice.exportedPath",
  default_clone_dir: "configuration.import.choice.defaultCloneDir",
  clone: "configuration.import.choice.clone",
  directory: "configuration.import.choice.directory",
  skip: "configuration.import.choice.skip",
};

/** Where the daemon found the project on its own host, which is what its preselected choice follows. */
const STATE_LABEL: Record<OfferedChoice | "none", PlainMessageKey> = {
  exported_path: "configuration.import.project.state.exportedPath",
  default_clone_dir: "configuration.import.project.state.defaultCloneDir",
  clone: "configuration.import.project.state.clone",
  none: "configuration.import.project.state.none",
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

type SettingMessage =
  | "configuration.import.settings.turnsOn"
  | "configuration.import.settings.archiveCleanup.turnsOn"
  | "configuration.import.settings.turnsOff"
  | "configuration.import.settings.alreadyOn"
  | "configuration.import.settings.alreadyOff";

/** What applying the document's value of one application setting does on this machine. Only
 * `turnsOn` differs between settings, as it says what the setting then makes {appName} do. */
function SettingNote({
  label,
  wanted,
  current,
  turnsOn,
}: {
  label: string;
  wanted: boolean;
  current: boolean;
  turnsOn: SettingMessage;
}): React.ReactElement {
  const setting = <InlineChip dir="auto" wrap size="inherit">{label}</InlineChip>;
  const id: SettingMessage =
    wanted === current
      ? wanted ? "configuration.import.settings.alreadyOn" : "configuration.import.settings.alreadyOff"
      : wanted ? turnsOn : "configuration.import.settings.turnsOff";
  return <span className="block"><Message id={id} params={{ setting }} /></span>;
}

/** An account reference that names no account of this machine, and what happens to it. */
function FallbackNote({ fallback }: { fallback: AccountFallback }): React.ReactElement {
  return (
    <p className="flex items-start gap-2 text-sm">
      <Info aria-hidden="true" className="mt-0.5 size-4 shrink-0 text-warning-glyph" />
      <span className="min-w-0">
        <Message id="configuration.import.fallback" params={{ agent: AGENT_LABEL[fallback.agent], name: <InlineChip dir="auto" wrap size="inherit">{fallback.name}</InlineChip> }} />
      </span>
    </p>
  );
}

function ProjectRow({
  project,
  row,
  problem,
  onChange,
}: {
  project: InspectedProject;
  row: RowState;
  /** What is wrong with the directory typed for this project, once the user has tried to import. */
  problem: React.ReactNode;
  onChange: (row: RowState) => void;
}): React.ReactElement {
  const t = useT();
  const [picking, setPicking] = useState(false);
  const target = targetOf(project, row);
  const inPlace = row.choice === "exported_path" && target === project.path;
  return (
    <li className="@container flex flex-col gap-1.5 groove-b py-2.5 last:border-b-0">
      {/* The name and the choice made for it are one decision, so they share a line; the choice is
          only as wide as its labels need and drops under the name where the dialog is narrow. */}
      <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-1.5">
        <span className="min-w-40 flex-1 font-medium text-foreground [overflow-wrap:anywhere]" dir="auto">
          {project.name}
        </span>
        <div className="w-full @md:w-56">
          <OptionSelect
            hideLabel
            label={t("configuration.import.project.choice", { project: project.name })}
            options={choicesFor(project).map((choice) => ({ value: choice, label: t(CHOICE_LABEL[choice]) }))}
            value={row.choice}
            onChange={(choice) => onChange({ ...row, choice })}
          />
        </div>
      </div>
      {/* Where the project is placed is the exported path itself for the commonest choice, and one
          line then says both. */}
      {/* The state is a sentence of its own beside the path, so the row stays two lines. */}
      <p>
        <span>
          <Message
            id={
              inPlace
                ? "configuration.import.project.exportedPathUsed"
                : "configuration.import.project.exportedPath"
            }
            params={{ path: <InlineChip monospace wrap size="inherit">{project.path}</InlineChip> }}
          />
        </span>
        {/* A project found at its exported path and placed there is already said by the path's own line. */}
        {!(inPlace && project.default === "exported_path") && (
          <>
            {" "}<span className="ms-2">{t(STATE_LABEL[project.default ?? "none"])}</span>
          </>
        )}
      </p>
      {row.choice === "directory" ? (
        <TextInput
          label={t("configuration.import.directory.label", { project: project.name })}
          value={row.directory}
          onChange={(directory) => onChange({ ...row, directory })}
          dir="ltr"
          placeholder={t("configuration.import.directory.placeholder")}
          errorMessage={problem}
          trailing={
            <Button type="button" variant="secondary" onPress={() => setPicking(true)}>
              {t("common.browse")}
            </Button>
          }
        />
      ) : row.choice === "skip" ? null : row.choice === "clone" ? (
        <p>
          <Message
            id="configuration.import.target.clone"
            params={{ url: <InlineChip monospace wrap size="inherit">{project.remote_url ?? ""}</InlineChip>, path: <InlineChip monospace wrap size="inherit">{target ?? ""}</InlineChip> }}
          />
        </p>
      ) : inPlace ? null : (
        <p>
          <Message id="configuration.import.target.place" params={{ path: <InlineChip monospace wrap size="inherit">{target ?? ""}</InlineChip> }} />
        </p>
      )}
      {project.account_fallbacks.map((fallback) => (
        <FallbackNote key={`${fallback.agent}:${fallback.name}`} fallback={fallback} />
      ))}
      {picking && (
        <DirectoryPicker
          title={t("dialog.chooseDirectory")}
          initialPath={isUsableDirectory(row.directory) ? row.directory : "~"}
          onPick={(directory) => {
            onChange({ ...row, directory });
            setPicking(false);
          }}
          onClose={() => setPicking(false)}
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
  shown: (message: string | undefined) => string | undefined;
  onReview: (review: ReviewState) => void;
}): React.ReactElement {
  const t = useT();
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className="flex flex-col gap-2">
      <div className="flex flex-col gap-0.5">
        <h3 id={headingId} className="text-base font-semibold text-foreground [overflow-wrap:anywhere]" dir="auto">
          {console.name}
        </h3>
        <p>{t("configuration.import.console.projects", { count: console.projects.length })}</p>
      </div>
      {console.account_fallbacks.map((fallback) => (
        <FallbackNote key={`${fallback.agent}:${fallback.name}`} fallback={fallback} />
      ))}
      {console.projects.length === 0 ? (
        <p>{t("configuration.import.console.noProjects")}</p>
      ) : (
        <ul className="flex flex-col">
          {console.projects.map((project, p) => {
            const row = review[index][p];
            const problem = shown(directoryProblem(row));
            return (
              <ProjectRow
                key={p}
                project={project}
                row={row}
                problem={
                  problem === "empty"
                    ? t("configuration.import.directory.required")
                    : problem === "not_absolute"
                      ? <Message id="configuration.import.directory.notAbsolute" params={TILDE_PARAMS} />
                      : undefined
                }
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
 * The step between reading a document and applying it: one row for every project the daemon's
 * inspection found, with the choices that project's state allows, the directory the chosen one
 * would use, and — for every console — the account references that would fall back to a default
 * account. The state of every choice is the caller's (`review`), so applying sends what is on
 * screen. A choice can be applied to every project it is available for at once, which is how a
 * whole layout under another root is placed without a row at a time.
 */
export function ImportReview({
  fileName,
  inspection,
  review,
  onReview,
  applySettings,
  onApplySettings,
  shown,
}: {
  fileName: string;
  inspection: Inspection;
  review: ReviewState;
  onReview: (review: ReviewState) => void;
  applySettings: boolean;
  onApplySettings: (apply: boolean) => void;
  /** Holds a field's message back until the user has tried to import. */
  shown: (message: string | undefined) => string | undefined;
}): React.ReactElement {
  const t = useT();
  const [announcement, setAnnouncement] = useState<string>();
  const settings = inspection.settings;
  const settingsNote = settings && (
    <>
      <SettingNote label={t("settings.git.autoPull.label")} wanted={settings.auto_pull_repositories} current={settings.current_auto_pull_repositories} turnsOn="configuration.import.settings.turnsOn" />
      {typeof settings.auto_delete_unresumable_archives === "boolean" && (
        <SettingNote label={t("settings.archiveCleanup.label")} wanted={settings.auto_delete_unresumable_archives} current={settings.current_auto_delete_unresumable_archives} turnsOn="configuration.import.settings.archiveCleanup.turnsOn" />
      )}
    </>
  );
  const cloning = review.some((rows) => rows.some((row) => row.choice === "clone"));

  return (
    <>
      <StatusAnnouncer text={announcement} />
      <p className="text-sm">
        <Message id="configuration.import.review.intro" params={{ file: <InlineChip monospace wrap size="inherit">{fileName}</InlineChip>, skip: <InlineChip dir="auto" wrap size="inherit">{t("configuration.import.choice.skip")}</InlineChip> }} />
      </p>
      <LabelledGroup label={t("configuration.import.all.label")}>
        <div className="flex flex-wrap gap-2">
          {APPLY_ALL.map(({ choice, label }) => {
            const count = availableFor(inspection.consoles, choice);
            return (
              <Button
                key={choice}
                type="button"
                size="sm"
                variant="secondary"
                isDisabled={count === 0}
                onPress={() => {
                  onReview(applyToAll(inspection.consoles, review, choice));
                  setAnnouncement(t("configuration.import.all.applied", { choice: t(CHOICE_LABEL[choice]), count }));
                }}
              >
                {t(label, { count })}
              </Button>
            );
          })}
        </div>
      </LabelledGroup>
      {settings && (
        <Checkbox variant="secondary" isSelected={applySettings} onChange={onApplySettings}>
          <Checkbox.Content className="min-h-6">
            <Checkbox.Control>
              <Checkbox.Indicator />
            </Checkbox.Control>
            <Label>{t("configuration.import.settings.label")}</Label>
          </Checkbox.Content>
          <Description>{settingsNote}</Description>
        </Checkbox>
      )}
      {inspection.consoles.map((console, index) => (
        <ConsoleGroup key={index} console={console} index={index} review={review} shown={shown} onReview={onReview} />
      ))}
      {cloning && (
        <p className="flex items-start gap-2 text-sm">
          <Info aria-hidden="true" className="mt-0.5 size-4 shrink-0 text-accent-glyph" />
          <span className="min-w-0">{t("configuration.import.review.cloneNote")}</span>
        </p>
      )}
    </>
  );
}
