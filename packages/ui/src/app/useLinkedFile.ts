import { useRef, useState } from "react";

import type { ViewedFile } from "../viewer/viewedFile";

/** The file a path in the terminal's output named, while the file viewer shows it over the window: `viewLinkedFile`
 * opens it, `setLinkedFile` replaces it from within the viewer, and `closeLinkedFile` closes it and puts focus back. */
export function useLinkedFile({ focusTerminal }: { focusTerminal: () => void }) {
  const [linkedFile, setLinkedFile] = useState<ViewedFile>();
  // Where the viewer returns focus to when it closes, as its opener gave it; the agent terminal when
  // the opener gave none or could not take it (its view is gone). Kept across the viewer opening
  // another file from within.
  const linkedReturnFocus = useRef<() => boolean>(undefined);
  const viewLinkedFile = (file: ViewedFile, returnFocus?: () => boolean) => {
    linkedReturnFocus.current = returnFocus;
    setLinkedFile(file);
  };
  const closeLinkedFile = () => {
    setLinkedFile(undefined);
    // Back where the click came from, once the viewer has gone and taken its own focus
    // restoring with it.
    queueMicrotask(() => {
      if (!linkedReturnFocus.current?.()) focusTerminal();
    });
  };
  return { linkedFile, viewLinkedFile, setLinkedFile, closeLinkedFile };
}
