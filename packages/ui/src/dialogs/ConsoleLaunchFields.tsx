import { Description, Tabs } from "@heroui/react";
import React, { useId, useRef, useState } from "react";

import { storedAccount } from "../accountChoices";
import {
  AGENT_ACCOUNT_FIELD,
  AGENT_CONSOLE_EFFORT_FIELD,
  AGENT_CONSOLE_MODEL_FIELD,
  AGENT_CONSOLE_PROJECT_EFFORT_FIELD,
  AGENT_CONSOLE_PROJECT_MODEL_FIELD,
  AGENT_LABEL,
  AGENT_OPTIONS,
  isAgentUnavailable,
} from "../agents";
import { AgentIcon } from "../components/AgentIcon";
import { useTabScrollLabels } from "../components/tabScrollLabels";
import { useFocusHandoff } from "../components/useFocusHandoff";
import { useT } from "../i18n/react";
import type { Agent } from "../protocol";
import { useDaemonStore } from "../store";
import { AdvancedSection, EffortPicker, type LaunchModels, type LaunchPair, ModelPicker, useLaunchModels } from "./LaunchSettingsFields";

type LaunchField =
  | (typeof AGENT_CONSOLE_MODEL_FIELD)[Agent]
  | (typeof AGENT_CONSOLE_EFFORT_FIELD)[Agent]
  | (typeof AGENT_CONSOLE_PROJECT_MODEL_FIELD)[Agent]
  | (typeof AGENT_CONSOLE_PROJECT_EFFORT_FIELD)[Agent];

/**
 * The console dialog's Advanced section: the model and reasoning effort of the console's console
 * sessions and of its project sessions, for one agent at a time, chosen on a row of tabs above them.
 * Each agent's panel is a table, the kinds of session as columns and the settings as rows, which
 * stacks below the `md` breakpoint (see `LaunchMatrix`).
 * Until a tab is picked it follows `defaultAgent`, the console session agent, as the user changes
 * it; a picked tab stays. Every agent's values are kept in `values` whichever tab is showing. An
 * agent known to be missing has no tab, unless the console still stores a value for it, which stays
 * until every one of its values is cleared. `children` are the section's further fields, which hold
 * for every agent: they sit below a frame around the tabs and their panels, so they do not read as
 * part of the agent shown.
 *
 * The list of models is the shown agent's, asked for the account the console is set to for it
 * (`values` holds the accounts too) and only once the section is open.
 */
export function ConsoleLaunchFields({
  defaultAgent,
  values,
  onChange,
  children,
}: {
  defaultAgent: Agent;
  values: Record<LaunchField | (typeof AGENT_ACCOUNT_FIELD)[Agent], string | null>;
  onChange: (field: LaunchField, value: string | null) => void;
  children?: React.ReactNode;
}): React.ReactElement {
  const t = useT();
  const tabScrollLabels = useTabScrollLabels();
  const agentAvailability = useDaemonStore((s) => s.agentAvailability);
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const [expanded, setExpanded] = useState(false);
  const [picked, setPicked] = useState<Agent>();
  const stores = (agent: Agent) =>
    !!(
      values[AGENT_CONSOLE_MODEL_FIELD[agent]] ||
      values[AGENT_CONSOLE_EFFORT_FIELD[agent]] ||
      values[AGENT_CONSOLE_PROJECT_MODEL_FIELD[agent]] ||
      values[AGENT_CONSOLE_PROJECT_EFFORT_FIELD[agent]]
    );
  const listed = AGENT_OPTIONS.map((option) => option.value).filter(
    (agent) => !isAgentUnavailable(agentAvailability, agent) || stores(agent),
  );
  // The agent picked, else the console session agent, else the first listed: a tab can go away
  // under the user, when the last value stored for a missing agent is cleared, and one that comes
  // back does not take the view from the one shown meanwhile.
  if (picked && !listed.includes(picked)) setPicked(undefined);
  const agent = [picked, defaultAgent].find((candidate) => candidate && listed.includes(candidate)) ?? listed[0] ?? defaultAgent;
  const rootRef = useRef<HTMLDivElement>(null);
  const focusHandoff = useFocusHandoff(() => rootRef.current?.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]')?.focus());
  const launch = useLaunchModels(agent, storedAccount(values[AGENT_ACCOUNT_FIELD[agent]], agent, accounts), expanded);

  return (
    <AdvancedSection expanded={expanded} onExpandedChange={setExpanded} status={launch.modelText}>
      <Description>{t("dialog.console.launchAgentNote")}</Description>
      <div ref={rootRef} {...focusHandoff} className="rounded-xl border border-separator p-4">
        <Tabs selectedKey={agent} onSelectionChange={(key) => setPicked(key as Agent)} className="gap-4">
          <Tabs.ListContainer render={(props) => <div {...props}>{tabScrollLabels(props.children)}</div>}>
            <Tabs.List aria-label={t("dialog.console.launchAgent")}>
              {listed.map((option) => (
                <Tabs.Tab key={option} id={option} className="gap-2 whitespace-nowrap hover:text-foreground hover:opacity-100">
                  <AgentIcon agent={option} />
                  {isAgentUnavailable(agentAvailability, option) ? t("agents.notInstalled", { agent: AGENT_LABEL[option] }) : AGENT_LABEL[option]}
                  <Tabs.Indicator />
                </Tabs.Tab>
              ))}
            </Tabs.List>
          </Tabs.ListContainer>
          {listed.map((option) => (
            <Tabs.Panel key={option} id={option} className="mt-0 p-0">
              <LaunchMatrix
                launch={launch}
                consoleSessions={{ model: values[AGENT_CONSOLE_MODEL_FIELD[agent]], effort: values[AGENT_CONSOLE_EFFORT_FIELD[agent]] }}
                projectSessions={{ model: values[AGENT_CONSOLE_PROJECT_MODEL_FIELD[agent]], effort: values[AGENT_CONSOLE_PROJECT_EFFORT_FIELD[agent]] }}
                onConsoleChange={(key, value) => onChange(key === "model" ? AGENT_CONSOLE_MODEL_FIELD[agent] : AGENT_CONSOLE_EFFORT_FIELD[agent], value)}
                onProjectChange={(key, value) => onChange(key === "model" ? AGENT_CONSOLE_PROJECT_MODEL_FIELD[agent] : AGENT_CONSOLE_PROJECT_EFFORT_FIELD[agent], value)}
              />
            </Tabs.Panel>
          ))}
        </Tabs>
      </div>
      {children}
    </AdvancedSection>
  );
}

