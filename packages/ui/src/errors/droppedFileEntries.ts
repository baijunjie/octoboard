import { format, isMessageKey } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import type { Language } from "../i18n/languages";
import type { MessageParams } from "./codedError";
import { CODE, messageKey } from "./codes";

/** One file of a drop that was refused, as a `dropped_files_refused` alert lists it: `name` is the file's name, and
 * `code`, `params` and `message` the failure it met, whether the window refused it before sending (a `dropped_file_*`
 * code, whose sentence names the file itself) or the daemon failed it (any other, said after the name). */
export type DroppedFileEntry = { name: string; code: string; params: MessageParams; message: string };

/** The codes of the files the window refuses before sending, whose sentences carry the file's name. */
const OWN_SENTENCES: ReadonlySet<string> = new Set([CODE.dropped_file_folder, CODE.dropped_file_empty, CODE.dropped_file_too_large, CODE.dropped_file_images_only, CODE.dropped_file_unreadable]);

/** Whether the failure of `entry` is worded by the UI with the file's name in it; any other is said after the name. */
export function isOwnSentence(entry: DroppedFileEntry): boolean {
  return OWN_SENTENCES.has(entry.code);
}

/** The params of a `dropped_files_refused` alert: the entries travel as one JSON param, so a name holding a comma, or
 * any other character, is never split. */
export function droppedFilesParams(entries: DroppedFileEntry[]): MessageParams {
  return { files: JSON.stringify(entries) };
}

/** The entries of a `dropped_files_refused` alert, or `undefined` for any other failure or malformed params. */
export function droppedFileEntries(code: string, params: MessageParams): DroppedFileEntry[] | undefined {
  if (code !== CODE.dropped_files_refused || !params.files) return undefined;
  try {
    const entries: unknown = JSON.parse(params.files);
    if (!Array.isArray(entries) || entries.length === 0) return undefined;
    const valid = entries.every((entry: Partial<DroppedFileEntry> | null) =>
      !!entry && typeof entry.name === "string" && typeof entry.code === "string" && typeof entry.message === "string" && !!entry.params && typeof entry.params === "object");
    return valid ? entries as DroppedFileEntry[] : undefined;
  } catch {
    return undefined;
  }
}

/** The entries of a `dropped_files_refused` alert in plain text, one to a line: the UI's own sentence for a file it
 * refused, the file's name and the failure's reason (`reason`, worded in the language) for any other. */
export function droppedFilesText(language: Language, entries: DroppedFileEntry[], reason: (entry: DroppedFileEntry) => string = (entry) => entry.message): string {
  return entries.map((entry) => {
    const key = messageKey(entry.code);
    if (isOwnSentence(entry) && isMessageKey(key)) {
      const limit = Number(entry.params.limit_mib);
      return format(language, key, { name: isolate(entry.name), ...(Number.isFinite(limit) && entry.params.limit_mib ? { limit_mib: limit } : {}) });
    }
    return format(language, "daemon.dropped_files_refused.row", { name: isolate(entry.name), reason: reason(entry) });
  }).join("\n");
}
