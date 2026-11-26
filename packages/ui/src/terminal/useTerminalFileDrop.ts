import { useEffect, type RefObject } from "react";

import { usePlatform } from "../platform/react";
import type { TerminalController } from "./TerminalController";
import { watchFileDrops } from "./watchFileDrops";

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
 * Either way the terminal view shows a drop target (the ring and tint of `watchFileDrops.ts`) while it would take the
 * drop, and not under a cover. `controllerRef` is read when a drag arrives, as the controller is built after the view
 * mounts.
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
    const sessionOf = () => (view.hasAttribute("data-covered") ? undefined : controllerRef.current?.inputSessionId);
    return watchFileDrops(view, fileDrop, {
      accepting: () => sessionOf() !== undefined,
      paths: (paths) => {
        const sessionId = sessionOf();
        if (sessionId !== undefined) controllerRef.current?.pasteDroppedPaths(paths, sessionId);
      },
      files: uploads
        ? (files) => {
          const sessionId = sessionOf();
          if (sessionId !== undefined) controllerRef.current?.dropFiles(files, sessionId);
        }
        : undefined,
    });
  }, []);
}
