import { CircleAlert, CircleCheck, ListX } from "lucide-react";
import React, { useRef } from "react";

import { DaemonErrorMessage } from "../components/alerts/DaemonErrorMessage";
import { DetailsButton } from "../components/alerts/DetailsButton";
import { errorDetails, errorRemedy } from "../components/alerts/errorParts";
import { FormCard } from "../components/card/FormCard";
import { useHandoffAfterRemoval } from "../components/focus/HandoffOnUnmount";
import { useHasPainted } from "../components/motion/useHasPainted";
import { EmptyPanel } from "../components/panel/EmptyPanel";
import { InlineSessionLink } from "../components/text/InlineSessionLink";
import { useFailureNode } from "../components/alerts/failureNode";
import { retryCanHelp } from "../errors/errorRecovery";
import type { PlainMessageKey } from "../i18n/catalog";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import type { ScheduledRunTrigger, ScheduledTaskRun, Session } from "../protocol";
import { useSession } from "../store";
import { knownValues } from "../values/knownValue";
import { formatInstant } from "./scheduleText";
import type { useScheduledTaskRuns } from "./useScheduledTaskRuns";
import { useTaskZone } from "./useTaskZone";

/** The runs of a task as `useScheduledTaskRuns` hands them to the page. */
export type TaskRuns = ReturnType<typeof useScheduledTaskRuns>;

/** The triggers this client words; another one, from a newer daemon, leaves the run's trigger blank. */
const isKnownTrigger = knownValues<ScheduledRunTrigger>({ scheduled: true, catch_up: true, run_now: true });

const TRIGGER_LABELS: Record<ScheduledRunTrigger, PlainMessageKey> = {
  scheduled: "scheduledTasks.runs.trigger.scheduled",
  catch_up: "scheduledTasks.runs.trigger.catchUp",
  run_now: "scheduledTasks.runs.trigger.runNow",
};

/**
 * The Run history view of a saved task: its kept runs (`runs`, listed by the page), most recent first, each with when it
 * ran, what started it (its schedule, a catch-up or Run now) and what came of it: the session it started, as a link
 * that shows that session (`onOpenSession`, which leaves the page), or the failure, whose reason, remedy and further
 * details are behind a Details button. A run whose session this window no longer holds says so, without a link; one
 * that started its session without the console session it was to report to says that too. Until the runs have been
 * listed nothing is drawn, and a failure to list them takes the history's place as the failure of a whole subject, with
 * Try again; with none, it says there are none yet. Each run leads with a small mark of what came of it, which its
 * words say too.
 */
export function ScheduledTaskRuns({ runs: listing, onOpenSession }: { runs: TaskRuns; onOpenSession: (session: Session) => void }): React.ReactElement {
  const t = useT();
  const { runs, failure } = listing;
  const painted = useHasPainted();
  // The failure goes, with the Try again that was pressed, when a retry succeeds: focus held on it goes to the tab of the
  // view, which is always there, the ring following the last input.
  const root = useRef<HTMLDivElement>(null);
  const handoffFrom = useHandoffAfterRemoval(() => {
    const labelledBy = root.current?.closest("[role=tabpanel]")?.getAttribute("aria-labelledby");
    return labelledBy ? document.getElementById(labelledBy) : undefined;
  });
  if (runs === undefined && failure) {
    const retry = retryCanHelp(failure.message) ? { label: t("error.tryAgain"), variant: "primary" as const, isPending: listing.pending, onPress: listing.retry } : undefined;
    return (
      <div data-runs-failure="">
        {handoffFrom("[data-runs-failure]")}
        <FormCard>
          <EmptyPanel
            tone="danger"
            icon={CircleAlert}
            headingLevel={2}
            message={t("scheduledTasks.runs.listFailedTitle")}
            description={<DaemonErrorMessage error={failure.message} part="summary" />}
            remedy={errorRemedy(failure.message)}
            details={errorDetails(failure.message)}
            action={retry}
            announce={painted}
            announceKey={failure.attempt}
          />
        </FormCard>
      </div>
    );
  }
  return (
    <div ref={root}>
      {runs !== undefined && (
        <FormCard>
          {runs.length === 0 ? (
            <EmptyPanel compact icon={ListX} message={t("scheduledTasks.runs.empty")} />
          ) : (
            <ul className="-my-1 flex flex-col">
              {runs.map((run) => (
                <li key={run.id} className="flex min-w-0 gap-3 py-3 [&:not(:first-child)]:hairline-t">
                  <RunMark kind={run.outcome.kind} />
                  <div className="flex min-w-0 flex-1 flex-col gap-1">
                    <RunEntry run={run} onOpenSession={onOpenSession} />
                  </div>
                </li>
              ))}
            </ul>
          )}
        </FormCard>
      )}
    </div>
  );
}

