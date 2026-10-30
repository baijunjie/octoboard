import type { Event, RequestBody } from "../protocol";

/** How many image reads are out at once in the window, whatever document they are for. Sized under
 * the daemon's bound of outstanding requests a connection together with the other browse requests
 * (see `MAX_WINDOW_IN_FLIGHT` in `browser/useDirectoryListings.ts`), with a unit beside it for the
 * read a newer one replaces in the slot. */
const MAX_WINDOW_IMAGE_READS = 1;
/** The slot of the lane's place. A read let go of while it is out keeps its place in the daemon
 * until the next image read, sent in the same slot, cancels it there; one the operating system
 * holds (a volume that stopped answering) keeps it until the volume answers. */
const SLOT = "document-image";

let inFlight = 0;
const waiting: (() => void)[] = [];

/**
 * Reads an image a document names, in the window's lane for them: a document can name dozens, and
 * each read sent at once would take the daemon's outstanding requests from the tree and the viewer.
 * Aborting `signal`, as the document's image does when it goes, drops a read still waiting, and
 * gives up a read already out: its place goes to the next read at once, which cancels it in the
 * daemon. The promise is rejected with the signal's reason then.
 */
export function readDocumentImage(
  request: (body: RequestBody) => Promise<Event>,
  project: string,
  path: string,
  signal: AbortSignal,
): Promise<Event> {
  return new Promise((resolve, reject) => {
    if (signal.aborted) return reject(signal.reason);
    let holding = false;
    const release = () => {
      if (!holding) return;
      holding = false;
      inFlight -= 1;
      waiting.shift()?.();
    };
    const start = () => {
      holding = true;
      inFlight += 1;
      request({ type: "read_project_file", project, path, slot: SLOT }).then(resolve, reject).finally(release);
    };
    signal.addEventListener(
      "abort",
      () => {
        const queued = waiting.indexOf(start);
        if (queued !== -1) waiting.splice(queued, 1);
        release();
        reject(signal.reason);
      },
      { once: true },
    );
    if (inFlight < MAX_WINDOW_IMAGE_READS) start();
    else waiting.push(start);
  });
}
