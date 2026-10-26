import React, { useId, useRef, useState } from "react";

import { Button } from "../components/Button";
import { FailureBody } from "../components/errorParts";
import { HandOffOnUnmount } from "../components/HandOffOnUnmount";
import { InlineReference } from "../components/InlineReference";
import { AFTER_FOCUS_MOVE_DELAY_MS, StatusAnnouncer } from "../components/StatusAnnouncer";
import { StatusMark } from "../components/StatusMark";
import { useHasPainted } from "../components/useHasPainted";
import { retryCanHelp } from "../errorRecovery";
import { Message, useT } from "../i18n/react";
import type { Console, Project, Session } from "../protocol";
import { sessionDisplayTitle } from "../sessionLabel";
import { useDaemon, useDaemonStore } from "../store";
import { useOctoboardTheme } from "../theme";
import { AgentAccountField } from "./AgentAccountField";
import { Dialog, useDialogAction } from "./Dialog";
import { dialogAround } from "./dialogElement";
import { LaunchSettingsFields } from "./LaunchSettingsFields";
import { useFailureWhenBlocked } from "./useFailureWhenBlocked";
import { useConfirmIncompleteLaunch } from "./useConfirmIncompleteLaunch";
import { useLaunchChoice } from "./useLaunchChoice";
import { useResponsibleBranches } from "./useSessionBranches";

/**
 * Hands a session off to a new session of another agent (`hand_off_session`). The fields are the
 * session dialog's: the agent, which is never the session's own (moving to another account of the
 * same agent is Switch account), joined with its account, and the model and reasoning effort,
 * which follow what a new session of that agent and account would take from its project or
 * console until changed. Above them it says what moves with the session. `hand_off_session` is
 * answered with `session_opened` for the receiving session, which `onHandedOff` selects; a
 * refusal or a late failure is an error alert, and the dialog stays for another try.
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
  const { busy, run } = useDialogAction(undefined, t("dialog.handOff.failedTitle"));
  const { confirmLaunch, confirmation } = useConfirmIncompleteLaunch(onClose);
  const { choice, blocked, unavailable, overrides, agentOptions, setPickedAgent, account, launch } = useLaunchChoice(parentConsole, project, session.agent);

  // No other agent could take the session over (one was removed or uninstalled while this was open).
  const failure = useFailureWhenBlocked(blocked, t("dialog.handOff.noOtherAgent"), t("dialog.handOff.blockedTitle"));

  const submit = () => {
    void confirmLaunch(choice.account, () => void run(async () => {
      const reply = await request({
        type: "hand_off_session",
        session: session.id,
        agent: choice.agent,
        account: choice.account,
        terminal_appearance: appearance,
        ...overrides,
      });
      if (reply.type === "session_opened") onHandedOff(reply.session.id);
      onClose();
    }));
  };

  return (
    <Dialog
      title={t("dialog.handOff.title")}
      onClose={onClose}
      failure={failure}
      submitLabel={t("dialog.handOff.confirm")}
      busy={busy}
      submitDisabled={unavailable !== undefined}
      autoFocusSubmit
      onSubmit={submit}
    >
      <p className="text-sm"><Message id="dialog.handOff.subject" params={{ title: <InlineReference dir="auto" wrap>{sessionDisplayTitle(t, session)}</InlineReference> }} /> {t("dialog.handOff.message")}</p>
      <HandoffMoves session={session} busy={busy} />
      <AgentAccountField
        label={t("dialog.handOff.agent")}
        options={agentOptions}
        value={choice.agent}
        onChange={setPickedAgent}
        account={account}
        errorMessage={unavailable}
      />
      <LaunchSettingsFields
        {...launch}
        note={t(project ? "dialog.handOff.launchInheritanceNote" : "dialog.handOff.consoleLaunchInheritanceNote")}
      />
      {/* Inside this dialog's tree, though it shows in its own layer: a focus scope that is not
          nested in the one beneath it is bounced back to it. */}
      {confirmation}
    </Dialog>
  );
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
  const inspected = useResponsibleBranches(session.id, session.role !== "console");
  const rootRef = useRef<HTMLDivElement>(null);
  const markId = useId();
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
    inspected.pending ? <li key="checking">{t("dialog.handOff.movesChecking")}</li> : undefined,
    branches > 0 ? <li key="branches">{t("dialog.handOff.movesBranches", { count: branches })}</li> : undefined,
    ownedCount > 0 ? <li key="owned">{t("dialog.handOff.movesSessions", { count: ownedCount })}</li> : undefined,
    ownerTitle !== undefined ? <li key="owner">{<Message id="dialog.handOff.movesOwner" params={{ owner: <InlineReference dir="auto" wrap>{ownerTitle}</InlineReference> }} />}</li> : undefined,
  ].filter(Boolean);
  return (
    <div ref={rootRef} className="flex flex-col gap-1 text-sm">
      <div className="flex items-center gap-1">
        <h3 id={headingId} className="font-semibold">{t("dialog.handOff.moves")}</h3>
        {inspected.error !== undefined && (
          <>
            {/* The check answers as a retry succeeds; focus the mark or its popover held goes to the dialog. */}
            <HandOffOnUnmount from={`[data-status-mark-id="${markId}"]`} to={() => dialogAround(rootRef)?.focus()} />
            <StatusMark
              markId={markId}
              label={t("dialog.handOff.movesBranchesFailed.mark")}
              actions={retryCanHelp(inspected.error) ? (close) => (
                <Button size="sm" variant="secondary" preventFocusOnPress isPending={inspected.pending} onPress={() => { close(); inspected.retry(); }}>
                  {t("error.tryAgain")}
                </Button>
              ) : undefined}
            >
              <p>{t("dialog.handOff.movesBranchesFailed")}</p>
              <FailureBody error={inspected.error} />
            </StatusMark>
          </>
        )}
      </div>
      {/* Empty while a retry is out, so a retry that fails again in the same words is written, and so spoken, again. */}
      <StatusAnnouncer text={painted && inspected.error !== undefined && !inspected.pending ? t("dialog.handOff.movesBranchesFailed") : undefined} firstWriteDelayMs={AFTER_FOCUS_MOVE_DELAY_MS} />
      {lines.length > 0
        ? <ul aria-labelledby={headingId} className="list-disc ps-5">{lines}</ul>
        : inspected.error !== undefined ? null : <p>{t("dialog.handOff.movesNothing")}</p>}
    </div>
  );
}
