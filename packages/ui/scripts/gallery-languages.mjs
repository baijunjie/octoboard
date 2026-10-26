// Which languages a gallery sweep loads when `--lang` is not given. The two catalogs are rendered by the
// same code except for `src/i18n/` (`joinPhrases.ts` is the one place that branches on the language
// explicitly), so a change that cannot reach that code is swept in English only, and every other change in
// every language. Kept apart from `gallery-sweep.mjs`, which runs on import, so that the rule has tests of
// its own.

/** The language swept alone when the change cannot reach what differs between languages. */
const SOURCE_LANGUAGE = "en";

/** Where a change can make the languages differ, or change what every page runs on, so that one language's
 * result no longer stands for the others. Each group names what it holds; add an entry when code that
 * branches on the language (a test of `language`, a locale-specific rule) is written outside `src/i18n/`.
 * An entry ending in `/` is a directory, another string is a file, a regular expression is tested on the
 * whole path. A path that matches none is taken to be harmless to the second language. */
export const LANGUAGE_SENSITIVE = {
  "the language code": ["packages/ui/src/i18n/"],
  // What loads and judges the pages: a change here can break one language alone, and must be seen doing so.
  "the gallery machinery": ["packages/ui/src/gallery/", "packages/ui/scripts/", "scripts/verify/"],
  "the build or configuration": [
    "config/",
    "patches/",
    "scripts/env.mjs",
    "scripts/env.d.mts",
    "pnpm-lock.yaml",
    "pnpm-workspace.yaml",
    ".npmrc",
    "packages/ui/typography.mjs",
    /^packages\/ui\/[^/]+\.html$/,
    // In any directory: manifests, TypeScript, Vite and Vitest configuration, env files.
    /(^|\/)package\.json$/,
    /(^|\/)tsconfig(\..+)?\.json$/,
    /(^|\/)vite(st)?(\..+)?\.config\.[cm]?[jt]s$/,
    /(^|\/)\.env(\..+)?$/,
  ],
};

/** What `file` belongs to in `LANGUAGE_SENSITIVE`, or undefined. */
function sensitiveReason(file) {
  for (const [what, entries] of Object.entries(LANGUAGE_SENSITIVE)) {
    const matches = entries.some((entry) => {
      if (typeof entry !== "string") return entry.test(file);
      return entry.endsWith("/") ? file.startsWith(entry) : file === entry;
    });
    if (matches) return what;
  }
  return undefined;
}

/** The languages to sweep, and why, from `base` (what `resolveBase` returned) and `files`, the paths changed
 * between the base and the working tree, the git-ignored extra scenarios included (null when git could not
 * say), as `branchChanges` in `gallery-impact.mjs` returns them. `languages` is every language with a catalog. */
export function chooseLanguages(languages, base, files) {
  if (!base.commit) return { languages, reason: `${base.reason}, so there are no changes to judge by` };
  if (files === null) return { languages, reason: "git could not list the changes since the merge base" };
  if (files.length === 0) return { languages, reason: "nothing differs from the merge base, so there are no changes to judge by" };
  for (const file of files) {
    const what = sensitiveReason(file);
    if (what) return { languages, reason: `${file} changed since the merge base, and a change to ${what} can reach what differs between languages` };
  }
  return { languages: [SOURCE_LANGUAGE], reason: "no changed path can make the languages differ; --lang all sweeps every language" };
}
