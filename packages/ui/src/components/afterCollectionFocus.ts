/** Runs `then` once the collection has had its two renders to re-aim focus: when what held focus
 * goes away, react-aria points its focused key at a row in one render and focuses that row from an
 * effect of the next, which would undo a focus move made any sooner. */
export function afterCollectionFocus(then: () => void): void {
  requestAnimationFrame(() => requestAnimationFrame(then));
}
