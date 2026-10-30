import React, { useId } from "react";

import { DaemonRequestError } from "../daemon-client";
import { daemonParams, DETAIL_BLOCK_CODES, RECOVERY_CODES, hasTrustDiagnostic, leftBranchDetail, leftBranchGroups, leftBranchItems, leftBranchName, richMessageCode, lacksSummary, sharingFailureItems, sharingRawReason, hasSharingReason, runningSessionIds, hasRefusedQuotaWindow, messageCode, type LeftBranchItem } from "../daemonMessage";
import { hasCodeParams, hasShellTimeoutDetails, systemCauseKey, uncataloguedSummaryKey, unwordedReason, type ErrorPresentation } from "../errorPresentation";
import { isUncataloguedFailure } from "../uncataloguedFailure";
import { ActionableError } from "../actionableError";
import { errorText, unexpectedFailureDiagnostic } from "../errorText";
import { globalParam, isMessageKey, type MessageKey } from "../i18n/catalog";
import { joinNodes } from "../i18n/joinPhrases";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import { NESTED_FAILURE_CODES, NESTED_REMEDY_CODES, type MessageParams } from "../protocol";
import { sessionTask } from "../sessionLabel";
import { useErrorRecords } from "../store";
import { BranchGroups } from "./BranchGroups";
import { DiagnosticBlock } from "./DiagnosticBlock";
import { hasErrorDetails, hasErrorRemedy } from "./errorPartPresence";
import { InlineReference } from "./InlineReference";
import { SharingCauseSummary, SharingDetails, SharingFailureReason } from "./SharingFailure";

export function DaemonErrorTitle(): React.ReactElement {
  const t = useT();
  return <>{t("daemon.shell_environment_timeout.title")}</>;
}

const NO_RECORDS = new Map<string, never>();

/** The three layers of a failure: `summary` is what happened and `remedy` the one main thing to do about
 * it, both in the application's own words and always visible (a code in `NO_SUMMARY_CODES` has no summary, as its
 * alert's title says what happened); `details` is what the Details button
 * holds: first any further explanation in the application's own words (the catalog's `.details` line, a reason per cause with its
 * fix, the lead lines of several groups), then the raw text that was not written for the reader (a
 * diagnostic block, the system's own message, raw entries). `further` and `raw` are those two halves of
 * `details` apart, for a surface whose body is the application's own words and keeps only the raw text
 * behind a disclosure (the error alert dialog). */
export type DaemonErrorPart = "summary" | "remedy" | "details" | "further" | "raw";

/** What a `session_branches_not_removed` alert names, grouped by why each is still there: a lead
 * line per group over a list of names as code chips, with a locked worktree's uncommitted changes
 * and git's own message for one that failed, and a group's note after its list. With a single group its lead says it all, and the
 * line over the groups is left out. All of it is Details: the summary is the line over the groups, or
 * the one group's lead, and there is no remedy. Where the raw text is kept apart (`further`, `raw`, or the
 * whole message), git's message is not under its name in the list but in `raw`, under that name again. */
function LeftBranches({ count, items, part }: { count: number; items: LeftBranchItem[]; part?: DaemonErrorPart }): React.ReactElement | null {
  const t = useT();
  const homeDir = useErrorRecords((s) => s.homeDir);
  const groups = leftBranchGroups(items).map((group) => ({
    key: group.key,
    lead: t(group.lead, { count: group.items.length }),
    note: group.note && t(group.note, { count: group.items.length }),
    rows: group.items.map((item, at) => ({
      id: `${at}`,
      worktree: item.kind === "worktree",
      path: leftBranchName(item, homeDir),
      branch: item.kind === "worktree" ? item.branch : item.name,
      uncommitted: item.state === "locked" && item.uncommitted,
      detail: leftBranchDetail(item),
    })),
  }));
  if (part === "remedy") return null;
  if (part === "summary") return <>{groups.length === 1 ? groups[0].lead : t("daemon.session_branches_not_removed.list", { count })}</>;
  if (part === "raw") {
    const failed = groups.flatMap((group) => group.rows).filter((row) => row.detail);
    return failed.length === 0 ? null : <>{failed.map((row) => <LeftBranchDetail key={`${row.worktree}:${row.path}`} name={row.path} detail={row.detail!} />)}</>;
  }
  const shown = part === "details" ? groups : groups.map((group) => ({ ...group, rows: group.rows.map((row) => ({ ...row, detail: undefined })) }));
  return (
    <div className="flex min-w-0 flex-col gap-2">
      {!part && groups.length > 1 && <span>{t("daemon.session_branches_not_removed.list", { count })}</span>}
      <BranchGroups groups={part && shown.length === 1 ? shown.map((group) => ({ ...group, lead: undefined })) : shown} />
    </div>
  );
}

