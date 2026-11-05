import { basename, type PathStyle } from "../../format/hostPath";
import type { ShellTerminal } from "../../protocol";

/** What a directory title may start with, on a host that writes paths in each style. */
const PATH_START: Record<PathStyle, string> = {
  unix: String.raw`\/|~\/`,
  windows: String.raw`[A-Za-z]:[\\/]|[\\/]|~[\\/]`,
};
const TITLE_PATTERNS = Object.fromEntries(
  Object.entries(PATH_START).map(([style, start]) => [
    style,
    [new RegExp(String.raw`^[^\s/:]+:\s*((?:${start})[^\r\n]*)$`), new RegExp(String.raw`^((?:${start})\S*)$`)],
  ]),
) as Record<PathStyle, [RegExp, RegExp]>;

/** Shell prompts commonly prefix their directory title with a user and host. Keep the
 * directory recognizable in a narrow tab without rewriting user-chosen names or commands. */
export function shellTerminalTitle(terminal: Pick<ShellTerminal, "title" | "renamed">, style: PathStyle = "unix"): string {
  if (terminal.renamed) return terminal.title;
  // The bare form first: on a Windows host a drive path (`C:\x`) also reads as a prefix (`C:`) and a path.
  const [prefixed, bare] = TITLE_PATTERNS[style];
  const path = bare.exec(terminal.title)?.[1] ?? prefixed.exec(terminal.title)?.[1];
  if (!path) return terminal.title;
  return basename(path, style);
}
