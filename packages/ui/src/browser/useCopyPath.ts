import { useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import { useDaemon } from "../store";

/** What a change row's Copy path does: puts the path on the clipboard and says so in a toast, or
 * says it could not. Absent where the platform has no clipboard (a plain browser served over
 * insecure http), so the row offers no such action. */
export function useCopyPath(): ((path: string) => void) | undefined {
  const t = useT();
  const { clipboard } = usePlatform();
  const { toastNotice, toastError } = useDaemon();
  if (!clipboard) return undefined;
  return (path) => {
    clipboard.writeText(path).then(
      () => toastNotice(t("git.row.pathCopied")),
      () => toastError(t("git.row.copyFailed")),
    );
  };
}
