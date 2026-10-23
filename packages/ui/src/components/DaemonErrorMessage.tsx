import { Disclosure } from "@heroui/react";
import React from "react";
import { useShallow } from "zustand/react/shallow";

import { DaemonRequestError } from "../daemon-client";
import { daemonParams, DETAIL_BLOCK_CODES, RECOVERY_CODES, hasTrustDiagnostic, leftBranchGroups, leftBranchItems, leftBranchName, richMessageCode, sharingFailureItems, sharingStateKey, sharingCauseGroups, sharingCauseKey, sharingFixKey, hasSharingReason, sharingReasonKey, sharingItemLabel, runningSessionIds, type LeftBranchItem, type SharingFailureItem } from "../daemonMessage";
import { hasCodeParams, hasShellTimeoutDetails, isUncataloguedFailure, type ErrorPresentation } from "../errorPresentation";
import { errorText } from "../errorText";
import { globalParam, isMessageKey } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import { joinNodes } from "../i18n/joinPhrases";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import type { MessageParams, SharingEntryCause } from "../protocol";
import { sessionTask } from "../sessionLabel";
import { useErrorRecords } from "../store";
import { BranchGroups } from "./BranchGroups";
import { DiagnosticBlock } from "./DiagnosticBlock";
import { InlineReference } from "./InlineReference";

export function DaemonErrorTitle(): React.ReactElement {
  const t = useT();
  return <>{t("daemon.shell_environment_timeout.title")}</>;
}

const NO_RECORDS = new Map<string, never>();

/** The three layers of a failure: `summary` is what happened and `remedy` the one main thing to do about
 * it, both in the application's own words and always visible; `details` is what the Details button
 * holds: first any further explanation in the application's own words (the catalog's `.details` line, a reason per cause with its
 * fix, the lead lines of several groups), then the raw text that was not written for the reader (a
 * diagnostic block, the system's own message, raw entries). */
export type DaemonErrorPart = "summary" | "remedy" | "details";

/** What a `session_branches_not_removed` alert names, grouped by why each is still there: a lead
 * line per group over a list of names as code chips, with a locked worktree's uncommitted changes
 * and git's own message for one that failed. With a single group its lead says it all, and the
 * line over the groups is left out. All of it is Details: the summary is the line over the groups, or
 * the one group's lead, and there is no remedy. */
