import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { expect, test } from "vitest";

const SOURCE = dirname(fileURLToPath(import.meta.url));

/** The modules `file` imports at run time: a relative `import`/`export … from` that is not type-only. A type-only
 * import is erased, so it cannot make a cycle that is there when the code runs. */
function runtimeImports(file: string): string[] {
  const text = readFileSync(file, "utf8");
  const found: string[] = [];
  for (const match of text.matchAll(/^(?:import|export)\s+(type\s+)?([^;]*?)\s+from\s+"(\.{1,2}\/[^"]+)";/gm)) {
    if (match[1] || /^\{(\s*type\s+[^,}]+,?)+\s*\}$/.test(match[2])) continue;
    const base = resolve(dirname(file), match[3]);
    const resolved = [".ts", ".tsx", "/index.ts"].map((suffix) => base + suffix).find((candidate) => existsSync(candidate));
    if (resolved) found.push(resolved);
  }
  return found;
}

/** Whether `target` can be reached from `from` through run-time imports. */
function reaches(from: string, target: string): boolean {
  const seen = new Set<string>();
  const queue = [from];
  while (queue.length) {
    const file = queue.pop()!;
    for (const next of runtimeImports(file)) {
      if (next === target) return true;
      if (!seen.has(next)) { seen.add(next); queue.push(next); }
    }
  }
  return false;
}

test("the error classification and presentation modules do not import each other at run time", () => {
  const recovery = resolve(SOURCE, "errorRecovery.ts");
  const presentation = resolve(SOURCE, "errorPresentation.ts");
  expect(reaches(recovery, recovery)).toBe(false);
  expect(reaches(presentation, presentation)).toBe(false);
});
