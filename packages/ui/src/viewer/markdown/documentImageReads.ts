import { BROWSE_SLOTS, documentImageLane } from "../../browse/budget";
import type { ReplyOf, RequestSender } from "../../requests";

/**
 * Reads an image a document names, in the window's lane for them: a document can name dozens, and
 * each read sent at once would take the daemon's outstanding requests from the tree and the viewer.
 * Aborting `signal`, as the document's image does when it goes, drops a read still waiting, and
 * gives up a read already out: its place goes to the next read at once, which cancels it in the
 * daemon, as every image read goes in one slot. One the operating system holds (a volume that
 * stopped answering) keeps its unit in the daemon until the volume answers. The promise is rejected
 * with the signal's reason then.
 */
export function readDocumentImage(
  request: RequestSender,
  project: string,
  path: string,
  signal: AbortSignal,
): Promise<ReplyOf<"read_project_file">> {
  const slot = BROWSE_SLOTS.documentImage;
  return documentImageLane.send(() => request({ type: "read_project_file", project, path, slot }), { signal });
}
