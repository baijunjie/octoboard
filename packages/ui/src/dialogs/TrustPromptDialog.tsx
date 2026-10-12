import { Alert, Checkbox, Code, Description, Label } from "@heroui/react";
import React, { useState } from "react";

import { AGENT_LABEL } from "../agents";
import { DaemonRequestError } from "../daemon-client";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import { TRUST_NOT_WAITING, TRUST_REFUSED_CODES, type Agent } from "../protocol";
import { abbreviateHome } from "../pathDisplay";
import { useDaemon, useDaemonStore, type TrustPrompt } from "../store";
import type { ConfirmDialogProps } from "./ConfirmDialog";

/** A file path or directory in the dialog's text. A block one (the project's path) breaks at any
 * character, so it wraps to the dialog's width rather than splitting at a hyphen; an inline one
 * breaks only where it would otherwise overflow, since a folder's name has no length limit. */
function Path({ children, block }: { children: string; block?: boolean }): React.ReactElement {
  return (
    <Code dir="ltr" className={block ? "block w-full break-all select-text" : "[overflow-wrap:anywhere] select-text"}>
      {children}
    </Code>
  );
}

/** What trusting lets the asking agent do with the project's own configuration: true for that agent
 * alone, since each one's trust gates something different. */
function Caution({ agent }: { agent: Agent }): React.ReactElement {
  const t = useT();
  switch (agent) {
    case "claude":
      return <Message id="dialog.trust.caution.claude" params={{ file: <Path>.claude/settings.json</Path> }} />;
    case "codex":
      return <>{t("dialog.trust.caution.codex", { agent: AGENT_LABEL[agent] })}</>;
    case "grok":
      return <>{t("dialog.trust.caution.grok", { agent: AGENT_LABEL[agent] })}</>;
  }
}

/** What the trust dialog says: which agent asks, the question with the project's path, what
 * agreeing does (the agent records its own trust, and the permission Octoboard records is shared by
 * every agent), for Codex that its record covers the whole repository, and the caution set apart.
 * The folder-wide choice is the checkbox under all of it (`TrustDirCheckbox`). */
function TrustPromptMessage({
  prompt,
  sessionTitle,
}: {
  prompt: TrustPrompt;
  sessionTitle: string | undefined;
}): React.ReactElement {
  const t = useT();
  // Shown as Settings lists trusted folders, with the home directory as `~`, so the folder named
  // here reads the same as the row it adds there.
  const home = useDaemonStore((s) => s.homeDir);
  const agent = AGENT_LABEL[prompt.agent];
  return (
    <>
      <div className="flex flex-col gap-2">
        <p>
          {sessionTitle
            ? t("dialog.trust.questionForSession", { agent, session: isolate(sessionTitle) })
            : t("dialog.trust.question", { agent })}
        </p>
        <Path block>{abbreviateHome(prompt.path, home)}</Path>
      </div>
      <p>{t("dialog.trust.behavior", { agent })}</p>
      {prompt.agent === "codex" && <p>{t("dialog.trust.codexRoot", { agent })}</p>}
      {/* HeroUI's own warning tint rather than its default surface, which is the dialog's own fill and
          leaves the callout unmarked. */}
      <Alert status="warning" className="bg-warning-soft shadow-none">
        <Alert.Indicator />
        <Alert.Content>
          <Alert.Description className="text-foreground">
            {/* The icon is presentational, so the word that makes this a caution is spoken instead. */}
            <span className="sr-only">{t("dialog.trust.cautionLabel")} </span>
            <Caution agent={prompt.agent} />
          </Alert.Description>
        </Alert.Content>
      </Alert>
    </>
  );
}

/** The folder-wide choice: checked, the confirmation is answered and that folder is trusted instead
 * of this project alone, which reaches beyond the project asked about — every project under the
 * folder, including ones added there later, for every agent. Shown only when the daemon offers a
 * folder. `directory` is already written as Settings writes it. */
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
  return (
    <Checkbox variant="secondary" isSelected={checked} onChange={onChange}>
      {/* Aligned to the top of a label that wraps, which a folder's name of any length can. The
          pressable part is one line of `text-sm`, so an `::after` lifts what the pointer hits to the
          24-pixel floor without changing what is drawn. */}
      <Checkbox.Content className="relative items-start after:absolute after:-inset-y-0.5 after:inset-x-0 after:content-['']">
        <Checkbox.Control>
          <Checkbox.Indicator />
        </Checkbox.Control>
        <Label>
          {/* The folder is named in plain text rather than through `Path`: HeroUI's `Code` is a
              react-aria `Text`, which inside a checkbox's field demands a slot of its own. */}
          <Message
            id="dialog.trust.parent"
            params={{
              directory: (
                <span dir="ltr" className="[overflow-wrap:anywhere]">
                  {directory}
                </span>
              ),
            }}
          />
        </Label>
      </Checkbox.Content>
      {/* A direct child of `Checkbox`, beside `Checkbox.Content`: from there it indents under the
          label and react-aria folds its id into the box's own `aria-describedby`. */}
      <Description>{t("dialog.trust.parentNote")}</Description>
    </Checkbox>
  );
}

/** What the dialog for a session waiting at its agent's folder-trust confirmation shows and does,
 * or nothing without a prompt. `sessionTitle` names the session when it is known. A hook rather
 * than a component, so the one dialog slot it shares with the requests for a console session
 * (`PendingQuestionDialog`) stays mounted between them. */
export function useTrustPromptDialogProps(
  prompt: TrustPrompt | undefined,
  sessionTitle: string | undefined,
): ConfirmDialogProps | undefined {
  const t = useT();
  const { request, toastError, dismissTrustPrompt } = useDaemon();
  // Written as Settings lists trusted folders, with the home directory as `~`, so the folder the
  // checkbox names reads the same as the row it adds there.
  const home = useDaemonStore((s) => s.homeDir);
  // Held against the session it was ticked for, so the next prompt in the queue starts unticked
  // without an effect watching the one dialog that serves them all in turn.
  const [trustDirFor, setTrustDirFor] = useState<string>();
  if (!prompt) return undefined;

  const trustDir = prompt.trustDir !== null && trustDirFor === prompt.session;

  const answer = async (trustParentDir: boolean) => {
    // Dismissed whether or not the request worked: the daemon answers a confirmation once, so retrying
    // from this dialog can never succeed. The error is toasted rather than shown inline for the
    // same reason. A failure to answer also reaches the user as the daemon's own session notice,
    // which covers a dialog closed meanwhile. The one exception is a parent directory that is too
    // broad: nothing was answered, so the error stays in the dialog and the user picks again.
    try {
      await request({
        type: "confirm_trust",
        session: prompt.session,
        remember: true,
        trust_parent_dir: trustParentDir,
      });
    } catch (err) {
      if (err instanceof DaemonRequestError && TRUST_REFUSED_CODES.includes(err.code)) throw err;
      if (err instanceof DaemonRequestError && err.code === TRUST_NOT_WAITING) {
        // Nothing to tell for the plain go-ahead; but a folder the user asked to trust was not.
        if (trustParentDir) {
          toastError(t("dialog.trust.alreadyAnswered"), prompt.session);
        }
      } else {
        toastError((err as Error).message, prompt.session);
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
        directory={abbreviateHome(prompt.trustDir, home)}
        checked={trustDir}
        onChange={(checked) => setTrustDirFor(checked ? prompt.session : undefined)}
      />
    ),
  };
}
