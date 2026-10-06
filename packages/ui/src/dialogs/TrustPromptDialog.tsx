import { Alert, Code } from "@heroui/react";
import React from "react";

import { DaemonRequestError } from "../daemon-client";
import { useDaemon, type TrustPrompt } from "../store";
import { ConfirmDialog } from "./ConfirmDialog";

/** The code the daemon's `error` carries when the parent directory to trust is the filesystem root,
 * the home directory or one containing it. The trust dialog stays open on it, with the error shown. */
const TRUST_DIRECTORY_TOO_BROAD = "trust_directory_too_broad";

/** The code the daemon's `error` carries for a go-ahead to a trust screen that is no longer waiting:
 * answered already, or its session gone. There is nothing to tell the user then. */
const CLAUDE_TRUST_NOT_WAITING = "claude_trust_not_waiting";

/** A file path or directory in the dialog's text. A block one (the project's full path) breaks at any
 * character, so it wraps to the dialog's width rather than splitting at a hyphen; an inline one
 * breaks only where it would otherwise overflow, since a folder's name has no length limit. */
function Path({ children, block }: { children: string; block?: boolean }): React.ReactElement {
  return (
    <Code className={block ? "block w-full break-all select-text" : "[overflow-wrap:anywhere] select-text"}>
      {children}
    </Code>
  );
}

/** What the trust dialog says: the question with the project's path, a line on the folder-wide
 * choice when it is offered, since it reaches beyond the project asked about (every project in the
 * folder, including ones added there later), and the caution set apart. Kept short on purpose:
 * the buttons' names carry the rest. */
function TrustPromptMessage({
  prompt,
  sessionTitle,
}: {
  prompt: TrustPrompt;
  sessionTitle: string | undefined;
}): React.ReactElement {
  return (
    <>
      <div className="flex flex-col gap-2">
        <p>
          Claude Code is asking whether to trust this folder{sessionTitle ? ` for session "${sessionTitle}"` : ""}:
        </p>
        <Path block>{prompt.path}</Path>
      </div>
      {prompt.trustDir && (
        <p>
          &ldquo;{TRUST_PARENT_LABEL}&rdquo; also trusts every project in <Path>{shortDirectory(prompt.trustDir)}</Path>, including
          ones added there later.
        </p>
      )}
      {/* HeroUI's own warning tint rather than its default surface, which is the dialog's own fill and
          leaves the callout unmarked. */}
      <Alert status="warning" className="bg-warning-soft shadow-none">
        <Alert.Indicator />
        <Alert.Content>
          <Alert.Description className="text-foreground">
            {/* The icon is presentational, so the word that makes this a caution is spoken instead. */}
            <span className="sr-only">Caution: </span>A trusted folder&rsquo;s <Path>.claude/settings.json</Path> may
            pre-approve tool permissions.
          </Alert.Description>
        </Alert.Content>
      </Alert>
    </>
  );
}

/** The folder-wide choice's button. Short, so the three buttons fit one row; the message names the
 * folder, and the button's tooltip gives its full path. */
const TRUST_PARENT_LABEL = "Trust parent folder";

/** A directory shortened for the dialog's text: its last two components. The project's own full path,
 * which the folder holds, is shown above it. */
function shortDirectory(path: string): string {
  const parts = path.split("/").filter((part) => part !== "");
  return parts.length <= 2 ? path : `…/${parts.slice(-2).join("/")}`;
}

/** The dialog for a Claude Code session waiting at its workspace-trust screen. `sessionTitle` names
 * the session when it is known. */
export function TrustPromptDialog({
  prompt,
  sessionTitle,
}: {
  prompt: TrustPrompt;
  sessionTitle: string | undefined;
}): React.ReactElement {
  const { request, toastError, dismissTrustPrompt } = useDaemon();

  const answer = async (trustParentDir: boolean) => {
    // Dismissed whether or not the request worked: the daemon answers a screen once, so retrying
    // from this dialog can never succeed. The error is toasted rather than shown inline for the
    // same reason. A failure to answer also reaches the user as the daemon's own session notice,
    // which covers a dialog closed meanwhile. The one exception is a parent directory that is too
    // broad: nothing was answered, so the error stays in the dialog and the user picks again.
    try {
      await request({
        type: "confirm_claude_trust",
        session: prompt.session,
        remember: true,
        trust_parent_dir: trustParentDir,
      });
    } catch (err) {
      if (err instanceof DaemonRequestError && err.code === TRUST_DIRECTORY_TOO_BROAD) throw err;
      if (err instanceof DaemonRequestError && err.code === CLAUDE_TRUST_NOT_WAITING) {
        // Nothing to tell for the plain go-ahead; but a folder the user asked to trust was not.
        if (trustParentDir) {
          toastError(
            "The trust screen was already answered, so the folder was not trusted. Use the dialog again for the next screen.",
          );
        }
      } else {
        toastError((err as Error).message);
      }
    }
    dismissTrustPrompt(prompt.session);
  };

  return (
    <ConfirmDialog
      // Not keyed on the session: remounting the modal for the next queued prompt would leave its
      // focus scope restoring focus to the element the previous one was holding, by then detached.
      resetKey={prompt.session}
      // Wide enough for the three buttons on one row.
      size="lg"
      title="Trust this folder?"
      message={<TrustPromptMessage prompt={prompt} sessionTitle={sessionTitle} />}
      confirmLabel="Trust and continue"
      cancelLabel="Not now"
      onCancel={() => dismissTrustPrompt(prompt.session)}
      onConfirm={() => answer(false)}
      extraAction={
        prompt.trustDir
          ? {
              label: TRUST_PARENT_LABEL,
              title: prompt.trustDir,
              onClick: () => answer(true),
            }
          : undefined
      }
    />
  );
}
