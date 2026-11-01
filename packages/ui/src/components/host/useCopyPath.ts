import { useT } from "../../i18n/react";
import { ActionableError } from "../../errors/actionableError";
import { usePlatform } from "../../platform/react";
import { useDaemon } from "../../store";
import { displayWirePath } from "../../format/wirePath";
import { copyText } from "./copyText";
import { raiseError } from "../alerts/raiseError";

/** What a Copy path does, for any row's action menu: puts the path on the clipboard and says so in
 * a toast, or says it could not. It takes the path in its wire form and copies it as the UI's own
 * text shows it: for a human to paste, so a byte that is not UTF-8 comes out as U+FFFD rather than
 * as the wire form's `%XX`. */
export function useCopyPath(): (wirePath: string) => void {
  const t = useT();
  const { clipboard } = usePlatform();
  const { toastNotice } = useDaemon();
  return (wirePath) => {
    copyText(displayWirePath(wirePath), { clipboard }).then(
      () => toastNotice(t("common.pathCopied")),
      (cause: unknown) => raiseError(new ActionableError(t("common.copyFailed"), "clipboard", { cause }), { title: t("common.copyFailedTitle") }),
    );
  };
}
