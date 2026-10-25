import { Checkbox, Label } from "@heroui/react";
import { CircleAlert, CircleCheck, CloudDownload, Info, type LucideIcon } from "lucide-react";
import React, { useId, useState } from "react";

import { Button } from "../components/Button";
import { AGENT_LABEL } from "../agents";
import { InlineReference } from "../components/InlineReference";
import { TILDE_PARAMS } from "../components/literalParams";
import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { DirectoryPicker } from "../dialogs/DirectoryPicker";
import { LabelledGroup } from "../dialogs/LabelledGroup";
import { OptionSelect } from "../dialogs/OptionSelect";
import { TextInput } from "../dialogs/TextInput";
import type { PlainMessageKey } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import { sameValue } from "../sameValue";
import type { AccountFallback, Event, InspectedConsole, InspectedProject } from "../protocol";
import {
  applyToAll,
  availableFor,
  choicesFor,
  directoryProblem,
  hasRecordedPath,
  isUsableDirectory,
  targetOf,
  withRow,
  type OfferedChoice,
  type ReviewState,
  type RowState,
} from "./importPlan";
import { BOOLEAN_SETTINGS, CARD_CLASS, SettingLine } from "./importParts";

export type Inspection = Extract<Event, { type: "configuration_inspected" }>;

const CHOICE_LABEL: Record<RowState["choice"], PlainMessageKey> = {
  exported_path: "configuration.import.choice.exportedPath",
  default_clone_dir: "configuration.import.choice.defaultCloneDir",
  clone: "configuration.import.choice.clone",
  directory: "configuration.import.choice.directory",
  skip: "configuration.import.choice.skip",
};

