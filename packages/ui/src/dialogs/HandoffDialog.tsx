import React, { useId, useRef, useState } from "react";

import { raiseError } from "../components/alerts/raiseError";
import { useHandoffAfterRemoval } from "../components/focus/HandoffOnUnmount";
import { InlineReference } from "../i18n/InlineReference";
import { AFTER_FOCUS_MOVE_DELAY_MS, StatusAnnouncer } from "../components/announce/StatusAnnouncer";
import { RetryStatusMark } from "../components/status/RetryStatusMark";
import { useHasPainted } from "../components/motion/useHasPainted";
import { isHandoffAfterMove } from "../errors/errorRecovery";
import { Message, useT } from "../i18n/react";
import type { Console, Project, Session } from "../protocol";
import { sessionDisplayTitle } from "../sessions/sessionLabel";
import { useDaemon, useDaemonStore } from "../store";
import { useOctoboardTheme } from "../preferences/theme";
import { AgentAccountField } from "./AgentAccountField";
import { Dialog } from "../components/dialog/Dialog";
import { useDialogAction } from "../components/dialog/dialogHooks";
import { dialogAround } from "../components/dialog/dialogElement";
import { LaunchSettingsFields } from "./LaunchSettingsFields";
import { useFailureWhenBlocked } from "../components/dialog/useFailureWhenBlocked";
import { useConfirmIncompleteLaunch } from "./useConfirmIncompleteLaunch";
import { useLaunchChoice } from "./useLaunchChoice";
import { useResponsibleBranches } from "./useBranchInspections";

/**
 * Hands a session off to a new session of another agent (`handoff_session`). The fields are the
 * session dialog's: the agent, which is never the session's own (moving to another account of the
 * same agent is Switch account), joined with its account, and the model and reasoning effort,
 * which follow what a new session of that agent and account would take from its project or
 * console until changed. Above them it says what moves with the session. `handoff_session` is
 * answered with `session_opened` for the receiving session, which `onHandedOff` selects; a
 * refusal or a late failure is an error alert, and the dialog stays for another try. A failure after the work
 * has moved (`handoff_archive_failed`, `handoff_move_failed`) is not a failed try: the dialog closes and
 * selects the receiving session as for a success, and the error alert follows.
 */
export function HandoffDialog({ session, onClose, onHandedOff }: {
  session: Session;
  onClose: () => void;
  onHandedOff: (sessionId: string) => void;
}): React.ReactElement | null {
  const parentConsole = useDaemonStore((s) => s.consoles.get(session.console_id));
  const project = useDaemonStore((s) => session.project_id ? s.projects.get(session.project_id) : undefined);
  if (!parentConsole) return null;
  return <HandoffForm session={session} console={parentConsole} project={project} onClose={onClose} onHandedOff={onHandedOff} />;
}

function HandoffForm({ session, console: parentConsole, project, onClose, onHandedOff }: {
  session: Session;
  console: Console;
  project?: Project;
  onClose: () => void;
  onHandedOff: (sessionId: string) => void;
}): React.ReactElement {
  const t = useT();
  const { resolved: appearance } = useOctoboardTheme();
  const { request } = useDaemon();
  const { busy, run } = useDialogAction(undefined, t("dialog.handoff.failedTitle"));
  const { confirmLaunch, confirmation } = useConfirmIncompleteLaunch(onClose);
  const { choice, blocked, unavailable, overrides, agentOptions, setPickedAgent, account, launch } = useLaunchChoice(parentConsole, project, session.agent, undefined, undefined, session.kind === "project");

  // No other agent could take the session over (one was removed or uninstalled while this was open).
  const failure = useFailureWhenBlocked(blocked, t("dialog.handoff.noOtherAgent"), t("dialog.handoff.blockedTitle"));

  const submit = () => {
    void confirmLaunch(choice.account, () => void run(async () => {
      let reply;
      try {
        reply = await request({
          type: "handoff_session",
          session: session.id,
          agent: choice.agent,
          account: choice.account,
          terminal_appearance: appearance,
          ...overrides,
        });
      } catch (err) {
        // The receiving session is running and the work has moved, so the handoff is done whatever the rest of it
        // could not do: the dialog has nothing left to try again, and the alert, titled by its code, says what remains.
        const receiving = afterMoveReceiving(err);
        if (receiving === undefined) throw err;
        onHandedOff(receiving);
        onClose();
        // The title is the fallback: the code's own `alertTitle` wins over it. No `session`: the message names the
        // receiving session and refers to the original by role, and the two usually share a name.
        raiseError(err, { title: t("dialog.handoff.failedTitle") });
        return;
      }
      if (reply.type === "session_opened") onHandedOff(reply.session.id);
      onClose();
    }));
  };

  return (
    <Dialog
      title={t("dialog.handoff.title")}
      onClose={onClose}
      failure={failure}
      submitLabel={t("dialog.handoff.confirm")}
      busy={busy}
      submitDisabled={unavailable !== undefined}
      autoFocusSubmit
      onSubmit={submit}
    >
      <p className="text-sm"><Message id="dialog.handoff.subject" params={{ title: <InlineReference dir="auto" wrap>{sessionDisplayTitle(t, session)}</InlineReference> }} />{`${t("common.sentenceSeparator")}${t("dialog.handoff.message")}`}</p>
      <HandoffMoves session={session} busy={busy} />
      <AgentAccountField
        label={t("dialog.handoff.agent")}
        options={agentOptions}
        value={choice.agent}
        onChange={setPickedAgent}
        account={account}
        errorMessage={unavailable}
      />
      <LaunchSettingsFields
        {...launch}
        note={t(project ? "dialog.handoff.launchInheritanceNote" : "dialog.handoff.consoleLaunchInheritanceNote")}
      />
      {/* Inside this dialog's tree, though it shows in its own layer: a focus scope that is not
          nested in the one beneath it is bounced back to it. */}
      {confirmation}
    </Dialog>
  );
}

