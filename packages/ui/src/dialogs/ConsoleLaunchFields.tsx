import { Description, Tabs } from "@heroui/react";
import React, { useId, useRef, useState } from "react";

import { storedAccount } from "../accountChoices";
import {
  AGENT_LABEL,
  agentChoiceBadge,
  listedPickerAgents,
} from "../agents";
import { AgentIcon } from "../components/AgentIcon";
import { HandOffOnUnmount } from "../components/HandOffOnUnmount";
import { StatusBadge } from "../components/StatusBadge";
import { useTabScrollLabels } from "../components/tabScrollLabels";
import { useFocusHandoff } from "../components/useFocusHandoff";
import { useListMotion } from "../components/useListMotion";
import { useT } from "../i18n/react";
import type { Agent, Launch, PerAgent } from "../protocol";
import { useDaemonStore } from "../store";
import { AdvancedSection } from "./AdvancedSection";
import { EffortPicker, type LaunchModels, ModelPicker, ModelsFailureMark, useLaunchModels } from "./LaunchSettingsFields";

/** What the section holds for each agent, the way the console dialog does: its account, and what its console
 * sessions and its project sessions are launched with. */
export interface ConsoleAgentSettings {
  accounts: PerAgent<string | null>;
  launch: PerAgent<Launch>;
  project_session_launch: PerAgent<Launch>;
}

