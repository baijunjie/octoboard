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

/** What the trust dialog says. The folder-wide choice is spelled out because it reaches beyond the
 * project being asked about: every project under the folder, including repositories the hub clones
 * or adds there later, is trusted without a question, and the permissions and hooks in their
 * `.claude/settings.json` then apply without asking. */
function trustPromptMessage(prompt: TrustPrompt, sessionLabel: string): string {
  const folderWide = prompt.trustDir
    ? ` "Trust all projects in ${shortDirectory(prompt.trustDir)}" trusts every project under ${prompt.trustDir} — those already there and any added there later, repositories the hub clones or adds into it included — without asking again. You can stop that again under "Trusted folders" in the sidebar.`
    : "";
  return `Claude Code is asking whether to trust ${prompt.path}${sessionLabel}. Octoboard can answer for you: "Trust and continue" trusts this project's sessions from now on.${folderWide} A folder's .claude/settings.json may pre-approve tool permissions, and trusting it applies them without asking. "Not now" leaves the question in the terminal for you to answer.`;
}

/** A directory shortened for a button: its last two components. The full path is in the message. */
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
      title="Trust this folder?"
      message={trustPromptMessage(prompt, sessionTitle ? ` for session "${sessionTitle}"` : "")}
      confirmLabel="Trust and continue"
      cancelLabel="Not now"
      onCancel={() => dismissTrustPrompt(prompt.session)}
      onConfirm={() => answer(false)}
      extraAction={
        prompt.trustDir
          ? {
              label: `Trust all projects in ${shortDirectory(prompt.trustDir)}`,
              title: prompt.trustDir,
              onClick: () => answer(true),
            }
          : undefined
      }
    />
  );
}
