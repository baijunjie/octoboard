import { shadowRootsIn } from "./code/rendererDom";
import { findRegions } from "./find/findDom";

function containsAcrossRoots(container: Element, node: Node | null): boolean {
  if (!node) return false;
  if (container.contains(node)) return true;
  const root = node.getRootNode();
  return root instanceof ShadowRoot && containsAcrossRoots(container, root.host);
}

/** One end to the other of a selection, as `Selection.setBaseAndExtent` takes them: the anchor is `start`, except for a selection read
 * from a shadow root's composed ranges, which come in document order and so lose a backward direction. */
export interface SelectionEnds {
  startContainer: Node;
  startOffset: number;
  endContainer: Node;
  endOffset: number;
}

/** What `readSelection` found: the text of the selection and its ends, to put it back with `restoreSelection`. */
export interface FoundSelection {
  text: string;
  ends: SelectionEnds;
}

/**
 * The selection inside one of `container`'s find regions, whether it is the document's or one of the code renderer's
 * shadow roots, as its text and its ends; `null` for no selection, a collapsed one, or one that is not (wholly)
 * inside a region. Read before focus moves: focusing a field clears what it does not hold. The text is the selection's
 * own, not its range's: `Range.toString()` drops the line breaks the code renderer draws between lines.
 */
export function readSelection(container: HTMLElement): FoundSelection | null {
  const roots = shadowRootsIn(container);
  const selections = roots.map((root) => (root as ShadowRoot & { getSelection?: () => Selection | null }).getSelection?.());
  selections.push(container.ownerDocument.getSelection());
  const regions = findRegions(container);
  for (const selection of selections) {
    const text = selection?.toString();
    if (!selection || !text) continue;
    // WebKit can report a collapsed light-DOM range for a nonempty selection in a shadow root.
    const ranges = selection.getComposedRanges?.({ shadowRoots: roots });
    const ends: SelectionEnds[] = ranges?.length
      ? ranges
      : selection.anchorNode && selection.focusNode
        ? [{ startContainer: selection.anchorNode, startOffset: selection.anchorOffset, endContainer: selection.focusNode, endOffset: selection.focusOffset }]
        : [];
    const inside = ends.find((range) => regions.some((region) => containsAcrossRoots(region, range.startContainer) && containsAcrossRoots(region, range.endContainer)));
    if (inside) return { text, ends: inside };
  }
  return null;
}

const TEXT_INPUT_TYPES = new Set(["text", "search", "url", "tel", "email", "password", "number"]);

/** Whether `element` takes text input of its own: a text field, a textarea or an editable element. A checkbox or a
 * button, which has focus without a caret, is not one. */
function isEditableElement(element: Element | null): boolean {
  if (!(element instanceof HTMLElement)) return false;
  if (element.isContentEditable || element instanceof HTMLTextAreaElement) return true;
  return element instanceof HTMLInputElement && TEXT_INPUT_TYPES.has(element.type);
}

/** Puts a selection `readSelection` read back, for a document that has lost it (WebKit clears it as focus moves into a
 * menu) or has another now (WebKit selects the word under a second right-click). Skipped where focus has gone to an
 * editable element outside `region` (the find bar's input, say), whose caret it would disturb. */
export function restoreSelection(ends: SelectionEnds, region: Element | null): void {
  const doc = ends.startContainer.ownerDocument;
  if (!doc) return;
  const active = doc.activeElement;
  if (isEditableElement(active) && !region?.contains(active)) return;
  const selection = doc.getSelection();
  if (!selection) return;
  // The surface may have re-rendered while the menu was open (the file changed), leaving the kept ends out of the document
  // or past the end of their nodes.
  const fits = (node: Node, offset: number) => node.isConnected && offset <= (node.nodeType === Node.TEXT_NODE ? (node as Text).length : node.childNodes.length);
  if (!fits(ends.startContainer, ends.startOffset) || !fits(ends.endContainer, ends.endOffset)) return;
  selection.setBaseAndExtent(ends.startContainer, ends.startOffset, ends.endContainer, ends.endOffset);
}
