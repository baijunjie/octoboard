import { createShellPreference } from "./shellSettings";

/** Whether the desktop app shows its menu bar icon (the notification area icon on Windows): the Settings switch the
 * desktop app offers. On until the user turns it off. The shell keeps the choice (`shellSettings.ts`) and hides the icon
 * from launch when it is off. */
export const showMenuBarIcon = createShellPreference("showMenuBarIcon");
