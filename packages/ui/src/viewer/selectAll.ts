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
