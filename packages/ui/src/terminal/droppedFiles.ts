import { raiseError } from "../components/alerts/raiseError";
import { ActionableError } from "../errors/actionableError";
import { CodedError } from "../errors/codedError";
import { droppedFilesParams, type DroppedFileEntry } from "../errors/droppedFileEntries";
import type { Translate } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import type { AttachmentLimits } from "../protocol";
import type { RequestSender } from "../requests";
import { dropUploads } from "./dropUploads";
import type { DroppedFile } from "./fileDrag";
import type { TerminalControllerHandlers } from "./TerminalController";
import { base64Of, isSavedAsIs, MAX_PASTED_IMAGE_BYTES, pastedPath, savePastedImagePath } from "./pastedImage";

/** The failures that concern the session and not the file, which end the drop's remaining uploads: they would fail the
 * same way, and one alert tells it. */
const SESSION_FAILURES = new Set(["session_not_running", "unknown_session"]);

/** Thrown when the user cancels the uploads, to end the file under way. */
class DropCancelled extends Error {}

/** The text pasted for dropped paths, as a terminal pastes them: each quoted where it needs it, one space between them
 * and one after the last, so what the user types next does not run into the final path. */
export function droppedPathsText(paths: string[]): string {
  return `${paths.map(pastedPath).join(" ")} `;
}

/** What a dropped or picked file became on the daemon: its path there, unquoted, and whether it was saved as an image
 * the session's agent takes as one (`save_pasted_image`) rather than attached as a file. */
export interface UploadedFile {
  file: DroppedFile;
  path: string;
  image: boolean;
}

/** What the uploads of files read from (`latest`), since a caller built once (a terminal controller) reads it late. */
export type UploadContext = () => { t: Translate; request: RequestSender; attachmentLimits: AttachmentLimits | null; takesImages: (sessionId: string) => boolean };

/** Sends `files` to the daemon of `sessionId` through the window's queue (`dropUploads`), one after the other, calling
 * `onUploaded` as each arrives, and answers with the ones that arrived, or `undefined` where none did. Before each file
 * it asks `wanted` whether the files still have a destination, and stops for the rest, quietly, once they have not or
 * the user cancelled. The files refused are told in one alert, listing each (`dropped_files_refused`), or as it is where
 * one was; a failure of the session itself ends the uploads and has an alert of its own. */
export function uploadFiles(
  files: DroppedFile[],
  sessionId: string,
  latest: UploadContext,
  { wanted = () => true, onUploaded }: { wanted?: () => boolean; onUploaded?: (uploaded: UploadedFile) => void } = {},
): Promise<UploadedFile[] | undefined> {
  return dropUploads.run(sessionId, files.map((dropped) => dropped.file.name), async (turn) => {
    const uploaded: UploadedFile[] = [];
    const refused: Array<{ name: string; error: unknown }> = [];
    const tellRefused = () => {
      const { t } = latest();
      const told = refused.splice(0);
      if (told.length === 1) return raiseError(told[0].error, { title: t("terminal.dropFileFailedTitle"), session: sessionId });
      if (told.length === 0) return;
      const entries = told.map(({ name, error }): DroppedFileEntry =>
        error instanceof CodedError ? { name, code: error.code, params: error.params, message: error.message } : { name, code: "unexpected_error", params: {}, message: error instanceof Error ? error.message : String(error) });
      raiseError(new ActionableError(t("daemon.dropped_files_refused"), "none", { code: "dropped_files_refused", params: droppedFilesParams(entries) }), { title: t("terminal.dropFilesFailedTitle"), session: sessionId });
    };
    for (const dropped of files) {
      if (turn.signal.aborted || !wanted()) {
        // Nothing is told for a destination that is gone, which the user has left.
        if (wanted()) tellRefused();
        return undefined;
      }
      const { t, request, attachmentLimits, takesImages } = latest();
      turn.begin(dropped.file.name);
      try {
        const sent = await uploadDropped(dropped, sessionId, { t, request, attachmentLimits, takesImages: takesImages(sessionId), signal: turn.signal, advance: turn.advance });
        const done = { file: dropped, ...sent };
        uploaded.push(done);
        turn.end();
        onUploaded?.(done);
      } catch (error) {
        turn.end();
        if (error instanceof DropCancelled) {
          tellRefused();
          return undefined;
        }
        if (error instanceof CodedError && SESSION_FAILURES.has(error.code)) {
          tellRefused();
          raiseError(error, { title: t("terminal.dropFileFailedTitle"), session: sessionId });
          break;
        }
        refused.push({ name: dropped.file.name, error });
      }
    }
    tellRefused();
    return uploaded.length > 0 ? uploaded : undefined;
  });
}