function LeftBranches({ count, items, part }: { count: number; items: LeftBranchItem[]; part?: DaemonErrorPart }): React.ReactElement | null {
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
  if (part === "remedy") return null;
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
  if (!isMessageKey(id)) return part === "details" || part === "remedy" ? null : <>{error.message}</>;
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
  if (error.code === "account_quota_exhausted" && error.params.window && !["5-hour", "Weekly", "Monthly", "Credits"].includes(error.params.window)) {
    params.quota = <Message id="quota.windowPhrase.named" params={{ window: <InlineReference dir="auto" wrap>{error.params.window}</InlineReference> }} />;
  }
  const running = error.code === "session_has_running_sessions" ? runningSessionIds(error.params) : undefined;
  if (running) {
    params.sessions = joinNodes(language, running, (id, key) => <InlineReference key={key} dir={sessions.has(id) ? "auto" : "ltr"} monospace={!sessions.has(id)} wrap>{sessions.get(id)?.title ?? id}</InlineReference>);
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
  // A sharing failure says why, and what to do about each cause, ahead of the raw text in Details (a
  // disclosure, or the alert's popover), and the raw entries and the system's own reason follow it,
  // whether it names entries or only says why the change could not start (the failure's `detail`, or a
  // repair's own message). A cause with no line of its own points at the raw reason, which is only
  // useful where the raw reason is folded away, so Details leaves that pointer out.
  const sharing = sharingFailureItems(error.code, error.params);
  const sharingFailure = error.code === "account_sharing_failed" || error.code === "account_sharing_repair_failed";
  const rawReason = error.code === "account_sharing_failed" ? error.params.detail : error.message;
  const folded = part !== "details";
  const pointedAt = folded ? rawReason : undefined;
  const reasonLines = sharing
    ? <SharingCauseSummary items={sharing} />
    : sharingFailure && hasSharingReason(error.params.cause, pointedAt)
      ? <span className="block"><SharingFailureReason cause={error.params.cause} detail={pointedAt} /></span>
      : null;
  const sharingRaw = sharing || (sharingFailure && rawReason) ? <SharingDetails items={sharing ?? []} detail={rawReason} folded={folded} /> : null;
  const diagnostic = !sharingFailure && (DETAIL_BLOCK_CODES.has(error.code) || trustDiagnostic) && error.params.detail ? <DiagnosticBlock>{error.params.detail}</DiagnosticBlock> : null;
  const recoveryId = `daemon.${error.code}.recovery`;
  const recovery = RECOVERY_CODES.has(error.code) && isMessageKey(recoveryId) ? <span className="block"><Message id={recoveryId} params={params as never} /></span> : null;
  // The application's own further explanation, which leads Details, ahead of the raw text.
  const furtherId = `daemon.${error.code}.details`;
  const further = isMessageKey(furtherId) ? <span className="block"><Message id={furtherId} params={params as never} /></span> : null;
  // The nested cause's recovery is the failure's own, a launch failure having none of its own.
  const remedy = recovery ?? (structuredNested && hasErrorRemedy(nested) ? <DaemonErrorMessage error={nested} part="remedy" /> : null);
  const nestedFurther = structuredNested ? <DaemonErrorMessage error={nested} part="details" /> : null;
  if (part === "details") return <>{further}{reasonLines}{sharingRaw}{diagnostic}{nestedFurther}</>;
  if (part === "remedy") return remedy;
  // One element, so a container that lays its children out as flex items (an alert's column) keeps
  // the sentence whole instead of making each run of text and each literal an item of its own.
  const sentence = <span><Message id={id} params={params as never} /></span>;
  if (part === "summary" || (!hasErrorRemedy(error) && !hasErrorDetails(error))) return sentence;
  return (
    <div className="flex min-w-0 flex-col gap-2">
      {sentence}
      {remedy}
      {further}
      {reasonLines}
      {sharingRaw}
      {diagnostic}
      {nestedFurther}
    </div>
  );
}

/** The names of the sessions with these ids, read through a selector that changes only when one of those names does. */
function useSessionTitles(items: SharingFailureItem[]): (id: string) => string | undefined {
  const ids = [...new Set(items.flatMap((item) => item.sessions))];
  const titles = useErrorRecords(useShallow((s) => ids.map((id) => s.sessions.get(id)?.title)));
  return (id) => titles[ids.indexOf(id)];
}

/** A reason a sharing change did not finish with what to do about it after it. The catalog joins the two, since what
 * stands between two sentences is the language's to say. */
function SharingReason({ reason, fix }: { reason: React.ReactNode; fix?: ReturnType<typeof sharingFixKey> }): React.ReactElement {
  return fix ? <Message id="daemon.sharing.withFix" params={{ cause: reason, fix: <Message id={fix} /> }} /> : <>{reason}</>;
}

/** Why a change failed as a whole, with no entry to name, and what to do about it: the cause's own line, or, for a
 * cause with none (`other`), pointing at the raw reason in Details when there is one. Nothing when there is neither. */
export function SharingFailureReason({ cause, detail, withFix = true }: { cause?: string; detail?: string; withFix?: boolean }): React.ReactElement | null {
  const reasonKey = sharingReasonKey(cause);
  if (reasonKey) return <SharingReason reason={<Message id={reasonKey} />} fix={withFix ? sharingFixKey(cause) : undefined} />;
  return detail ? <Message id="daemon.sharing.fix.other" /> : null;
}

/** One cause of a sharing failure as a line naming the entries it hit ("You do not have permission to change
 * skills/ and 2 more."), a conversation by the name of a session that records it, and, unless `withFix` is false,
 * going on with what to do about it. */
export function SharingCauseLine({ group, withFix = true }: { group: { cause: SharingEntryCause; items: SharingFailureItem[] }; withFix?: boolean }): React.ReactElement {
  const title = useSessionTitles(group.items);
  const chips = group.items.map((item, at) => {
    const label = sharingItemLabel(item, title);
    return <InlineReference key={at} dir={label.session ? "auto" : "ltr"} monospace={!label.session} wrap>{label.text}</InlineReference>;
  });
  const phrase = chips.length === 1 ? chips[0] : <Message id="daemon.sharing.items" params={{ first: chips[0], count: chips.length - 1 }} />;
  return <SharingReason reason={<Message id={sharingCauseKey(group.cause)} params={{ items: phrase, count: chips.length } as never} />} fix={withFix ? sharingFixKey(group.cause) : undefined} />;
}

/** What a sharing failure left, said short: one line per cause naming the entries it hit ("You do not have permission
 * to change skills/ and 2 more."), a conversation by the name of a session that records it. Each line goes on with
 * what to do about its cause. It is the further explanation at the top of Details, above the raw entries. */
export function SharingCauseSummary({ items }: { items: SharingFailureItem[] }): React.ReactElement {
  return (
    <span className="flex min-w-0 flex-col gap-1">
      {sharingCauseGroups(items).map((group) => <span key={group.cause}><SharingCauseLine group={group} /></span>)}
    </span>
  );
}

/** What the system said. After a sentence it runs on as a small code chip; alone on its line (`alone`) it is plain
 * monospace text, since a chip is for a reference within copy. A message of several lines keeps them in a block.
 * Compact, so a row stays one line. */
function RawDiagnostic({ children, alone = false }: { children: string; alone?: boolean }): React.ReactElement {
  if (children.trim().includes("\n")) return <div className="mt-1"><DiagnosticBlock>{children}</DiagnosticBlock></div>;
  if (alone) return <code dir="ltr" className="block font-mono text-xs [overflow-wrap:anywhere]">{children}</code>;
  return <>{" "}<InlineReference dir="ltr" monospace wrap>{children}</InlineReference></>;
}

/** The raw side of a sharing failure: each entry with what it was left as and what the system said of it, or, with
 * no entry, the one reason the change could not start. Folded away behind a Details disclosure unless `folded` is
 * false, which is for a place that is itself the Details (an alert's popover). Nothing at all when there is nothing
 * raw. */
export function SharingDetails({ items, detail, folded = true }: { items: SharingFailureItem[]; detail?: string; folded?: boolean }): React.ReactElement | null {
  const t = useT();
  const language = useCurrentLanguage();
  const title = useSessionTitles(items);
  if (items.length === 0 && !detail) return null;
  const body = items.length === 0 ? <RawDiagnostic alone>{detail!}</RawDiagnostic> : (
    <ul className="ms-4 flex min-w-0 list-disc flex-col gap-1">
      {items.map((item, index) => {
        const state = t(sharingStateKey(item));
        const name = <InlineReference dir="ltr" monospace wrap>{item.item}</InlineReference>;
        return (
          <li key={index}>
            <span>{item.sessions.length === 0
              ? <Message id="daemon.sharing.item.summary" params={{ item: name, state }} />
              : <Message id="daemon.sharing.conversation.summary" params={{
                item: name,
                sessions: joinNodes(language, item.sessions, (id, key) => <InlineReference key={key} dir={title(id) !== undefined ? "auto" : "ltr"} monospace={title(id) === undefined} wrap>{title(id) ?? id}</InlineReference>),
                state,
              }} />}</span>
            {item.detail && <RawDiagnostic>{item.detail}</RawDiagnostic>}
          </li>
        );
      })}
    </ul>
  );
  if (!folded) return body;
  return (
    <Disclosure className="min-w-0 max-w-full">
      <Disclosure.Trigger className="flex items-center gap-1 text-sm font-medium text-foreground">
        {t("error.details")}
        <Disclosure.Indicator className="ms-0" />
      </Disclosure.Trigger>
      <Disclosure.Content>
        <Disclosure.Body className="px-0 pb-0 pt-1">{body}</Disclosure.Body>
      </Disclosure.Content>
    </Disclosure>
  );
}

/** Whether `DaemonErrorMessage` has anything for `part="details"` of this failure: the same conditions
 * `CodedDaemonMessage` builds those blocks under, answered without rendering, so an alert knows whether
 * it has a Details button to offer. What counts is further explanation in the application's own words
 * (a reason for the cause, the lead of a group of names) and raw text: a diagnostic block, the entries
 * of a failure, the system's own message of a failure the application has no words for. */
export function hasErrorDetails(error: { code: string; params: MessageParams; message: string }): boolean {
  if (isUncataloguedFailure(error)) return error.message !== "";
  if (hasShellTimeoutDetails(error)) return true;
  if (!hasCodeParams(error) || !isMessageKey(`daemon.${richMessageCode(error.code, error.params)}`)) return false;
  if (leftBranchItems(error.code, error.params)) return true;
  if (isMessageKey(`daemon.${error.code}.details`)) return true;
  const sharingFailure = error.code === "account_sharing_failed" || error.code === "account_sharing_repair_failed";
  const rawReason = error.code === "account_sharing_failed" ? error.params.detail : error.message;
  if (sharingFailureItems(error.code, error.params) || (sharingFailure && (rawReason || sharingReasonKey(error.params.cause)))) return true;
  if (!sharingFailure && (DETAIL_BLOCK_CODES.has(error.code) || hasTrustDiagnostic(error.code, error.params)) && error.params.detail) return true;
  const nested = nestedFailure(error);
  return nested !== undefined && hasErrorDetails(nested);
}

/** Whether `DaemonErrorMessage` has anything for `part="remedy"`: the failure's recovery sentence, the one
 * main thing to do about it, answered without rendering. */
export function hasErrorRemedy(error: { code: string; params: MessageParams; message: string }): boolean {
  if (isUncataloguedFailure(error)) return false;
  if (hasShellTimeoutDetails(error)) return true;
  if (!hasCodeParams(error) || !isMessageKey(`daemon.${richMessageCode(error.code, error.params)}`)) return false;
  if (RECOVERY_CODES.has(error.code) && isMessageKey(`daemon.${error.code}.recovery`)) return true;
  const nested = nestedFailure(error);
  return nested !== undefined && hasErrorRemedy(nested);
}

/** The structured cause a `launch_failed` carries under its own code, as a failure of its own. */
function nestedFailure(error: { code: string; params: MessageParams }): { code: string; params: MessageParams; message: string } | undefined {
  const { reason_code: reason } = error.params;
  if (error.code !== "launch_failed" || !reason || reason === "launch_failed") return undefined;
  const nested = { code: reason, params: error.params, message: error.params.reason ?? "" };
  return hasCodeParams(nested) || hasShellTimeoutDetails(nested) ? nested : undefined;
}

/** The details of a failure for an alert's Details popover (further explanation, then raw text), or
 * nothing when there is neither. */
export function errorDetails(error: ErrorPresentation): React.ReactNode {
  return typeof error !== "string" && hasErrorDetails(error) ? <DaemonErrorMessage error={error} part="details" /> : undefined;
}

/** The one main thing to do about a failure, in the application's own words, shown in the alert's
 * description and never behind Details, or nothing when the failure has none. */
export function errorRemedy(error: ErrorPresentation): React.ReactNode {
  return typeof error !== "string" && hasErrorRemedy(error) ? <DaemonErrorMessage error={error} part="remedy" /> : undefined;
}

/** The popover body of a failure mark, which is itself the details surface: the sentence (`summary`, or
 * the failure's own), the main remedy under it, then the details (further explanation, raw text last). */
export function FailureBody({ error, summary }: { error: ErrorPresentation; summary?: React.ReactNode }): React.ReactElement {
  const remedy = errorRemedy(error);
  return (
    <>
      <p>{summary ?? <DaemonErrorMessage error={error} part="summary" />}</p>
      {remedy && <p>{remedy}</p>}
      {errorDetails(error)}
    </>
  );
}

/** `FailureBody` for a failure worded as `text` in the current language (see `codedMessageNode`). */
export function CodedFailureBody({ failure, text }: { failure: { code: string; params: MessageParams }; text: string }): React.ReactElement {
  return <FailureBody error={new DaemonRequestError(text, failure.code, failure.params)} summary={codedMessageNode(failure, text, "summary")} />;
}

/** A failure worded as `text` (in the current language), as a node: where its message sets some params
 * as code they are set as code, anywhere else it is the text itself. A failure the application has no
 * words for is its generic sentence as a `summary`, and `text`, which is then the raw message, goes to
 * the `details`. */
export function codedMessageNode(failure: { code: string; params: MessageParams }, text: string, part?: DaemonErrorPart): React.ReactNode {
  const error = new DaemonRequestError(text, failure.code, failure.params);
  if (part === "details") return errorDetails(error) ?? null;
  if (part === "remedy") return errorRemedy(error) ?? null;
  if (part && isUncataloguedFailure(failure)) return <DaemonErrorMessage error={error} part={part} />;
  return hasCodeParams(failure) || (part && hasShellTimeoutDetails(failure)) ? <DaemonErrorMessage error={error} part={part} /> : text;
}

/** A failure as a param of a sentence: the structured message where it has one, otherwise its text
 * isolated, as any value put inside a sentence is. */
export function failureParam(error: ErrorPresentation): React.ReactNode {
  return typeof error !== "string" && (hasCodeParams(error) || hasShellTimeoutDetails(error) || isUncataloguedFailure(error)) ? <DaemonErrorMessage error={error} showTitle={false} part="summary" /> : isolate(errorText(error));
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

/** Structured failures retain their metadata; `summary` is inline-safe for Alert.Description, `remedy`
 * holds the one main thing to do (also inline-safe) and `details` the further explanation, then the raw blocks. Omitting
 * `part` renders the complete message. A failure the application has no words for shows its generic
 * sentence as the `summary` and the raw message as its `details`. */
export function DaemonErrorMessage({ error, showTitle = true, part }: { error: ErrorPresentation; showTitle?: boolean; part?: DaemonErrorPart }): React.ReactElement | null {
  const t = useT();
  if (part && isUncataloguedFailure(error)) {
    if (part === "summary") return <>{t("error.unexpected.summary")}</>;
    return part === "details" && errorText(error) !== "" ? <RawDiagnostic alone>{errorText(error)}</RawDiagnostic> : null;
  }
  if (typeof error !== "string" && !hasShellTimeoutDetails(error) && hasCodeParams(error)) return <CodedDaemonMessage error={error} part={part} />;
  if (typeof error === "string" || !hasShellTimeoutDetails(error)) return part === "summary" || !part ? <>{errorText(error)}</> : null;
  if (part === "summary") return <>{t("daemon.shell_environment_timeout.cause")}</>;
  const consequence = <Message id="daemon.shell_environment_timeout.consequence" params={{ appName: <InlineReference dir="auto" wrap>{globalParam("appName")}</InlineReference> } as never} />;
  if (part === "remedy") return <span className="block">{t("daemon.shell_environment_timeout.recovery")}</span>;
  if (part === "details") return <><p>{consequence}</p><ShellTimeoutCommand params={error.params} /></>;
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