/**
 * The console dialog's Advanced section: the model and reasoning effort of the console's console
 * sessions and of its project sessions, for one agent at a time, chosen on a row of tabs above them.
 * Each agent's panel is a table, the kinds of session as columns and the settings as rows, which
 * stacks below the `md` breakpoint (see `LaunchMatrix`).
 * Until a tab is picked it follows `defaultAgent`, the console session agent, as the user changes
 * it; a picked tab stays. Every agent's values are kept in `values` whichever tab is showing. The tabs
 * are the added agents, in the order they were added; an agent known to be missing or not added has no
 * tab, unless the console still stores a value for it, which stays, marked, until every one of its
 * values is cleared. `children` are the section's further fields, which hold
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
  values: ConsoleAgentSettings;
  onChange: (kind: "launch" | "project_session_launch", agent: Agent, key: keyof Launch, value: string | null) => void;
  children?: React.ReactNode;
}): React.ReactElement {
  const t = useT();
  const tabScrollLabels = useTabScrollLabels();
  const agentAvailability = useDaemonStore((s) => s.agentAvailability);
  const addedAgents = useDaemonStore((s) => s.settings.added_agents);
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const [expanded, setExpanded] = useState(false);
  const [picked, setPicked] = useState<Agent>();
  const stores = (agent: Agent) =>
    !!(
      values.launch[agent].model ||
      values.launch[agent].effort ||
      values.project_session_launch[agent].model ||
      values.project_session_launch[agent].effort
    );
  const listed = listedPickerAgents(agentAvailability, addedAgents, stores);
  // The agent picked, else the console session agent, else the first listed: a tab can go away
  // under the user, when the last value stored for a missing or not added agent is cleared, and one that comes
  // back does not take the view from the one shown meanwhile.
  if (picked && !listed.includes(picked)) setPicked(undefined);
  const agent = [picked, defaultAgent].find((candidate) => candidate && listed.includes(candidate)) ?? listed[0] ?? defaultAgent;
  const rootRef = useRef<HTMLDivElement>(null);
  // Tabs come and go while the section is open, as values for a missing agent are cleared or availability changes.
  const tabsRef = useListMotion<HTMLDivElement>({ contentWidth: true });
  const focusHandoff = useFocusHandoff(() => rootRef.current?.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]')?.focus());
  const launch = useLaunchModels(agent, storedAccount(values.accounts[agent], agent, accounts), expanded);

  return (
    <AdvancedSection expanded={expanded} onExpandedChange={setExpanded} status={launch.modelText}>
      <div ref={rootRef} {...focusHandoff} className="rounded-xl border border-separator p-4">
        <Tabs selectedKey={agent} onSelectionChange={(key) => setPicked(key as Agent)} className="gap-4">
          <Tabs.ListContainer className="w-max max-w-full self-start" render={(props) => <div {...props}>{tabScrollLabels(props.children)}</div>}>
            {/* Positioned, so a departing tab's visual stays in the strip's scroll. */}
            <Tabs.List ref={tabsRef} aria-label={t("dialog.console.launchAgent")} className="relative w-max min-w-0 p-1.5">
              {listed.map((option) => {
                const badge = agentChoiceBadge(t, agentAvailability, addedAgents, option);
                return (
                  <Tabs.Tab key={option} id={option} data-list-motion={option} className="w-auto flex-none scroll-mx-1.5 gap-2 whitespace-nowrap hover:text-foreground">
                    <AgentIcon agent={option} />
                    {AGENT_LABEL[option]}
                    {badge && <StatusBadge>{badge}</StatusBadge>}
                    <Tabs.Indicator />
                  </Tabs.Tab>
                );
              })}
            </Tabs.List>
          </Tabs.ListContainer>
          {listed.map((option) => (
            <Tabs.Panel key={option} id={option} className="mt-0 p-0">
              <LaunchMatrix
                launch={launch}
                consoleSessions={values.launch[agent]}
                projectSessions={values.project_session_launch[agent]}
                onConsoleChange={(key, value) => onChange("launch", agent, key, value)}
                onProjectChange={(key, value) => onChange("project_session_launch", agent, key, value)}
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
 * named by the two it sits under, its kind first. The project pickers are also described by their
 * override note below the table. Both Model pickers are described by the model list note while it loads,
 * which they carry; why there is none is a warning mark after the Model setting's name, in the row header
 * (stacked, above each picker).
 */
export function LaunchMatrix({
  launch,
  consoleSessions,
  projectSessions,
  onConsoleChange,
  onProjectChange,
}: {
  launch: LaunchModels;
  consoleSessions: Launch;
  projectSessions: Launch;
  onConsoleChange: (key: keyof Launch, value: string | null) => void;
  onProjectChange: (key: keyof Launch, value: string | null) => void;
}): React.ReactElement {
  const t = useT();
  const id = useId();
  const [consoleKindId, projectKindId, modelRowId, effortRowId, projectNoteId] = [
    "console-kind",
    "project-kind",
    "model-row",
    "effort-row",
    "project-note",
  ].map((name) => `${id}-${name}`);
  const model = t("dialog.agentSettings.model");
  const effort = t("dialog.agentSettings.effort");
  // Each cell is placed by its own classes, so the source order (a kind's heading and pickers, then
  // the next kind's) is the stacked order and the order of the tab stops. A row header is as high
  // as a picker and at its top, so it stays with the picker when a note makes the cell taller.
  const cell = "flex min-w-0 flex-col gap-1.5";
  const settingName = "text-sm md:hidden";
  const kindHeading = "text-sm font-medium md:font-normal";
  const rowHeader = "hidden text-sm md:col-start-1 md:flex md:h-9 md:items-center md:gap-1 md:self-start";
  // The mark on the Model setting's name, a button, stands beside the name rather than in it: the name is what
  // the pickers are labelled by, and is hidden from assistive technology, which the mark must not be.
  const modelMarkId = `${id}-model-mark`;
  const modelMark = <ModelsFailureMark launch={launch} markId={modelMarkId} />;
  const consoleModelCell = useRef<HTMLDivElement>(null);
  // When the mark goes while it holds focus, the console's Model picker takes it, as a labelled field's trigger does. The
  // hand-off sits before the first of the mark's places, so it runs while the others are still in the document.
  const modelMarkHandOff = launch.modelFailure !== undefined && (
    <HandOffOnUnmount
      from={`[data-status-mark-id="${modelMarkId}"]`}
      to={() => queueMicrotask(() => {
        const target = consoleModelCell.current?.querySelector<HTMLButtonElement>('[data-slot="select-trigger"]');
        if (target?.isConnected && !target.disabled && (!document.activeElement || document.activeElement === document.body)) target.focus();
      })}
    />
  );
  return (
    <div className="flex flex-col gap-3">
      <div className="grid grid-cols-[minmax(0,1fr)] gap-x-6 gap-y-3 md:grid-cols-[max-content_repeat(2,minmax(0,1fr))]">
        <div className={`${rowHeader} md:row-start-2`}>
          <span id={modelRowId} aria-hidden="true">{model}</span>
          {modelMarkHandOff}
          {modelMark}
        </div>
        <div id={effortRowId} aria-hidden="true" className={`${rowHeader} md:row-start-3`}>{effort}</div>
        <div id={consoleKindId} aria-hidden="true" className={`${kindHeading} md:col-start-2 md:row-start-1`}>{t("dialog.console.launchConsole")}</div>
        <div ref={consoleModelCell} className={`${cell} md:col-start-2 md:row-start-2`}>
          <span className={`${settingName} flex items-center gap-1`}>
            <span aria-hidden="true">{model}</span>
            {modelMark}
          </span>
          <ModelPicker launch={launch} model={consoleSessions.model} onChange={(value) => onConsoleChange("model", value)} hideLabel labelledBy={`${consoleKindId} ${modelRowId}`} />
        </div>
        <div className={`${cell} md:col-start-2 md:row-start-3`}>
          <span aria-hidden="true" className={settingName}>{effort}</span>
          <EffortPicker launch={launch} model={consoleSessions.model} effort={consoleSessions.effort} onChange={(value) => onConsoleChange("effort", value)} hideLabel labelledBy={`${consoleKindId} ${effortRowId}`} />
        </div>
        <div id={projectKindId} aria-hidden="true" className={`mt-1 md:mt-0 ${kindHeading} md:col-start-3 md:row-start-1`}>{t("dialog.console.launchProject")}</div>
        <div className={`${cell} md:col-start-3 md:row-start-2`}>
          <span className={`${settingName} flex items-center gap-1`}>
            <span aria-hidden="true">{model}</span>
            {modelMark}
          </span>
          <ModelPicker launch={launch} model={projectSessions.model} onChange={(value) => onProjectChange("model", value)} hideLabel labelledBy={`${projectKindId} ${modelRowId}`} describedBy={projectNoteId} />
        </div>
        <div className={`${cell} md:col-start-3 md:row-start-3`}>
          <span aria-hidden="true" className={settingName}>{effort}</span>
          <EffortPicker launch={launch} model={projectSessions.model} effort={projectSessions.effort} onChange={(value) => onProjectChange("effort", value)} hideLabel labelledBy={`${projectKindId} ${effortRowId}`} describedBy={projectNoteId} />
        </div>
      </div>
      <Description id={projectNoteId}>{t("dialog.console.projectLaunchOverridesNote")}</Description>
    </div>
  );
}
