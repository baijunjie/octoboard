import React from "react";

import { DaemonRequestError } from "../daemon-client";
import { daemonParams, DETAIL_BLOCK_CODES, leftBranchGroups, leftBranchItems, leftBranchName, messageCode, runningSessionIds, type LeftBranchItem } from "../daemonMessage";
import { hasCodeParams, hasShellTimeoutDetails, type ErrorPresentation } from "../errorPresentation";
import { errorText } from "../errorText";
import { isMessageKey } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import { joinNodes } from "../i18n/joinPhrases";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import type { MessageParams } from "../protocol";
import { useDaemonStore } from "../store";
import { BranchGroups } from "./BranchGroups";
import { CodeBlock } from "./CodeBlock";
import { InlineChip } from "./InlineChip";

export function DaemonErrorTitle(): React.ReactElement {
  const t = useT();
  return <>{t("daemon.shell_environment_timeout.title")}</>;
}

const NO_RECORDS = new Map<string, never>();

/** What a `session_branches_not_removed` alert names, grouped by why each is still there: a lead
 * line per group over a list of names as code chips, with a locked worktree's uncommitted changes
 * and git's own message for one that failed. With a single group its lead says it all, and the
 * line over the groups is left out. */
function LeftBranches({ count, items }: { count: number; items: LeftBranchItem[] }): React.ReactElement {
  const t = useT();
  const homeDir = useDaemonStore((s) => s.homeDir);
  const groups = leftBranchGroups(items).map((group) => ({
    key: group.key,
    lead: t(group.lead, { count: group.items.length }),
    rows: group.items.map((item, at) => ({
      id: `${at}`,
      worktree: item.kind === "worktree",
      path: leftBranchName(item, homeDir),
      branch: item.kind === "worktree" ? item.branch : item.name,
      uncommitted: item.state === "locked" && item.uncommitted,
      detail: item.detail,
    })),
  }));
  return (
    <div className="flex min-w-0 flex-col gap-2">
      {groups.length > 1 && <span>{t("daemon.session_branches_not_removed.list", { count })}</span>}
      <BranchGroups groups={groups} />
    </div>
  );
}

/** A failure with names or technical literals set apart as chips in the catalog's wording. */
function CodedDaemonMessage({ error }: { error: DaemonRequestError }): React.ReactElement {
  const language = useCurrentLanguage();
  // The records are subscribed to only for a message that names one, so a failure that does not
  // re-renders with none of them.
  const consoles = useDaemonStore((s) => ("console" in error.params ? s.consoles : NO_RECORDS));
  const projects = useDaemonStore((s) => ("project" in error.params ? s.projects : NO_RECORDS));
  const sessions = useDaemonStore((s) => ("session" in error.params || "session_ids" in error.params ? s.sessions : NO_RECORDS));
  const id = `daemon.${messageCode(error.code, error.params)}`;
  if (!isMessageKey(id)) return <>{error.message}</>;
  const left = leftBranchItems(error.code, error.params);
  if (left) return <LeftBranches count={left.length} items={left} />;
  const params = daemonParams<React.ReactNode>(language, error.code, error.params, { consoles, projects, sessions }, (value, monospace) => <InlineChip monospace={monospace} wrap size="inherit">{value}</InlineChip>);
  const running = error.code === "session_has_running_sessions" ? runningSessionIds(error.params) : undefined;
  if (running) {
    const titles = running.map((id) => sessions.get(id)?.title ?? id);
    params.sessions = joinNodes(language, titles, (title, key) => <InlineChip key={key} wrap size="inherit">{title}</InlineChip>);
  }
  if (error.code === "launch_failed" && error.params.reason_code && error.params.reason_code !== "launch_failed" && hasCodeParams({ code: error.params.reason_code, params: error.params })) {
    params.reason = codedMessageNode({ code: error.params.reason_code, params: error.params }, String(params.reason ?? ""));
  }
  // One element, so a container that lays its children out as flex items (an alert's column) keeps
  // the sentence whole instead of making each run of text and each literal an item of its own.
  const sentence = <span><Message id={id} params={params as never} /></span>;
  if (!DETAIL_BLOCK_CODES.has(error.code) || !error.params.detail) return sentence;
  return (
    <div className="flex min-w-0 flex-col gap-2">
      {sentence}
      <CodeBlock dir="ltr">{error.params.detail}</CodeBlock>
    </div>
  );
}

/** A failure worded as `text` (in the current language), as a node: where its message sets some params
 * as code they are set as code, anywhere else it is the text itself. */
export function codedMessageNode(failure: { code: string; params: MessageParams }, text: string): React.ReactNode {
  return hasCodeParams(failure) ? <DaemonErrorMessage error={new DaemonRequestError(text, failure.code, failure.params)} /> : text;
}

/** A failure as a param of a sentence: the structured message where it has one, otherwise its text
 * isolated, as any value put inside a sentence is. */
export function failureParam(error: ErrorPresentation): React.ReactNode {
  return typeof error !== "string" && hasCodeParams(error) ? <DaemonErrorMessage error={error} showTitle={false} /> : isolate(errorText(error));
}

/** Structured daemon failures retain their code and parameters; other errors stay plain text. */
export function DaemonErrorMessage({ error, showTitle = true }: { error: ErrorPresentation; showTitle?: boolean }): React.ReactElement {
  const t = useT();
  if (typeof error !== "string" && hasCodeParams(error)) return <CodedDaemonMessage error={error} />;
  if (typeof error === "string" || !hasShellTimeoutDetails(error)) return <>{errorText(error)}</>;
  const { shell, command, timeout } = error.params;
  return (
    <div className="flex min-w-0 flex-col gap-2 [overflow-wrap:anywhere]">
      {showTitle && <h3 className="text-base font-semibold text-foreground"><DaemonErrorTitle /></h3>}
      <p>
        <Message
          id="daemon.shell_environment_timeout.command"
          params={{ command: <InlineChip monospace wrap size="inherit">{`${shell} -l -i -c '${command}'`}</InlineChip>, timeout }}
        />
      </p>
      <p>{t("daemon.shell_environment_timeout.cause")}</p>
      <p>{t("daemon.shell_environment_timeout.consequence")}</p>
      <p>{t("daemon.shell_environment_timeout.recovery")}</p>
    </div>
  );
}
