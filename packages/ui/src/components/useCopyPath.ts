import { useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import { useDaemon } from "../store";
import { displayWirePath } from "../wirePath";

/** What a Copy path does, on a change row and in the file viewer's header alike: puts the path on
 * the clipboard and says so in a toast, or says it could not. It takes the path in its wire form
 * and copies it as the UI's own text shows it: for a human to paste, so a byte that is not UTF-8
 * comes out as U+FFFD rather than as the wire form's `%XX`. Absent where the platform has no
 * clipboard (a plain browser served over insecure http), so the row or header offers no such
 * action. */
export function useCopyPath(): ((wirePath: string) => void) | undefined {
  const t = useT();
  const { clipboard } = usePlatform();
  const { toastNotice, toastError } = useDaemon();
  if (!clipboard) return undefined;
  return (wirePath) => {
    clipboard.writeText(displayWirePath(wirePath)).then(
      () => toastNotice(t("common.pathCopied")),
      () => toastError(t("common.copyFailed")),
    );
  };
}
