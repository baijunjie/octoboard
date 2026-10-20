import { Tabs } from "@heroui/react";
import React, { useRef, useState } from "react";

import { ErrorAlert } from "../components/ErrorAlert";
import { FadeOverflow } from "../components/FadeOverflow";
import { useTabScrollLabels } from "../components/tabScrollLabels";
import { DaemonRequestError } from "../daemon-client";
import { errorPresentation } from "../errorPresentation";
import type { MessageKey } from "../i18n/catalog";
import { InlineChip } from "../components/InlineChip";
import { Message, useT } from "../i18n/react";
import type { Console, Project, Session } from "../protocol";
import { sessionChoiceTitle, sessionDisplayTitle } from "../sessionLabel";
import { BindingBadge } from "../sidebar/BindingBadge";
import { liveConsoleSessions } from "../sidebar/order";
import { useDaemon, useDaemonStore } from "../store";
import { AgentAccountField } from "./AgentAccountField";
import { Dialog, DialogError, useDialogAction, useSubmitValidation } from "./Dialog";
import type { NewSessionBinding } from "./dialogRequest";
import { LaunchSettingsFields } from "./LaunchSettingsFields";
import { ResumeConversationFields } from "./ResumeConversationFields";
import { useCloseWhenBlocked } from "./useCloseWhenBlocked";
import { useLaunchChoice } from "./useLaunchChoice";
import { useResumeTarget, type ResumeTarget } from "./useResumeTarget";
import { openSessionRequest } from "./openSessionRequest";
import { OptionSelect } from "./OptionSelect";
import { TextInput } from "./TextInput";

type Conversation = "new" | "resume";

/** The daemon's refusals that are about the conversation to resume, shown under the Session ID
 * field rather than at the foot of the dialog. */
const RESUME_REFUSALS = new Set([
  "conversation_to_resume_not_found",
  "conversation_in_session",
  "conversation_outside_project",
  "resume_id_invalid",
]);

/** The submit button's label for what the Resume tab found; a new conversation, or nothing found
 * yet, is "Open". */
const SUBMIT_LABEL = {
  archived: "dialog.session.reopen",
  interrupted: "dialog.session.resume",
  running: "dialog.session.goTo",
  record: "dialog.session.open",
} as const satisfies Record<ResumeTarget["kind"], MessageKey>;

/** The panel of a conversation tab: the dialog's own column of fields, with HeroUI's padding and
 * top margin off so the first field keeps the dialog's spacing below the tabs. */
const PANEL_CLASS = "mt-0 flex flex-col gap-4 p-0";

/** `OptionSelect`'s value for "no owner"; a session id is never this. */
const NO_OWNER = "none";

/**
 * Opens a console session or a session manually under a project. Joined pickers settle the agent and its account
 * together. They open on the agent per the "Which agent a session uses" section of docs/product/sessions.md (for a
 * project session: the project's default, then the console's project session default, then its `default_agent`; a
 * console session uses only `default_agent`) with the account that agent resolves to when none is chosen (the
 * project's reference, else the console's), and the user can pick another agent or one of its accounts for this
 * session; the choice goes out as `open_session`'s `agent` and `account` fields.
 *
 * `open_session` answers with `session_opened`, which names the session it started — the broadcast
 * that puts it in the tree carries no request id, so this reply is the only way to tell which of
 * the sessions appearing there is ours to select.
 *
 * Where the dialog was opened from settles the session's binding (`binding`). From the project
 * list it is a choice: the console's console sessions that are not archived, each beside its colour
 * (the binding badge's own dot), and "none", which is the default, since a session the user starts
 * stays outside the orchestration unless they say otherwise; with no console session to offer there
 * is no choice at all and the session goes out unbound. From a project's focus mode it is never
 * offered and the session is always unbound. From a console session's focus mode the session is
 * bound to it, shown as a fixed line rather than a field. The binding is immutable once the session
 * opens, so this is the only place it is ever set; it goes out as `open_session`'s `bound_to`.
 *
 * A project session, unless it is fixed to its owner, opens with a pair of tabs at the head of the
 * form, choosing between a new conversation and resuming one started outside Octoboard. Resuming
 * asks for the conversation's session ID, always required, and takes the choice of owner away, since
 * such a session is always unbound. The tab holds the ID alone until it is valid and the daemon has
 * said where the conversation is held (`locate_conversation_to_resume`); that settles which fields
 * follow and what the submit button does. An archived session of the project is resumed (`onResume`;
 * the name is editable and `rename_session` follows for a name changed), and an interrupted one is
 * resumed the same way with its name fixed; both show their agent and account, fixed, with no launch
 * settings since the session keeps its own. A running session of the project is gone to. An agent's
 * own record opens a new session (`resume_agent_session_id`) on the agent and account that hold it,
 * shown fixed beside the name and the launch settings. A session outside the project or no
 * conversation at all is said under the ID field; the submit button waits in each case.
 */
