import { hasCodeParams, uncataloguedSummaryKey, type ErrorPresentation } from "../errorPresentation";
import { isUncataloguedFailure } from "../uncataloguedFailure";
import { rawFailureText } from "../errorText";
import { daemonMessage, messageCode } from "../daemonMessage";
import type { PlainMessageKey, Translate } from "../i18n/catalog";
import type { Language } from "../i18n/languages";
import { displayWirePath } from "../wirePath";
import { ChangeBodiesError } from "./content";

const SUMMARIES = new Map<string, PlainMessageKey>([
  ["invalid_path", "viewer.error.invalidPath"],
  ["file_not_found", "viewer.error.fileNotFound"],
  ["outside_scope", "viewer.error.outsideScope"],
  ["unsupported_file_type", "viewer.error.unsupportedFileType"],
  ["unsupported_file_type.unmerged", "viewer.error.unmerged"],
  ["source_changed", "viewer.error.sourceChanged"],
]);

export function viewerErrorKey(error: ErrorPresentation, path: string): PlainMessageKey | undefined {
  if (typeof error === "string" || error.params.path !== displayWirePath(path)) return undefined;
  if (error.code === "permission_denied" && error.params.detail) return "viewer.error.permissionDenied.summary";
  return SUMMARIES.get(messageCode(error.code, error.params));
}

/** The failure as one plain line. The system's own reason is raw text, kept out of this line (the
 * failure shows it behind Details where it has them, `rawFailureText`), so a daemon failure that is not
 * the viewer's own file is worded as its summary sentence, never with the raw `{detail}` filled in, and
 * one the application has no words for as its generic sentence. */
export function viewerErrorText(t: Translate, language: Language, error: ErrorPresentation, path: string, records: Parameters<typeof daemonMessage>[4]): string {
  const key = viewerErrorKey(error, path);
  if (key) return t(key);
  if (typeof error === "string") return error;
  if (isUncataloguedFailure(error)) return t(uncataloguedSummaryKey(error));
  return daemonMessage(language, error.code, error.params, error.message, records, true);
}

/** The `ChangeBodiesError` for a read of a change's bodies that failed with `error`: the viewer's own wording of it
 * (absent for a failure classified unexpected, which the viewer says as its generic sentence), the raw text the failure
 * carried, and, where the wording sets names as chips, the coded failure behind it. */
export function changeBodiesFailure(t: Translate, language: Language, error: ErrorPresentation, path: string, records: Parameters<typeof daemonMessage>[4]): ChangeBodiesError {
  const raw = rawFailureText(error);
  if (typeof error !== "string" && isUncataloguedFailure(error) && uncataloguedSummaryKey(error) === "error.unexpected.summary") return new ChangeBodiesError("failed", undefined, raw);
  const coded = typeof error !== "string" && !viewerErrorKey(error, path) && hasCodeParams(error) ? error : undefined;
  return new ChangeBodiesError("failed", viewerErrorText(t, language, error, path, records), raw, coded);
}
