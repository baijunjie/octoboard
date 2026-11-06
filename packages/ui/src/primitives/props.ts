/** `data-*` attributes a part passes on to its element, for the project's own styling and lookups. */
export type DataAttributes = { [key: `data-${string}`]: string | boolean | undefined };

/** `props` without the keys whose value is `undefined`, so that an unset prop reaches the library as no prop at all.
 * HeroUI parts such as `ModalBackdrop` spread their `...props` after the values they set themselves (its `data-slot`),
 * so a key present with the value `undefined` would overwrite what the part set; an absent key leaves it. */
export function definedProps<T extends object>(props: T): Partial<T> {
  return Object.fromEntries(Object.entries(props).filter(([, value]) => value !== undefined)) as Partial<T>;
}
