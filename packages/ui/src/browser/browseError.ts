import { DaemonRequestError } from "../daemon-client";
import { daemonMessage } from "../daemonMessage";
import type { Translate } from "../i18n/catalog";
import type { Language } from "../i18n/languages";
import { REQUEST_SUPERSEDED, SOURCE_CHANGED, type MessageParams } from "../protocol";
import type { State } from "../store";
import { formatFileSize } from "../viewer/format";
import { displayWirePath } from "../wirePath";

/** How a failed browse request is treated: given up for a newer one (nothing to show), worth asking
 * again at once, lost with the connection (asked again once it is back), or a failure to show. */
export type BrowseFailure =
  | { kind: "superseded" }
  | { kind: "changed" }
  | { kind: "disconnected" }
  | { kind: "failed"; message: string; code: string };

/** The browse budgets counted in bytes; the others (`pending_requests`, `change_entries`) are counts. */
const BYTE_LIMITS = new Set(["file_bytes", "reply_bytes", "git_output", "patch_bytes"]);

/** A browse error's params as they read: a path or a branch is a wire path, shown decoded, and a
 * budget and a size are worded as a size or a number. */
function shownParams(language: Language, params: MessageParams, root: string | undefined): MessageParams {
  const shown: MessageParams = { ...params };
  for (const name of ["path", "branch"]) {
    if (shown[name] !== undefined) shown[name] = displayWirePath(shown[name]);
  }
  // The empty path is the project's own directory, which reads better as where it is.
  if (shown.path === "" && root !== undefined) shown.path = root;
  const amount = (value: string | undefined) => {
    const number = Number(value);
    if (value === undefined || !Number.isFinite(number)) return value;
    return BYTE_LIMITS.has(params.limit ?? "") ? formatFileSize(language, number) : new Intl.NumberFormat(language).format(number);
  };
  if (params.max !== undefined) shown.max = amount(params.max)!;
  if (params.size !== undefined) shown.size = amount(params.size)!;
  return shown;
}

/**
 * Sorts out a browse request's rejection, worded in the current language from its code with its
 * params as they read (`shownParams`); a file too large to read gets a sentence of its own, with
 * both sizes. Anything that is not a daemon `error` means the request never got an answer: the
 * connection went while it was out, and the reconnect asks again.
 */
export function browseFailure(
  t: Translate,
  language: Language,
  err: unknown,
  records: Pick<State, "consoles" | "projects" | "sessions">,
  /** The project's directory, shown for a path naming the directory itself. */
  root?: string,
): BrowseFailure {
  if (!(err instanceof DaemonRequestError)) return { kind: "disconnected" };
  if (err.code === REQUEST_SUPERSEDED) return { kind: "superseded" };
  if (err.code === SOURCE_CHANGED) return { kind: "changed" };
  const params = shownParams(language, err.params, root);
  if (err.code === "limit_exceeded" && err.params.limit === "file_bytes") {
    return {
      kind: "failed",
      code: err.code,
      message:
        err.params.size !== undefined
          ? t("browser.error.fileTooLarge", { size: params.size!, max: params.max })
          : t("browser.error.fileTooLargeNoSize", { max: params.max }),
    };
  }
  if (err.code === "limit_exceeded" && err.params.limit === "patch_bytes") {
    return { kind: "failed", code: err.code, message: t("browser.error.patchTooLarge", { max: params.max }) };
  }
  if (err.code === "unsupported_file_type" && err.params.file_type === "unmerged") {
    return { kind: "failed", code: err.code, message: t("browser.error.unmerged", { path: params.path ?? "" }) };
  }
  return { kind: "failed", code: err.code, message: daemonMessage(language, err.code, params, err.message, records) };
}
