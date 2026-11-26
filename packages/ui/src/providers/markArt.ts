import type { ProviderMark } from "./types";

/** The `art` of `mark` with the ids it defines made `prefix`-unique, so two drawings of it on one page do not share
 * their masks and filters; `undefined` for a mark drawn as its path alone. A drawing whose shared definition sits in a
 * hidden copy would otherwise lose them. */
export function markArt(mark: ProviderMark, prefix: string): string | undefined {
  return mark.art?.replaceAll("{id}", prefix);
}
