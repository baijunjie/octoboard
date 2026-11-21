import React, { useLayoutEffect, useRef } from "react";
import { createPortal } from "react-dom";

import { FormCard } from "../components/card/FormCard";
import { useFocusLeavingWhenDisabled } from "../components/dialog/dialogHooks";
import { DateTimeInput } from "../components/form/DateTimeInput";
import { LabelledGroup } from "../components/form/LabelledGroup";
import { type Option, OptionSelect } from "../components/form/OptionSelect";
import { TextAreaInput } from "../components/form/TextAreaInput";
import { TextInput } from "../components/form/TextInput";
import { TimeOfDayInput } from "../components/form/TimeOfDayInput";
import { useFocusFirstInvalid } from "../components/form/useValidationMarks";
import { BindingBadge } from "../components/status/BindingBadge";
import { AgentAccountField } from "../dialogs/AgentAccountField";
import { LaunchSettingsFields } from "../dialogs/LaunchSettingsFields";
import { useCurrentLanguage, useT } from "../i18n/react";
import type { Console, Project } from "../protocol";
import { Button, Checkbox, NumberField, Switch } from "../primitives";
import { liveConsoleSessions } from "../sessions/order";
import { sessionChoiceTitle, sessionDisplayTitle } from "../sessions/sessionLabel";
import { SettingRow } from "../settings/SettingRow";
import { useConsoleSessions, useDaemonStore } from "../store";
import { SCHEDULE_KINDS, type ScheduleKind } from "../values/scheduleKind";
import { SCHEDULE_KIND_LABELS, weekdayName, WEEKDAYS } from "./scheduleText";
import { NO_OWNER, NO_PROJECT } from "./taskFields";
import type { useTaskDraft } from "./useTaskDraft";

type TaskDraft = ReturnType<typeof useTaskDraft>;

/** A row of fields in equal columns that stacks where the card is narrow, its fields at the top whatever their
 * descriptions and errors. */
const ROW = "grid grid-cols-1 gap-5 @md:grid-cols-2 @md:items-start";

/**
 * The form that creates a scheduled task or edits one, in the content area of the Scheduled tasks page: the view over
 * a draft (`useTaskDraft`), which the page holds above its tabs so that unsaved edits survive a look at Run history.
 * A saved task whose agent or account can no longer be picked shows both pickers empty with the reason, as the session
 * dialog's handoff does, and is saved as it is until one is picked.
 * Save is available while the form would save something other than the task holds (always, for a new task); Revert
 * drops every change (Cancel, for a new task, leaves it: `onCancel`); neither asks, each being the user's own discard, and
 * both wait while a save is in flight. Save and Revert, which disable themselves as
 * the draft comes to equal the task, hand focus to the Name field then. A Save pressed with something wrong moves focus
 * to the first invalid field.
 */
