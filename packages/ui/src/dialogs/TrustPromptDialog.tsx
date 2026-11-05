import { ActionableError } from "../errors/actionableError";
import React, { useId, useState } from "react";

import { ProviderMessage } from "../agents/ProviderMessage";
import { providerMessageKey } from "../agents/providers";
import { ChoiceWarning } from "../components/alerts/ChoiceWarning";
import { raiseError } from "../components/alerts/raiseError";
import { InlineReference } from "../i18n/InlineReference";
import { CodedError } from "../errors/codedError";
import { Message, useT } from "../i18n/react";
import { CODE, TRUST_REFUSED_CODES } from "../errors/codes";
import { display } from "../format/hostPath";
import { useDaemon, type TrustPrompt, useProviders } from "../store";
import { useHostPaths } from "../components/hooks/useHostPaths";
import { ProviderName } from "../components/text/ProviderName";
import type { ConfirmDialogProps } from "../components/dialog/ConfirmDialog";
import { Checkbox } from "../primitives";

function Path({ children }: { children: string }): React.ReactElement {
  return <InlineReference monospace wrap>{children}</InlineReference>;
}

/** What the trust dialog says: which agent asks, the question with the project's path, what
 * agreeing does (the agent records its own trust, and the permission Octoboard records is shared by
 * every agent), where the provider says so how far its record reaches, and the caution, in plain text like the rest.
 * The folder-wide choice is the checkbox under all of it (`TrustDirCheckbox`). */
function TrustPromptMessage({
  prompt,
  sessionTitle,
}: {
  prompt: TrustPrompt;
  sessionTitle: string | undefined;
}): React.ReactElement {
  // Shown as Settings lists trusted folders, with the home directory as `~`, so the folder named
  // here reads the same as the row it adds there.
  const paths = useHostPaths();
  const providers = useProviders();
  const agent = <ProviderName agent={prompt.agent} />;
  return (
    <>
      <p>
        {sessionTitle
          ? <Message id="dialog.trust.questionForSession" params={{ agent, session: <InlineReference dir="auto" wrap>{sessionTitle}</InlineReference>, path: <Path>{display(prompt.path, paths)}</Path> }} />
          : <Message id="dialog.trust.question" params={{ agent, path: <Path>{display(prompt.path, paths)}</Path> }} />}
      </p>
      <p><Message id="dialog.trust.behavior" params={{ agent }} /></p>
      {providerMessageKey(prompt.agent, "trustScope") && <p><ProviderMessage providers={providers} id={prompt.agent} name="trustScope" /></p>}
      <p><ProviderMessage providers={providers} id={prompt.agent} name="trustCaution" fallback="dialog.trust.caution.generic" /></p>
    </>
  );
}

/** The folder-wide choice: checked, the confirmation is answered and that folder is trusted instead
 * of this project alone, which reaches beyond the project asked about — every project under the
 * folder, including ones added there later, for every agent. While it is checked a warning Alert
 * under the option says so; the option's note follows its state and does not repeat the Alert. Shown
 * only when the daemon offers a folder. `directory` is already written as Settings writes it. */
function TrustDirCheckbox({
  directory,
  checked,
  onChange,
}: {
  directory: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}): React.ReactElement {
  const t = useT();
  const warningId = useId();
  return (
    <>
      {/* The pressable part is one line of `text-sm`, so an `::after` lifts what the pointer hits to the 24-pixel
          floor without changing what is drawn. The note's id comes first in the box's description, the Alert's after
          it while it is shown. */}
      <Checkbox
        variant="secondary"
        selected={checked}
        onSelectedChange={onChange}
        describedBy={checked ? warningId : undefined}
        contentClassName="relative after:absolute after:-inset-y-0.5 after:inset-x-0 after:content-['']"
        description={t(checked ? "dialog.trust.parentNote.on" : "dialog.trust.parentNote.off")}
      >
        <Message
          id="dialog.trust.parent"
          params={{
            directory: (
              <InlineReference monospace wrap>{directory}</InlineReference>
            ),
          }}
        />
      </Checkbox>
      {checked && (
        <ChoiceWarning
          id={warningId}
          title={t("dialog.trust.parentWarning.title")}
          remedy={t("dialog.trust.parentWarning.remedy")}
        >
          {t("dialog.trust.parentWarning.description")}
        </ChoiceWarning>
      )}
    </>
  );
}