/** The receiving session of a handoff that failed once it was running and the work had moved (`handoff_archive_failed`,
 * `handoff_move_failed`); undefined for any other failure, which left nothing handed off. */
function afterMoveReceiving(error: unknown): string | undefined {
  return isHandoffAfterMove(error) ? error.params.receiving_session || undefined : undefined;
}

/** What moves to the receiving session along with the original's place, from what the window
 * already knows: the sessions it owns (archived ones included) and its owner, and for a project
 * session the branches it is responsible for, which the daemon's inspection of the branches bound
 * to a session finds. A console session has no branches, so it asks for none. While the handoff
 * runs the list is kept as it was, since the daemon's rebinding would otherwise empty it. The check
 * failing leaves the dialog doing its job (the handoff does not depend on it), so the failure is a
 * mark after the heading, its popover holding the reason and Try again, and the list says nothing of
 * branches. The failure arrives after the dialog opened, so the dialog's own `StatusAnnouncer` speaks
 * it, after the opening focus move, and again when a retry fails again. A retry that succeeds takes
 * the mark, and the Try again focused in its popover, away, so focus goes to the dialog. */
function HandoffMoves({ session, busy }: { session: Session; busy: boolean }): React.ReactElement {
  const t = useT();
  const headingId = useId();
  const owned = useDaemonStore((s) => Array.from(s.sessions.values()).filter((candidate) => candidate.bound_to === session.id).length);
  const owner = useDaemonStore((s) => session.bound_to ? s.sessions.get(session.bound_to) : undefined);
  const inspected = useResponsibleBranches(session.id, session.kind !== "console");
  const rootRef = useRef<HTMLDivElement>(null);
  const markId = useId();
  const handoffFrom = useHandoffAfterRemoval(() => dialogAround(rootRef));
  // The check fails after the dialog opened, so its mark is announced.
  const painted = useHasPainted();
  const live = {
    owned,
    ownerTitle: owner ? sessionDisplayTitle(t, owner) : undefined,
    branches: new Set(inspected.items.flatMap((item) => item.branch ? [item.branch] : [])).size,
  };
  const [frozen, setFrozen] = useState<typeof live>();
  if (busy && !frozen) setFrozen(live);
  if (!busy && frozen) setFrozen(undefined);
  const { owned: ownedCount, ownerTitle, branches } = frozen ?? live;
  const lines = [
    inspected.pending ? <li key="checking">{t("dialog.handoff.movesChecking")}</li> : undefined,
    branches > 0 ? <li key="branches">{t("dialog.handoff.movesBranches", { count: branches })}</li> : undefined,
    ownedCount > 0 ? <li key="owned">{t("dialog.handoff.movesSessions", { count: ownedCount })}</li> : undefined,
    ownerTitle !== undefined ? <li key="owner">{<Message id="dialog.handoff.movesOwner" params={{ owner: <InlineReference dir="auto" wrap>{ownerTitle}</InlineReference> }} />}</li> : undefined,
  ].filter(Boolean);
  return (
    <div ref={rootRef} className="flex flex-col gap-1 text-sm">
      <div className="flex items-center gap-1">
        <h3 id={headingId} className="font-semibold">{t("dialog.handoff.moves")}</h3>
        {inspected.error !== undefined && (
          <>
            {/* The check answers as a retry succeeds; focus the mark or its popover held goes to the dialog. */}
            {handoffFrom(`[data-status-mark-id="${markId}"]`)}
            <RetryStatusMark
              markId={markId}
              label={t("dialog.handoff.movesBranchesFailed.mark")}
              error={inspected.error}
              onRetry={inspected.retry}
              retrying={inspected.pending}
            >
              <p>{t("dialog.handoff.movesBranchesFailed")}</p>
            </RetryStatusMark>
          </>
        )}
      </div>
      {/* Empty while a retry is out, so a retry that fails again in the same words is written, and so spoken, again. */}
      <StatusAnnouncer text={painted && inspected.error !== undefined && !inspected.pending ? t("dialog.handoff.movesBranchesFailed") : undefined} firstWriteDelayMs={AFTER_FOCUS_MOVE_DELAY_MS} />
      {lines.length > 0
        ? <ul aria-labelledby={headingId} className="list-disc ps-5">{lines}</ul>
        : inspected.error !== undefined ? null : <p>{t("dialog.handoff.movesNothing")}</p>}
    </div>
  );
}
