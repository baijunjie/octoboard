import { createContext } from "react";

/** Closes the dialog the viewer's text surfaces are shown in: the file viewer, or the Find in files modal whose preview
 * holds one. Provided by those two only: absent where a surface is hosted with no dialog of its own to close (the
 * gallery's bare ones). Read by the text menu, whose Quote closes that dialog so the input it wrote to shows. */
export const SurfaceCloseContext = createContext<(() => void) | undefined>(undefined);
