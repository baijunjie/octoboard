import { CircleCheck, CircleDashed } from "lucide-react";
import React, { useId } from "react";

import { providerLabel } from "../agents/providers";
import { AFTER_FOCUS_MOVE_DELAY_MS, StatusAnnouncer } from "../components/announce/StatusAnnouncer";
import { useT } from "../i18n/react";
import { Label, ProgressBar, ProgressBarTrack, Spinner } from "../primitives";
import type { PlainMessageKey } from "../i18n/catalog";
import { useProviders } from "../store";
import { FlatCard } from "../components/card/FlatCard";
import { EntryStatus, SHARED_STATUS, STATUS_TONE, type StatusEntry, StatusLine } from "./importParts";
import type { Inspection } from "./importPlan";
import { itemKey, progressCount, type ImportProgress, type ItemProgress } from "./progressState";
import { ResultCard, ResultRow } from "./ResultCards";

const STATUS: Record<Exclude<ItemProgress["state"], "creating" | "cloning">, StatusEntry> = {
  waiting: { label: "configuration.import.progress.status.waiting", icon: CircleDashed, tone: STATUS_TONE.muted },
  done: { label: "configuration.import.progress.status.done", icon: CircleCheck, tone: STATUS_TONE.success },
  ...SHARED_STATUS,
};

const WORKING_LABEL: Record<"creating" | "cloning", PlainMessageKey> = {
  creating: "configuration.import.progress.status.creating",
  cloning: "configuration.import.progress.status.cloning",
};

/** An item's state as a glyph (a spinner while it is being worked on) and its word. */
function ItemState({ item }: { item: ItemProgress }): React.ReactElement {
  if (item.state === "creating" || item.state === "cloning") {
    return (
      <StatusLine label={WORKING_LABEL[item.state]}>
        <Spinner size="sm" color="current" className="size-4 shrink-0" />
      </StatusLine>
    );
  }
  return <EntryStatus entry={STATUS[item.state]} />;
}

/** The row of a project or an account of the import: its state beside its name, and what it adds to that under it. */
function ItemRow({ name, item }: { name: string; item: ItemProgress }): React.ReactElement {
  const t = useT();
  return (
    <ResultRow
      name={name}
      status={<ItemState item={item} />}
      detail={item.state === "not_attempted" ? t("configuration.import.result.detail.consoleFailed") : undefined}
      failure={item.state === "failed" ? item.error : undefined}
    />
  );
}

/**
 * A running import, as the daemon's progress events say it: a progress bar of how many items have settled, then the
 * items it announced, grouped and in the order of the review (each console with its projects, the default clone
 * directory, the application settings, the Agent accounts settings, each agent with its accounts), each with its state. A project the
 * import leaves out is not announced and so not listed. The bar is named for the import and its value is read as the
 * count, which is not announced on every step: a status speaks the import when it starts and again when every item has
 * settled. Until the first event there is no count, and the bar is indeterminate, described by the line saying what it
 * waits for.
 */
export function ImportProgressView({ inspection, progress }: { inspection: Inspection; progress: ImportProgress }): React.ReactElement {
  const t = useT();
  const providers = useProviders();
  const id = useId();
  const { settled, total } = progressCount(progress);
  const agentSettings = progress.get(itemKey({ kind: "agent_settings" }));
  const cloneDir = progress.get(itemKey({ kind: "clone_dir" }));
  const settings = progress.get(itemKey({ kind: "settings" }));
  const counting = total > 0;
  const count = t("configuration.import.progress.summary", { count: settled, total });
  const label = t("configuration.import.progress.label");
  return (
    <>
      <StatusAnnouncer
        text={counting && settled === total ? t("configuration.import.progress.allProcessed") : label}
        firstWriteDelayMs={AFTER_FOCUS_MOVE_DELAY_MS}
      />
      <ProgressBar value={settled} maxValue={Math.max(total, 1)} indeterminate={!counting} valueLabel={count} describedBy={counting ? undefined : `${id}-value`}>
        <div className="flex items-baseline justify-between gap-2 text-sm">
          <Label>{label}</Label>
          <span id={`${id}-value`} className="text-app-muted">{counting ? count : t("configuration.import.progress.starting")}</span>
        </div>
        <ProgressBarTrack />
      </ProgressBar>
      {inspection.consoles.map((console, c) => {
        const item = progress.get(itemKey({ kind: "console", console: c }));
        if (!item) return null;
        const projects = console.projects.flatMap((project, p) => {
          const state = progress.get(itemKey({ kind: "project", console: c, project: p }));
          return state ? [<ItemRow key={p} name={project.name} item={state} />] : [];
        });
        return (
          <ResultCard key={`console-${c}`} name={console.name} status={<ItemState item={item} />} failure={item.state === "failed" ? item.error : undefined}>
            {projects.length > 0 && <FlatCard.Rows>{projects}</FlatCard.Rows>}
          </ResultCard>
        );
      })}
      {cloneDir && <ResultCard name={t("settings.cloneDir.label")} status={<ItemState item={cloneDir} />} failure={cloneDir.state === "failed" ? cloneDir.error : undefined} />}
      {settings && <ResultCard name={t("settings.title")} status={<ItemState item={settings} />} failure={settings.state === "failed" ? settings.error : undefined} />}
      {agentSettings && <ResultCard name={t("configuration.import.agentSettings.heading")} status={<ItemState item={agentSettings} />} failure={agentSettings.state === "failed" ? agentSettings.error : undefined} />}
      {inspection.accounts?.agents.map((agent) => {
        const rows = agent.accounts.flatMap((account, i) => {
          const state = progress.get(itemKey({ kind: "account", agent: agent.agent, account: i }));
          return state ? [<ItemRow key={i} name={account.name} item={state} />] : [];
        });
        if (rows.length === 0) return null;
        return (
          <ResultCard key={`agent-${agent.agent}`} name={providerLabel(t, providers, agent.agent)}>
            <FlatCard.Rows>{rows}</FlatCard.Rows>
          </ResultCard>
        );
      })}
    </>
  );
}
