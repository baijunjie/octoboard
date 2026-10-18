import React, { useId, useState } from "react";

import { InlineChip } from "../components/InlineChip";
import { Message, useT } from "../i18n/react";
import { isolate } from "../i18n/isolate";
import type { Console, Project, Session } from "../protocol";
import { sessionDisplayTitle } from "../sessionLabel";
import { useDaemon, useDaemonStore } from "../store";
import { AgentAccountField } from "./AgentAccountField";
import { Dialog, DialogError, useDialogAction } from "./Dialog";
import { LaunchSettingsFields } from "./LaunchSettingsFields";
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
  const { request } = useDaemon();
  const { error, busy, run } = useDialogAction();
  const { choice, blocked, overrides, agentOptions, setPickedAgent, account, launch } = useLaunchChoice(parentConsole, project, session.agent);

  const submit = () => {
    void run(async () => {
      const reply = await request({
        type: "hand_off_session",
        session: session.id,
        agent: choice.agent,
        account: choice.account,
        ...overrides,
      });
      if (reply.type === "session_opened") onHandedOff(reply.session.id);
      onClose();
    });
  };

  return (
    <Dialog
      title={<Message id="dialog.handOff.title" params={{ title: <InlineChip dir="auto" wrap size="inherit">{sessionDisplayTitle(t, session)}</InlineChip> }} />}
      onClose={onClose}
      submitLabel={t("dialog.handOff.confirm")}
      busy={busy}
      submitDisabled={blocked}
      autoFocusSubmit
      onSubmit={submit}
    >
      <p className="text-sm">{t("dialog.handOff.message")}</p>
      <HandoffMoves session={session} busy={busy} />
      {blocked && <p className="text-sm text-app-danger">{t("dialog.handOff.noOtherAgent")}</p>}
      <AgentAccountField
        label={t("dialog.handOff.agent")}
        options={agentOptions}
        value={choice.agent}
        onChange={setPickedAgent}
        isDisabled={blocked}
        account={account}
      />
      <LaunchSettingsFields
        {...launch}
        note={t(project ? "dialog.handOff.launchSettingsNote" : "dialog.handOff.consoleLaunchSettingsNote")}
        isDisabled={blocked}
      />
      <DialogError message={error} />
    </Dialog>
  );
}

/** What moves to the receiving session along with the original's place, from what the window
 * already knows: the sessions it owns (archived ones included) and its owner, and for a project
 * session the branches it is responsible for, which the daemon's inspection of the branches bound
 * to a session finds. A console session has no branches, so it asks for none. While the handoff
 * runs the list is kept as it was, since the daemon's rebinding would otherwise empty it. */
function HandoffMoves({ session, busy }: { session: Session; busy: boolean }): React.ReactElement {
  const t = useT();
  const headingId = useId();
  const owned = useDaemonStore((s) => Array.from(s.sessions.values()).filter((candidate) => candidate.bound_to === session.id).length);
  const owner = useDaemonStore((s) => session.bound_to ? s.sessions.get(session.bound_to) : undefined);
  const inspected = useResponsibleBranches(session.id, session.role !== "console");
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
    inspected.failed ? <li key="failed">{t("dialog.handOff.movesBranchesFailed")}</li> : undefined,
    branches > 0 ? <li key="branches">{t("dialog.handOff.movesBranches", { count: branches })}</li> : undefined,
    ownedCount > 0 ? <li key="owned">{t("dialog.handOff.movesSessions", { count: ownedCount })}</li> : undefined,
    ownerTitle !== undefined ? <li key="owner">{t("dialog.handOff.movesOwner", { owner: isolate(ownerTitle) })}</li> : undefined,
  ].filter(Boolean);
  return (
    <div className="flex flex-col gap-1 text-sm">
      <h3 id={headingId} className="font-semibold">{t("dialog.handOff.moves")}</h3>
      {lines.length > 0
        ? <ul aria-labelledby={headingId} className="list-disc ps-5">{lines}</ul>
        : <p>{t("dialog.handOff.movesNothing")}</p>}
    </div>
  );
}