export function SessionDialog({
  console: parentConsole,
  project,
  binding = { kind: "unbound" },
  sessions,
  onClose,
  onOpened,
  onResume,
}: {
  console: Console;
  project?: Project;
  binding?: NewSessionBinding;
  sessions: Session[];
  onClose: () => void;
  onOpened: (sessionId: string) => void;
  /** Resumes an archived or interrupted session the way the Resume action does; rejects with the reason, shown at the dialog's foot. */
  onResume: (sessionId: string) => Promise<void>;
}): React.ReactElement {
  const t = useT();
  const tabScrollLabels = useTabScrollLabels();
  const { request, toastError } = useDaemon();
  const [title, setTitle] = useState("");
  const [pickedOwner, setPickedOwner] = useState(NO_OWNER);
  const [conversation, setConversation] = useState<Conversation>("new");
  // The Name field takes focus as the dialog opens, so Enter submits at once; a field mounted by
  // a tab switch must not, or it would take focus off the tab the user is on.
  const [switched, setSwitched] = useState(false);
  const [resumeId, setResumeId] = useState("");
  // The name typed over an archived session's own, for that session only; unedited, the field shows its title.
  const [heldName, setHeldName] = useState<{ session: string; value: string }>();
  // A refusal of the conversation to resume, with the ID and (for a record) the agent and account it refused.
  const [refusal, setRefusal] = useState<{ error: DaemonRequestError; id: string; record?: string }>();
  const { error, busy, run } = useDialogAction();

  // A resumed conversation is always a plain session, so choosing to resume takes the choice of
  // owner away; a session fixed to its owner never offers the choice in the first place.
  const sessionBinding: NewSessionBinding = project ? binding : { kind: "unbound" };
  const resumable = project !== undefined && sessionBinding.kind !== "bound";
  const resuming = resumable && conversation === "resume";
  // The daemon is asked once the ID is valid; where the conversation is held settles what the
  // submit does (resume a session, go to one, or open one on an agent's record) and on which
  // agent and account.
  const resume = useResumeTarget(project, resumeId, resuming, sessions);
  const { target } = resume;
  const record = target?.kind === "record" ? target : undefined;

  const { choice, blocked, usedUp, overrides, agentOptions, setPickedAgent, account: accountField, launch } = useLaunchChoice(parentConsole, project, undefined, record);
  // A used-up account only matters for a session about to be opened on it, not for resuming one that has its own.
  const quotaUsedUp = !resuming || record ? usedUp : undefined;
  // Nothing to open a session with (an agent was removed or uninstalled while this was open).
  useCloseWhenBlocked(blocked, t("dialog.session.noAgent"), onClose);

  // The select is shown from the moment there is a console session to offer and stays mounted
  // for the rest of the dialog's life, even if the list later empties, leaving "none" alone: it is
  // a focusable control, and unmounting it under the user would drop focus to the body and take
  // Escape and Tab containment with it. Hiding the field when the dialog opens with nothing to
  // offer is the rule for that state; a list that empties under the user is not that state, and a
  // field vanishing would also hide that their choice went with it. The one exception is a
  // conversation to resume, which a session cannot combine with an owner: the select goes while
  // that is the choice made, and `pickedOwner` keeps the earlier pick for the way back. Focus is on
  // the Conversation tabs, not in the select, when it goes, so nothing is dropped.
  //
  // `owner` is only ever one still offered, never the picked id as it stands: `open_session` does
  // not check the target's status, so sending an archived console session's id would create a live
  // session bound to an archived one, whose reports would be refused. An archived pick therefore
  // falls back to none, visibly in the select. `pickedOwner` keeps the stale id, so reopening that
  // console session while the dialog is still open restores the choice, which is harmless. A fixed
  // owner that is no longer live cannot fall back to anything, so it stops the dialog instead.
  const owners = liveConsoleSessions(sessions, parentConsole.id);
  const storedFixedOwner = useDaemonStore((s) => sessionBinding.kind === "bound" ? s.sessions.get(sessionBinding.to.id) : undefined);
  const fixedOwner = sessionBinding.kind === "bound"
    ? storedFixedOwner ?? sessions.find((s) => s.id === sessionBinding.to.id) ?? sessionBinding.to
    : undefined;
  const fixedOwnerGone = fixedOwner !== undefined && !owners.some((s) => s.id === fixedOwner.id);
  const owner = fixedOwner ?? (sessionBinding.kind === "choose" && !resuming ? owners.find((s) => s.id === pickedOwner) : undefined);
  const [ownerOffered, setOwnerOffered] = useState(owners.length > 0);
  if (owners.length > 0 && !ownerOffered) setOwnerOffered(true);

  const { shown, attempt } = useSubmitValidation();
  const resumeFieldRef = useRef<HTMLDivElement>(null);
  // What the user's attempt to submit found wrong with the ID, by kind.
  const shownProblem = shown(resume.idProblem);

  // The name of a found session: an archived one's may be edited, and only while it is archived;
  // any other state shows the session's own.
  const foundSession = target && target.kind !== "record" ? target.session : undefined;
  const foundName = target?.kind === "archived" && heldName?.session === target.session.id ? heldName.value : foundSession?.title ?? "";

  // A refusal stands under the field only while the ID and the record are still those it refused,
  // since changing either asks a different question; between its answers the lookup has no record,
  // and an answer that is no record settles the matter.
  const recordKey = record && JSON.stringify([record.agent, record.account]);
  const refused = resuming && refusal?.id === resumeId && (resume.looking || recordKey === refusal.record) ? refusal.error : undefined;

  const submit = () => {
    if (!attempt(resume.idProblem)) {
      resumeFieldRef.current?.querySelector("input")?.focus();
      return;
    }
    void run(async () => {
      setRefusal(undefined);
      if (target && target.kind !== "record") {
        // The session continues its own conversation, so only a name the user changed to another
        // one is applied: a rename that fails after the resume is reported, not allowed to hold
        // the dialog open.
        if (target.kind !== "running") await onResume(target.session.id);
        const name = foundName.trim();
        if (target.kind === "archived" && name && name !== target.session.title) await request({ type: "rename_session", session: target.session.id, title: name }).catch((err: unknown) => toastError(errorPresentation(err)));
        onOpened(target.session.id);
        onClose();
        return;
      }
      const reply = await request(openSessionRequest(t, {
        console: parentConsole,
        project,
        ownerId: owner?.id,
        resumeId: resuming ? resumeId : undefined,
        choice,
        title,
        overrides,
      })).catch((err: unknown) => {
        if (!(err instanceof DaemonRequestError) || !resuming || !RESUME_REFUSALS.has(err.code)) throw err;
        setRefusal({ error: err, id: resumeId, record: recordKey });
        // Anything but a conversation that is not there at all may mean the answer held is stale
        // (it was taken into a session, say), so the daemon is asked again.
        if (err.code !== "conversation_to_resume_not_found") resume.reask();
        resumeFieldRef.current?.querySelector("input")?.focus();
      });
      if (!reply) return;
      if (reply.type === "session_opened") onOpened(reply.session.id);
      onClose();
    });
  };

  // The Name field of a session about to be opened, in the New tab, on the Resume tab once an
  // agent's record is found and, where there are no tabs, on its own.
  const nameField = (
    <TextInput
      label={t("common.name")}
      value={title}
      onChange={setTitle}
      placeholder={t("session.defaultName", { name: project?.name ?? parentConsole.name })}
      autoFocus={!switched}
      description={t("dialog.session.defaultNameHint")}
    />
  );
  // What a new conversation asks for: the New tab's panel and, where there are no tabs, the whole form.
  const sharedFields = (
    <>
      {nameField}
      <AgentAccountField
        label={t("dialog.session.agent")}
        options={agentOptions}
        value={choice.agent}
        onChange={setPickedAgent}
        account={accountField}
      />
      {fixedOwner && (
        <div data-marquee-scope className="flex items-center gap-2 text-sm">
          <BindingBadge owner={fixedOwner} decorative />
          <FadeOverflow dir="auto" className="min-w-0 flex-1" titleWhenClipped={<Message id="dialog.session.ownerFixed" params={{ name: <InlineChip dir="auto" wrap size="inherit">{sessionDisplayTitle(t, fixedOwner)}</InlineChip> }} />}>
            {<Message id="dialog.session.ownerFixed" params={{ name: <InlineChip dir="auto" wrap size="inherit">{sessionDisplayTitle(t, fixedOwner)}</InlineChip> }} />}
          </FadeOverflow>
        </div>
      )}
      {sessionBinding.kind === "choose" && ownerOffered && !resuming && (
        <OptionSelect
          label={t("dialog.session.owner")}
          value={owner?.id ?? NO_OWNER}
          onChange={setPickedOwner}
          options={[
            { value: NO_OWNER, label: t("dialog.session.ownerNone") },
            ...owners.map((s) => ({ value: s.id, label: sessionChoiceTitle(t, s), labelContent: sessionDisplayTitle(t, s), icon: <BindingBadge owner={s} decorative /> })),
          ]}
        />
      )}
      <LaunchSettingsFields
        {...launch}
        note={t(project ? "dialog.session.launchSettingsNote" : "dialog.consoleSession.launchSettingsNote")}
        isDisabled={blocked}
      />
    </>
  );

  return (
    <Dialog
      title={project
        ? <Message id="dialog.session.title" params={{ project: <InlineChip dir="auto" wrap size="inherit">{project.name}</InlineChip> }} />
        : <Message id="dialog.consoleSession.title" params={{ console: <InlineChip dir="auto" wrap size="inherit">{parentConsole.name}</InlineChip> }} />}
      onClose={onClose}
      submitLabel={t(target ? SUBMIT_LABEL[target.kind] : "dialog.session.open")}
      busy={busy}
      submitDisabled={blocked || fixedOwnerGone || quotaUsedUp !== undefined || (resuming && resume.idProblem === undefined && target === undefined)}
      onSubmit={submit}
    >
      {quotaUsedUp && <ErrorAlert title={quotaUsedUp.title} announce>{quotaUsedUp.description}</ErrorAlert>}
      {fixedOwner && fixedOwnerGone && <ErrorAlert>{<Message id="dialog.session.ownerGone" params={{ name: <InlineChip dir="auto" wrap size="inherit">{sessionDisplayTitle(t, fixedOwner)}</InlineChip> }} />}</ErrorAlert>}
      {resumable && (
        <Tabs selectedKey={conversation} onSelectionChange={(key) => {
          setSwitched(true);
          setConversation(key as Conversation);
        }} isDisabled={blocked} className="gap-4">
          <Tabs.ListContainer render={(props) => <div {...props}>{tabScrollLabels(props.children)}</div>}>
            <Tabs.List aria-label={t("dialog.session.conversation")}>
              {/* Keep hovered labels at full opacity and use the foreground colour for clear feedback. */}
              <Tabs.Tab id="new" className="whitespace-nowrap hover:text-foreground hover:opacity-100">
                {t("dialog.session.conversationNew")}
                <Tabs.Indicator />
              </Tabs.Tab>
              <Tabs.Tab id="resume" className="whitespace-nowrap hover:text-foreground hover:opacity-100">
                {t("dialog.session.conversationResume")}
                <Tabs.Indicator />
              </Tabs.Tab>
            </Tabs.List>
          </Tabs.ListContainer>
          {/* The New tab holds the fields of a new conversation. The Resume tab holds its Session
              ID, then the fields that what the lookup found calls for. Only the selected tab's
              panel is mounted, so what the form remembers lives in this dialog's state; only
              whether Advanced is unfolded is the section's own, and it folds back on a switch. */}
          <Tabs.Panel id="new" className={PANEL_CLASS}>
            {sharedFields}
          </Tabs.Panel>
          <Tabs.Panel id="resume" className={PANEL_CLASS}>
            <ResumeConversationFields
              idFieldRef={resumeFieldRef}
              resumeId={resumeId}
              onResumeIdChange={setResumeId}
              shownProblem={shownProblem}
              refusal={refused}
              resume={resume}
              name={foundName}
              onNameChange={(value) => target && target.kind !== "record" && setHeldName({ session: target.session.id, value })}
              recordName={nameField}
              launch={launch}
              blocked={blocked}
            />
          </Tabs.Panel>
        </Tabs>
      )}
      {!resumable && sharedFields}
      <DialogError message={error} />
    </Dialog>
  );
}
