import React, { useState } from "react";

import { choiceGroups, currentChoice, initialChoice, type AccountChoice } from "../accountChoices";
import { noAgentAvailable } from "../agents";
import { useT } from "../i18n/react";
import type { Console, Project, Session } from "../protocol";
import { BindingBadge } from "../sidebar/BindingBadge";
import { liveConsoleSessions } from "../sidebar/order";
import { useDaemon, useDaemonStore } from "../store";
import { AccountSelect } from "./AccountSelect";
import { Dialog, DialogError, useDialogAction } from "./Dialog";
import { OptionSelect } from "./OptionSelect";
import { TextInput } from "./TextInput";

/** `OptionSelect`'s value for "no owner"; a session id is never this. */
const NO_OWNER = "none";

/**
 * Opens a session manually under a project. One grouped control settles the agent and its account
 * together. It opens on the agent per the "Which agent a session uses" section of
 * docs/product/sessions.md (project default, then console default) with the account that agent
 * resolves to for the console, and the user can pick any other entry for this one session; the
 * entry goes out as `open_session`'s `agent` and `account` fields.
 *
 * `open_session` answers with `session_opened`, which names the session it started — the broadcast
 * that puts it in the tree carries no request id, so this reply is the only way to tell which of
 * the sessions appearing there is ours to select.
 *
 * The owner choice below offers the console's console sessions that are not archived, each beside
 * its colour (the binding badge's own dot), and "none", which is the default: a session the user
 * starts stays outside the orchestration unless they say otherwise. With no console session to
 * offer there is no choice at all and the session goes out unbound. The binding is immutable once
 * the session opens, so this is the only place it is ever chosen; it goes out as `open_session`'s
 * `bound_to`.
 */
export function SessionDialog({
  console: parentConsole,
  project,
  sessions,
  onClose,
  onOpened,
}: {
  console: Console;
  project: Project;
  sessions: Session[];
  onClose: () => void;
  onOpened: (sessionId: string) => void;
}): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const agentAvailability = useDaemonStore((s) => s.agentAvailability);
  const blocked = noAgentAvailable(agentAvailability);
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const [picked, setPicked] = useState<AccountChoice>();
  const [title, setTitle] = useState("");
  const [pickedOwner, setPickedOwner] = useState(NO_OWNER);
  const { error, busy, run } = useDialogAction();

  const groups = choiceGroups(accounts, agentAvailability, t("settings.accounts.defaultName"));
  const choice = currentChoice(picked, initialChoice(parentConsole, project, accounts, agentAvailability), groups);

  // The select is shown from the moment there is a console session to offer and stays mounted
  // for the rest of the dialog's life, even if the list later empties, leaving "none" alone: it is
  // a focusable control, and unmounting it under the user would drop focus to the body and take
  // Escape and Tab containment with it. Hiding the field when the dialog opens with nothing to
  // offer is the milestone's rule for that state; a list that empties under the user is not that
  // state, and a field vanishing would also hide that their choice went with it.
  //
  // `owner` is only ever one still offered, never the picked id as it stands: `open_session` does
  // not check the target's status, so sending an archived console session's id would create a live
  // session bound to an archived one, whose reports would be refused. An archived pick therefore
  // falls back to none, visibly in the select. `pickedOwner` keeps the stale id, so reopening that
  // console session while the dialog is still open restores the choice, which is harmless.
  //
  // TODO(docs/plans/20261008-console-sessions-and-agent-accounts/13-focus-modes.md): a focus mode
  // that forces the owner makes it `fixedOwner ?? owners.find(...)`, and one that removes the
  // choice leaves it unset.
  const owners = liveConsoleSessions(sessions, parentConsole.id);
  const owner = owners.find((s) => s.id === pickedOwner);
  const [ownerOffered, setOwnerOffered] = useState(owners.length > 0);
  if (owners.length > 0 && !ownerOffered) setOwnerOffered(true);

  const submit = () =>
    void run(async () => {
      const reply = await request({
        type: "open_session",
        console_id: parentConsole.id,
        project_id: project.id,
        agent: choice.agent,
        account: choice.account,
        title: title || undefined,
        bound_to: owner?.id,
      });
      if (reply.type === "session_opened") onOpened(reply.session.id);
      onClose();
    });

  return (
    <Dialog
      title={t("dialog.session.title", { project: project.name })}
      onClose={onClose}
      submitLabel={t("dialog.session.open")}
      busy={busy}
      submitDisabled={blocked}
      onSubmit={submit}
    >
      {blocked && <p className="text-sm text-danger">{t("agents.installPrompt")}</p>}
      <AccountSelect label={t("dialog.session.agentAccount")} groups={groups} value={choice} onChange={setPicked} />
      <TextInput label={t("dialog.session.titleOptional")} value={title} onChange={setTitle} />
      {ownerOffered && (
        <OptionSelect
          label={t("dialog.session.owner")}
          value={owner?.id ?? NO_OWNER}
          onChange={setPickedOwner}
          options={[
            { value: NO_OWNER, label: t("dialog.session.ownerNone") },
            ...owners.map((s) => ({ value: s.id, label: s.title, icon: <BindingBadge owner={s} decorative /> })),
          ]}
        />
      )}
      <DialogError message={error} />
    </Dialog>
  );
}
