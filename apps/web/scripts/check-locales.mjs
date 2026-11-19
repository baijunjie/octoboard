import assert from "node:assert/strict";
import { realpathSync } from "node:fs";
import { readdir, readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { locales } from "../seo/site-seo.ts";

// The product name comes from `{appName}` (config/app.json), never from a literal. A name that continues a Latin word
// (the daemon's `octoboardd`) or is in lowercase (a URL's host) is not the name; next to CJK, Hangul, Thai or other
// scripts, which have no word spaces, it still is.
export function productNameLiterals(messages, name) {
  const word = new RegExp(`(?<![\\p{Script=Latin}\\p{N}_])${name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(?![\\p{Script=Latin}\\p{N}_])`, "u");
  return Object.keys(messages).filter((key) => word.test(messages[key]));
}

if (process.argv[1] && realpathSync(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const directory = new URL("../i18n/locales/", import.meta.url);
  const expected = locales.map((locale) => locale.language);
  const files = (await readdir(directory))
    .filter((name) => name.endsWith(".json"))
    .sort();
  assert.deepEqual(
    files,
    expected.map((locale) => `${locale}.json`).sort(),
    "Locale coverage differs from the supported languages",
  );

  const appName = JSON.parse(
    await readFile(new URL("../../../config/app.json", import.meta.url), "utf8"),
  ).name;
  const source = JSON.parse(
    await readFile(new URL("en.json", directory), "utf8"),
  );
  const keys = Object.keys(source).sort();
  const placeholders = (value) =>
    [...value.matchAll(/\{(\w+)\}/g)].map((match) => match[1]).sort();

  for (const file of files) {
    const messages = JSON.parse(await readFile(new URL(file, directory), "utf8"));
    assert.deepEqual(
      Object.keys(messages).sort(),
      keys,
      `${file}: missing or unexpected messages`,
    );
    for (const key of keys) {
      const value = messages[key];
      assert.equal(typeof value, "string", `${file}:${key}: expected a string`);
      assert.ok(value.trim(), `${file}:${key}: empty translation`);
      assert.deepEqual(
        placeholders(value),
        placeholders(source[key]),
        `${file}:${key}: interpolation differs from English`,
      );
    }
    assert.deepEqual(
      productNameLiterals(messages, appName),
      [],
      `${file}: messages contain the product name; write {appName} instead`,
    );
  }

  console.log(
    `Verified ${files.length} complete locales with ${keys.length} messages each.`,
  );
}
