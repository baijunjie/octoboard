import type { ShellTerminal } from "../protocol";

/** Shell prompts commonly prefix their directory title with a user and host. Keep the
 * directory recognizable in a narrow tab without rewriting user-chosen names or commands. */
export function shellTerminalTitle(terminal: Pick<ShellTerminal, "title" | "renamed">): string {
  if (terminal.renamed) return terminal.title;
  const path = /^[^\s/:]+:\s*((?:\/|~\/)[^\r\n]*)$/.exec(terminal.title)?.[1]
    ?? /^((?:\/|~\/)\S*)$/.exec(terminal.title)?.[1];
  if (!path) return terminal.title;
  const trimmed = path.replace(/\/+$/, "");
  return trimmed.slice(trimmed.lastIndexOf("/") + 1) || "/";
}
