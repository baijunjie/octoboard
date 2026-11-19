// The screenshot manifest: which UI state gallery scenarios the Features pages show, and how each is captured.
// One module per feature group in this directory, each default-exporting an array of entries, so the writers of
// different groups never edit the same file. `scripts/screenshots.mjs` captures them; the fields and their validation
// are in `entries.mjs` (read it for the entry fields), and the filtering and loading live here, apart from the
// browser, so a test can run them.
import { pathToFileURL } from "node:url";
import path from "node:path";
import { wildcard } from "../../../packages/ui/scripts/gallery-browser.mjs";
import { GROUPS, normalizeManifest } from "./entries.mjs";

export * from "./entries.mjs";

/** The entries whose id matches one of `patterns` (`*` is a wildcard); all of them when there are none. A pattern
 * that matches nothing is an error, so a misspelled `--only` does not capture nothing quietly. */
export function selectEntries(entries, patterns) {
  if (patterns.length === 0) return entries;
  const matchers = patterns.map((pattern) => ({ pattern, re: wildcard(pattern) }));
  const unmatched = matchers.filter(({ re }) => !entries.some((entry) => re.test(entry.id))).map(({ pattern }) => pattern);
  if (unmatched.length > 0) throw new Error(`No manifest entry matches: ${unmatched.join(", ")}`);
  return entries.filter((entry) => matchers.some(({ re }) => re.test(entry.id)));
}

/** What keeps an entry from being captured as written against `scenario` (a gallery scenario as `readGallery` lists
 * it), or undefined: a window narrower than the scenario exists in, or a width or height other than the one it fixes. */
export function viewportProblem(entry, scenario) {
  if (scenario.width && entry.width !== scenario.width) return `${entry.id}: ${scenario.id} is fixed at ${scenario.width}px wide, not ${entry.width}px`;
  if (scenario.height && entry.height !== scenario.height) return `${entry.id}: ${scenario.id} is fixed at ${scenario.height}px high, not ${entry.height}px`;
  if (scenario.minWidth && entry.width < scenario.minWidth) return `${entry.id}: ${scenario.id} exists only from ${scenario.minWidth}px wide, not ${entry.width}px`;
  return undefined;
}

/** The frame address of an entry on the gallery served at `origin`. The theme is always light and the language English, and the steps press
 * as a mouse does, which draws no keyboard focus ring. */
export function frameUrl(origin, entry) {
  const query = new URLSearchParams({ scenario: entry.scenario, theme: "light", lang: "en", input: "mouse" });
  return `${origin}/gallery-frame.html?${query}`;
}

/** The manifest of this directory, loaded from its modules. */
export async function loadManifest(dir = import.meta.dirname) {
  const groups = {};
  for (const group of GROUPS) groups[group] = (await import(pathToFileURL(path.join(dir, `${group}.mjs`)).href)).default;
  return normalizeManifest(groups);
}