export function ScheduledTaskForm({ draft, consoles, projects, actionBar, onCancel, autoFocus = false }: {
  draft: TaskDraft;
  consoles: Console[];
  projects: Project[];
  /** The element Save and Revert (Cancel) are drawn in, pinned by the page below the form's scroll; none draws neither. */
  actionBar: HTMLElement | null;
  /** Given for a new task, whose form has no saved state to revert to. */
  onCancel?: () => void;
  /** Puts focus in the form's Name field, its first text field, for a new task's. */
  autoFocus?: boolean;
}): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const { fields, change, leave, shownError, target, projectTask, zoneDescription, launchPickers, dirty, busy, submit, revert } = draft;
  const formRef = useRef<HTMLFormElement>(null);
  const saveRef = useRef<HTMLButtonElement>(null);
  const revertRef = useRef<HTMLButtonElement>(null);
  useFocusFirstInvalid(formRef, draft.invalidAttempt);
  // Save and Revert disable themselves once the draft equals the task, which is where focus is when they were pressed
  // from the keyboard: it goes to the Name field, the form's first.
  const nameRef = useRef<HTMLElement | null>(null);
  // After every render, since the field is not known until its content is in the page.
  useLayoutEffect(() => {
    nameRef.current = formRef.current?.querySelector<HTMLElement>("[data-task-name] input") ?? null;
  });
  useFocusLeavingWhenDisabled(saveRef, !dirty, nameRef, () => nameRef.current);
  useFocusLeavingWhenDisabled(revertRef, !dirty || busy, nameRef, () => nameRef.current);

  const consoleOptions = consoles.map((one) => ({ value: one.id, label: one.name }));
  const projectOptions = [
    { value: NO_PROJECT, label: t("scheduledTasks.form.project.none") },
    ...projects.filter((project) => project.console_id === target.console.id).map((project) => ({ value: project.id, label: project.name })),
  ];
  const kindOptions: Option<ScheduleKind | "">[] = SCHEDULE_KINDS.map((kind) => ({ value: kind, label: t(SCHEDULE_KIND_LABELS[kind]) }));

  return (
    <form
      ref={formRef}
      className="flex flex-col gap-6"
      onSubmit={(event) => {
        event.preventDefault();
        submit();
      }}
    >
      <FormCard title={t("scheduledTasks.form.section.session")}>
        <div className={ROW}>
          <div className="min-w-0">
            <OptionSelect
              label={t("scheduledTasks.form.console")}
              options={consoleOptions}
              value={target.console.id}
              onChange={(value) => change({ consoleId: value, projectId: NO_PROJECT, boundTo: NO_OWNER })}
            />
          </div>
          <div className="min-w-0">
            <OptionSelect
              label={t("scheduledTasks.form.project")}
              options={projectOptions}
              value={target.project?.id ?? NO_PROJECT}
              onChange={(value) => change({ projectId: value })}
            />
          </div>
        </div>
        <div data-task-name="" className="min-w-0">
          <TextInput
            label={t("common.name")}
            value={fields.name}
            onChange={(name) => change({ name })}
            placeholder={t("session.defaultName", { name: target.project?.name ?? target.console.name })}
            description={t("scheduledTasks.form.name.description")}
            autoFocus={autoFocus}
          />
        </div>
        <AgentAccountField
          label={t("dialog.session.agent")}
          options={launchPickers.agentOptions}
          value={launchPickers.agent}
          onChange={launchPickers.pickAgent}
          account={launchPickers.account}
          placeholders={launchPickers.gone ? { agent: t("scheduledTasks.form.agent.placeholder"), account: t("scheduledTasks.form.account.placeholder") } : undefined}
          errorMessage={launchPickers.gone ? t("scheduledTasks.form.agent.gone") : draft.accountError}
        />
        {projectTask && <ReportToField consoleId={target.console.id} value={fields.boundTo} onChange={(boundTo) => change({ boundTo })} />}
        <TextAreaInput
          label={t("scheduledTasks.form.prompt")}
          value={fields.prompt}
          onChange={(value) => change({ prompt: value })}
          onBlur={() => leave("prompt")}
          placeholder={t("common.example", { example: t("scheduledTasks.form.prompt.example") })}
          description={t("scheduledTasks.form.prompt.description")}
          errorMessage={shownError("prompt")}
          rows={5}
        />
        <LaunchSettingsFields {...launchPickers.settings} isDisabled={launchPickers.gone} note={t(projectTask ? "dialog.session.launchInheritanceNote" : "dialog.consoleSession.launchInheritanceNote")} />
      </FormCard>
      <FormCard title={t("scheduledTasks.form.section.schedule")}>
        {/* The kind and its time in one row where the card is wide enough, and stacked where it is not, as the remote
            device form's address is. Every kind keeps the same two tracks: what a weekly or monthly schedule adds goes
            in a row of its own below. A kind this client does not know shows no value and no further field. */}
        <div className={ROW}>
          <div className="min-w-0">
            <OptionSelect<ScheduleKind | "">
              label={t("scheduledTasks.form.schedule")}
              options={kindOptions}
              value={fields.kind ?? ""}
              onChange={(kind) => kind !== "" && change({ kind })}
            />
          </div>
          {fields.kind === "once" && (
            <div className="min-w-0">
              <DateTimeInput
                label={t("scheduledTasks.form.runAt")}
                value={fields.runAt}
                onChange={(runAt) => change({ runAt })}
                onBlur={() => leave("runAt")}
                description={zoneDescription}
                errorMessage={shownError("runAt")}
              />
            </div>
          )}
          {fields.kind === "hourly" && (
            <div className="min-w-0">
              <NumberField
                label={t("scheduledTasks.form.minute")}
                visibleLabel
                value={fields.minute}
                onChange={(minute) => change({ minute })}
                min={0}
                max={59}
                incrementLabel={t("scheduledTasks.form.minute.increment")}
                decrementLabel={t("scheduledTasks.form.minute.decrement")}
                variant="secondary"
                className="w-full"
              />
            </div>
          )}
          {(fields.kind === "daily" || fields.kind === "weekdays" || fields.kind === "weekly" || fields.kind === "monthly") && (
            <div className="min-w-0">
              <TimeOfDayInput
                label={t("scheduledTasks.form.time")}
                value={fields.time}
                onChange={(time) => change({ time })}
                onBlur={() => leave("time")}
                description={zoneDescription}
                errorMessage={shownError("time")}
              />
            </div>
          )}
        </div>
        {fields.kind === "monthly" && (
          <div className={ROW}>
            <div className="min-w-0">
              <NumberField
                label={t("scheduledTasks.form.day")}
                visibleLabel
                description={fields.day >= 29 ? t("scheduledTasks.form.day.description") : undefined}
                value={fields.day}
                onChange={(day) => change({ day })}
                min={1}
                max={31}
                incrementLabel={t("scheduledTasks.form.day.increment")}
                decrementLabel={t("scheduledTasks.form.day.decrement")}
                variant="secondary"
                className="w-full"
              />
            </div>
          </div>
        )}
        {fields.kind === "weekly" && (
          <LabelledGroup label={t("scheduledTasks.form.days")} errorMessage={shownError("days")}>
            <div className="flex flex-wrap gap-x-4 gap-y-2">
              {WEEKDAYS.map((day) => (
                <Checkbox
                  key={day}
                  variant="secondary"
                  selected={fields.days.includes(day)}
                  onSelectedChange={(selected) => {
                    change({ days: WEEKDAYS.filter((one) => (one === day ? selected : fields.days.includes(one))) });
                    draft.leaveDays();
                  }}
                >
                  {weekdayName(language, day, "long")}
                </Checkbox>
              ))}
            </div>
          </LabelledGroup>
        )}
        {/* The row's own padding is the card's. */}
        <div className="-my-4">
          <SettingRow label={t("scheduledTasks.form.enabled")} description={t(fields.enabled ? "scheduledTasks.form.enabled.descriptionOn" : "scheduledTasks.form.enabled.descriptionOff")} compact>
            <Switch label={t("scheduledTasks.form.enabled")} selected={fields.enabled} onSelectedChange={(enabled) => change({ enabled })} />
          </SettingRow>
        </div>
      </FormCard>
      {actionBar && createPortal(
        <div className="flex items-center gap-2">
          <Button ref={saveRef} type="button" size="sm" variant="primary" focusOnPress={false} pending={busy} disabled={!dirty} onPress={submit}>
            {t("common.save")}
          </Button>
          {onCancel ? (
            <Button type="button" size="sm" variant="secondary" focusOnPress={false} disabled={busy} onPress={onCancel}>{t("common.cancel")}</Button>
          ) : (
            <Button ref={revertRef} type="button" size="sm" variant="secondary" focusOnPress={false} disabled={!dirty || busy} onPress={revert}>{t("scheduledTasks.form.revert")}</Button>
          )}
        </div>,
        actionBar,
      )}
    </form>
  );
}

