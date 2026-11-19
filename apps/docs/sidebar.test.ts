import assert from "node:assert/strict";
import { existsSync, readdirSync } from "node:fs";
import test from "node:test";
import { LANGUAGES } from "@octoboard/ui/languages";
import { sidebar } from "./sidebar.ts";

// The Features group is the one part of the sidebar whose pages are not in every language yet: each page exists in
// English and Simplified Chinese, and a reader of another language is served the English one. The names the group and its
// subgroups carry are held to the website's in `webParity.test.ts`.

interface Group {
  label: string;
  translations: Record<string, string>;
  items: (string | Group)[];
}

const docs = new URL("./src/content/docs/", import.meta.url);
const isGroup = (item: unknown): item is Group => typeof item === "object" && item !== null && "items" in item;
const topGroups = (sidebar as unknown[]).filter(isGroup);
const slugsOf = (group: Group): string[] => group.items.flatMap((item) => (isGroup(item) ? slugsOf(item) : [item]));
const featuresGroup = topGroups.find((group) => group.label === "Features");
const subgroups = (featuresGroup?.items ?? []).filter(isGroup);
const featureSlugs = featuresGroup ? slugsOf(featuresGroup) : [];

const pageExists = (directory: string, slug: string) => existsSync(new URL(`${directory}${slug}.mdx`, docs)) || existsSync(new URL(`${directory}${slug}.md`, docs));
const pagesIn = (directory: string) =>
  readdirSync(new URL(directory, docs))
    .filter((file) => /\.mdx?$/.test(file))
    .map((file) => `features/${file.replace(/\.mdx?$/, "")}`)
    .sort();

test("the sidebar has a Features group of three subgroups, and no page appears in it twice", () => {
  assert.ok(featuresGroup, "a top-level group labelled Features");
  assert.equal(subgroups.length, 3);
  assert.equal(featuresGroup.items.length, 3, "the group holds only its subgroups");
  assert.equal(new Set(featureSlugs).size, featureSlugs.length);
  const all = topGroups.flatMap(slugsOf);
  assert.equal(new Set(all).size, all.length, "a page is listed once in the whole sidebar");
});

test("every page the sidebar lists exists in English", () => {
  for (const slug of topGroups.flatMap(slugsOf)) assert.ok(pageExists("", slug), `${slug} has no English page`);
});

test("every Features page exists in Simplified Chinese too", () => {
  for (const slug of featureSlugs) assert.ok(pageExists("zh-CN/", slug), `${slug} has no zh-CN page`);
});

test("the Features pages on disk are exactly the ones the sidebar lists, in both languages", () => {
  const listed = [...featureSlugs].sort();
  assert.deepEqual(pagesIn("features/"), listed, "English");
  assert.deepEqual(pagesIn("zh-CN/features/"), listed, "zh-CN");
});

test("every group is labelled in every language of the application, with English as its own label", () => {
  for (const group of [...topGroups, ...subgroups]) {
    assert.deepEqual(Object.keys(group.translations).sort(), [...LANGUAGES].sort(), group.label);
    assert.equal(group.label, group.translations.en);
  }
});
