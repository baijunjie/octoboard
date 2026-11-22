import { BROWSE_SLOTS } from "../browse/budget";
import { fileTooLarge } from "../browse/failure";
import { CodedError } from "../errors/codedError";
import type { FileContent, RangeFile } from "../protocol";
import type { RequestSender } from "../requests";
import { imageMediaTypeOfName } from "./content";

/**
 * The largest image the viewer reads in parts. An image is drawn from a `data:` URL, which holds it once as base64
 * text and once decoded, so the bound is on what a window keeps for one picture; a photograph, a screenshot of a
 * large display and an exported design all fit with room.
 */
export const MAX_IMAGE_BYTES = 32 * 1024 * 1024;

/**
 * What a part asks for: the most the daemon answers (1 MiB) rounded down to a multiple of three bytes, so that the
 * base64 of each part ends without padding and the parts join into the base64 of the whole file.
 */
const PART_BYTES = 1024 * 1024 - 1;

/** How many bytes a base64 text without line breaks decodes to. */
function decodedLength(data: string): number {
  const padding = data.endsWith("==") ? 2 : data.endsWith("=") ? 1 : 0;
  return (data.length / 4) * 3 - padding;
}

/** A live image read whole, as a read of the file would have answered it had it fitted. */
export interface LargeImage {
  file: FileContent;
  version: string;
}

/**
 * Reads in parts the live image a whole read refused for its size, for a viewer to show it as it shows a smaller one.
 * `refusal` is that read's rejection, and it is what this rejects with when the image is not to be read after all:
 * the rejection is not one for a file's size, `name` is not a binary image's, the file is past `MAX_IMAGE_BYTES`, or
 * it ended before the size its first part gave. The caller then shows what it shows for any file too large. A file
 * that the first part finds past `MAX_IMAGE_BYTES` is rejected with the refusal at that part's size, the one the file
 * has by then.
 *
 * The parts are asked one at a time in the window's viewer slot, each pinned to the version the first saw, so the
 * viewer moving on cancels the part that is out. `wanted` is asked before each part and ends the read (with
 * `refusal`) once the caller has moved on: a part sent after that would cancel the read that took its place. A part's
 * own failure (the file changed, the connection closed, the part was superseded) is rejected with as it is.
 */
export async function readLargeImage({ refusal, file, name, request, wanted }: {
  refusal: unknown;
  file: RangeFile;
  name: string;
  request: RequestSender;
  wanted: () => boolean;
}): Promise<LargeImage> {
  const mediaType = imageMediaTypeOfName(name);
  const tooLarge = fileTooLarge(refusal);
  if (!(refusal instanceof CodedError) || tooLarge === undefined || mediaType === undefined) throw refusal;
  if (tooLarge.size !== undefined && tooLarge.size > MAX_IMAGE_BYTES) throw refusal;

  const parts: string[] = [];
  let received = 0;
  let size: number | undefined;
  let version: string | undefined;
  do {
    if (!wanted()) throw refusal;
    const reply = await request({
      type: "read_file_range",
      file,
      offset: received,
      length: PART_BYTES,
      ...(version !== undefined && { version }),
      slot: BROWSE_SLOTS.viewer,
    });
    if (size === undefined) {
      size = reply.size;
      version = reply.version;
      if (size > MAX_IMAGE_BYTES) throw new CodedError(refusal.message, refusal.code, { ...refusal.params, size: String(size) });
    }
    const bytes = decodedLength(reply.data);
    if (received + bytes >= size) {
      parts.push(reply.data);
      received += bytes;
      break;
    }
    // A part short of a multiple of three bytes ends in padding, which only the last part may: its whole triples are
    // kept and the rest is asked for again.
    const kept = Math.floor(bytes / 3);
    if (kept === 0) throw refusal;
    parts.push(reply.data.slice(0, kept * 4));
    received += kept * 3;
  } while (received < size);

  return { file: { size: received, kind: "binary", media_type: mediaType, text: null, data: parts.join("") }, version: version! };
}
