import { useEffect, type RefObject } from "react";

import { usePlatform } from "../platform/react";
import { dragsFiles, filesOfDrop } from "./fileDrag";
import type { TerminalController } from "./TerminalController";

/** The attribute that marks a terminal view while a file drag is over it and it would take the drop. */
const DROP_TARGET = "data-file-drop";

/**
 * Lets files be dropped on a terminal view, which pastes where the files are:
 *
 * - Where the platform reports the paths of dragged files (`fileDrop`: the local daemon's window of the desktop
 *   application), any terminal that can take input takes their paths as one paste, a shell's included. The shell
 *   reports positions only, so the terminal under the pointer is found by hit-testing.
 * - Elsewhere the page receives the drag itself, and only an agent session's terminal (`uploads`) takes it: the files are
 *   uploaded to its daemon and the paths it saved them at are pasted (`TerminalController.dropFiles`). Any other
 *   terminal refuses the drag.
 *
 * Either way the terminal view shows a drop target (ring and tint, see `TerminalSurface`) while it would take the drop, and not under a cover. `controllerRef` is read when a drag arrives,
 * as the controller is built after the view mounts.
 */
export function useTerminalFileDrop({
  controllerRef,
  containerRef,
  uploads,
}: {
  controllerRef: RefObject<TerminalController | undefined>;
  /** The element xterm mounts in, inside the terminal view (`TerminalSurface`). */
  containerRef: RefObject<HTMLElement | null>;
  /** Whether the terminal takes dropped files as uploads (an agent session's terminal). Read once. */
  uploads: boolean;
}): void {
  const { fileDrop } = usePlatform();

  useEffect(() => {
    const view = containerRef.current?.closest<HTMLElement>("[data-terminal-surface]");
    if (!view) return;
    // Not under a cover, which hides the terminal: it takes nothing then, as it draws no focus ring.
    const accepting = () => (view.hasAttribute("data-covered") ? undefined : controllerRef.current?.inputSessionId);
    const mark = (on: boolean) => view.toggleAttribute(DROP_TARGET, on);

    if (fileDrop) {
      const over = (position: { x: number; y: number }) => {
        const found = document.elementFromPoint(position.x, position.y);
        return found !== null && view.contains(found) ? accepting() : undefined;
      };
      const stop = fileDrop.listen((event) => {
        if (event.kind === "leave") return mark(false);
        const sessionId = over(event.position);
        mark(event.kind === "over" && sessionId !== undefined);
        if (event.kind === "drop" && sessionId !== undefined && event.paths.length > 0) controllerRef.current?.pasteDroppedPaths(event.paths, sessionId);
      });
      return () => {
        stop();
        mark(false);
      };
    }

    if (!uploads) return;
    const controller = new AbortController();
    const { signal } = controller;
    const offer = (event: DragEvent) => {
      if (!dragsFiles(event.dataTransfer)) return;
      if (accepting() === undefined) return mark(false);
      event.preventDefault();
      if (event.dataTransfer) event.dataTransfer.dropEffect = "copy";
      mark(true);
    };
    // Entering and leaving are counted, as every element crossed within the view fires both: the mark goes when the count
    // is back to zero, which a drag cancelled over the view (Escape, whose `dragleave` lands there too) also reaches
    // without a `dragend` the page never gets for a drag that came from the system.
    let depth = 0;
    view.addEventListener(
      "dragenter",
      (event) => {
        if (dragsFiles(event.dataTransfer)) depth++;
        offer(event);
      },
      { signal },
    );
    view.addEventListener("dragover", offer, { signal });
    view.addEventListener(
      "dragleave",
      (event) => {
        if (!dragsFiles(event.dataTransfer)) return;
        depth = Math.max(0, depth - 1);
        if (depth === 0) mark(false);
      },
      { signal },
    );
    view.addEventListener(
      "drop",
      (event) => {
        depth = 0;
        mark(false);
        const sessionId = accepting();
        if (!event.dataTransfer || !dragsFiles(event.dataTransfer) || sessionId === undefined) return;
        event.preventDefault();
        // Read now: the drag's data is gone once this handler returns.
        const files = filesOfDrop(event.dataTransfer);
        if (files.length > 0) controllerRef.current?.dropFiles(files, sessionId);
      },
      { signal },
    );
    return () => {
      controller.abort();
      mark(false);
    };
  }, []);
}
