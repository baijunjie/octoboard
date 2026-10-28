import { Description } from "@heroui/react";
import React, { useRef, useState } from "react";

import { inheritedSessionLaunch, launchWithOverrides, type LaunchPair } from "../agents";
import { InlineReference } from "../components/InlineReference";
import { AFTER_FOCUS_MOVE_DELAY_MS, StatusAnnouncer } from "../components/StatusAnnouncer";
import { useFocusHandoff } from "../components/useFocusHandoff";
import { Message, useT } from "../i18n/react";
import type { Session } from "../protocol";
import { sessionDisplayTitle } from "../sessionLabel";
import { useDaemonStore } from "../store";
import { Dialog } from "./Dialog";
import { dialogAround } from "./dialogElement";
import { LaunchPickers, useLaunchModels } from "./LaunchSettingsFields";

/** What the dialog sends: each value the user moved off what the session runs with, `null` for the
 * agent's own default. */
export type ModelEdits = { model?: string | null; effort?: string | null };

/** The edits that differ from what the pickers started on, so a value changed and changed back is
 * not sent, and a session is never restarted for nothing. */
function changedFrom(start: LaunchPair, picked: ModelEdits): ModelEdits {
  return {
    ...(picked.model !== undefined && picked.model !== start.model ? { model: picked.model } : {}),
    ...(picked.effort !== undefined && picked.effort !== start.effort ? { effort: picked.effort } : {}),
  };
}

/**
 * Changes the model and reasoning effort a session runs with (`set_session_model`), on its own agent
 * and account, with the pickers the session dialog's Advanced section has, shown at once since they
 * are what the dialog is for. They start on what the session is launched with — its own choice, else
 * what its project or console gives it — and the values the user changes become the session's own;
 * clearing one chooses the agent's own default. Confirm waits for a change. A running session is
 * restarted to take them, keeping its conversation, which the dialog says; an interrupted one takes
 * them at its next resume. Confirm closes the dialog at once and `onChangeModel` carries the change
 * out in the background, as a switch of account is: the session shows as starting while it
 * restarts, and the daemon's refusal or a failure, which can come seconds later, is an error alert
 * where the sessions are shown.
 */
export function SessionModelDialog({
  session: opened,
  onClose,
  onChangeModel,
}: {
  session: Session;
  onClose: () => void;
  /** Starts changing the model and effort of the session `sessionId` to `edits` in the background and
   * returns at once; the outcome is reported where the session is shown. */
  onChangeModel: (sessionId: string, edits: ModelEdits) => void;
}): React.ReactElement | null {
  const session = useDaemonStore((s) => s.sessions.get(opened.id)) ?? opened;
  const parentConsole = useDaemonStore((s) => s.consoles.get(session.console_id));
  const project = useDaemonStore((s) => (session.project_id ? s.projects.get(session.project_id) : undefined));
  if (!parentConsole) return null;
  const inherited = inheritedSessionLaunch(parentConsole, project, session.agent);
  return <SessionModelForm session={session} start={launchWithOverrides(inherited, { model: session.model, effort: session.effort })} onClose={onClose} onChangeModel={onChangeModel} />;
}

function SessionModelForm({
  session,
  start: current,
  onClose,
  onChangeModel,
}: {
  session: Session;
  start: LaunchPair;
  onClose: () => void;
  onChangeModel: (sessionId: string, edits: ModelEdits) => void;
}): React.ReactElement {
  const t = useT();
  // Held from the opening: a restart under way changes nothing the user picked against.
  const [start] = useState(current);
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
  const held = session.status === "archived" || session.status === "starting" || session.switching_account === true;
  const running = session.status !== "interrupted";

  const submit = () => {
    onChangeModel(session.id, edits);
    onClose();
  };

  return (
    <Dialog
      title={t("dialog.sessionModel.title")}
      onClose={onClose}
      submitLabel={t("dialog.sessionModel.confirm")}
      submitDisabled={Object.keys(edits).length === 0 || held}
      autoFocusSubmit
      onSubmit={submit}
    >
      <p className="text-sm">
        <Message id="dialog.sessionModel.subject" params={{ title: <InlineReference dir="auto" wrap>{sessionDisplayTitle(t, session)}</InlineReference> }} />
        {`${t("common.sentenceSeparator")}${t(running ? "dialog.sessionModel.restart" : "dialog.sessionModel.nextResume")}`}
      </p>
      <StatusAnnouncer text={launch.modelText} firstWriteDelayMs={AFTER_FOCUS_MOVE_DELAY_MS} />
      <div ref={pickers} {...focusHandoff} className="flex flex-col gap-4">
        <Description>{t("dialog.sessionModel.note")}</Description>
        <LaunchPickers
          launch={launch}
          model={model}
          effort={effort}
          onModelChange={(value) => setPicked((now) => ({ ...now, model: value }))}
          onEffortChange={(value) => setPicked((now) => ({ ...now, effort: value }))}
        />
      </div>
    </Dialog>
  );
}