/** The `dropFiles` handler of a terminal controller: `upload` sends the files as `uploadFiles` does and answers with the
 * text to paste for the ones that arrived, or `undefined` where none did. Everything comes through `latest`, since the
 * controller is built once. */
export function dropFilesHandler(latest: UploadContext): NonNullable<TerminalControllerHandlers["dropFiles"]> {
  return {
    async upload(files, sessionId, wanted = () => true) {
      const uploaded = await uploadFiles(files, sessionId, latest, { wanted });
      return uploaded && droppedPathsText(uploaded.map((done) => done.path));
    },
  };
}

/** Sends one dropped file and answers with the daemon's unquoted path, and whether it went as an image: an image the daemon takes as it is goes the way a
 * pasted image does where the session's agent takes images (`takesImages`), so the agent takes it as an attached image,
 * and any other file is attached. Rejects with the failure to show, before anything is sent where the file is one the
 * daemon would refuse, and with `DropCancelled` once the user cancels. */
async function uploadDropped(
  { file, folder }: DroppedFile,
  sessionId: string,
  { t, request, attachmentLimits, takesImages, signal, advance }: { t: Translate; request: RequestSender; attachmentLimits: AttachmentLimits | null; takesImages: boolean; signal: AbortSignal; advance: (uploaded: number, size: number) => void },
): Promise<{ path: string; image: boolean }> {
  const { name } = file;
  const unreadable = (cause: unknown) => new ActionableError(t("daemon.dropped_file_unreadable", { name: isolate(name) }), "none", { cause, code: "dropped_file_unreadable", params: { name } });
  if (folder) throw new ActionableError(t("daemon.dropped_file_folder", { name: isolate(name) }), "none", { code: "dropped_file_folder", params: { name } });
  if (file.size === 0) throw new ActionableError(t("daemon.dropped_file_empty", { name: isolate(name) }), "none", { code: "dropped_file_empty", params: { name } });
  if (takesImages && isSavedAsIs(file)) {
    // What is not a coded failure here is the file's bytes failing to be read.
    const path = await savePastedImagePath(file, sessionId, { t, request }).catch((error: unknown) => {
      throw error instanceof CodedError ? error : unreadable(error);
    });
    return { path, image: true };
  }
  if (!attachmentLimits) {
    const limit_mib = MAX_PASTED_IMAGE_BYTES / 1024 / 1024;
    throw new ActionableError(t("daemon.dropped_file_images_only", { name: isolate(name), limit_mib }), "none", { code: "dropped_file_images_only", params: { name, limit_mib: String(limit_mib) } });
  }
  if (file.size > attachmentLimits.max_bytes) {
    const limit_mib = Math.floor(attachmentLimits.max_bytes / 1024 / 1024);
    throw new ActionableError(t("daemon.dropped_file_too_large", { name: isolate(name), limit_mib }), "none", { code: "dropped_file_too_large", params: { name, limit_mib: String(limit_mib) } });
  }
  return { path: await attachFile(file, sessionId, { request, max_part_bytes: attachmentLimits.max_part_bytes, signal, unreadable, advance }), image: false };
}

/** Uploads `file` in parts and saves it as an attachment of the session, answering with its path on the daemon's host,
 * unquoted. The upload is discarded, best effort, when any step fails or the user cancels. */
async function attachFile(
  file: File,
  sessionId: string,
  { request, max_part_bytes, signal, unreadable, advance }: { request: RequestSender; max_part_bytes: number; signal: AbortSignal; unreadable: (cause: unknown) => Error; advance: (uploaded: number, size: number) => void },
): Promise<string> {
  const { upload } = await request({ type: "create_attachment_upload", session: sessionId, name: file.name, size: file.size });
  try {
    advance(0, file.size);
    for (let offset = 0; offset < file.size; offset += max_part_bytes) {
      if (signal.aborted) throw new DropCancelled();
      // A part at a time, so a large file is never held in memory whole.
      const data = await base64Of(file.slice(offset, offset + max_part_bytes)).catch((cause: unknown) => {
        throw unreadable(cause);
      });
      await request({ type: "write_attachment_part", upload, offset, data });
      advance(Math.min(offset + max_part_bytes, file.size), file.size);
    }
    if (signal.aborted) throw new DropCancelled();
    const saved = await request({ type: "save_attachment", session: sessionId, upload });
    return saved.path;
  } catch (error) {
    await request({ type: "delete_attachment_upload", upload }).catch(() => {});
    throw error;
  }
}
