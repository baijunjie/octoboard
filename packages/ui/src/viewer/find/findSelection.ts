import { shadowRootsIn } from "../code/rendererDom";
import { findRegions } from "./findDom";

function containsAcrossRoots(container: Element, node: Node | null): boolean {
  if (!node) return false;
  if (container.contains(node)) return true;
  const root = node.getRootNode();
  return root instanceof ShadowRoot && containsAcrossRoots(container, root.host);
}

/** Read before focusing the query: code selections may belong to the renderer's shadow root. */
export function findSelection(container: HTMLElement): string {
  const roots = shadowRootsIn(container);
  const selections = roots.map((root) => (root as ShadowRoot & { getSelection?: () => Selection | null }).getSelection?.());
  selections.push(container.ownerDocument.getSelection());
  const regions = findRegions(container);
  for (const selection of selections) {
    const text = selection?.toString();
    if (!selection || !text) continue;
    // WebKit can report a collapsed light-DOM range for a nonempty selection in a shadow root.
    const ranges = selection.getComposedRanges?.({ shadowRoots: roots });
    const endpoints = ranges?.length ? ranges : [{ startContainer: selection.anchorNode, endContainer: selection.focusNode }];
    if (endpoints.some((range) => regions.some((region) => containsAcrossRoots(region, range.startContainer) && containsAcrossRoots(region, range.endContainer)))) {
      return text;
    }
  }
  return "";
}
