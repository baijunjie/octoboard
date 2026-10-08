import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";

const directory = new URL("../i18n/locales/", import.meta.url);
const expected = [
  "ar",
  "de",
  "en",
  "es",
  "fr",
  "hi",
  "id",
  "it",
  "ja",
  "ko",
  "pt-BR",
  "ru",
  "th",
  "tr",
  "vi",
  "zh-Hans",
  "zh-Hant",
];
const files = (await readdir(directory))
  .filter((name) => name.endsWith(".json"))
  .sort();
assert.deepEqual(
  files,
  expected.map((locale) => `${locale}.json`).sort(),
  "Locale coverage differs from the supported languages",
);

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
}

console.log(
  `Verified ${files.length} complete locales with ${keys.length} messages each.`,
);
