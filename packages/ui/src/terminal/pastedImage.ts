import { raiseError } from "../components/alerts/raiseError";
import { ActionableError } from "../errors/actionableError";
import type { Translate } from "../i18n/catalog";
import type { RequestSender } from "../requests";
import type { TerminalControllerHandlers } from "./TerminalController";

/** The largest image sent, in bytes: what the daemon takes (`MAX_BYTES` in `apps/daemon/src/session/pasted_image.rs`). */
export const MAX_PASTED_IMAGE_BYTES = 10 * 1024 * 1024;

/** The image formats the daemon saves as they are; any other the clipboard holds is sent as a PNG. */
const SAVED_AS_IS = new Set(["image/png", "image/jpeg", "image/gif", "image/webp"]);

/** Whether the daemon saves `file` as a pasted image just as it is: one of its formats, and no larger than it takes. */
export function isSavedAsIs(file: Blob): boolean {
  return SAVED_AS_IS.has(file.type) && file.size <= MAX_PASTED_IMAGE_BYTES;
}

/** The image a paste event carries, if any: the file a browser or webview makes of an image on the clipboard. */
export function imageInPaste(data: DataTransfer | null): File | undefined {
  return Array.from(data?.files ?? []).find((file) => file.type.startsWith("image/"));
}

/** `path` as it is pasted: a path with whitespace is quoted as a POSIX shell does, as a terminal quotes a file dropped
 * on it, since an agent splits an unquoted one at the space. */
export function pastedPath(path: string): string {
  return /\s/.test(path) ? `'${path.replaceAll("'", "'\\''")}'` : path;
}

/** `image` as one of the formats the daemon saves: itself, or a PNG drawn from it. */
async function savedFormat(image: Blob): Promise<Blob> {
  if (SAVED_AS_IS.has(image.type)) return image;
  const bitmap = await createImageBitmap(image);
  try {
    const canvas = document.createElement("canvas");
    canvas.width = bitmap.width;
    canvas.height = bitmap.height;
    const context = canvas.getContext("2d");
    if (!context) throw new Error("no canvas to draw the image on");
    context.drawImage(bitmap, 0, 0);
    return await new Promise<Blob>((resolve, reject) =>
      canvas.toBlob((blob) => (blob ? resolve(blob) : reject(new Error("the image could not be encoded"))), "image/png"),
    );
  } finally {
    bitmap.close();
  }
}

/** The base64 text of `blob`'s bytes. */
export function base64Of(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result).slice(String(reader.result).indexOf(",") + 1));
    reader.onerror = () => reject(reader.error ?? new Error("the data could not be read"));
    reader.readAsDataURL(blob);
  });
}

/** Saves `image` on the daemon of `sessionId` and answers with its path there, as the daemon wrote it (unquoted); rejects
 * with the failure to show. */
export async function savePastedImagePath(image: Blob, sessionId: string, { t, request }: { t: Translate; request: RequestSender }): Promise<string> {
  const file = await savedFormat(image).catch((cause: unknown) => {
    throw new ActionableError(t("terminal.pasteImageUnconvertible"), "none", { cause });
  });
  if (file.size > MAX_PASTED_IMAGE_BYTES) {
    throw new ActionableError(t("terminal.pasteImageTooLarge", { limit_mib: MAX_PASTED_IMAGE_BYTES / 1024 / 1024 }), "none");
  }
  const reply = await request({ type: "save_pasted_image", session: sessionId, data: await base64Of(file) });
  return reply.path;
}

/** The `pasteImage` handlers of a terminal controller: `takes` is whether the session's agent takes images; `save`
 * sends the image to the session's daemon and answers with the path to paste; `read` is the platform's, where it has
 * one. Everything comes through `latest` and `readImage`, since the controller is built once. */
export function pasteImageHandlers(
  latest: () => { t: Translate; request: RequestSender; takesImages: (sessionId: string) => boolean },
  readImage: (() => Promise<Blob | undefined>) | undefined,
): NonNullable<TerminalControllerHandlers["pasteImage"]> {
  return {
    takes: (sessionId) => latest().takesImages(sessionId),
    read: readImage && (() => readImage()),
    async save(image, sessionId) {
      const { t, request } = latest();
      try {
        return pastedPath(await savePastedImagePath(image, sessionId, { t, request }));
      } catch (error) {
        raiseError(error, { title: t("terminal.pasteImageFailedTitle"), session: sessionId });
        return undefined;
      }
    },
  };
}
