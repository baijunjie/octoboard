import { ActionableError } from "../errorRecovery";
import type { Translate } from "../i18n/catalog";
import type { ClipboardCapability } from "../platform";

/** The OSC 52 identifier, the escape sequence a program sends to put text on the terminal's clipboard. */
export const OSC_52 = 52;

/**
 * The text an OSC 52 sequence asks to put on the clipboard, from its payload `Pc;Pd`: `Pc` names
 * which selections (clipboard, primary, cut buffers), all of which go to the one system clipboard
 * here, and `Pd` is the text in base64, decoded as UTF-8. `undefined` for a query (`Pd` is `?`),
 * which is never answered, since it would hand the clipboard to whatever runs in the terminal; for a
 * request to clear the clipboard (an empty `Pd`), which is left alone; and for a malformed payload.
 */
export function osc52ClipboardText(payload: string): string | undefined {
  const separator = payload.indexOf(";");
  if (separator < 0) return undefined;
  const data = payload.slice(separator + 1);
  if (!data || data === "?") return undefined;
  try {
    const bytes = Uint8Array.from(atob(data), (char) => char.charCodeAt(0));
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    return undefined;
  }
}

/**
 * The terminal's `copyToClipboard`: puts what a program asks to copy on `clipboard`, and shows an
 * error alert when that fails. Not when a browser refuses a copy the user did not just make (no
 * gesture, an unfocused document), which some browsers (Safari) do for every such copy, since the
 * request arrives after the user's own press is over: an alert each time would only be noise. `undefined` without a clipboard.
 */
export function programCopyHandler(
  clipboard: ClipboardCapability | undefined,
  report: () => { t: Translate; toastError: (error: ActionableError, session?: string, title?: string) => void },
): ((text: string) => void) | undefined {
  if (!clipboard) return undefined;
  return (text) => {
    clipboard.writeText(text).catch((cause: unknown) => {
      if (cause instanceof DOMException && cause.name === "NotAllowedError") return;
      const { t, toastError } = report();
      toastError(new ActionableError(t("terminal.copyFailed"), "clipboard", { cause }), undefined, t("common.copyFailedTitle"));
    });
  };
}