/** What the dialog for a session waiting at its agent's folder-trust confirmation shows and does,
 * or nothing without a prompt. `sessionTitle` names the session when it is known. A hook rather
 * than a component, so the one dialog slot it shares with the session requests
 * (`PendingQuestionDialog`) stays mounted between them. */
export function useTrustPromptDialogProps(
  prompt: TrustPrompt | undefined,
  sessionTitle: string | undefined,
): ConfirmDialogProps | undefined {
  const t = useT();
  const { request, dismissTrustPrompt } = useDaemon();
  // Written as Settings lists trusted folders, with the home directory as `~`, so the folder the
  // checkbox names reads the same as the row it adds there.
  const paths = useHostPaths();
  // Held against the session it was ticked for, so the next prompt in the queue starts unticked
  // without an effect watching the one dialog that serves them all in turn.
  const [trustDirFor, setTrustDirFor] = useState<string>();
  if (!prompt) return undefined;

  const trustDir = prompt.trustDir !== null && trustDirFor === prompt.session;

  const answer = async (trustParentDir: boolean) => {
    // Dismissed whether or not the request worked: the daemon answers a confirmation once, so retrying
    // from this dialog can never succeed. The error is raised as a notification rather than shown inline for the same
    // reason. The window that pressed shows this reply's alert, raised even if the dialog closed meanwhile; the
    // daemon's session notice of the same failure is for the other windows. The exception is a refusal of the folder
    // before anything was pressed (`TRUST_REFUSED_CODES`): it is too broad, the project's path is not absolute,
    // the home directory is unknown, or the folder could not be checked in time. Nothing was answered, so the
    // error stays in the dialog and the user picks again.
    try {
      await request({
        type: "confirm_trust",
        session: prompt.session,
        remember: true,
        trust_parent_dir: trustParentDir,
      });
    } catch (err) {
      if (err instanceof CodedError && TRUST_REFUSED_CODES.includes(err.code)) throw err;
      if (err instanceof CodedError && err.code === CODE.trust_not_waiting) {
        // Nothing to tell for the plain go-ahead; but a folder the user asked to trust was not.
        if (trustParentDir) {
          raiseError(new ActionableError(t("dialog.trust.alreadyAnswered"), "session"), { title: t("dialog.trust.failedTitle"), keepTitle: true, session: prompt.session });
        }
      } else {
        // The answer itself went through when only carrying the trust entry over failed, so that failure keeps its
        // code's own title, the one its session notice has too.
        const answered = err instanceof CodedError && err.code === CODE.trust_not_carried_over;
        raiseError(err, { title: t("dialog.trust.failedTitle"), keepTitle: !answered, session: prompt.session });
      }
    }
    setTrustDirFor(undefined);
    dismissTrustPrompt(prompt.session);
  };

  return {
    // Not keyed on the session: remounting the modal for the next queued prompt would leave its
    // focus scope restoring focus to the element the previous one was holding, by then detached.
    resetKey: `trust:${prompt.session}`,
    // Wide enough that the paths it sets apart wrap no more than they have to.
    size: "lg",
    title: t("dialog.trust.title"),
    errorTitle: t("dialog.trust.failedTitle"),
    message: <TrustPromptMessage prompt={prompt} sessionTitle={sessionTitle} />,
    confirmLabel: t("dialog.trust.confirm"),
    cancelLabel: t("dialog.trust.notNow"),
    onCancel: () => {
      setTrustDirFor(undefined);
      dismissTrustPrompt(prompt.session);
    },
    onConfirm: () => answer(trustDir),
    extraControl: prompt.trustDir !== null && (
      <TrustDirCheckbox
        directory={display(prompt.trustDir, paths)}
        checked={trustDir}
        onChange={(checked) => setTrustDirFor(checked ? prompt.session : undefined)}
      />
    ),
  };
}
