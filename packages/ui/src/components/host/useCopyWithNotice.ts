import { ActionableError } from "../../errors/actionableError";
import { useT } from "../../i18n/react";
import { usePlatform } from "../../platform/react";
import { useDaemon } from "../../store";
import { raiseError } from "../alerts/raiseError";
import { copyText } from "./copyText";

/** What a copy says of its outcome. */
export interface CopyNotice {
  /** The notice that says the text was copied; none when the caller shows that itself (`onCopied`). */
  copied?: string;
  /** What the error alert says when the copy fails. */
  failed: string;
  /** Called once the text is on the clipboard. */
  onCopied?: () => void;
  /** The element the user pressed, so the selection copy can stay inside its dialog (see `copyText`). */
  anchor?: Element | null;
}

/** Puts text on the clipboard and says how it went: a notice when it is copied (`copied`), or the error alert saying
 * `failed` when it could not be. The one place a copy raises that alert. */
export function useCopyWithNotice(): (text: string, notice: CopyNotice) => void {
  const t = useT();
  const { clipboard } = usePlatform();
  const { showNotice } = useDaemon();
  return (text, { copied, failed, onCopied, anchor }) => {
    copyText(text, { clipboard, anchor }).then(
      () => {
        onCopied?.();
        if (copied) showNotice(copied);
      },
      (cause: unknown) => raiseError(new ActionableError(failed, "clipboard", { cause }), { title: t("common.copyFailedTitle") }),
    );
  };
}
