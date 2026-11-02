import type { Project } from "../protocol";

/** The selected tags that some project still carries, in the vocabulary's order: a tag the last
 * project dropped stops filtering and stops showing instead of emptying the list. */
export function effectiveTags(selected: string[], vocabulary: string[]): string[] {
  const wanted = new Set(selected.map((tag) => tag.toLowerCase()));
  return vocabulary.filter((tag) => wanted.has(tag.toLowerCase()));
}

/** Whether `project` is listed under the filter: its name contains the keyword (case-insensitive;
 * an empty keyword matches everything) and it carries every one of `tags`. */
export function matchesFilter(project: Project, keyword: string, tags: string[]): boolean {
  const needle = keyword.trim().toLowerCase();
  if (needle !== "" && !project.name.toLowerCase().includes(needle)) return false;
  const carried = new Set(project.tags.map((tag) => tag.toLowerCase()));
  return tags.every((tag) => carried.has(tag.toLowerCase()));
}

/** `selected` with `tag` added unless it is there already (ignoring case). It is applied to the
 * stored selection, which may hold tags that no project carries at the moment. */
export function withTag(selected: string[], tag: string): string[] {
  const key = tag.toLowerCase();
  return selected.some((x) => x.toLowerCase() === key) ? selected : [...selected, tag];
}

/** `selected` without `tags` (ignoring case), every other stored tag left as it is. */
export function withoutTags(selected: string[], tags: string[]): string[] {
  const gone = new Set(tags.map((tag) => tag.toLowerCase()));
  return selected.filter((tag) => !gone.has(tag.toLowerCase()));
}
