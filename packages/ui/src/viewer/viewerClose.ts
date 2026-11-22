import { createContext } from "react";

/** Closes the file viewer dialog the surface is shown in. Provided by `FileViewer` only: absent where the same surfaces
 * are hosted outside the dialog (the Find in files preview), which has nothing to close. Read by the text menu,
 * whose Quote closes the viewer so the input it wrote to shows. */
export const ViewerCloseContext = createContext<(() => void) | undefined>(undefined);
