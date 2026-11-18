import { useLayoutEffect, useRef, useState } from "react";

import type { ViewedFile } from "../viewer/viewedFile";

/** The file a path in the terminal's output named, while the file viewer shows it over the window: `viewLinkedFile`
 * opens it, `setLinkedFile` replaces it from within the viewer, and `closeLinkedFile` closes it and puts focus back. */
export function useLinkedFile({ focusTerminal }: { focusTerminal: () => void }) {
  const [linkedFile, setLinkedFile] = useState<ViewedFile>();
  // Where the viewer returns focus to when it closes, as its opener gave it; the agent terminal when
  // the opener gave none or could not take it (its view is gone). Kept across the viewer opening
  // another file from within.
  const linkedReturnFocus = useRef<() => boolean>(undefined);
  const returnPending = useRef(false);
  useLayoutEffect(() => {
    if (linkedFile || !returnPending.current) return;
    returnPending.current = false;
    const restoreFocus = linkedReturnFocus.current;
    linkedReturnFocus.current = undefined;
    // The viewer's focus scope is gone. Giving focus back in this commit takes precedence over its deferred restore.
    if (!restoreFocus?.()) focusTerminal();
  }, [linkedFile, focusTerminal]);

  const viewLinkedFile = (file: ViewedFile, returnFocus?: () => boolean) => {
    returnPending.current = false;
    linkedReturnFocus.current = returnFocus;
    setLinkedFile(file);
  };
  const closeLinkedFile = () => {
    returnPending.current = true;
    setLinkedFile(undefined);
  };
  return { linkedFile, viewLinkedFile, setLinkedFile, closeLinkedFile };
}
