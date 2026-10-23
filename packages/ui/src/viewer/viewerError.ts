import type { ErrorPresentation } from "../errorPresentation";
import { daemonMessage, messageCode } from "../daemonMessage";
import type { PlainMessageKey, Translate } from "../i18n/catalog";
import type { Language } from "../i18n/languages";
import { displayWirePath } from "../wirePath";

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
 * failure shows it behind Details where it has them), so a daemon failure that is not the viewer's
 * own file is worded as its summary sentence, never with the raw `{detail}` filled in. */
export function viewerErrorText(t: Translate, language: Language, error: ErrorPresentation, path: string, records: Parameters<typeof daemonMessage>[4]): string {
  const key = viewerErrorKey(error, path);
  if (key) return t(key);
  if (typeof error === "string") return error;
  return daemonMessage(language, error.code, error.params, error.message, records, true);
}
