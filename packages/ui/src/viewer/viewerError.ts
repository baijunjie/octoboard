import type { ErrorPresentation } from "../errorPresentation";
import { errorText } from "../errorText";
import { messageCode } from "../daemonMessage";
import type { PlainMessageKey, Translate } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import { displayWirePath } from "../wirePath";

const SUMMARIES = new Map<string, PlainMessageKey>([
  ["invalid_path", "viewer.error.invalidPath"],
  ["file_not_found", "viewer.error.fileNotFound"],
  ["outside_scope", "viewer.error.outsideScope"],
  ["unsupported_file_type", "viewer.error.unsupportedFileType"],
  ["unsupported_file_type.unmerged", "viewer.error.unmerged"],
  ["source_changed", "viewer.error.sourceChanged"],
]);

export function viewerErrorKey(error: ErrorPresentation, path: string): PlainMessageKey | "viewer.error.permissionDenied" | undefined {
  if (typeof error === "string" || error.params.path !== displayWirePath(path)) return undefined;
  if (error.code === "permission_denied" && error.params.detail) return "viewer.error.permissionDenied";
  return SUMMARIES.get(messageCode(error.code, error.params));
}

export function viewerErrorText(t: Translate, error: ErrorPresentation, path: string): string {
  const key = viewerErrorKey(error, path);
  if (key === "viewer.error.permissionDenied" && typeof error !== "string") {
    return t(key, { detail: isolate(error.params.detail) });
  }
  return key && key !== "viewer.error.permissionDenied" ? t(key) : errorText(error);
}