/** Git's own message for one branch or worktree that could not be removed, under its name, which names the block. */
function LeftBranchDetail({ name, detail }: { name: string; detail: string }): React.ReactElement {
  const id = useId();
  return (
    <div className="flex min-w-0 flex-col gap-1">
      <h3 id={id} dir="ltr" className="font-mono text-sm font-semibold [overflow-wrap:anywhere]">{name}</h3>
      <DiagnosticBlock labelledBy={id}>{detail}</DiagnosticBlock>
    </div>
  );
}

/** A failure with names or technical literals set apart as chips in the catalog's wording. */
function CodedDaemonMessage({ error, part }: { error: DaemonRequestError; part?: DaemonErrorPart }): React.ReactElement | null {
  const t = useT();
  const language = useCurrentLanguage();
  // The records are subscribed to only for a message that names one, so a failure that does not
  // re-renders with none of them.
  const consoles = useErrorRecords((s) => ("console" in error.params ? s.consoles : NO_RECORDS));
  const projects = useErrorRecords((s) => ("project" in error.params ? s.projects : NO_RECORDS));
  const sessions = useErrorRecords((s) => ("session" in error.params || "session_ids" in error.params ? s.sessions : NO_RECORDS));
  const shellTerminals = useErrorRecords((s) => ("shell_terminal" in error.params ? s.shellTerminals : NO_RECORDS));
  const accounts = useErrorRecords((s) => (error.code === "unknown_account" ? s.settings.accounts : undefined));
  const id = `daemon.${richMessageCode(error.code, error.params)}`;
  // An `ActionableError`'s `detail` is raw text of its own, whatever the catalog says of its code.
  const actionableRaw = error instanceof ActionableError && error.detail ? <DiagnosticBlock>{error.detail}</DiagnosticBlock> : null;
  const summaryless = lacksSummary(error.code);
  if (!summaryless && !isMessageKey(id)) return part === undefined || part === "summary" ? <>{error.message}</> : part === "details" || part === "raw" ? actionableRaw : null;
  const left = leftBranchItems(error.code, error.params);
  if (left) return <LeftBranches count={left.length} items={left} part={part} />;
  const params = daemonParams<React.ReactNode>(language, error.code, error.params, { consoles, projects, sessions, shellTerminals, accounts }, (value, monospace) => <InlineReference dir={monospace ? "ltr" : "auto"} monospace={monospace} wrap>{value}</InlineReference>);
  if (error.code === "conversation_held_in_project" || error.code === "conversation_held_in_console") {
    const holder = sessions.get(error.params.session);
    const task = holder && sessionTask(holder);
    if (holder && task) {
      // One chip around the joined title; each name is isolated so mixed directions cannot reorder
      // across the separator.
      params.session = <InlineReference dir="auto" wrap><Message id="session.displayTitle" params={{ title: <bdi>{task}</bdi>, name: <bdi>{holder.title}</bdi> }} /></InlineReference>;
    }
  }
  if (error.code === "account_quota_exhausted" && error.params.window && !hasRefusedQuotaWindow(error.params)) {
    params.quota = <Message id="quota.windowPhrase.named" params={{ window: <InlineReference dir="auto" wrap>{error.params.window}</InlineReference> }} />;
  }
  const running = error.code === "session_has_running_sessions" ? runningSessionIds(error.params) : undefined;
  if (running) {
    params.sessions = joinNodes(language, running, (id, key) => <InlineReference key={key} dir={sessions.has(id) ? "auto" : "ltr"} monospace={!sessions.has(id)} wrap>{sessions.get(id)?.title ?? id}</InlineReference>);
  }
  const nested = NESTED_FAILURE_CODES.has(error.code) && error.params.reason_code && error.params.reason_code !== error.code
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
  // A cause the application has no words for is said as a generic reason, its own text being raw.
  const unworded = unwordedReason(error.code, error.params);
  if (unworded !== undefined) params.reason = <Message id={nested ? uncataloguedSummaryKey(nested) : "daemon.trust_reason.unexpected"} />;
  // A sharing failure says why, and what to do about each cause, ahead of the raw text in Details (a
  // disclosure, or the alert's popover), and the raw entries and the system's own reason follow it,
  // whether it names entries or only says why the change could not start (the failure's `detail`, or a
  // repair's own message). A cause with no line of its own points at the raw reason, which is only
  // useful where the raw reason is folded away, so Details leaves that pointer out.
  const sharing = sharingFailureItems(error.code, error.params);
  const sharingReason = sharingRawReason(error);
  const sharingFailure = sharingReason !== undefined;
  const rawReason = sharingReason?.raw;
  const pointedAt = part === "details" ? undefined : rawReason;
  const reasonLines = sharing
    ? <SharingCauseSummary items={sharing} />
    : sharingFailure && hasSharingReason(error.params.cause, pointedAt)
      ? <span className="block"><SharingFailureReason cause={error.params.cause} detail={pointedAt} /></span>
      : null;
  const sharingRaw = sharing || (sharingFailure && rawReason) ? <SharingDetails items={sharing ?? []} detail={rawReason} /> : null;
  // Verbatim diagnostic output (a push's hook checks) can run to hundreds of lines: it wraps and scrolls inside the block.
  const diagnostic = !sharingFailure && (DETAIL_BLOCK_CODES.has(error.code) || trustDiagnostic) && error.params.detail ? <DiagnosticBlock wrap tall>{error.params.detail}</DiagnosticBlock> : null;
  const unwordedRaw = unworded ? <DiagnosticBlock>{unworded}</DiagnosticBlock> : null;
  const recoveryId = `daemon.${messageCode(error.code, error.params)}.recovery`;
  const recovery = RECOVERY_CODES.has(error.code) && isMessageKey(recoveryId) ? <span className="block"><Message id={recoveryId} params={params as never} /></span> : null;
  // The application's own further explanation, which leads Details, ahead of the raw text.
  const furtherId = `daemon.${error.code}.details`;
  const further = isMessageKey(furtherId) ? <span className="block"><Message id={furtherId} params={params as never} /></span> : null;
  // The nested cause's recovery is the failure's own, a launch failure having none of its own.
  const remedy = recovery ?? (structuredNested && NESTED_REMEDY_CODES.has(error.code) && hasErrorRemedy(nested) ? <DaemonErrorMessage error={nested} part="remedy" /> : null);
  const nestedFurther = structuredNested ? <DaemonErrorMessage error={nested} part="further" /> : null;
  const nestedRaw = structuredNested ? <DaemonErrorMessage error={nested} part="raw" /> : null;
  const explanation = <>{further}{reasonLines}{nestedFurther}</>;
  const raw = <>{actionableRaw}{sharingRaw}{diagnostic}{unwordedRaw}{nestedRaw}</>;
  if (part === "details") return <>{explanation}{raw}</>;
  if (part === "further") return explanation;
  if (part === "raw") return raw;
  if (part === "remedy") return remedy;
  // One element, so a container that lays its children out as flex items (an alert's column) keeps
  // the sentence whole instead of making each run of text and each literal an item of its own.
  // A code with no opening sentence (`NO_SUMMARY_CODES`) has no sentence: the body starts with the remedy.
  // A failure that states only its cause says the cause the system's text names after its sentence.
  const systemCause = systemCauseKey(error);
  const sentence = summaryless ? null : <span><Message id={id as MessageKey} params={params as never} />{systemCause && `${t("common.sentenceSeparator")}${t(systemCause)}`}</span>;
  // A failed automatic switch says what happened in its sentence and why on a line of its own, which is
  // the nested failure's summary (or the generic sentence of one the application has no words for).
  const reasonLine = error.code === "account_switch_automatically_failed" && params.reason !== undefined ? <span className="block">{params.reason}</span> : null;
  if (part === "summary" || (!reasonLine && !hasErrorRemedy(error) && !hasErrorDetails(error))) return sentence;
  // With no opening sentence and nothing else of the application's own, the body is empty rather than an empty column, so the
  // alert lays out the remedy it adds as its first line.
  if (summaryless && !remedy && !further && !reasonLines && !nestedFurther) return null;
  // The whole message is the application's own words; the raw text is the `raw` part, kept behind a disclosure.
  return (
    <div className="flex min-w-0 flex-col gap-2">
      {sentence}
      {reasonLine}
      {remedy}
      {explanation}
    </div>
  );
}

