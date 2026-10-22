import { Disclosure } from "@heroui/react";
import React from "react";
import { useShallow } from "zustand/react/shallow";

import { DaemonRequestError } from "../daemon-client";
import { daemonParams, DETAIL_BLOCK_CODES, RECOVERY_CODES, hasTrustDiagnostic, leftBranchGroups, leftBranchItems, leftBranchName, richMessageCode, sharingFailureItems, sharingStateKey, sharingCauseGroups, sharingCauseKey, sharingFixKey, hasSharingReason, sharingReasonKey, sharingItemLabel, runningSessionIds, type LeftBranchItem, type SharingFailureItem } from "../daemonMessage";
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

export type DaemonErrorPart = "summary" | "details" | "recovery";

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
  if (!isMessageKey(id)) return part === "details" || part === "recovery" ? null : <>{error.message}</>;
  const left = leftBranchItems(error.code, error.params);
  if (left) return part === "recovery" ? null : <LeftBranches count={left.length} items={left} part={part} />;
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
  // A sharing failure says why in a line or two and folds the raw diagnostics away behind Details, whether it names
  // entries or only says why the change could not start (the failure's `detail`, or a repair's own message).
  const sharing = sharingFailureItems(error.code, error.params);
  const sharingFailure = error.code === "account_sharing_failed" || error.code === "account_sharing_repair_failed";
  const rawReason = error.code === "account_sharing_failed" ? error.params.detail : error.message;
  const sharingDetails = sharing
    ? <SharingFailures items={sharing} />
    : sharingFailure && hasSharingReason(error.params.cause, rawReason)
      ? <div className="flex min-w-0 flex-col gap-2"><p><SharingFailureReason cause={error.params.cause} detail={rawReason} /></p><SharingDetails items={[]} detail={rawReason} /></div>
      : null;
  const diagnostic = !sharingFailure && (DETAIL_BLOCK_CODES.has(error.code) || trustDiagnostic) && error.params.detail ? <DiagnosticBlock>{error.params.detail}</DiagnosticBlock> : null;
  const recoveryId = `daemon.${error.code}.recovery`;
  const recovery = RECOVERY_CODES.has(error.code) && isMessageKey(recoveryId) ? <p><Message id={recoveryId} params={params as never} /></p> : null;
  const nestedDetails = structuredNested && part !== "summary" ? <DaemonErrorMessage error={nested} part="details" /> : null;
  if (part === "recovery") return recovery;
  if (part === "details") return <>{sharingDetails}{diagnostic}{recovery}{nestedDetails}</>;
  // One element, so a container that lays its children out as flex items (an alert's column) keeps
  // the sentence whole instead of making each run of text and each literal an item of its own.
  const sentence = <span><Message id={id} params={params as never} /></span>;
  if (part === "summary" || (!sharingDetails && !diagnostic && !recovery && !nestedDetails)) return sentence;
  return (
    <div className="flex min-w-0 flex-col gap-2">
      {sentence}
      {sharingDetails}
      {diagnostic}
      {recovery}
      {nestedDetails}
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
export function SharingFailureReason({ cause, detail }: { cause?: string; detail?: string }): React.ReactElement | null {
  const reasonKey = sharingReasonKey(cause);
  if (reasonKey) return <SharingReason reason={<Message id={reasonKey} />} fix={sharingFixKey(cause)} />;
  return detail ? <Message id="daemon.sharing.fix.other" /> : null;
}

/** What a sharing failure left, said short: one line per cause naming the entries it hit ("You do not have permission
 * to change skills/ and 2 more."), a conversation by the name of a session that records it. Lines are spans, so the
 * summary fits where only inline content may go (an `Alert.Description`). Each line goes on with what to do about its
 * cause. */
export function SharingCauseSummary({ items }: { items: SharingFailureItem[] }): React.ReactElement {
  const title = useSessionTitles(items);
  return (
    <span className="flex min-w-0 flex-col gap-1">
      {sharingCauseGroups(items).map((group) => {
        const chips = group.items.map((item, at) => {
          const label = sharingItemLabel(item, title);
          return <InlineChip key={at} dir={label.session ? "auto" : "ltr"} monospace={!label.session} wrap size="inherit">{label.text}</InlineChip>;
        });
        const phrase = chips.length === 1 ? chips[0] : <Message id="daemon.sharing.items" params={{ first: chips[0], count: chips.length - 1 }} />;
        return <span key={group.cause}><SharingReason reason={<Message id={sharingCauseKey(group.cause)} params={{ items: phrase, count: chips.length } as never} />} fix={sharingFixKey(group.cause)} /></span>;
      })}
    </span>
  );
}

/** What the system said, as small code that runs on after the sentence it belongs to; a message of several lines keeps
 * them in a block under it. Compact, so a row stays one line. */
function RawDiagnostic({ children }: { children: string }): React.ReactElement {
  return children.trim().includes("\n")
    ? <div className="mt-1"><DiagnosticBlock>{children}</DiagnosticBlock></div>
    : <>{" "}<InlineChip dir="ltr" monospace wrap size="inherit">{children}</InlineChip></>;
}

/** The raw side of a sharing failure, folded away behind Details: each entry with what it was left as and what the
 * system said of it, or, with no entry, the one reason the change could not start. Nothing at all when there is
 * nothing raw. */
export function SharingDetails({ items, detail }: { items: SharingFailureItem[]; detail?: string }): React.ReactElement | null {
  const t = useT();
  const language = useCurrentLanguage();
  const title = useSessionTitles(items);
  if (items.length === 0 && !detail) return null;
  return (
    <Disclosure className="min-w-0 max-w-full">
      <Disclosure.Trigger className="flex items-center gap-1 text-sm font-medium text-foreground">
        {t("branchCleanup.details")}
        <Disclosure.Indicator className="ms-0" />
      </Disclosure.Trigger>
      <Disclosure.Content>
        <Disclosure.Body className="px-0 pb-0 pt-1">
          {items.length === 0 ? <RawDiagnostic>{detail!}</RawDiagnostic> : (
            <ul className="ms-4 flex min-w-0 list-disc flex-col gap-1">
              {items.map((item, index) => {
                const state = t(sharingStateKey(item));
                const name = <InlineChip dir="ltr" monospace wrap size="inherit">{item.item}</InlineChip>;
                return (
                  <li key={index}>
                    <span>{item.sessions.length === 0
                      ? <Message id="daemon.sharing.item.summary" params={{ item: name, state }} />
                      : <Message id="daemon.sharing.conversation.summary" params={{
                        item: name,
                        sessions: joinNodes(language, item.sessions, (id, key) => <InlineChip key={key} dir={title(id) !== undefined ? "auto" : "ltr"} monospace={title(id) === undefined} wrap size="inherit">{title(id) ?? id}</InlineChip>),
                        state,
                      }} />}</span>
                    {item.detail && <RawDiagnostic>{item.detail}</RawDiagnostic>}
                  </li>
                );
              })}
            </ul>
          )}
        </Disclosure.Body>
      </Disclosure.Content>
    </Disclosure>
  );
}

/** The cause lines with the folded diagnostics under them. */
function SharingFailures({ items }: { items: SharingFailureItem[] }): React.ReactElement {
  return (
    <div className="flex min-w-0 flex-col gap-2">
      <SharingCauseSummary items={items} />
      <SharingDetails items={items} />
    </div>
  );
}

/** A failure worded as `text` (in the current language), as a node: where its message sets some params
 * as code they are set as code, anywhere else it is the text itself. */
export function codedMessageNode(failure: { code: string; params: MessageParams }, text: string, part?: DaemonErrorPart): React.ReactNode {
  return hasCodeParams(failure) || (part && hasShellTimeoutDetails(failure)) ? <DaemonErrorMessage error={new DaemonRequestError(text, failure.code, failure.params)} part={part} /> : part === "details" || part === "recovery" ? null : text;
}

/** A failure as a param of a sentence: the structured message where it has one, otherwise its text
 * isolated, as any value put inside a sentence is. */
export function failureParam(error: ErrorPresentation): React.ReactNode {
  return typeof error !== "string" && (hasCodeParams(error) || hasShellTimeoutDetails(error)) ? <DaemonErrorMessage error={error} showTitle={false} part="summary" /> : isolate(errorText(error));
}

/** The login-shell command that timed out and for how long, which a shell-environment failure's details open with. */
export function ShellTimeoutCommand({ params: { shell, command, timeout } }: { params: MessageParams }): React.ReactElement {
  return (
    <p>
      <Message
        id="daemon.shell_environment_timeout.command"
        params={{ command: <InlineChip monospace wrap size="inherit">{`${shell} -l -i -c '${command}'`}</InlineChip>, timeout }}
      />
    </p>
  );
}

/** Structured failures retain their metadata; `summary` is inline-safe for Alert.Description,
 * while `details` holds only the remaining blocks. Omitting `part` renders the complete message. */
export function DaemonErrorMessage({ error, showTitle = true, part }: { error: ErrorPresentation; showTitle?: boolean; part?: DaemonErrorPart }): React.ReactElement | null {
  const t = useT();
  if (typeof error !== "string" && !hasShellTimeoutDetails(error) && hasCodeParams(error)) return <CodedDaemonMessage error={error} part={part} />;
  if (typeof error === "string" || !hasShellTimeoutDetails(error)) return part === "details" || part === "recovery" ? null : <>{errorText(error)}</>;
  if (part === "recovery") return <p>{t("daemon.shell_environment_timeout.recovery")}</p>;
  if (part === "summary") return <>{t("daemon.shell_environment_timeout.cause")}</>;
  return (
    <div className="flex min-w-0 flex-col gap-2 [overflow-wrap:anywhere]">
      {!part && showTitle && <h3 className="text-base font-semibold text-foreground"><DaemonErrorTitle /></h3>}
      <ShellTimeoutCommand params={error.params} />
      {!part && <p>{t("daemon.shell_environment_timeout.cause")}</p>}
      <p><Message id="daemon.shell_environment_timeout.consequence" params={{ appName: <InlineChip dir="auto" wrap size="inherit">{globalParam("appName")}</InlineChip> } as never} /></p>
      <p>{t("daemon.shell_environment_timeout.recovery")}</p>
    </div>
  );
}
