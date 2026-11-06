/** A collection item's key as react-aria marks it in the page (`data-key`, on a tab, a tree row, a list option): what
 * the application has to read to tell which item an element is, or to find an item again once the collection has
 * rebuilt its element. The attribute is react-aria's business, so it is named here only. */
export function itemKeyOf(element: Element | null | undefined): string | undefined {
  return element instanceof HTMLElement ? element.dataset.key : undefined;
}

/** The selector of the collection items that have a key, or of the one `key` names; joined to a role (`[role=row]` +
 * `itemKeySelector()`) to leave out an element that merely carries a key. */
export function itemKeySelector(key?: string): string {
  return key === undefined ? "[data-key]" : `[data-key="${CSS.escape(key)}"]`;
}
