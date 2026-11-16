import { BROWSE_SLOTS } from "../../browse/budget";
import type { RangeFile } from "../../protocol";
import type { RemoteFilesCapability } from "../../platform";
import type { RequestSender } from "../../requests";

/** The most bytes a part asks for, which is what the daemon answers at most. */
const PART_BYTES = 1024 * 1024;

/** The copy was given up on request: nothing is wrong, and nothing is said. */
export class DownloadCancelled extends Error {
  constructor() {
    super("The download was cancelled");
    this.name = "AbortError";
  }
}

function bytesOf(data: string): Uint8Array {
  const text = atob(data);
  const bytes = new Uint8Array(text.length);
  for (let i = 0; i < text.length; i += 1) bytes[i] = text.charCodeAt(i);
  return bytes;
}

/**
 * Copies a live file of the remote daemon to this computer and opens the copy: a first request for no bytes gives the
 * file's size, its version and the daemon's word on whether the system could launch it, all of which the shell, which
 * begins the copy next, is told: it refuses a launchable file, and one its name says is, with a `RemoteFileError` of
 * kind `launchable` before it stores anything. Each part, pinned to that first version so a file that
 * changes mid-copy ends it rather than becoming a file of two versions, is written before the next is asked for. The
 * copy is opened once every byte has arrived. A failure, or `signal` aborting, discards what arrived.
 *
 * Rejects with the failure of the request (a `CodedError`) or of the shell (a `RemoteFileError`), or with a
 * `DownloadCancelled` when `signal` aborted.
 */
export async function downloadFile({ file, name, request, files, onProgress, signal }: {
  file: RangeFile;
  /** The name the copy is given, which picks its application. */
  name: string;
  request: RequestSender;
  files: RemoteFilesCapability;
  /** Called with how much has arrived of how much there is, as each part is written. */
  onProgress: (received: number, total: number) => void;
  signal: AbortSignal;
}): Promise<void> {
  const part = async (offset: number, length: number, version?: string) => {
    if (signal.aborted) throw new DownloadCancelled();
    const reply = await request({ type: "read_file_range", file, offset, length, ...(version !== undefined && { version }), slot: BROWSE_SLOTS.download });
    if (signal.aborted) throw new DownloadCancelled();
    return reply;
  };
  const first = await part(0, 0);
  // The shell refuses a file the system could launch, or one its name says it would, before it stores anything.
  const id = await files.begin({ name, size: first.size, launchable: first.launchable === true });
  let opened = false;
  try {
    let received = 0;
    while (received < first.size) {
      const reply = await part(received, PART_BYTES, first.version);
      const bytes = bytesOf(reply.data);
      // An empty part before the size is reached leaves the shell to say the copy is short.
      if (bytes.length === 0) break;
      await files.write(id, bytes);
      received += bytes.length;
      onProgress(received, first.size);
    }
    if (signal.aborted) throw new DownloadCancelled();
    await files.open(id);
    opened = true;
  } finally {
    if (!opened) await files.discard(id);
  }
}