/**
 * One agent's two pairs of settings as a table from the `md` breakpoint up: a column for each kind
 * of session, a row for each setting, so a row is as tall as the taller of its two pickers. Below
 * it the cells stack under a heading for each kind, and each picker shows the name of its setting
 * above it. The headers are for the eye alone, hidden from assistive technology: each picker is
 * named by the two it sits under, its kind first, and described by the note on its kind, which sits
 * below the table. Both Model pickers are also described by the note on the model list (loading, or
 * why there is none), which they carry.
 */
function LaunchMatrix({
  launch,
  consoleSessions,
  projectSessions,
  onConsoleChange,
  onProjectChange,
}: {
  launch: LaunchModels;
  consoleSessions: LaunchPair;
  projectSessions: LaunchPair;
  onConsoleChange: (key: keyof LaunchPair, value: string | null) => void;
  onProjectChange: (key: keyof LaunchPair, value: string | null) => void;
}): React.ReactElement {
  const t = useT();
  const id = useId();
  const [consoleKindId, projectKindId, modelRowId, effortRowId, consoleNoteId, projectNoteId] = [
    "console-kind",
    "project-kind",
    "model-row",
    "effort-row",
    "console-note",
    "project-note",
  ].map((name) => `${id}-${name}`);
  const model = t("dialog.agentSettings.model");
  const effort = t("dialog.agentSettings.effort");
  // Each cell is placed by its own classes, so the source order (a kind's heading and pickers, then
  // the next kind's) is the stacked order and the order of the tab stops. A row header is as high
  // as a picker and at its top, so it stays with the picker when a note makes the cell taller.
  const cell = "flex min-w-0 flex-col gap-1.5";
  const settingName = "text-sm md:hidden";
  const kindHeading = "text-sm font-medium text-foreground md:font-normal md:text-inherit";
  const rowHeader = "hidden text-sm md:col-start-1 md:flex md:h-9 md:items-center md:self-start";
  return (
    <div className="flex flex-col gap-3">
      <div className="grid grid-cols-[minmax(0,1fr)] gap-x-6 gap-y-3 md:grid-cols-[max-content_repeat(2,minmax(0,1fr))]">
        <div id={modelRowId} aria-hidden="true" className={`${rowHeader} md:row-start-2`}>{model}</div>
        <div id={effortRowId} aria-hidden="true" className={`${rowHeader} md:row-start-3`}>{effort}</div>
        <div id={consoleKindId} aria-hidden="true" className={`${kindHeading} md:col-start-2 md:row-start-1`}>{t("dialog.console.launchConsole")}</div>
        <div className={`${cell} md:col-start-2 md:row-start-2`}>
          <span aria-hidden="true" className={settingName}>{model}</span>
          <ModelPicker launch={launch} model={consoleSessions.model} onChange={(value) => onConsoleChange("model", value)} hideLabel labelledBy={`${consoleKindId} ${modelRowId}`} describedBy={consoleNoteId} />
        </div>
        <div className={`${cell} md:col-start-2 md:row-start-3`}>
          <span aria-hidden="true" className={settingName}>{effort}</span>
          <EffortPicker launch={launch} model={consoleSessions.model} effort={consoleSessions.effort} onChange={(value) => onConsoleChange("effort", value)} hideLabel labelledBy={`${consoleKindId} ${effortRowId}`} describedBy={consoleNoteId} />
        </div>
        <div id={projectKindId} aria-hidden="true" className={`mt-1 md:mt-0 ${kindHeading} md:col-start-3 md:row-start-1`}>{t("dialog.console.launchProject")}</div>
        <div className={`${cell} md:col-start-3 md:row-start-2`}>
          <span aria-hidden="true" className={settingName}>{model}</span>
          <ModelPicker launch={launch} model={projectSessions.model} onChange={(value) => onProjectChange("model", value)} hideLabel labelledBy={`${projectKindId} ${modelRowId}`} describedBy={projectNoteId} />
        </div>
        <div className={`${cell} md:col-start-3 md:row-start-3`}>
          <span aria-hidden="true" className={settingName}>{effort}</span>
          <EffortPicker launch={launch} model={projectSessions.model} effort={projectSessions.effort} onChange={(value) => onProjectChange("effort", value)} hideLabel labelledBy={`${projectKindId} ${effortRowId}`} describedBy={projectNoteId} />
        </div>
      </div>
      <Description id={consoleNoteId}>{t("dialog.console.launchSettingsNote")}</Description>
      <Description id={projectNoteId}>{t("dialog.console.projectLaunchSettingsNote")}</Description>
    </div>
  );
}
