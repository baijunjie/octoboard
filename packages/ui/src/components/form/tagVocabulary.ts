/** The distinct tags across the tag lists given, ordered for a stable list. There is no tag registry: a
 * tag exists exactly while some list carries it, and tags that differ only in case are one,
 * shown in the spelling met first. */
export function tagVocabulary(tagLists: string[][]): string[] {
  const seen = new Map<string, string>();
  for (const tags of tagLists) {
    for (const tag of tags) {
      const key = tag.toLowerCase();
      if (!seen.has(key)) seen.set(key, tag);
    }
  }
  return [...seen.values()].sort((a, b) => a.localeCompare(b));
}
