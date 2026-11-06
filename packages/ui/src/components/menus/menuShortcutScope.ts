import { createContext } from "react";

/** Menu content can outlive its open state while the popover animates out. */
export const MenuShortcutScope = createContext(true);