/** The login-shell command that timed out and for how long, which a shell-environment failure's details close with. */
export function ShellTimeoutCommand({ params: { shell, command, timeout } }: { params: MessageParams }): React.ReactElement {
  return (
    <p className="[overflow-wrap:anywhere]">
      <Message
        id="daemon.shell_environment_timeout.command"
        params={{ command: <InlineReference monospace wrap>{`${shell} -l -i -c '${command}'`}</InlineReference>, timeout }}
      />
    </p>
  );
}

/** Structured failures retain their metadata; `summary` is inline-safe for Alert.Description (empty for a code in `NO_SUMMARY_CODES`), `remedy`
 * holds the one main thing to do (also inline-safe) and `details` the further explanation, then the raw blocks
 * (`further` and `raw` apart). Omitting `part` renders the complete message in the application's own words: the
 * sentence, the remedy and the further explanation, never the raw text; a code in `NO_SUMMARY_CODES` has no sentence, and
 * its whole message is empty where nothing else of the application's own remains. A failure the application has no words for
 * shows its generic sentence, and its raw message is its `details` and its `raw` (with a script error's stack). */
export function DaemonErrorMessage({ error, showTitle = true, part }: { error: ErrorPresentation; showTitle?: boolean; part?: DaemonErrorPart }): React.ReactElement | null {
  const t = useT();
  if (typeof error !== "string" && isUncataloguedFailure(error)) {
    if (part === undefined || part === "summary") return <>{t(uncataloguedSummaryKey(error))}</>;
    if (part === "details") return errorText(error) !== "" ? <DiagnosticBlock>{errorText(error)}</DiagnosticBlock> : null;
    const diagnostic = unexpectedFailureDiagnostic(error);
    return part === "raw" && diagnostic !== "" ? <DiagnosticBlock>{diagnostic}</DiagnosticBlock> : null;
  }
  if (typeof error !== "string" && !hasShellTimeoutDetails(error) && hasCodeParams(error)) return <CodedDaemonMessage error={error} part={part} />;
  if (typeof error === "string" || !hasShellTimeoutDetails(error)) {
    if (part === "summary" || !part) return <>{errorText(error)}</>;
    const detail = error instanceof ActionableError ? error.detail : undefined;
    return detail && (part === "details" || part === "raw") ? <DiagnosticBlock>{detail}</DiagnosticBlock> : null;
  }
  if (part === "summary") return <>{t("daemon.shell_environment_timeout.cause")}</>;
  const consequence = <Message id="daemon.shell_environment_timeout.consequence" params={{ appName: <InlineReference dir="auto" wrap>{globalParam("appName")}</InlineReference> } as never} />;
  if (part === "remedy") return <span className="block">{t("daemon.shell_environment_timeout.recovery")}</span>;
  if (part === "raw") return null;
  if (part === "details" || part === "further") return <><p>{consequence}</p><ShellTimeoutCommand params={error.params} /></>;
  return (
    <div className="flex min-w-0 flex-col gap-2 [overflow-wrap:anywhere]">
      {showTitle && <h3 className="text-base font-semibold text-foreground"><DaemonErrorTitle /></h3>}
      <ShellTimeoutCommand params={error.params} />
      <p>{t("daemon.shell_environment_timeout.cause")}</p>
      <p>{consequence}</p>
      <p>{t("daemon.shell_environment_timeout.recovery")}</p>
    </div>
  );
}
