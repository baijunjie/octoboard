import { createContext } from "react";

import type { FileDownload } from "./download/useFileDownload";

/** What the stand-in of a live file the viewer cannot show offers to do with it outside the viewer: open it in the
 * daemon's default application (the header's Default application, in the main window and a browser), or copy it to this
 * computer and open the copy (Download and open, in a remote window of the desktop shell). Absent where nothing can be
 * done with it, which includes every file the system could launch. Provided by the viewer for a subject that is a live
 * file; read by `BodyView` for the body of one. */
export type StandInAction = { kind: "open"; run: () => void } | { kind: "download"; download: FileDownload };

export const StandInActionContext = createContext<StandInAction | undefined>(undefined);