/** The mark a run leads with: a check for one that started its session, an alert for one that could not; its place is
 * kept for an outcome this client does not know, so the runs' words stay in one column. */
function RunMark({ kind }: { kind: string }): React.ReactElement {
  return (
    <span aria-hidden="true" className="flex h-5 w-4 shrink-0 items-center">
      {kind === "started" && <CircleCheck className="size-4 text-success-soft-foreground" />}
      {kind === "failed" && <CircleAlert className="size-4 text-danger-soft-foreground" />}
    </span>
  );
}

function RunEntry({ run, onOpenSession }: { run: ScheduledTaskRun; onOpenSession: (session: Session) => void }): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const zone = useTaskZone();
  const trigger = isKnownTrigger(run.trigger) ? t(TRIGGER_LABELS[run.trigger]) : undefined;
  return (
    <>
      <p className="flex min-w-0 flex-wrap items-baseline gap-x-2 text-sm">
        <span className="font-medium">{formatInstant(language, run.ran_at, zone)}</span>
        {trigger && <span className="text-app-muted">{trigger}</span>}
      </p>
      <RunOutcome run={run} onOpenSession={onOpenSession} />
    </>
  );
}

/** What came of a run; nothing for an outcome a newer daemon names that this client does not know. */
function RunOutcome({ run, onOpenSession }: { run: ScheduledTaskRun; onOpenSession: (session: Session) => void }): React.ReactElement | null {
  const outcome = run.outcome;
  switch (outcome.kind) {
    case "started":
      return <StartedOutcome sessionId={outcome.session} independent={outcome.started_independent} onOpenSession={onOpenSession} />;
    case "failed":
      return <FailedOutcome failure={outcome} />;
    default:
      return null;
  }
}

function StartedOutcome({ sessionId, independent, onOpenSession }: { sessionId: string; independent: boolean; onOpenSession: (session: Session) => void }): React.ReactElement {
  const t = useT();
  // An archived session is still held, and shows its history; a removed one is not.
  const session = useSession(sessionId);
  return (
    <div className="flex min-w-0 flex-col gap-0.5 text-sm">
      {session ? (
        <p className="flex min-w-0 whitespace-pre">
          <Message id="scheduledTasks.runs.started" params={{ session: <InlineSessionLink session={session} onOpen={onOpenSession} /> }} />
        </p>
      ) : (
        <p className="text-app-muted">{t("scheduledTasks.runs.startedGone")}</p>
      )}
      {independent && <p className="text-app-muted">{t("scheduledTasks.runs.startedIndependent")}</p>}
    </div>
  );
}

/** A run that started no session: the failure mark's words, in plain text (the row's mark carries the colour), as the row's outcome, and a Details button whose popover holds
 * the reason in the application's own words, the one main thing to do about it, and what more there is (further
 * explanation, the raw text). No reason is written into the row. */
function FailedOutcome({ failure }: { failure: Extract<ScheduledTaskRun["outcome"], { kind: "failed" }> }): React.ReactElement {
  const t = useT();
  const failureNode = useFailureNode();
  const remedy = failureNode(failure, "remedy");
  const details = failureNode(failure, "details");
  return (
    <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 text-sm">
      <p className="min-w-0 font-medium [overflow-wrap:anywhere]">{t("scheduledTasks.runs.failed")}</p>
      <DetailsButton>
        <p className="min-w-0 text-foreground [overflow-wrap:anywhere]" dir="auto">{failureNode(failure, "summary")}</p>
        {remedy && <p className="min-w-0 [overflow-wrap:anywhere]" dir="auto">{remedy}</p>}
        {details}
      </DetailsButton>
    </div>
  );
}
