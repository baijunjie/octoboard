/** Elements whose own handling of Left and Right comes first: text the user edits, and composite
 * widgets that move among their items with the arrows (the diff layout's toggle group, a list). */
const OWNS_ARROWS = [
  "input",
  "textarea",
  "select",
  '[contenteditable]:not([contenteditable="false"])',
  ...["radiogroup", "tablist", "toolbar", "slider", "listbox", "menu", "grid", "treegrid"].map((role) => `[role=${role}]`),
].join(", ");

/**
 * What a key pressed inside the viewer asks of it: Left and Right move to the previous and next
 * subject, mirrored under right-to-left, where the previous one is to the right. Nothing is asked
 * while a modifier is held (Shift extends a selection, the others belong to the system), during an
 * input method's composition, when the key is for a control that uses the arrows itself, or while
 * text inside the viewer is selected, so a selection made to copy is not lost to a stray arrow. A
 * key some listener has already prevented is left alone too, but the viewer's listener is a native
 * one on the dialog, which runs before React's own handlers inside it: those are covered by the
 * controls `OWNS_ARROWS` names, not by that check.
 */
export function arrowNavigation(event: KeyboardEvent, viewer: Element): "previous" | "next" | undefined {
  if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return undefined;
  if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return undefined;
  if (event.isComposing || event.keyCode === 229 || event.defaultPrevented) return undefined;
  const target = event.target instanceof Element ? event.target : undefined;
  if (target?.closest(OWNS_ARROWS)) return undefined;
  // `type`, not `isCollapsed`: WebKit reports a selection lying inside a shadow root, as all the
  // rendered code does, as collapsed at the shadow host, and only its `type` still says "Range".
  const selection = viewer.ownerDocument.getSelection();
  if (selection?.type === "Range" && selection.anchorNode && within(viewer, selection.anchorNode)) return undefined;
  const rtl = getComputedStyle(viewer).direction === "rtl";
  return (event.key === "ArrowLeft") !== rtl ? "previous" : "next";
}

/** Whether `node` is inside `container`, looking out of the shadow roots the code renderer draws
 * into. */
function within(container: Element, node: Node): boolean {
  let current: Node | undefined = node;
  while (current) {
    if (container.contains(current)) return true;
    const root = current.getRootNode();
    current = root instanceof ShadowRoot ? root.host : undefined;
  }
  return false;
}
