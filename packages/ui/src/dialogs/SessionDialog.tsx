import { Tabs } from "@heroui/react";
import React, { useRef, useState } from "react";

import { DaemonErrorMessage } from "../components/DaemonErrorMessage";
import { FadeOverflow } from "../components/FadeOverflow";
import { InlineChip } from "../components/InlineChip";
import { PATH_VAR_PARAMS } from "../components/literalParams";
import { useTabScrollLabels } from "../components/tabScrollLabels";
import { CONVERSATION_ID, EXAMPLE_CONVERSATION_ID } from "../conversationId";
import { DaemonRequestError } from "../daemon-client";
import { errorPresentation } from "../errorPresentation";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import type { Console, Project, Session } from "../protocol";
import { sessionDisplayTitle } from "../sessionLabel";
import { BindingBadge } from "../sidebar/BindingBadge";
import { liveConsoleSessions } from "../sidebar/order";
import { useDaemon, useDaemonStore } from "../store";
import { AgentAccountField } from "./AgentAccountField";
import { Dialog, DialogError, useDialogAction, useSubmitValidation } from "./Dialog";
import type { NewSessionBinding } from "./dialogRequest";
import { LaunchSettingsFields } from "./LaunchSettingsFields";
import { useLaunchChoice } from "./useLaunchChoice";
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
 * asks for the conversation's session ID (`resume_agent_session_id`), always required, and takes
 * the choice of owner away, since such a session is always unbound.
 */
