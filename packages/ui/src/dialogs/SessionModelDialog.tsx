import { Description } from "@heroui/react";
import React, { useRef, useState } from "react";

import { ActionableError } from "../actionableError";
import { inheritedSessionLaunch, launchWithOverrides, type LaunchPair } from "../agents";
import { ErrorAlert } from "../components/ErrorAlert";
import { InlineReference } from "../components/InlineReference";
import { AFTER_FOCUS_MOVE_DELAY_MS, StatusAnnouncer } from "../components/StatusAnnouncer";
import { useHasPainted } from "../components/useHasPainted";
import { useFocusHandoff } from "../components/useFocusHandoff";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import type { Session } from "../protocol";
import type { AcknowledgementView } from "../errorAlerts";
import { sessionDisplayTitle } from "../sessionLabel";
import { useDaemonStore } from "../store";
import { Dialog } from "./Dialog";
import { dialogAround } from "./dialogElement";
import { LaunchPickers, useLaunchModels } from "./LaunchSettingsFields";
import { useFailureWhenBlocked } from "./useFailureWhenBlocked";

/** What the dialog sends: each value the user moved off what the session runs with, `null` for the
 * agent's own default. */
export type ModelEdits = { model?: string | null; effort?: string | null };

/** The edits that differ from what the pickers started on, so a value that now equals the session's own
 * (another window made the same change) is not sent, and a session is never restarted for nothing. */
function changedFrom(start: LaunchPair, picked: ModelEdits): ModelEdits {
  return {
    ...(picked.model !== undefined && picked.model !== start.model ? { model: picked.model } : {}),
    ...(picked.effort !== undefined && picked.effort !== start.effort ? { effort: picked.effort } : {}),
  };
}

/**
 * Changes the model and reasoning effort a session runs with (`set_session_model`), on its own agent
 * and account. The pickers are shown at once, since they are what the dialog is for. They start on
 * what the session is launched with — its own choice, else what its project or console gives it — and
 * follow the session while the dialog is open, except for a field the user has edited; the values the
 * user changes become the session's own, and clearing one chooses the agent's own default. Confirm
 * waits for a change. A field changed back to the session's value follows the session again. A running
 * session is restarted to take them, which the dialog says: keeping its
 * conversation, or starting a fresh one where it has none, and ending the prompt a session waiting for
 * the user is showing. An interrupted session takes them at its next resume. Confirm closes the
 * dialog at once and `onChangeModel` carries the change out in the background: the session shows as
 * starting while it restarts, and the daemon's refusal or a failure, which can come seconds later, is
 * an error alert where the sessions are shown. A session that is deleted or archived while the dialog
 * is open ends its purpose, so the dialog turns into a failure modal saying so; one that is starting
 * or switching its account is a momentary hold, Confirm waiting.
 */
export function SessionModelDialog({
  session: opened,
  onClose,
  onChangeModel,
}: {
  session: Session;
  onClose: () => void;
  /** Starts changing the model and effort of the session `sessionId` to `edits` in the background and
   * returns at once; the outcome is announced, or reported as an error alert, where the session is shown. */
  onChangeModel: (sessionId: string, edits: ModelEdits) => void;
}): React.ReactElement | null {
  const t = useT();
  const stored = useDaemonStore((s) => s.sessions.get(opened.id));
  const [latest, setLatest] = useState(opened);
  if (stored && stored !== latest) setLatest(stored);
  const session = stored ?? latest;
  const parentConsole = useDaemonStore((s) => s.consoles.get(session.console_id));
  const project = useDaemonStore((s) => (session.project_id ? s.projects.get(session.project_id) : undefined));
  // Held from the moment it happens, with the title the session had.
  const gone = stored === undefined;
  const title = isolate(sessionDisplayTitle(t, session));
  const failure = useFailureWhenBlocked(
    gone || session.status === "archived",
    new ActionableError(t(gone ? "daemon.session_model_session_gone" : "daemon.session_model_session_archived", { title }), "none", { code: gone ? "session_model_session_gone" : "session_model_session_archived", params: { title: sessionDisplayTitle(t, session) } }),
    t("dialog.sessionModel.blockedTitle"),
  );
  if (!parentConsole && !failure) return null;
  const inherited = parentConsole ? inheritedSessionLaunch(parentConsole, project, session.agent) : { model: null, effort: null };
  return <SessionModelForm session={session} start={launchWithOverrides(inherited, { model: session.model, effort: session.effort })} failure={failure} onClose={onClose} onChangeModel={onChangeModel} />;
}

