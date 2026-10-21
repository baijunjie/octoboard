import React from "react";

import { DaemonRequestError } from "../daemon-client";
import { daemonParams, DETAIL_BLOCK_CODES, RECOVERY_CODES, hasTrustDiagnostic, leftBranchGroups, leftBranchItems, leftBranchName, richMessageCode, runningSessionIds, type LeftBranchItem } from "../daemonMessage";
import { hasCodeParams, hasShellTimeoutDetails, type ErrorPresentation } from "../errorPresentation";
import { errorText } from "../errorText";
import { globalParam, isMessageKey } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import { joinNodes } from "../i18n/joinPhrases";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import type { MessageParams } from "../protocol";
import { useErrorRecords } from "../store";
import { BranchGroups } from "./BranchGroups";
import { DiagnosticBlock } from "./DiagnosticBlock";
import { InlineChip } from "./InlineChip";

export function DaemonErrorTitle(): React.ReactElement {
  const t = useT();
  return <>{t("daemon.shell_environment_timeout.title")}</>;
}

const NO_RECORDS = new Map<string, never>();

export type DaemonErrorPart = "summary" | "details";

/** What a `session_branches_not_removed` alert names, grouped by why each is still there: a lead
 * line per group over a list of names as code chips, with a locked worktree's uncommitted changes
 * and git's own message for one that failed. With a single group its lead says it all, and the
 * line over the groups is left out. */
function LeftBranches({ count, items, part }: { count: number; items: LeftBranchItem[]; part?: DaemonErrorPart }): React.ReactElement {
  const t = useT();
  const homeDir = useErrorRecords((s) => s.homeDir);
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
  if (part === "summary") return <>{groups.length === 1 ? groups[0].lead : t("daemon.session_branches_not_removed.list", { count })}</>;
  return (
    <div className="flex min-w-0 flex-col gap-2">
      {!part && groups.length > 1 && <span>{t("daemon.session_branches_not_removed.list", { count })}</span>}
      <BranchGroups groups={part && groups.length === 1 ? groups.map((group) => ({ ...group, lead: undefined })) : groups} />
    </div>
  );
}

/** A failure with names or technical literals set apart as chips in the catalog's wording. */
function CodedDaemonMessage({ error, part }: { error: DaemonRequestError; part?: DaemonErrorPart }): React.ReactElement | null {
  const language = useCurrentLanguage();
  // The records are subscribed to only for a message that names one, so a failure that does not
  // re-renders with none of them.
  const consoles = useErrorRecords((s) => ("console" in error.params ? s.consoles : NO_RECORDS));
  const projects = useErrorRecords((s) => ("project" in error.params ? s.projects : NO_RECORDS));
  const sessions = useErrorRecords((s) => ("session" in error.params || "session_ids" in error.params ? s.sessions : NO_RECORDS));
  const shellTerminals = useErrorRecords((s) => ("shell_terminal" in error.params ? s.shellTerminals : NO_RECORDS));
  const accounts = useErrorRecords((s) => (error.code === "unknown_account" ? s.settings.accounts : undefined));
  const id = `daemon.${richMessageCode(error.code, error.params)}`;
  if (!isMessageKey(id)) return part === "details" ? null : <>{error.message}</>;
  const left = leftBranchItems(error.code, error.params);
  if (left) return <LeftBranches count={left.length} items={left} part={part} />;
  const params = daemonParams<React.ReactNode>(language, error.code, error.params, { consoles, projects, sessions, shellTerminals, accounts }, (value, monospace) => <InlineChip dir={monospace ? "ltr" : "auto"} monospace={monospace} wrap size="inherit">{value}</InlineChip>);
  if (error.code === "conversation_held_in_project" || error.code === "conversation_held_in_console") {
    const holder = sessions.get(error.params.session);
    const task = holder?.terminal_title?.trim();
    if (holder && task && task !== holder.title.trim()) {
      params.session = <Message id="session.displayTitle" params={{
        title: <InlineChip dir="auto" wrap size="inherit">{task}</InlineChip>,
        name: <InlineChip dir="auto" wrap size="inherit">{holder.title}</InlineChip>,
      }} />;
    }
  }
  if (error.code === "account_quota_exhausted" && error.params.window && !["5-hour", "Weekly", "Monthly", "Credits"].includes(error.params.window)) {
    params.quota = <Message id="quota.windowPhrase.named" params={{ window: <InlineChip dir="auto" wrap size="inherit">{error.params.window}</InlineChip> }} />;
  }
  const running = error.code === "session_has_running_sessions" ? runningSessionIds(error.params) : undefined;
  if (running) {
    params.sessions = joinNodes(language, running, (id, key) => <InlineChip key={key} dir={sessions.has(id) ? "auto" : "ltr"} monospace={!sessions.has(id)} wrap size="inherit">{sessions.get(id)?.title ?? id}</InlineChip>);
  }
  const nested = error.code === "launch_failed" && error.params.reason_code && error.params.reason_code !== "launch_failed"
    ? new DaemonRequestError(String(params.reason ?? ""), error.params.reason_code, error.params)
    : undefined;
  const structuredNested = nested && (hasCodeParams(nested) || hasShellTimeoutDetails(nested));
  if (structuredNested) params.reason = <DaemonErrorMessage error={nested} part="summary" />;
  const trustReasonId = `daemon.trust_reason.${error.params.reason_code}`;
  if ((error.code === "trust_answer_failed" || error.code === "trust_not_carried_over") && isMessageKey(trustReasonId)) {
    params.reason = <Message id={trustReasonId} params={{ ...params } as never} />;
  }
  const trustDiagnostic = hasTrustDiagnostic(error.code, error.params);
  const trustReason = `daemon.trust_reason.${error.params.reason_code}.summary`;
  if (trustDiagnostic && isMessageKey(trustReason)) params.reason = <Message id={trustReason} params={{} as never} />;
  const diagnostic = (DETAIL_BLOCK_CODES.has(error.code) || trustDiagnostic) && error.params.detail ? <DiagnosticBlock>{error.params.detail}</DiagnosticBlock> : null;
  const recoveryId = `daemon.${error.code}.recovery`;
  const recovery = RECOVERY_CODES.has(error.code) && isMessageKey(recoveryId) ? <p><Message id={recoveryId} params={params as never} /></p> : null;
  const nestedDetails = structuredNested && part !== "summary" ? <DaemonErrorMessage error={nested} part="details" /> : null;
  if (part === "details") return <>{diagnostic}{recovery}{nestedDetails}</>;
  // One element, so a container that lays its children out as flex items (an alert's column) keeps
  // the sentence whole instead of making each run of text and each literal an item of its own.
  const sentence = <span><Message id={id} params={params as never} /></span>;
  if (part === "summary" || (!diagnostic && !recovery && !nestedDetails)) return sentence;
  return (
    <div className="flex min-w-0 flex-col gap-2">
      {sentence}
      {diagnostic}
      {recovery}
      {nestedDetails}
    </div>
  );
}

