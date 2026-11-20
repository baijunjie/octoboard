import { useEffect } from "react";

import { usePlatform } from "../platform/react";
import { dragsFiles } from "../terminal/fileDrag";

/**
 * Keeps a file dropped anywhere in the page from navigating the webview or browser tab to it. A drag of files over the
 * page is refused (drop effect `none`) unless something under the pointer took it (`preventDefault` on its own
 * `dragover`), and a drop the page receives at all is cancelled. Not where the platform's shell takes the drag itself
 * (`fileDrop`), which the page never sees.
 */
export function useStrayFileDrops(): void {
  const { fileDrop } = usePlatform();

  useEffect(() => {
    if (fileDrop) return;
    const controller = new AbortController();
    const { signal } = controller;
    window.addEventListener(
      "dragover",
      (event) => {
        if (!dragsFiles(event.dataTransfer) || event.defaultPrevented) return;
        event.preventDefault();
        if (event.dataTransfer) event.dataTransfer.dropEffect = "none";
      },
      { signal },
    );
    window.addEventListener(
      "drop",
      (event) => {
        if (dragsFiles(event.dataTransfer)) event.preventDefault();
      },
      { signal },
    );
    return () => controller.abort();
  }, [fileDrop]);
}