/** Report to of a project task: None, the default, then the console's console sessions that are not archived, each
 * beside its colour, as the session dialog offers them. A saved choice that is no longer among them (an archived
 * or still starting console session, which the daemon takes as well) stays listed while it is the value, so an untouched
 * form does not change it; an archived one carries the Archived badge, since a run cannot report to it and starts without. */
function ReportToField({ consoleId, value, onChange }: { consoleId: string; value: string; onChange: (value: string) => void }): React.ReactElement {
  const t = useT();
  const sessions = useConsoleSessions(consoleId);
  const owners = liveConsoleSessions(sessions, consoleId);
  const held = useDaemonStore((s) => (value !== NO_OWNER && !owners.some((one) => one.id === value) ? s.sessions.get(value) : undefined));
  const listed = held ? [...owners, held] : owners;
  return (
    <OptionSelect
      label={t("scheduledTasks.form.reportTo")}
      value={listed.some((one) => one.id === value) ? value : NO_OWNER}
      onChange={onChange}
      description={t("scheduledTasks.form.reportTo.description")}
      options={[
        { value: NO_OWNER, label: t("dialog.session.ownerNone") },
        ...listed.map((session) => ({ value: session.id, label: sessionChoiceTitle(t, session), labelContent: sessionDisplayTitle(t, session), badge: session.status === "archived" ? t("session.status.archived") : undefined, icon: <BindingBadge owner={session} decorative /> })),
      ]}
    />
  );
}
