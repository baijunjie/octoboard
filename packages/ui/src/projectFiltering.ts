import type { Project } from "./protocol";

/** The distinct tags across `projects`, ordered for a stable list. There is no tag registry: a
 * tag exists exactly while some project carries it, and tags that differ only in case are one,
 * shown in the spelling met first. */
export function tagVocabulary(projects: Project[]): string[] {
  const seen = new Map<string, string>();
  for (const project of projects) {
    for (const tag of project.tags) {
      const key = tag.toLocaleLowerCase();
      if (!seen.has(key)) seen.set(key, tag);
    }
  }
  return [...seen.values()].sort((a, b) => a.localeCompare(b));
}

/** The selected tags that some project still carries, in the vocabulary's order: a tag the last
 * project dropped stops filtering and stops showing instead of emptying the list. */
export function effectiveTags(selected: string[], vocabulary: string[]): string[] {
  const wanted = new Set(selected.map((tag) => tag.toLocaleLowerCase()));
  return vocabulary.filter((tag) => wanted.has(tag.toLocaleLowerCase()));
}

/** Whether `project` is listed under the filter: its name contains the keyword (case-insensitive;
 * an empty keyword matches everything) and it carries every one of `tags`. */
export function matchesFilter(project: Project, keyword: string, tags: string[]): boolean {
  const needle = keyword.trim().toLocaleLowerCase();
  if (needle !== "" && !project.name.toLocaleLowerCase().includes(needle)) return false;
  const carried = new Set(project.tags.map((tag) => tag.toLocaleLowerCase()));
  return tags.every((tag) => carried.has(tag.toLocaleLowerCase()));
}

/** `selected` with `tag` added unless it is there already (ignoring case). It is applied to the
 * stored selection, which may hold tags that no project carries at the moment. */
export function withTag(selected: string[], tag: string): string[] {
  const key = tag.toLocaleLowerCase();
  return selected.some((x) => x.toLocaleLowerCase() === key) ? selected : [...selected, tag];
}

/** `selected` without `tags` (ignoring case), every other stored tag left as it is. */
export function withoutTags(selected: string[], tags: string[]): string[] {
  const gone = new Set(tags.map((tag) => tag.toLocaleLowerCase()));
  return selected.filter((tag) => !gone.has(tag.toLocaleLowerCase()));
}
