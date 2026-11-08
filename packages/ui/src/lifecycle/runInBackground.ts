import { createShellPreference } from "./shellSettings";

/** Whether closing the window leaves the app running in the background (the Settings switch the
 * desktop app offers) rather than quitting it. Off until the user turns it on. The shell keeps the choice
 * (`shellSettings.ts`) and answers the close button from it. */
export const runInBackground = createShellPreference("runInBackground");
