import { type Dispatch, type SetStateAction, useRef, useState } from "react";

import { runAccountSwitchBatch } from "../dialogs/accountSwitchBatch";
import { DaemonRequestError } from "../daemon-client";
import type { ConfirmLaunch } from "../dialogs/useConfirmIncompleteLaunch";
import type { ModelEdits } from "../dialogs/SessionModelDialog";
import { modelChangeRecovery } from "../errors/errorRecovery";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import { ALREADY_RUNNING_CODES } from "../protocol";
import { sessionDisplayTitle } from "../sessions/sessionLabel";
import { useDaemon } from "../store";
import { useOctoboardTheme } from "../preferences/theme";

/**
 * The requests that launch a session again or change how it runs: resuming or reopening it, from a control or from
 * the session dialog, switching sessions to another account, and changing a session's model. `resumingIds` is the
 * sessions whose resume is in flight, for the terminal's loading state. A launch goes through `confirmLaunch` first,
 * and a background change that succeeds is told through `announce`.
 */
export function useSessionRequests({ confirmLaunch, announce }: { confirmLaunch: ConfirmLaunch; announce: (text: string) => void }) {
  const t = useT();
  const { resolved: appearance } = useOctoboardTheme();
  const { request, raiseError, store } = useDaemon();
  // Sessions whose `resume_session` is in flight, for the terminal's loading state.
  const [resumingIds, setResumingIds] = useState<ReadonlySet<string>>(new Set());

  // A session without a process can be resumed through several controls, so overlapping requests for the
  // same session share one in-flight guard.
  const inFlightRef = useRef<Set<string>>(new Set());

  const runOnce = (key: string, title: string, action: () => Promise<void>): Promise<boolean> =>
    runOnceGuarded(inFlightRef.current, key, action, (error) => raiseError(error, { title }));

  /** Resumes a session; resolves to whether the daemon accepted it. A console may now
   * hold any number of live console sessions, so there is nothing to pre-emptively refuse here —
   * selecting, resuming or reopening one simply works, whatever else is running in the console. An
   * account whose last sharing change did not finish is asked about first; cancelling resolves
   * false and sends nothing. */
  const resumeSession = async (sessionId: string): Promise<boolean> => {
    let accepted = false;
    const session = store.getState().sessions.get(sessionId);
    // An archived session is reopened, an interrupted one resumed; the failure is titled for which.
    const title = t(session?.status === "archived" ? "sidebar.session.reopenFailedTitle" : "sidebar.session.resumeFailedTitle");
    await confirmLaunch(session?.account_id, async () => {
      accepted = await runOnce(`resume:${sessionId}`, title, () => startResume(sessionId));
    });
    return accepted;
  };

  const startResume = (sessionId: string): Promise<void> =>
    whileResuming(setResumingIds, sessionId, () => request({ type: "resume_session", session: sessionId, terminal_appearance: appearance }));

  /** Resumes an archived or interrupted session for the session dialog: the same request as
   * `resumeSession`, with the terminal's resuming state and in-flight guard, but rejecting with the
   * daemon's reason (an unresumable conversation, a failed unarchive) for the dialog to show, rather
   * than toasting it. */
  const resumeFromDialog = (sessionId: string): Promise<void> =>
    runOnceRejecting(inFlightRef.current, `resume:${sessionId}`, () => startResume(sessionId));

  /** Switches sessions to an account in the background, the dialog that asked being gone by then.
   * Each session shows as switching its account until its turn is over (see
   * `runAccountSwitchBatch`), so its terminal says so instead of offering a Resume that would race
   * the switch; the failures come as one error alert. */
  const switchAccounts = (sessionIds: string[], account: string | null, accountName: string): void => {
    void runAccountSwitchBatch(store, sessionIds, account, {
      accountName,
      switchOne: (sessionId, target) =>
        request({ type: "switch_session_account", session: sessionId, account: target, terminal_appearance: appearance }).then(() => undefined),
      onFinished: ({ switched, failed }) => {
        // The dialog that asked is gone, and with it anything that would say the outcome. A batch with
        // failures is told by its error alert alone. A switch takes seconds, so this is never within the
        // focus move of the dialog closing.
        if (failed > 0 || switched === 0) return;
        announce(t("dialog.switchAccount.done", { count: switched, account: isolate(accountName) }));
      },
    });
  };

  /** Changes a session's model and reasoning effort in the background, the dialog that asked being gone
   * by then. The daemon shows the session as starting while it restarts; a refusal or a failure,
   * which can come seconds later, is an error alert, and success is announced as the reply's `outcome`
   * tells: a restart, or values only recorded for the next resume. A session that had the values already
   * changed nothing, so nothing is announced. A model the agent does not offer points back to Change model,
   * not to launch settings. */
  const changeSessionModel = (sessionId: string, edits: ModelEdits): void => {
    request({ type: "set_session_model", session: sessionId, ...edits, terminal_appearance: appearance })
      .then((reply) => {
        // Named as it is now: it may have been renamed while the change was out, and is gone if deleted since.
        const session = store.getState().sessions.get(sessionId);
        if (!session || reply.type !== "session_model_set") return;
        const params = { session: isolate(sessionDisplayTitle(t, session)) };
        // A restart is answered only once the relaunch has settled, long past the dialog's focus move; a recorded
        // change is answered at once, and `announce` holds both back past the focus move.
        if (reply.outcome === "recorded") announce(t("dialog.sessionModel.doneRecorded", params));
        else if (reply.outcome === "restarted") announce(t("dialog.sessionModel.done", params));
      }, (err: unknown) => raiseError(err, { title: t("dialog.sessionModel.failedTitle"), session: sessionId, recovery: modelChangeRecovery(err) }));
  };

  return { resumingIds, resumeSession, resumeFromDialog, switchAccounts, changeSessionModel };
}

