import { isPrimaryModifier } from "../useWindowShortcut";
import { renderedCodeRange } from "./rendererDom";

/** Whether a key press is the platform's Select All: ⌘A on an Apple keyboard, Ctrl+A elsewhere. */
export function isSelectAll(event: Pick<KeyboardEvent, "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">): boolean {
  return event.key.toLowerCase() === "a" && !event.altKey && !event.shiftKey && isPrimaryModifier(event);
}

/**
 * Selects the code shown in one of the viewer's code frames, and nothing else, so a copy takes the
 * code alone. The browser's own Select All cannot: it selects the document, and the rendered code
 * lies in the renderer's shadow root, which a document selection does not enter, so it took the
 * viewer's title, path and size instead. Plain text is the frame's own contents.
 *
 * Returns whether the key is handled. While the renderer has drawn no code yet it is, selecting
 * nothing, as the browser's own would select the viewer's title, path and size. A browser may
 * ignore a selection set across a shadow root; the key is then left to the browser's own Select
 * All rather than swallowed for nothing.
 */
export function selectCode(frame: HTMLElement): boolean {
  const selection = frame.ownerDocument.getSelection();
  if (!selection) return false;
  const range = renderedCodeRange(frame);
  selection.removeAllRanges();
  if (range === null) return true;
  if (range) {
    selection.setBaseAndExtent(range.startContainer, range.startOffset, range.endContainer, range.endOffset);
  } else {
    selection.selectAllChildren(frame);
  }
  const anchor = selection.anchorNode;
  if (!anchor) return false;
  const root = anchor.getRootNode();
  return frame.contains(anchor) || (root instanceof ShadowRoot && frame.contains(root.host));
}

/**
 * Select All in a rendered document: everything the document holds. The browser's own would take the
 * viewer's title, path and size with it. The selection is marked in the DOM only, and cannot hold the
 * code of fenced blocks, which lie in the renderer's shadow roots, out of reach of a document
 * selection — `copyDocument` is what makes a copy of it whole.
 */
export function selectDocument(region: HTMLElement): boolean {
  const selection = region.ownerDocument.getSelection();
  if (!selection) return false;
  selection.selectAllChildren(region);
  return true;
}

/**
 * Makes a copy of a selection that is the whole of a rendered document put the document's Markdown
 * source on the clipboard: a document selection holds no text for its fenced blocks (see
 * `selectDocument`), so the browser's own copy would leave a gap where each one was. A selection of
 * part of the document is left to the browser, which copies what it can reach. Returns whether it
 * handled the copy.
 */
export function copyDocument(
  event: { clipboardData: DataTransfer | null; preventDefault: () => void },
  region: HTMLElement,
  source: string,
): boolean {
  const selection = region.ownerDocument.getSelection();
  if (!event.clipboardData || !selection || selection.rangeCount !== 1) return false;
  const range = selection.getRangeAt(0);
  const whole =
    range.startContainer === region && range.endContainer === region && range.startOffset === 0 && range.endOffset === region.childNodes.length;
  if (!whole) return false;
  event.clipboardData.setData("text/plain", source);
  event.preventDefault();
  return true;
}
