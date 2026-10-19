import type { ClipboardCapability } from "../platform";

/** The copy every browser still allows, and a webview or an insecure page (no clipboard API) needs:
 * a hidden element whose text is selected and copied by the document. The command only runs while
 * something is selected, so the element stays; what goes on the clipboard is set by a one-shot
 * `copy` listener instead of read off the selection, which would carry the element's styles as
 * HTML (and in Firefox leaves the clipboard alone while a textarea has focus). Focus never moves,
 * since a focus change blurs the button that was pressed and a blur closes its tooltip
 * (react-aria), and the page's own selection is put back. The element goes inside `anchor`'s
 * dialog if it is in one: react-aria marks what lies outside a modal dialog inert, which cannot be
 * selected, and it does so once the DOM mutation is noticed, so a node added outside may or may
 * not have been marked by the time of the copy. Returns whether it worked. */
function copyThroughSelection(text: string, anchor: Element | null | undefined): boolean {
  const holder = document.createElement("span");
  holder.textContent = text;
  holder.style.position = "fixed";
  holder.style.opacity = "0";
  holder.style.whiteSpace = "pre";
  holder.style.userSelect = "text";
  // react-aria sets the prefixed property to none on iOS during a touch press.
  holder.style.setProperty("-webkit-user-select", "text");
  (anchor?.closest('[role="dialog"],[role="alertdialog"]') ?? document.body).appendChild(holder);
  const selection = document.getSelection();
  const saved = selection ? Array.from({ length: selection.rangeCount }, (_, i) => selection.getRangeAt(i)) : [];
  const onCopy = (event: ClipboardEvent) => {
    event.clipboardData?.setData("text/plain", text);
    event.preventDefault();
    event.stopImmediatePropagation();
  };
  document.addEventListener("copy", onCopy, true);
  try {
    const range = document.createRange();
    range.selectNodeContents(holder);
    selection?.removeAllRanges();
    selection?.addRange(range);
    return document.execCommand("copy");
  } catch {
    return false;
  } finally {
    document.removeEventListener("copy", onCopy, true);
    holder.remove();
    selection?.removeAllRanges();
    for (const range of saved) selection?.addRange(range);
  }
}

/**
 * Puts `text` on the clipboard: through the platform's clipboard when it has one, and through a
 * selection copy when it has none (a plain browser over insecure http) or it refuses. It resolves
 * once one of them worked and rejects when neither did, with the platform clipboard's own error
 * when it gave one. Call it from the press itself: both ways need the user's gesture, and with no
 * clipboard the selection copy runs before this returns.
 *
 * `anchor` is the element the user pressed, so the selection copy can stay inside its dialog.
 */
export function copyText(text: string, { clipboard, anchor }: { clipboard?: ClipboardCapability; anchor?: Element | null }): Promise<void> {
  const viaSelection = (cause: unknown) => {
    if (!copyThroughSelection(text, anchor)) throw cause ?? new Error("The text could not be copied");
  };
  if (!clipboard) {
    // The executor runs before this returns, so the copy keeps the press's gesture; a throw rejects.
    return new Promise((resolve) => {
      viaSelection(undefined);
      resolve();
    });
  }
  return clipboard.writeText(text).catch(viaSelection);
}
