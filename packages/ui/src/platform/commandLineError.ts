/** Why the command-line tool could not be installed or removed, as the shell says: `refused` the system did not allow
 * the change (an administrator prompt declined), `not_a_link` something that is not a link stands at `path`,
 * `foreign_link` a link there leads elsewhere (its `detail` is the target), `failed` anything else. `path` is the one
 * concerned, where the shell names it; `detail` is the shell's own words, which cannot be localized (for a
 * `foreign_link`, the target). */
export class CommandLineError extends Error {
  readonly code: "refused" | "not_a_link" | "foreign_link" | "failed";
  readonly path: string | null;
  readonly detail: string;
  constructor(code: CommandLineError["code"], path: string | null, detail: string) {
    super(detail);
    this.code = code;
    this.path = path;
    this.detail = detail;
  }
}

const CODES: readonly CommandLineError["code"][] = ["refused", "not_a_link", "foreign_link", "failed"];

/** What a rejection of the shell's install or remove command is: a `CommandLineError` of the code it says, and `failed`
 * with its text as the detail for anything else (an older shell's plain string, an unknown code). */
export function commandLineError(failure: unknown): CommandLineError {
  const { code, path, detail } = (typeof failure === "object" && failure !== null ? failure : {}) as { code?: unknown; path?: unknown; detail?: unknown };
  const known = CODES.find((candidate) => candidate === code);
  const text = typeof detail === "string" ? detail : failure instanceof Error ? failure.message : typeof failure === "string" ? failure : String(failure);
  return new CommandLineError(known ?? "failed", typeof path === "string" ? path : null, text);
}
