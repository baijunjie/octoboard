import { DaemonRequestError } from "../daemon-client";
import { daemonMessage, type RecordNames } from "../errors/daemonMessage";
import { errorPresentation, type ErrorPresentation } from "../errors/errorPresentation";
import type { Translate } from "../i18n/catalog";
import type { Language } from "../i18n/languages";
import { REQUEST_SUPERSEDED, SOURCE_CHANGED, type MessageParams } from "../protocol";
import { formatFileSize } from "../format/fileSize";
import { abbreviateHome } from "../format/pathDisplay";
import { displayWirePath } from "../format/wirePath";

/** How a failed browse request is treated: given up for a newer one (nothing to show), worth asking
 * again at once, lost with the connection (asked again once it is back), or a failure to show. */
export type BrowseFailure =
  | { kind: "superseded" }
  | { kind: "changed" }
  | { kind: "disconnected" }
  | { kind: "failed"; message: ErrorPresentation; code: string };

/** The code the daemon client rejects a pending request with when its connection closes (`daemon-client.ts`). */
const CONNECTION_CLOSED = "connection_closed";

/** How many times a browse request is sent again at once when the daemon says what it read changed while it was read
 * (`source_changed`): reading it again gets a consistent copy. */
export const CHANGED_RETRIES = 2;

/** The browse budgets counted in bytes; the others (`pending_requests`, `change_entries`) are counts. */
const BYTE_LIMITS = new Set(["file_bytes", "reply_bytes", "git_output", "patch_bytes"]);

/** A browse error's params as they read: a path or a branch is a wire path, shown decoded, and a
 * budget and a size are worded as a size or a number. Not isolated here: `daemonMessage` isolates
 * every param it fills in, so only a message worded from the catalog directly isolates its own.
 * A formatted size or count is not worth isolating: the direct calls leave it bare, and it is
 * isolated through `daemonMessage` only because that isolates every param. */
function shownParams(language: Language, params: MessageParams, root: string | undefined): MessageParams {
  const shown: MessageParams = { ...params };
  for (const name of ["path", "branch", "left", "right"]) {
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
 * both sizes. A request rejected because the connection closed while it was out never got an answer, and the reconnect
 * asks again; any other rejection that is not a daemon `error` is shown as an unexpected failure.
 */
export function browseFailure(
  t: Translate,
  language: Language,
  err: unknown,
  records: RecordNames,
  /** The project's directory, shown for a path naming the directory itself. */
  root?: string,
): BrowseFailure {
  if (!(err instanceof DaemonRequestError)) {
    const unexpected = errorPresentation(err);
    return { kind: "failed", code: unexpected.code, message: unexpected };
  }
  if (err.code === CONNECTION_CLOSED) return { kind: "disconnected" };
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
  // The shaped params, not the raw ones: a structured render of the message must read them as the
  // plain text does.
  const message = daemonMessage(language, err.code, params, err.message, records);
  return { kind: "failed", code: err.code, message: errorPresentation(new DaemonRequestError(message, err.code, params)) };
}

/** `browseFailure` for a request about `project`, whose directory (home-abbreviated) it shows for a path naming the
 * directory itself; an unknown project shows the empty path as it is. */
export function projectBrowseFailure(
  t: Translate,
  language: Language,
  err: unknown,
  state: RecordNames,
  project: string,
): BrowseFailure {
  const root = state.projects.get(project)?.path;
  return browseFailure(t, language, err, state, root && abbreviateHome(root, state.homeDir));
}
