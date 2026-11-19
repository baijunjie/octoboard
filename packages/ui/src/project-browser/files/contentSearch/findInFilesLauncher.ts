import { createLauncher } from "../../../components/hooks/createLauncher";

/** Opens Find in files on the whole project of the project browser on screen, which registers itself while it is; the
 * window's shortcut (`app/useFindInFilesShortcut.ts`) calls it. Nothing happens while no browser is on screen. Kept apart
 * from the browser, which is loaded only once it is first shown, so the window can import it at startup. */
export const findInFilesLauncher = createLauncher<[]>();
