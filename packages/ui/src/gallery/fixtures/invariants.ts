// Rules a fixture repository as a whole has to keep, which neither a factory's signature nor a
// single reply can state: a test holds them over the fixtures (`projectGit.test.ts`), so that one
// added later cannot break one unnoticed.
import { listedCommits, type FixtureChange, type FixtureGit } from "./changeFixtures";

/** Every change of a repository, its worktrees', its comparisons' and its commits' alike: one
 * repository's object ids are one set, whichever list a change is listed in. */
function changesOf(git: FixtureGit): FixtureChange[] {
  const commits = Object.values(git.commits ?? {}).flatMap((list) => listedCommits(list).map(({ changes }) => changes));
  return [...Object.values(git.changes), ...Object.values(git.comparisons ?? {}), ...commits].flatMap((list) => (Array.isArray(list) ? list : []));
}

/** The content a change carries for one of its sides: the whole text, or the body its reply carries
 * in place of a patch. A side it carries none for — a link, whose target it writes into the patch
 * and nowhere else, or one of a change refused before anything is read — has nothing to compare. */
function contentOf(change: FixtureChange, side: "old" | "new"): string | undefined {
  const body = change.bodies?.[side];
  return change.whole?.[side] ?? body?.text ?? body?.data ?? undefined;
}

/** The ids a change names each of its sides by: the one its source holds, and the one its patch's
 * first section gives that side. Only the first section is read, because that is the one describing
 * the change's own two sides; a second describes something else — the link a type change ends at —
 * whose own side is named by its source anyway. An end with no object at all, which `git` writes as
 * zeros, names nothing. */
function idsOf(change: FixtureChange): [string, "old" | "new"][] {
  const { entry } = change;
  if (!("old" in entry)) return [];
  const named: [string, "old" | "new"][] = [];
  for (const side of ["old", "new"] as const) {
    const source = entry[side].state === "present" ? entry[side].source : undefined;
    if (source && source.kind !== "live") named.push([source.blob, side]);
  }
  const section = (change.patch ?? "").split(/^diff --git /m)[1] ?? "";
  const line = /^index ([0-9a-f]+)\.\.([0-9a-f]+)/m.exec(section);
  if (line) named.push([line[1], "old"], [line[2], "new"]);
  return named.filter(([id]) => !/^0+$/.test(id));
}

/** Contents of one generated file begin alike, so a checksum is what tells two of them apart. */
const checksum = (content: string) => (Array.from(content).reduce((sum, char) => (sum * 31 + char.charCodeAt(0)) >>> 0, 7)).toString(16);
const label = (content: string) => `${JSON.stringify(content.slice(0, 24))} (${content.length} chars, #${checksum(content)})`;

/**
 * Whether a repository's fixtures keep Git's own rule that an object id names one content and a
 * content is named by one id. `conflicts` is one message per break of it. `unchecked` names the ids
 * of a side the change carries no content for, which the rule cannot be held over: a test pins them
 * so that a kind of side added later cannot drop out of the check and leave it passing vacuously.
 */
export function objectIdCheck(git: FixtureGit): { conflicts: string[]; unchecked: string[] } {
  const contents = new Map<string, Set<string>>();
  const ids = new Map<string, Set<string>>();
  const unchecked = new Set<string>();
  const name = (map: Map<string, Set<string>>, key: string, value: string) => map.set(key, (map.get(key) ?? new Set<string>()).add(value));
  for (const change of changesOf(git)) {
    for (const [id, side] of idsOf(change)) {
      const content = contentOf(change, side);
      if (content === undefined) {
        unchecked.add(id);
        continue;
      }
      name(contents, id, content);
      name(ids, content, id);
    }
  }
  return {
    conflicts: [
      ...[...contents].filter(([, held]) => held.size > 1).map(([id, held]) => `${id} names ${held.size} contents: ${[...held].map(label).join(", ")}`),
      ...[...ids].filter(([, named]) => named.size > 1).map(([content, named]) => `${label(content)} is named by ${[...named].join(", ")}`),
    ],
    unchecked: [...unchecked].sort(),
  };
}
