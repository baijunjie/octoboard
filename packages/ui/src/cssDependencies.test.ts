import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { expect, it } from "vitest";

// The third-party licenses list (`scripts/generate-licenses.mjs`) walks the production `dependencies` only, while
// the bundle also takes in what the stylesheets name: a stylesheet imported by package, a Tailwind plugin. A
// package that is only a dev dependency would ship without its license listed, so each one the CSS reaches by
// package name has to be a `dependencies` entry.

const PACKAGE_DIR = fileURLToPath(new URL("../", import.meta.url));

function cssFiles(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const file = path.join(directory, entry.name);
    if (entry.isDirectory()) return cssFiles(file);
    return entry.name.endsWith(".css") ? [file] : [];
  });
}

/** The package a bare specifier names: its first segment, or its first two for a scoped package. */
const packageOf = (specifier: string) => specifier.split("/").slice(0, specifier.startsWith("@") ? 2 : 1).join("/");

function bareSpecifiers(source: string, pattern: RegExp): string[] {
  return Array.from(source.matchAll(pattern), (match) => match[1]).filter((specifier) => !specifier.startsWith(".") && !specifier.startsWith("/"));
}

it("holds every package the stylesheets import or load as a plugin to the production dependencies", () => {
  const { dependencies } = JSON.parse(readFileSync(path.join(PACKAGE_DIR, "package.json"), "utf8")) as { dependencies: Record<string, string> };
  const named = new Set<string>();
  for (const file of cssFiles(path.join(PACKAGE_DIR, "src"))) {
    for (const specifier of bareSpecifiers(readFileSync(file, "utf8"), /@(?:import|plugin)\s+(?:url\()?["']([^"']+)["']/g)) named.add(packageOf(specifier));
  }
  // The stylesheet loads `typography.mjs` as a plugin, which imports the package that does the work.
  for (const specifier of bareSpecifiers(readFileSync(path.join(PACKAGE_DIR, "typography.mjs"), "utf8"), /^import\s[^"']*["']([^"']+)["']/gm)) named.add(packageOf(specifier));
  expect([...named].length).toBeGreaterThan(0);
  expect([...named].filter((name) => !(name in dependencies)).sort()).toEqual([]);
});
