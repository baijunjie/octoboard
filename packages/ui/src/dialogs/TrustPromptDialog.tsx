import { Alert, Code } from "@heroui/react";
import React from "react";

import { AGENT_LABEL } from "../agents";
import { DaemonRequestError } from "../daemon-client";
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
 * every agent), for Codex that its record covers the whole repository, a line on the folder-wide
 * choice when it is offered, since it reaches beyond the project asked about (every project in the
 * folder, including ones added there later), and the caution set apart. */
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
            ? t("dialog.trust.questionForSession", { agent, session: sessionTitle })
            : t("dialog.trust.question", { agent })}
        </p>
        <Path block>{abbreviateHome(prompt.path, home)}</Path>
      </div>
      <p>{t("dialog.trust.behavior", { agent })}</p>
      {prompt.agent === "codex" && <p>{t("dialog.trust.codexRoot", { agent })}</p>}
      {prompt.trustDir && (
        <p>
          <Message
            id="dialog.trust.parentNote"
            params={{ label: t("dialog.trust.parent"), directory: <Path>{abbreviateHome(prompt.trustDir, home)}</Path> }}
          />
        </p>
      )}
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
  if (!prompt) return undefined;

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
    dismissTrustPrompt(prompt.session);
  };

  return {
    // Not keyed on the session: remounting the modal for the next queued prompt would leave its
    // focus scope restoring focus to the element the previous one was holding, by then detached.
    resetKey: `trust:${prompt.session}`,
    // Wide enough for the three buttons on one row.
    size: "lg",
    title: t("dialog.trust.title"),
    message: <TrustPromptMessage prompt={prompt} sessionTitle={sessionTitle} />,
    confirmLabel: t("dialog.trust.confirm"),
    cancelLabel: t("dialog.trust.notNow"),
    onCancel: () => dismissTrustPrompt(prompt.session),
    onConfirm: () => answer(false),
    extraAction: prompt.trustDir
      ? {
          // Short, so the three buttons fit one row; the message names the folder, and the
          // button's tooltip gives its full path.
          label: t("dialog.trust.parent"),
          title: prompt.trustDir,
          onClick: () => answer(true),
        }
      : undefined,
  };
}
