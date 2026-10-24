import { Spinner } from "@heroui/react";
import React, { useId } from "react";

import { AGENT_LABEL } from "../agents";
import { agentIconPickerOptions } from "../components/AgentIcon";
import { DaemonErrorMessage, errorDetails, errorRemedy } from "../components/DaemonErrorMessage";
import { ErrorAlert } from "../components/ErrorAlert";
import { InlineReference } from "../components/InlineReference";
import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { useFocusHandoff } from "../components/useFocusHandoff";
import { EXAMPLE_CONVERSATION_ID } from "../conversationId";
import { Message, useT } from "../i18n/react";
import type { Agent } from "../protocol";
import { sessionAccountName } from "../sessionLabel";
import { useDaemonStore } from "../store";
import { AgentAccountField } from "./AgentAccountField";
import { LaunchSettingsFields } from "./LaunchSettingsFields";
import type { useLaunchChoice } from "./useLaunchChoice";
import type { useResumeTarget } from "./useResumeTarget";
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
 * `shownProblem` is the ID's own problem once the user has tried to submit, and `idFieldRef` is the
 * ID field's wrapper, for the caller to focus it. What the daemon refuses the submit for is not
 * shown here: it is the error alert dialog's.
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
  resume: ReturnType<typeof useResumeTarget>;
  name: string;
  onNameChange: (value: string) => void;
  recordName: React.ReactNode;
  launch: ReturnType<typeof useLaunchChoice>["launch"];
  blocked: boolean;
}): React.ReactElement {
  const t = useT();
  const lookupAlertId = useId();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const defaultNames = useDaemonStore((s) => s.settings.default_account_names);
  const handoff = useFocusHandoff(() => idFieldRef.current?.querySelector("input")?.focus());
  const idProblems = {
    required: t("dialog.session.resumeRequired"),
    invalid: <Message id="dialog.session.resumeInvalid" params={{ example: <InlineReference monospace wrap>{EXAMPLE_CONVERSATION_ID}</InlineReference> }} />,
  };
  return (
    <>
      <div ref={idFieldRef}>
        <TextInput
          label={t("dialog.session.resumeId")}
          value={resumeId}
          onChange={onResumeIdChange}
          placeholder={t("dialog.session.resumeExample", { example: EXAMPLE_CONVERSATION_ID })}
          errorMessage={shownProblem ? idProblems[shownProblem] : undefined}
          describedBy={!shownProblem && problem ? lookupAlertId : undefined}
          endAdornment={looking ? <Spinner size="sm" aria-hidden /> : null}
          dir="ltr"
          isDisabled={blocked}
        />
        {/* What the lookup the field makes on its own found against the ID is not a verdict on its text
            (that is the field's error, above) but the state of the lookup, so it is an Alert directly
            under the field; it is announced as it appears. */}
        {!shownProblem && problem && (
          <ErrorAlert
            id={lookupAlertId}
            className="mt-2"
            announce
            title={t("dialog.session.resumeProblemTitle")}
            remedy={errorRemedy(problem)}
            details={errorDetails(problem)}
          >
            {typeof problem === "string" ? problem : <DaemonErrorMessage error={problem} part="summary" />}
          </ErrorAlert>
        )}
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
            <LaunchSettingsFields {...launch} note={t("dialog.session.launchInheritanceNote")} isDisabled={blocked} />
          </>
        )}
      </div>
    </>
  );
}
