import { useHost } from "../platform/react";
import { findInFilesLauncher } from "../project-browser/files/contentSearch/findInFilesLauncher";
import { matchesCommand } from "../shortcuts/store";
import { useWindowShortcut } from "../shortcuts/useWindowShortcut";

/** Opens Find in files from the keyboard (⇧⌘F, Ctrl+Shift+F by default), on the whole project of the project browser
 * on screen: the browser cannot import the shortcut registry, so it registers itself with `findInFilesLauncher`. */
export function useFindInFilesShortcut(): void {
  const host = useHost();
  // A held key's repeats are taken too, so they reach no terminal, but open nothing more.
  useWindowShortcut((event) => (matchesCommand("findInFiles", event, host) ? { repeat: event.repeat } : undefined), ({ repeat }) => {
    if (!repeat) findInFilesLauncher.open();
  });
}
