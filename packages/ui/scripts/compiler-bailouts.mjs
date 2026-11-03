#!/usr/bin/env node
// Lists the components and hooks the React Compiler leaves uncompiled. The compiler skips such a function silently
// (most causes pass lint), so this runs it over the given files, or over every production file under `src/` when none
// is given, with a logger for `CompileError` events, and exits 1 when any is found. A function left uncompiled on
// purpose carries `"use no memo"`, which the compiler honours without logging an error.
//
// Usage: node scripts/compiler-bailouts.mjs [file ...]
import { readdirSync, readFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { transformSync } from "@babel/core";

const root = resolve(fileURLToPath(new URL("..", import.meta.url)));
const src = join(root, "src");

// Vite's preset hands the compiler only files that may hold a component or hook (`@vitejs/plugin-react`'s
// `reactCompilerPreset` filter); the same filter keeps this run to the files the build compiles.
const MAY_HOLD_COMPONENT = /forwardRef|memo|\b(?:[A-Z]|use[A-Z0-9])/;
const SKIPPED_DIRECTORIES = new Set(["gallery", "fixtures", "test", "website-preview"]);

function productionFiles(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) return SKIPPED_DIRECTORIES.has(entry.name) ? [] : productionFiles(path);
    if (!/\.tsx?$/.test(entry.name) || /\.d\.ts$/.test(entry.name)) return [];
    if (/\.test\.tsx?$/.test(entry.name) || /TestSupport\.tsx?$|testSupport\.tsx?$/.test(entry.name)) return [];
    return [path];
  });
}

const given = process.argv.slice(2);
const files = given.length > 0 ? given.map((file) => resolve(file)) : productionFiles(src);

const bailouts = new Set();
for (const filename of files) {
  const code = readFileSync(filename, "utf8");
  if (!MAY_HOLD_COMPONENT.test(code)) continue;
  transformSync(code, {
    filename,
    babelrc: false,
    configFile: false,
    parserOpts: { plugins: ["typescript", "jsx"] },
    plugins: [
      [
        "babel-plugin-react-compiler",
        {
          logger: {
            logEvent(_file, event) {
              if (event.kind !== "CompileError") return;
              const detail = event.detail;
              const reason = detail?.reason ?? detail?.options?.reason ?? String(detail);
              const line = detail?.loc?.start?.line ?? detail?.options?.loc?.start?.line ?? event.fnLoc?.start?.line;
              bailouts.add(`${relative(root, filename)}:${event.fnLoc?.start?.line ?? line ?? "?"} ${reason}`);
            },
          },
        },
      ],
    ],
    code: false,
  });
}

if (bailouts.size > 0) {
  console.error(`The React Compiler left ${bailouts.size} function(s) uncompiled:`);
  for (const bailout of bailouts) console.error(`  ${bailout}`);
  process.exit(1);
}
console.log(`The React Compiler compiled every component and hook in ${files.length} file(s).`);
