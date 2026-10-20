import { Spinner } from "@heroui/react";
import React from "react";

import { AGENT_LABEL } from "../agents";
import { agentIconPickerOptions } from "../components/AgentIcon";
import { DaemonErrorMessage } from "../components/DaemonErrorMessage";
import { InlineChip } from "../components/InlineChip";
import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { useFocusHandoff } from "../components/useFocusHandoff";
import { EXAMPLE_CONVERSATION_ID } from "../conversationId";
import type { DaemonRequestError } from "../daemon-client";
import { errorPresentation } from "../errorPresentation";
import { Message, useT } from "../i18n/react";
import type { Agent } from "../protocol";
import { sessionAccountName } from "../sessionLabel";
import { useDaemonStore } from "../store";
import { AgentAccountField } from "./AgentAccountField";
import { LaunchSettingsFields } from "./LaunchSettingsFields";
import type { useLaunchChoice } from "./useLaunchChoice";
import { useHeldElsewhere, type useResumeTarget } from "./useResumeTarget";
import { TextInput } from "./TextInput";

/** An agent and account fixed by what holds the conversation, shown as the picker pair a new session
 * has, disabled. Each is a single option, so the field reads as the enabled one does; the account
 * half is there as in a new session's, unless the account has no name to show. */
function HeldAgentAccount({ agent, accountName }: { agent: Agent; accountName: string | undefined }): React.ReactElement {
  const t = useT();
  const agentAvailability = useDaemonStore((s) => s.agentAvailability);
  const addedAgents = useDaemonStore((s) => s.settings.added_agents);
  return (
    <AgentAccountField
      label={t("dialog.session.agent")}
      options={agentIconPickerOptions(t, agentAvailability, addedAgents, agent).filter((option) => option.value === agent)}
      value={agent}
      onChange={() => {}}
      account={accountName === undefined ? undefined : {
        label: t("dialog.agentSettings.account", { agent: AGENT_LABEL[agent] }),
        options: [{ value: "held", label: accountName }],
        value: "held",
        onChange: () => {},
      }}
      isDisabled
    />
  );
}

/**
 * The Resume tab's fields: the Session ID with the live region saying what was found, then the
 * fields that depend on the `resume` target (see `useResumeTarget`). A session of the project shows
 * its name (editable only while it is archived, as `name` and `onNameChange`), agent and account,
 * fixed; the launch settings are the session's own, so none are offered. An agent's record shows
 * `recordName`, the Name field of a new session, the agent and account that hold it, fixed, and
 * the launch settings (`launch`). With no target the ID alone shows.
 *
 * `shownProblem` is the ID's own problem once the user has tried to submit; `refusal` is what the
 * daemon refused the last submit for, and `idFieldRef` is the ID field's wrapper, for the caller to
 * focus it.
 *
 * When the kind of target changes under a focused field (the archived Name turning disabled, the
 * record's fields leaving), focus would be lost to `<body>` or stay on a disabled field, so it
 * goes to the ID input, which stays.
 */
export function ResumeConversationFields({
  idFieldRef,
  resumeId,
  onResumeIdChange,
  shownProblem,
  refusal,
  resume: { target, looking, problem, status },
  name,
  onNameChange,
  recordName,
  launch,
  blocked,
}: {
  idFieldRef: React.RefObject<HTMLDivElement | null>;
  resumeId: string;
  onResumeIdChange: (value: string) => void;
  shownProblem: "required" | "invalid" | undefined;
  refusal: DaemonRequestError | undefined;
  resume: ReturnType<typeof useResumeTarget>;
  name: string;
  onNameChange: (value: string) => void;
  recordName: React.ReactNode;
  launch: ReturnType<typeof useLaunchChoice>["launch"];
  blocked: boolean;
}): React.ReactElement {
  const t = useT();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const defaultNames = useDaemonStore((s) => s.settings.default_account_names);
  const handoff = useFocusHandoff(() => idFieldRef.current?.querySelector("input")?.focus());
  const idProblems = {
    required: t("dialog.session.resumeRequired"),
    invalid: <Message id="dialog.session.resumeInvalid" params={{ example: <InlineChip monospace wrap size="inherit">{EXAMPLE_CONVERSATION_ID}</InlineChip> }} />,
  };
  const heldElsewhere = useHeldElsewhere();
  // The open-time refusal names the holder as the lookup does, when the store knows it.
  const refusalProblem = refusal && ((refusal.code === "conversation_outside_project" ? heldElsewhere(refusal.params.session) : undefined) ?? <DaemonErrorMessage error={errorPresentation(refusal)} />);
  const lookupProblem = typeof problem === "string" ? problem : problem && <DaemonErrorMessage error={problem} />;
  return (
    <>
      <div ref={idFieldRef}>
        <TextInput
          label={t("dialog.session.resumeId")}
          value={resumeId}
          onChange={onResumeIdChange}
          placeholder={t("dialog.session.resumeExample", { example: EXAMPLE_CONVERSATION_ID })}
          errorMessage={shownProblem ? idProblems[shownProblem] : refusalProblem ?? lookupProblem}
          endAdornment={looking ? <Spinner size="sm" aria-hidden /> : null}
          dir="ltr"
          isDisabled={blocked}
        />
        {/* Always mounted, so an outcome that appears is announced; the fields that follow it
            are what is seen. */}
        <StatusAnnouncer text={status} />
      </div>
      <div className="contents" {...handoff}>
        {target && target.kind !== "record" && (
          <>
            <TextInput
              label={t("common.name")}
              value={name}
              onChange={onNameChange}
              placeholder={target.session.title}
              isDisabled={target.kind !== "archived"}
            />
            <HeldAgentAccount agent={target.session.agent} accountName={sessionAccountName(t, target.session, accounts, defaultNames)} />
          </>
        )}
        {target?.kind === "record" && (
          <>
            {recordName}
            <HeldAgentAccount agent={target.agent} accountName={target.accountName} />
            <LaunchSettingsFields {...launch} note={t("dialog.session.launchSettingsNote")} isDisabled={blocked} />
          </>
        )}
      </div>
    </>
  );
}