export function SessionDialog({
  console: parentConsole,
  project,
  binding = { kind: "unbound" },
  sessions,
  onClose,
  onOpened,
}: {
  console: Console;
  project?: Project;
  binding?: NewSessionBinding;
  sessions: Session[];
  onClose: () => void;
  onOpened: (sessionId: string) => void;
}): React.ReactElement {
  const t = useT();
  const tabScrollLabels = useTabScrollLabels();
  const { request } = useDaemon();
  const [title, setTitle] = useState("");
  const [pickedOwner, setPickedOwner] = useState(NO_OWNER);
  const [conversation, setConversation] = useState<Conversation>("new");
  // The Name field takes focus as the dialog opens, so Enter submits at once; a field mounted by
  // a tab switch must not, or it would take focus off the tab the user is on.
  const [switched, setSwitched] = useState(false);
  const [resumeId, setResumeId] = useState("");
  const [refusal, setRefusal] = useState<{ error: DaemonRequestError; asked: string }>();
  const { error, busy, run } = useDialogAction();

  const { choice, blocked, overrides, agentOptions, setPickedAgent, account: accountField, launch } = useLaunchChoice(parentConsole, project);

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
  const sessionBinding: NewSessionBinding = project ? binding : { kind: "unbound" };
  const owners = liveConsoleSessions(sessions, parentConsole.id);
  const storedFixedOwner = useDaemonStore((s) => sessionBinding.kind === "bound" ? s.sessions.get(sessionBinding.to.id) : undefined);
  const fixedOwner = sessionBinding.kind === "bound"
    ? storedFixedOwner ?? sessions.find((s) => s.id === sessionBinding.to.id) ?? sessionBinding.to
    : undefined;
  const fixedOwnerGone = fixedOwner !== undefined && !owners.some((s) => s.id === fixedOwner.id);
  // A resumed conversation is always a plain session, so choosing to resume takes the choice of
  // owner away; a session fixed to its owner never offers the choice in the first place.
  const resumable = project !== undefined && sessionBinding.kind !== "bound";
  const resuming = resumable && conversation === "resume";
  const owner = fixedOwner ?? (sessionBinding.kind === "choose" && !resuming ? owners.find((s) => s.id === pickedOwner) : undefined);
  const [ownerOffered, setOwnerOffered] = useState(owners.length > 0);
  if (owners.length > 0 && !ownerOffered) setOwnerOffered(true);

  const { shown, attempt } = useSubmitValidation();
  const resumeFieldRef = useRef<HTMLDivElement>(null);
  // What is wrong with the ID, by kind; the wording is chosen where it is shown.
  const resumeProblem: "required" | "invalid" | undefined = !resuming ? undefined : resumeId === "" ? "required" : !CONVERSATION_ID.test(resumeId) ? "invalid" : undefined;
  const resumeProblems = {
    required: t("dialog.session.resumeRequired"),
    invalid: <Message id="dialog.session.resumeInvalid" params={{ example: <InlineChip monospace wrap size="inherit">{EXAMPLE_CONVERSATION_ID}</InlineChip> }} />,
  };
  const shownProblem = shown(resumeProblem);

  // What a refusal was about: it stands under the field only while the ID, agent and account are
  // still those it refused, since changing any of them asks a different question.
  const asked = JSON.stringify([resumeId, choice.agent, choice.account]);
  const refused = resuming && refusal?.asked === asked ? refusal.error : undefined;

  const submit = () => {
    if (!attempt(resumeProblem)) {
      resumeFieldRef.current?.querySelector("input")?.focus();
      return;
    }
    void run(async () => {
      setRefusal(undefined);
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
        setRefusal({ error: err, asked });
        resumeFieldRef.current?.querySelector("input")?.focus();
      });
      if (!reply) return;
      if (reply.type === "session_opened") onOpened(reply.session.id);
      onClose();
    });
  };

  // The fields every session has, in either conversation tab and, where there are no tabs, on their own.
  const sharedFields = (
    <>
      <TextInput
        label={t("common.name")}
        value={title}
        onChange={setTitle}
        placeholder={t("session.defaultName", { name: project?.name ?? parentConsole.name })}
        autoFocus={!switched}
        description={t("dialog.session.defaultNameHint")}
      />
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
          <FadeOverflow dir="auto" className="min-w-0 flex-1" titleWhenClipped={t("dialog.session.ownerFixed", { name: isolate(sessionDisplayTitle(t, fixedOwner)) })}>
            {t("dialog.session.ownerFixed", { name: isolate(sessionDisplayTitle(t, fixedOwner)) })}
          </FadeOverflow>
        </div>
      )}
      {fixedOwner && fixedOwnerGone && <p className="text-sm text-app-danger">{t("dialog.session.ownerGone", { name: isolate(sessionDisplayTitle(t, fixedOwner)) })}</p>}
      {sessionBinding.kind === "choose" && ownerOffered && !resuming && (
        <OptionSelect
          label={t("dialog.session.owner")}
          value={owner?.id ?? NO_OWNER}
          onChange={setPickedOwner}
          options={[
            { value: NO_OWNER, label: t("dialog.session.ownerNone") },
            ...owners.map((s) => ({ value: s.id, label: sessionDisplayTitle(t, s), icon: <BindingBadge owner={s} decorative /> })),
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
        ? t("dialog.session.title", { project: isolate(project.name) })
        : t("dialog.consoleSession.title", { console: isolate(parentConsole.name) })}
      onClose={onClose}
      submitLabel={t("dialog.session.open")}
      busy={busy}
      submitDisabled={blocked || fixedOwnerGone}
      onSubmit={submit}
    >
      {blocked && <p className="text-sm text-app-danger"><Message id="agents.installPrompt" params={PATH_VAR_PARAMS} /></p>}
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
          {/* Both tabs hold the fields they share, so each has a real panel to name: the tab's own
              part (the Session ID, while resuming) comes first. They are mounted one at a time, so
              what the form remembers lives in this dialog's state; only whether Advanced is unfolded
              is the section's own, and it folds back on a switch. */}
          <Tabs.Panel id="new" className={PANEL_CLASS}>
            {sharedFields}
          </Tabs.Panel>
          <Tabs.Panel id="resume" className={PANEL_CLASS}>
            <div ref={resumeFieldRef}>
              <TextInput
                label={t("dialog.session.resumeId")}
                value={resumeId}
                onChange={setResumeId}
                placeholder={t("dialog.session.resumeExample", { example: EXAMPLE_CONVERSATION_ID })}
                description={t("dialog.session.resumeHint")}
                errorMessage={shownProblem ? resumeProblems[shownProblem] : refused && <DaemonErrorMessage error={errorPresentation(refused)} />}
                dir="ltr"
                isDisabled={blocked}
              />
            </div>
            {sharedFields}
          </Tabs.Panel>
        </Tabs>
      )}
      {!resumable && sharedFields}
      <DialogError message={error} />
    </Dialog>
  );
}
