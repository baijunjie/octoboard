// @vitest-environment node
import { existsSync, readdirSync, readFileSync } from "node:fs";

import { expect, it } from "vitest";

// `style.css` imports the HeroUI stylesheets of the components this app renders instead of the whole
// package, so a component newly taken from `@heroui/react` renders unstyled until its sheet is added.
// The sheets wanted are those of the components the source imports plus those they are built from
// (read from the `@heroui/react` package's own module imports), and they keep the order of the
// package's `components/index.css`.

const SRC = new URL("./", import.meta.url);
const REACT_DIST = new URL("../node_modules/@heroui/react/dist/", import.meta.url);
const STYLES_DIST = new URL("../node_modules/@heroui/styles/dist/components/", import.meta.url);

function sourceFiles(): string[] {
  return readdirSync(SRC, { recursive: true, encoding: "utf8" })
    .filter((file) => /\.tsx?$/.test(file) && !/\.test\.tsx?$/.test(file))
    .map((file) => readFileSync(new URL(file, SRC), "utf8"));
}

/** Component directory by exported name, from the package's own index. */
function directoriesByExport(): Map<string, string> {
  const index = readFileSync(new URL("index.js", REACT_DIST), "utf8");
  const directories = new Map<string, string>();
  for (const [, names, directory] of index.matchAll(/export \{([^}]*)\} from '\.\/components\/([\w-]+)\//g)) {
    for (const name of names.split(",")) directories.set(name.trim(), directory);
  }
  return directories;
}

function importedComponents(): string[] {
  const directories = directoriesByExport();
  const pattern = /import\s+(?:type\s+)?\{([^}]*)\}\s+from\s+["']@heroui\/react["']/g;
  const used = new Set<string>();
  for (const source of sourceFiles()) {
    for (const [, names] of source.matchAll(pattern)) {
      for (const name of names.split(",")) {
        const directory = directories.get(name.trim().replace(/^type\s+/, "").split(/\s+as\s+/)[0]);
        if (directory) used.add(directory);
      }
    }
  }
  return [...used];
}

/** The components themselves and everything their modules import from a sibling component. */
function closure(roots: string[]): string[] {
  const seen = new Set(roots);
  const pending = [...roots];
  for (let directory = pending.pop(); directory; directory = pending.pop()) {
    const folder = new URL(`components/${directory}/`, REACT_DIST);
    for (const file of readdirSync(folder).filter((name) => name.endsWith(".js"))) {
      for (const [, dependency] of readFileSync(new URL(file, folder), "utf8").matchAll(/from ["']\.\.\/([\w-]+)\//g)) {
        if (seen.has(dependency)) continue;
        seen.add(dependency);
        pending.push(dependency);
      }
    }
  }
  return [...seen].filter((name) => existsSync(new URL(`${name}.css`, STYLES_DIST)));
}

const imported = [...readFileSync(new URL("style.css", SRC), "utf8").matchAll(/@heroui\/styles\/components\/([\w-]+)\.css/g)].map(
  (match) => match[1],
);
const needed = closure(importedComponents());

it("imports the stylesheet of every HeroUI component the source uses or depends on", () => {
  expect(needed.filter((name) => !imported.includes(name)).sort(), "sheets missing from style.css").toEqual([]);
});

it("imports no HeroUI stylesheet that nothing uses", () => {
  expect(imported.filter((name) => !needed.includes(name)).sort(), "unused sheets in style.css").toEqual([]);
});

it("keeps the stylesheets in the order of the package's components/index.css", () => {
  const index = readFileSync(new URL("index.css", STYLES_DIST), "utf8");
  const packageOrder = [...index.matchAll(/@import "\.\/([\w-]+)\.css"/g)].map((match) => match[1]);
  expect(imported, "sheets in style.css").toEqual(packageOrder.filter((name) => imported.includes(name)));
});

it("pins tw-animate-css to the version @heroui/styles depends on", () => {
  const read = (url: URL) => JSON.parse(readFileSync(url, "utf8")) as { devDependencies?: Record<string, string>; dependencies?: Record<string, string> };
  const own = read(new URL("../package.json", import.meta.url)).devDependencies?.["tw-animate-css"];
  const heroui = read(new URL("../node_modules/@heroui/styles/package.json", import.meta.url)).dependencies?.["tw-animate-css"];
  expect(own, "tw-animate-css in package.json").toBe(heroui);
});
