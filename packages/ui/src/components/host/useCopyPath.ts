import { useT } from "../../i18n/react";
import { displayWirePath } from "../../format/wirePath";
import { useCopyWithNotice } from "./useCopyWithNotice";

/** What a Copy path does, for any row's action menu: puts the path on the clipboard and says so in
 * a notification, or says it could not. It takes the path in its wire form and copies it as the
 * UI's own text shows it: for a human to paste, so a byte that is not UTF-8 comes out as U+FFFD
 * rather than as the wire form's `%XX`. */
export function useCopyPath(): (wirePath: string) => void {
  const t = useT();
  const copy = useCopyWithNotice();
  return (wirePath) => copy(displayWirePath(wirePath), { copied: t("common.pathCopied"), failed: t("common.copyFailed") });
}
