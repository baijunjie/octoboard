import { Checkbox, Description, Label } from "@heroui/react";
import { Info } from "lucide-react";
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
  | "configuration.import.settings.autoSelectAccount.turnsOn"
  | "configuration.import.settings.sidebarOrder.turnsOn"
  | "configuration.import.settings.autoStartQuotaWindows.turnsOn"
  | "configuration.import.settings.autoApproveSession.turnsOn"
  | "configuration.import.settings.autoApproveResume.turnsOn"
  | "configuration.import.settings.autoApproveAccountSwitches.turnsOn"
  | "configuration.import.settings.autoApproveHandoffs.turnsOn"
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
  const setting = <InlineReference dir="auto" wrap>{label}</InlineReference>;
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
      <Info aria-hidden="true" className="mt-0.5 size-4 shrink-0 text-warning-soft-foreground" />
      <span className="min-w-0">
        <Message id="configuration.import.fallback" params={{ agent: <InlineReference dir="auto" wrap>{AGENT_LABEL[fallback.agent]}</InlineReference>, name: <InlineReference dir="auto" wrap>{fallback.name}</InlineReference> }} />
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
  const recordedPath = hasRecordedPath(project.path);
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
            label={t("configuration.import.project.choice", { project: isolate(project.name) })}
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
        {/* An export made without full paths records only the directory's name, which is no path to show. */}
        {recordedPath && (
          <span>
            <Message
              id={
                inPlace
                  ? "configuration.import.project.exportedPathUsed"
                  : "configuration.import.project.exportedPath"
              }
              params={{ path: <InlineReference monospace wrap>{project.path}</InlineReference> }}
            />
          </span>
        )}
        {/* A project found at its exported path and placed there is already said by the path's own line. */}
        {!(inPlace && project.default === "exported_path") && (
          <>
            {recordedPath && " "}
            <span className={recordedPath ? "ms-2" : undefined}>
              <Message id={STATE_LABEL[project.default ?? "none"]} />
            </span>
          </>
        )}
      </p>
      {row.choice === "directory" ? (
        <TextInput
          label={t("configuration.import.directory.label", { project: isolate(project.name) })}
          labelContent={<Message id="configuration.import.directory.label" params={{ project: <InlineReference dir="auto" wrap>{project.name}</InlineReference> }} />}
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
            params={{ url: <InlineReference monospace wrap>{project.remote_url ?? ""}</InlineReference>, path: <InlineReference monospace wrap>{target ?? ""}</InlineReference> }}
          />
        </p>
      ) : inPlace ? null : (
        <p>
          <Message id="configuration.import.target.place" params={{ path: <InlineReference monospace wrap>{target ?? ""}</InlineReference> }} />
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
  const settingsNote = settings && (
    <>
      <SettingNote label={t("settings.git.autoPull.label")} wanted={settings.auto_pull_repositories} current={settings.current_auto_pull_repositories} turnsOn="configuration.import.settings.turnsOn" />
      {typeof settings.auto_delete_unresumable_archives === "boolean" && (
        <SettingNote label={t("settings.archiveCleanup.label")} wanted={settings.auto_delete_unresumable_archives} current={settings.current_auto_delete_unresumable_archives} turnsOn="configuration.import.settings.archiveCleanup.turnsOn" />
      )}
      {typeof settings.auto_select_account === "boolean" && typeof settings.current_auto_select_account === "boolean" && (
        <SettingNote label={t("settings.autoSelect.label")} wanted={settings.auto_select_account} current={settings.current_auto_select_account} turnsOn="configuration.import.settings.autoSelectAccount.turnsOn" />
      )}
      {typeof settings.sidebar_order_by_status === "boolean" && typeof settings.current_sidebar_order_by_status === "boolean" && (
        <SettingNote label={t("settings.sidebarOrder.label")} wanted={settings.sidebar_order_by_status} current={settings.current_sidebar_order_by_status} turnsOn="configuration.import.settings.sidebarOrder.turnsOn" />
      )}
      {typeof settings.auto_start_quota_windows === "boolean" && typeof settings.current_auto_start_quota_windows === "boolean" && (
        <SettingNote label={t("settings.startQuotaWindows.label")} wanted={settings.auto_start_quota_windows} current={settings.current_auto_start_quota_windows} turnsOn="configuration.import.settings.autoStartQuotaWindows.turnsOn" />
      )}
      {typeof settings.auto_approve_session_requests === "boolean" && typeof settings.current_auto_approve_session_requests === "boolean" && (
        <SettingNote label={t("settings.autoApproveSession.label")} wanted={settings.auto_approve_session_requests} current={settings.current_auto_approve_session_requests} turnsOn="configuration.import.settings.autoApproveSession.turnsOn" />
      )}
      {typeof settings.auto_approve_resume_requests === "boolean" && typeof settings.current_auto_approve_resume_requests === "boolean" && (
        <SettingNote label={t("settings.autoApproveResume.label")} wanted={settings.auto_approve_resume_requests} current={settings.current_auto_approve_resume_requests} turnsOn="configuration.import.settings.autoApproveResume.turnsOn" />
      )}
      {typeof settings.auto_approve_account_switches === "boolean" && typeof settings.current_auto_approve_account_switches === "boolean" && (
        <SettingNote label={t("settings.autoApproveAccountSwitches.label")} wanted={settings.auto_approve_account_switches} current={settings.current_auto_approve_account_switches} turnsOn="configuration.import.settings.autoApproveAccountSwitches.turnsOn" />
      )}
      {typeof settings.auto_approve_handoffs === "boolean" && typeof settings.current_auto_approve_handoffs === "boolean" && (
        <SettingNote label={t("settings.autoApproveHandoffs.label")} wanted={settings.auto_approve_handoffs} current={settings.current_auto_approve_handoffs} turnsOn="configuration.import.settings.autoApproveHandoffs.turnsOn" />
      )}
      {settings.worktrees && settings.current_worktrees && (
        <span className="block">
          <Message
            id={sameValue(settings.worktrees, settings.current_worktrees) ? "configuration.import.settings.worktrees.same" : "configuration.import.settings.worktrees.changes"}
            params={{ setting: <InlineReference dir="auto" wrap>{t("settings.section.worktrees")}</InlineReference> }}
          />
        </span>
      )}
      {shortcuts && (
        <span className="block">
          <Message
            id={!shortcuts.changes ? "configuration.import.settings.shortcuts.same" : shortcuts.count === 0 ? "configuration.import.settings.shortcuts.reset" : "configuration.import.settings.shortcuts.changes"}
            params={{ setting: <InlineReference dir="auto" wrap>{t("settings.section.shortcuts")}</InlineReference>, count: shortcuts.count }}
          />
        </span>
      )}
    </>
  );
  const cloning = review.some((rows) => rows.some((row) => row.choice === "clone"));

  return (
    <>
      <StatusAnnouncer text={announcement} />
      <p className="text-sm">
        <Message id="configuration.import.review.intro" params={{ file: <InlineReference monospace wrap>{fileName}</InlineReference>, skip: <InlineReference dir="auto" wrap>{t("configuration.import.choice.skip")}</InlineReference> }} />
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
          <Info aria-hidden="true" className="mt-0.5 size-4 shrink-0 text-accent-soft-foreground" />
          <span className="min-w-0">{t("configuration.import.review.cloneNote")}</span>
        </p>
      )}
    </>
  );
}