// The helpers below sit outside the hook because the React Compiler cannot compile a function
// holding a `try` with a `finally`.

/** Runs `action` unless one with the same `key` is in `inFlight`; says whether it went through. A
 * second call while one is in flight counts as having gone through, since what it asks is under
 * way, and so does an "already running" refusal that is not shown. */
async function runOnceGuarded(
  inFlight: Set<string>,
  key: string,
  action: () => Promise<void>,
  raise: (error: unknown) => void,
): Promise<boolean> {
  try {
    await runOnceRejecting(inFlight, key, action);
    return true;
  } catch (err) {
    raise(err);
    return false;
  }
}

/** `runOnceGuarded` without the toast: a failure is rethrown for the caller to show. The same
 * calls count as having gone through: one while another is in flight, and an "already running"
 * refusal. */
async function runOnceRejecting(inFlight: Set<string>, key: string, action: () => Promise<void>): Promise<void> {
  if (inFlight.has(key)) return;
  inFlight.add(key);
  try {
    await action();
  } catch (err) {
    // The in-flight guard above already stops this client from causing the double-click kind of
    // "already running" (see `ALREADY_RUNNING_CODES`), but another path to the same session — the
    // sidebar row and the terminal's Resume button, for instance — can still race it.
    const isAlreadyRunning = err instanceof DaemonRequestError && ALREADY_RUNNING_CODES.includes(err.code);
    if (!isAlreadyRunning) throw err;
  } finally {
    inFlight.delete(key);
  }
}

/** Runs `work` with `sessionId` counted as resuming, for the terminal's loading state, until it
 * settles. A failure goes to `onError` first and is then passed on. */
async function whileResuming(
  setResumingIds: Dispatch<SetStateAction<ReadonlySet<string>>>,
  sessionId: string,
  work: () => Promise<unknown>,
  onError?: (err: unknown) => void,
): Promise<void> {
  setResumingIds((ids) => new Set(ids).add(sessionId));
  try {
    await work();
  } catch (err) {
    onError?.(err);
    throw err;
  } finally {
    setResumingIds((ids) => {
      const next = new Set(ids);
      next.delete(sessionId);
      return next;
    });
  }
}
