import { createContext } from "react";

/** An entry that enters focus mode is absent while a focus view already represents that command. */
export const FocusModeContext = createContext(false);
