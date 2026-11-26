import type { FileDropCapability } from "../platform";
import { dragsFiles, filesOfDrop, type DroppedFile } from "./fileDrag";

/** The attribute that marks a drop target (a terminal view, the message composer) while a file drag is over it and it
 * would take the drop. */
export const FILE_DROP_TARGET = "data-file-drop";

/** What a drop target looks like while it carries `FILE_DROP_TARGET`: the accent ring, and a tint over the whole target,
 * which tells it from the keyboard focus ring alone. The target is positioned, so the overlay covers it above what it
 * holds (the xterm canvas, a text area), takes no pointer or drag event and moves nothing. */
export const FILE_DROP_TARGET_CLASS = "data-file-drop:ring-2 data-file-drop:ring-inset data-file-drop:ring-accent data-file-drop:after:pointer-events-none data-file-drop:after:absolute data-file-drop:after:inset-0 data-file-drop:after:z-1 data-file-drop:after:bg-(--tint-accent-bg)";

/**
 * Lets files be dropped on `element`, marking it (`FILE_DROP_TARGET`) while a drag over it would be taken, and only
 * while `accepting` says it takes one (not under a cover, not inert):
 *
 * - Where the platform reports the paths of dragged files (`fileDrop`: the local daemon's window of the desktop
 *   application), `paths` is given them. The shell reports positions only, so the element under the pointer is found by
 *   hit-testing.
 * - Elsewhere the page receives the drag itself, and `files` is given the files it read; without `files` the element
 *   takes no such drag.
 *
 * Returns the function that stops. Kept outside a component, whose compiler cannot lower the drag's counter.
 */
export function watchFileDrops(
  element: HTMLElement,
  fileDrop: FileDropCapability | undefined,
  { accepting, paths, files }: { accepting: () => boolean; paths: (paths: string[]) => void; files?: (files: DroppedFile[]) => void },
): () => void {
  const mark = (on: boolean) => element.toggleAttribute(FILE_DROP_TARGET, on);

  if (fileDrop) {
    const over = (position: { x: number; y: number }) => {
      const found = document.elementFromPoint(position.x, position.y);
      return found !== null && element.contains(found) && accepting();
    };
    const stop = fileDrop.listen((event) => {
      if (event.kind === "leave") return mark(false);
      const inside = over(event.position);
      mark(event.kind === "over" && inside);
      if (event.kind === "drop" && inside && event.paths.length > 0) paths(event.paths);
    });
    return () => {
      stop();
      mark(false);
    };
  }

  if (!files) return () => {};
  const controller = new AbortController();
  const { signal } = controller;
  const offer = (event: DragEvent) => {
    if (!dragsFiles(event.dataTransfer)) return;
    if (!accepting()) return mark(false);
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = "copy";
    mark(true);
  };
  // Entering and leaving are counted, as every element crossed within the target fires both: the mark goes when the count
  // is back to zero, which a drag cancelled over the target (Escape, whose `dragleave` lands there too) also reaches
  // without a `dragend` the page never gets for a drag that came from the system.
  let depth = 0;
  element.addEventListener("dragenter", (event) => {
    if (dragsFiles(event.dataTransfer)) depth++;
    offer(event);
  }, { signal });
  element.addEventListener("dragover", offer, { signal });
  element.addEventListener("dragleave", (event) => {
    if (!dragsFiles(event.dataTransfer)) return;
    depth = Math.max(0, depth - 1);
    if (depth === 0) mark(false);
  }, { signal });
  element.addEventListener("drop", (event) => {
    depth = 0;
    mark(false);
    if (!event.dataTransfer || !dragsFiles(event.dataTransfer) || !accepting()) return;
    event.preventDefault();
    // Read now: the drag's data is gone once this handler returns.
    const dropped = filesOfDrop(event.dataTransfer);
    if (dropped.length > 0) files(dropped);
  }, { signal });
  return () => {
    controller.abort();
    mark(false);
  };
}