/** A failure worded as `text` (in the current language), as a node: where its message sets some params
 * as code they are set as code, anywhere else it is the text itself. */
export function codedMessageNode(failure: { code: string; params: MessageParams }, text: string, part?: DaemonErrorPart): React.ReactNode {
  return hasCodeParams(failure) || (part && hasShellTimeoutDetails(failure)) ? <DaemonErrorMessage error={new DaemonRequestError(text, failure.code, failure.params)} part={part} /> : part === "details" ? null : text;
}

/** A failure as a param of a sentence: the structured message where it has one, otherwise its text
 * isolated, as any value put inside a sentence is. */
export function failureParam(error: ErrorPresentation): React.ReactNode {
  return typeof error !== "string" && (hasCodeParams(error) || hasShellTimeoutDetails(error)) ? <DaemonErrorMessage error={error} showTitle={false} part="summary" /> : isolate(errorText(error));
}

/** Structured failures retain their metadata; `summary` is inline-safe for Alert.Description,
 * while `details` holds only the remaining blocks. Omitting `part` renders the complete message. */
export function DaemonErrorMessage({ error, showTitle = true, part }: { error: ErrorPresentation; showTitle?: boolean; part?: DaemonErrorPart }): React.ReactElement | null {
  const t = useT();
  if (typeof error !== "string" && !hasShellTimeoutDetails(error) && hasCodeParams(error)) return <CodedDaemonMessage error={error} part={part} />;
  if (typeof error === "string" || !hasShellTimeoutDetails(error)) return part === "details" ? null : <>{errorText(error)}</>;
  if (part === "summary") return <>{t("daemon.shell_environment_timeout.cause")}</>;
  const { shell, command, timeout } = error.params;
  return (
    <div className="flex min-w-0 flex-col gap-2 [overflow-wrap:anywhere]">
      {!part && showTitle && <h3 className="text-base font-semibold text-foreground"><DaemonErrorTitle /></h3>}
      <p>
        <Message
          id="daemon.shell_environment_timeout.command"
          params={{ command: <InlineChip monospace wrap size="inherit">{`${shell} -l -i -c '${command}'`}</InlineChip>, timeout }}
        />
      </p>
      {!part && <p>{t("daemon.shell_environment_timeout.cause")}</p>}
      <p><Message id="daemon.shell_environment_timeout.consequence" params={{ appName: <InlineChip dir="auto" wrap size="inherit">{globalParam("appName")}</InlineChip> } as never} /></p>
      <p>{t("daemon.shell_environment_timeout.recovery")}</p>
    </div>
  );
}
