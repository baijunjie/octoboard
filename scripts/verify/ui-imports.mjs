// What a package imports from the UI package (`packages/ui`), found by reading its sources: every `@octoboard/ui/<name>`
// import resolved through the UI package's `exports` map, every relative import that lands in the UI package, and every
// module those import in turn by a relative path.
// Type-only imports count, since the importer's type check reads them. `scope.mjs` builds its relation lists from this,
// so a module the package starts to reach (a provider module added to the marks it imports) is in scope with no edit.
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";

const UI_PACKAGE = "packages/ui";
const UI_NAME = "@octoboard/ui";
// Build output, caches, dependencies and static files, none of which holds a source that imports.
const SKIPPED_DIRECTORIES = new Set([".nuxt", ".output", ".astro", "dist", "node_modules", "public"]);
const SOURCE = /\.(ts|tsx|vue|mjs|astro|mdx)$/;
const IMPORT = /(?:from|import)\s*\(?\s*["']([^"']+)["']/g;
// An import names a module without its extension, or a directory by its index.
const SUFFIXES = ["", ".ts", ".tsx", "/index.ts", "/index.tsx"];

const isFile = (file) => existsSync(file) && statSync(file).isFile();

function sourcesUnder(directory) {
  if (!existsSync(directory)) return [];
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    if (SKIPPED_DIRECTORIES.has(entry.name)) return [];
    const full = path.join(directory, entry.name);
    if (entry.isDirectory()) return sourcesUnder(full);
    return SOURCE.test(entry.name) ? [full] : [];
  });
}

// The file a `@octoboard/ui` import of `specifier` names, through the `exports` map: `null` where the map does not name
// it, which the importer's own build fails on. A target this cannot read as the bundler would (a conditions object, a
// pattern) throws: taking it for nothing would leave the modules behind it out of scope with nothing to show for it.
function exportedFile(root, exported, specifier, importer) {
  const key = specifier === UI_NAME ? "." : `./${specifier.slice(UI_NAME.length + 1)}`;
  const target = typeof exported === "string" ? (key === "." ? exported : undefined) : exported[key];
  const where = `\`${specifier}\`, imported by ${path.relative(root, importer)}`;
  if (target === undefined) {
    const patterns = typeof exported === "string" ? [] : Object.keys(exported).filter((name) => name.includes("*") || !name.startsWith("."));
    if (patterns.length > 0) throw new Error(`ui-imports: ${where}, is not named in the \`exports\` map of ${UI_PACKAGE}/package.json, which has entries this does not interpret (${patterns.join(", ")}); teach scripts/verify/ui-imports.mjs to resolve them`);
    return null;
  }
  if (typeof target !== "string" || target.includes("*")) throw new Error(`ui-imports: ${where}, has an \`exports\` target this does not interpret (${JSON.stringify(target)}); teach scripts/verify/ui-imports.mjs to resolve it`);
  return path.join(root, UI_PACKAGE, target);
}

// The files of the UI package that the sources under `directory` (relative to `root`, the repository) import, with what
// those import in turn, as sorted repository-relative paths. An import that lands in the UI package and cannot be
// resolved to a file there throws, naming the specifier and its importer: an unknown input must never select fewer
// checks than it should, and a module this cannot follow would silently do that.
export function importsFromUi(root, directory) {
  const manifest = path.join(root, UI_PACKAGE, "package.json");
  const exported = existsSync(manifest) ? (JSON.parse(readFileSync(manifest, "utf8")).exports ?? {}) : {};
  const uiRoot = path.join(root, UI_PACKAGE) + path.sep;
  const seen = new Set();
  const visit = (file) => {
    for (const [, specifier] of readFileSync(file, "utf8").matchAll(IMPORT)) {
      let base;
      if (specifier === UI_NAME || specifier.startsWith(`${UI_NAME}/`)) {
        base = exportedFile(root, exported, specifier, file);
        if (base === null) continue;
      } else if (specifier.startsWith(".")) {
        // A bundler query (`?inline`, `?raw`) names the same file.
        base = path.resolve(path.dirname(file), specifier.replace(/\?.*$/, ""));
        // A relative import that stays in the importing package is that package's own file, and is not followed.
        if (!base.startsWith(uiRoot)) continue;
      } else continue;
      const resolved = SUFFIXES.map((suffix) => base + suffix).find(isFile);
      if (!resolved) throw new Error(`ui-imports: \`${specifier}\`, imported by ${path.relative(root, file)}, resolves to no file (${path.relative(root, base)}, tried the suffixes ${SUFFIXES.filter(Boolean).join(", ")}); if the bundler resolves it, teach scripts/verify/ui-imports.mjs how`);
      if (!resolved.startsWith(uiRoot) || seen.has(resolved)) continue;
      seen.add(resolved);
      // An asset (a stylesheet, an image) is an input like any module, and imports nothing this could follow.
      if (SOURCE.test(resolved)) visit(resolved);
    }
  };
  for (const source of sourcesUnder(path.join(root, directory))) visit(source);
  return [...seen].map((file) => path.relative(root, file).split(path.sep).join("/")).sort();
}