/** Where the daemon found the project on its own host, which is what its preselected choice follows. */
const STATE: Record<OfferedChoice | "none", { label: PlainMessageKey; icon: LucideIcon; tone: string }> = {
  exported_path: { label: "configuration.import.project.state.exportedPath", icon: CircleCheck, tone: "text-success-soft-foreground" },
  default_clone_dir: { label: "configuration.import.project.state.defaultCloneDir", icon: CircleCheck, tone: "text-success-soft-foreground" },
  clone: { label: "configuration.import.project.state.clone", icon: CloudDownload, tone: "text-accent-soft-foreground" },
  none: { label: "configuration.import.project.state.none", icon: CircleAlert, tone: "text-warning-soft-foreground" },
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

/** A boolean application setting: whether applying turns it on, off, or leaves it as it is. */
function BooleanSettingLine({ label, wanted, current, consequence }: { label: string; wanted: boolean; current: boolean; consequence: PlainMessageKey }): React.ReactElement {
  const t = useT();
  if (wanted === current) return <SettingLine label={label} badge={t("configuration.import.settings.badge.same")} />;
  return wanted ? (
    <SettingLine label={label} badge={t("configuration.import.settings.badge.turnsOn")} tone="accent">
      <Message id={consequence} />
    </SettingLine>
  ) : (
    <SettingLine label={label} badge={t("configuration.import.settings.badge.turnsOff")} />
  );
}

/** An account reference that names no account of this machine, and what happens to it. */
function FallbackNote({ fallback }: { fallback: AccountFallback }): React.ReactElement {
  return (
    <p className="flex items-start gap-2 text-sm">
      <Info aria-hidden="true" className="mt-0.5 size-4 shrink-0 text-warning-soft-foreground" />
      <span className="min-w-0">
        <Message id="configuration.import.fallback" params={{ agent: <InlineReference dir="auto" wrap>{AGENT_LABEL[fallback.agent]}</InlineReference>, name: <InlineReference dir="auto" wrap>{fallback.name}</InlineReference> }} />
      </span>
    </p>
  );
}

/** A labelled path or URL on a line of its own, shown as plain monospace text (see the "References in
 * copy use `InlineReference`" section of docs/ui-accessibility.md). */
function Field({ label, value }: { label: string; value: string }): React.ReactElement {
  return (
    <>
      <dt className="text-app-muted">{label}</dt>
      <dd className="min-w-0 font-mono [overflow-wrap:anywhere]" dir="ltr">{value}</dd>
    </>
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
  const recordedPath = hasRecordedPath(project.path);
  const state = STATE[project.default ?? "none"];
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
          <span className="font-medium text-foreground [overflow-wrap:anywhere]" dir="auto">
            {project.name}
          </span>
          <span className="flex items-center gap-1.5 text-sm text-app-muted">
            <state.icon aria-hidden="true" className={`size-3.5 shrink-0 ${state.tone}`} />
            <Message id={state.label} />
          </span>
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
        <TextInput
          label={t("configuration.import.directory.label", { project: isolate(project.name) })}
          labelContent={t("configuration.import.directory.visibleLabel")}
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
    <section aria-labelledby={headingId} className={CARD_CLASS}>
      <div className="flex flex-col gap-1.5 groove-b px-4 py-3">
        <div className="flex flex-wrap items-baseline gap-x-2">
          <h3 id={headingId} className="text-base font-semibold text-foreground [overflow-wrap:anywhere]" dir="auto">
            {console.name}
          </h3>
          <span className="text-sm text-app-muted">{t("configuration.import.console.projects", { count: console.projects.length })}</span>
        </div>
        {console.account_fallbacks.map((fallback) => (
          <FallbackNote key={`${fallback.agent}:${fallback.name}`} fallback={fallback} />
        ))}
      </div>
      {console.projects.length === 0 ? (
        <p className="px-4 py-3 text-sm text-app-muted">{t("configuration.import.console.noProjects")}</p>
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
 * The step between reading a document and applying it: a card for every console, with a row for each
 * of its projects (the choices that project's state allows and the directory the chosen one would use)
 * and the account references that would fall back to a default account, then, when the document
 * carries application settings, a card saying what applying them would change and a checkbox to
 * apply them. The state of every choice is the caller's (`review`), so applying sends what is on
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
  shortcuts,
  shown,
}: {
  fileName: string;
  inspection: Inspection;
  review: ReviewState;
  onReview: (review: ReviewState) => void;
  applySettings: boolean;
  onApplySettings: (apply: boolean) => void;
  /** What the document's keyboard shortcuts would do on this device; `undefined` when it carries none. */
  shortcuts?: { count: number; changes: boolean };
  /** Holds a field's message back until the user has tried to import. */
  shown: (message: string | undefined) => string | undefined;
}): React.ReactElement {
  const t = useT();
  const [announcement, setAnnouncement] = useState<string>();
  const settings = inspection.settings;
  // The lines say what ticking the box would do, so they describe it.
  const settingsId = useId();
  const settingLines = settings && (
    <ul id={settingsId} className="flex flex-col groove-t px-4 py-1 [&>li]:groove-b [&>li:last-child]:border-b-0">
      {BOOLEAN_SETTINGS.map(({ label, field, consequence }) => {
        const wanted = settings[field];
        const now = settings[`current_${field}` as const];
        return typeof wanted === "boolean" && typeof now === "boolean" && <BooleanSettingLine key={field} label={t(label)} wanted={wanted} current={now} consequence={consequence} />;
      })}
      {settings.worktrees && settings.current_worktrees && (
        sameValue(settings.worktrees, settings.current_worktrees)
          ? <SettingLine label={t("settings.section.worktrees")} badge={t("configuration.import.settings.badge.same")} />
          : <SettingLine label={t("settings.section.worktrees")} badge={t("configuration.import.settings.badge.changes")} tone="accent" />
      )}
      {shortcuts && (
        !shortcuts.changes
          ? <SettingLine label={t("settings.section.shortcuts")} badge={t("configuration.import.settings.badge.same")} />
          : shortcuts.count === 0
            ? <SettingLine label={t("settings.section.shortcuts")} badge={t("configuration.import.settings.badge.reset")} tone="accent">{t("configuration.import.settings.consequence.shortcutsReset")}</SettingLine>
            : <SettingLine label={t("settings.section.shortcuts")} badge={t("configuration.import.settings.badge.changes")} tone="accent">{t("configuration.import.settings.consequence.shortcuts", { count: shortcuts.count })}</SettingLine>
      )}
    </ul>
  );
  const cloning = review.some((rows) => rows.some((row) => row.choice === "clone"));

  return (
    <>
      <StatusAnnouncer text={announcement} />
      <p className="text-sm">
        <Message id="configuration.import.review.intro" params={{ file: <InlineReference monospace wrap>{fileName}</InlineReference> }} />
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
      {inspection.consoles.map((console, index) => (
        <ConsoleGroup key={index} console={console} index={index} review={review} shown={shown} onReview={onReview} />
      ))}
      {settings && (
        <div className={CARD_CLASS}>
          <div className="px-4 py-3">
            <Checkbox variant="secondary" isSelected={applySettings} onChange={onApplySettings} aria-describedby={settingsId}>
              <Checkbox.Content className="min-h-6">
                <Checkbox.Control>
                  <Checkbox.Indicator />
                </Checkbox.Control>
                <Label>{t("configuration.import.settings.label")}</Label>
              </Checkbox.Content>
            </Checkbox>
          </div>
          {settingLines}
        </div>
      )}
      {cloning && (
        <p className="flex items-start gap-2 text-sm">
          <Info aria-hidden="true" className="mt-0.5 size-4 shrink-0 text-accent-soft-foreground" />
          <span className="min-w-0">{t("configuration.import.review.cloneNote")}</span>
        </p>
      )}
    </>
  );
}