function SessionModelForm({
  session,
  start,
  failure,
  onClose,
  onChangeModel,
}: {
  session: Session;
  start: LaunchPair;
  failure: AcknowledgementView | undefined;
  onClose: () => void;
  onChangeModel: (sessionId: string, edits: ModelEdits) => void;
}): React.ReactElement {
  const t = useT();
  const painted = useHasPainted();
  const [picked, setPicked] = useState<ModelEdits>({});
  const launch = useLaunchModels(session.agent, session.account_id ?? null, true);
  const pickers = useRef<HTMLDivElement>(null);
  // An account change can remove the focused failure mark and disable both pickers in one render.
  const focusHandoff = useFocusHandoff(() => {
    const trigger = pickers.current?.querySelector<HTMLButtonElement>("[data-slot=select-trigger]");
    const target = trigger && !trigger.disabled
      ? trigger
      : dialogAround(pickers)?.querySelector<HTMLButtonElement>('[data-slot="modal-footer"] button[type="button"]');
    target?.focus();
  });
  const edits = changedFrom(start, picked);
  const { model, effort } = launchWithOverrides(start, picked);
  // The daemon would refuse these; the menu does not offer the dialog for them, but one can come about while it
  // is open, so Confirm waits. Any other refusal comes after the dialog has closed, as an error alert.
  const held = session.status === "starting" || session.switching_account === true;
  const running = session.status !== "interrupted" && session.status !== "archived";
  // A value picked back to the session's own is no edit: the field follows the session again.
  const pick = (field: keyof ModelEdits, value: string | null) => setPicked((now) => {
    const next = { ...now, [field]: value };
    if (value === start[field]) delete next[field];
    return next;
  });
  const restart = session.has_conversation ? "dialog.sessionModel.restart" : "dialog.sessionModel.restartFresh";

  const submit = () => {
    onChangeModel(session.id, edits);
    onClose();
  };

  return (
    <Dialog
      title={t("dialog.sessionModel.title")}
      onClose={onClose}
      failure={failure}
      submitLabel={t("dialog.sessionModel.confirm")}
      submitDisabled={Object.keys(edits).length === 0 || held}
      autoFocusSubmit
      onSubmit={submit}
    >
      <p className="text-sm">
        <Message id="dialog.sessionModel.subject" params={{ title: <InlineReference dir="auto" wrap>{sessionDisplayTitle(t, session)}</InlineReference> }} />
        {`${t("common.sentenceSeparator")}${t(running ? restart : "dialog.sessionModel.nextResume")}`}
        {running && session.status === "waiting_user" ? `${t("common.sentenceSeparator")}${t("dialog.sessionModel.restartPrompt")}` : null}
      </p>
      {held && (
        <ErrorAlert
          status="warning"
          announce={painted}
          announceDelayMs={AFTER_FOCUS_MOVE_DELAY_MS}
          title={t(session.switching_account ? "dialog.sessionModel.heldSwitching" : "dialog.sessionModel.heldStarting")}
          remedy={t("dialog.sessionModel.heldRemedy")}
        />
      )}
      <StatusAnnouncer text={launch.modelText} firstWriteDelayMs={AFTER_FOCUS_MOVE_DELAY_MS} />
      <div ref={pickers} {...focusHandoff} className="flex flex-col gap-4">
        <Description>{t("dialog.sessionModel.note")}</Description>
        <LaunchPickers
          launch={launch}
          model={model}
          effort={effort}
          onModelChange={(value) => pick("model", value)}
          onEffortChange={(value) => pick("effort", value)}
        />
      </div>
    </Dialog>
  );
}
