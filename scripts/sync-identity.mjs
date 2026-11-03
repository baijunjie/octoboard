#!/usr/bin/env node
import { readFileSync, realpathSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.dirname(path.dirname(fileURLToPath(import.meta.url)));

// These formats cannot import JSON. Keep their required literal fields as generated output;
// checking never writes, so verification can run on the exact committed tree.
export function syncIdentity({ root = repoRoot, check = false } = {}) {
  const config = JSON.parse(readFileSync(path.join(root, "config/app.json"), "utf8"));
  const author = config.author?.name;
  const year = config.copyrightYear;
  if (typeof author !== "string" || !author.trim() || /[\r\n]/.test(author) || !Number.isInteger(year) || year < 1) {
    throw new Error("config/app.json must define a nonempty single-line author.name and a positive integer copyrightYear");
  }
  const outputs = [
    { file: "LICENSE", pattern: /^Copyright \(c\) .+$/gm, value: `Copyright (c) ${year} ${author}` },
    { file: "Cargo.toml", pattern: /(^\[workspace\.package\]\r?\n)([\s\S]*?)(?=^\[|(?![\s\S]))/m, workspace: true },
  ];
  const changes = [];
  for (const output of outputs) {
    const target = path.join(root, output.file);
    const content = readFileSync(target, "utf8");
    let generated;
    if (output.workspace) {
      const section = content.match(output.pattern);
      if (!section || (section[2].match(/^authors\s*=.*$/gm) ?? []).length !== 1) {
        throw new Error("Cargo.toml must define one workspace.package authors field");
      }
      const body = section[2].replace(/^authors\s*=.*$/m, () => `authors = [${JSON.stringify(author)}]`);
      generated = content.replace(output.pattern, () => section[1] + body);
    } else {
      if ((content.match(output.pattern) ?? []).length !== 1) throw new Error("LICENSE must define one copyright line");
      generated = content.replace(output.pattern, () => output.value);
    }
    if (generated !== content) changes.push({ target, file: output.file, generated });
  }
  if (check && changes.length) throw new Error(`Generated identity is stale in ${changes.map(({ file }) => file).join(", ")}. Run pnpm identity:sync.`);
  for (const { target, generated } of changes) writeFileSync(target, generated);
  return changes.map(({ file }) => file);
}

if (process.argv[1] && realpathSync(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const args = process.argv.slice(2);
    if (args.some((arg) => arg !== "--check")) throw new Error("Usage: node scripts/sync-identity.mjs [--check]");
    const files = syncIdentity({ check: args.includes("--check") });
    if (files.length) console.log(`Generated identity in ${files.join(", ")}.`);
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
