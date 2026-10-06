import { createContext, useContext, type DependencyList } from "react";

import { useRefocusIfLost } from "../dialogs/Dialog";

/** Where `SettingsDialog` wants keyboard focus back when a control inside a section unmounts under
 * it. */
export const SettingsFocusContext = createContext<() => HTMLElement | null>(() => null);

/** For a section that can unmount the control holding focus (a folder's Remove, the Enable button
 * once answered): pass what makes that happen as `deps`, and focus goes back to the dialog's
 * selected section entry instead of falling to `<body>`, outside the dialog. */
export function useSectionRefocus(deps: DependencyList): void {
  useRefocusIfLost(useContext(SettingsFocusContext), deps);
}
