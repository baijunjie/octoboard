import assert from "node:assert/strict";
import { mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import test, { after } from "node:test";
import { DEFAULT_CLIP_PADDING, DEFAULT_HEIGHT, DEFAULT_WIDTH, frameUrl, loadManifest, normalizeEntry, normalizeManifest, selectEntries, viewportProblem } from "./manifest.mjs";

const entry = (fields: object = {}) => ({ id: "a", scenario: "s", ...fields });
const resolved = (fields: object = {}) => normalizeEntry(entry(fields), "t");
const ids = (entries: { id: string }[]) => entries.map((e) => e.id);

test("an entry gets the default viewport and scale, and no clip, keep or hover", () => {
  assert.deepEqual(resolved(), { id: "a", scenario: "s", width: DEFAULT_WIDTH, height: DEFAULT_HEIGHT, scale: 2, clip: undefined, keep: undefined, hover: undefined });
});

test("a viewport, a scale and a clip as a selector or as a selector with padding are taken as written", () => {
  assert.deepEqual(resolved({ width: 800, height: 600 }), { ...resolved(), width: 800, height: 600 });
  assert.equal(resolved({ hover: "[data-row]" }).hover, "[data-row]");
  for (const scale of [1, 2]) assert.equal(resolved({ scale }).scale, scale);
  assert.deepEqual(resolved({ clip: "main", keep: ".tip" }).keep, [".tip"]);
  assert.deepEqual(resolved({ clip: "main", keep: [".tip", "[role=menu]"] }).keep, [".tip", "[role=menu]"]);
  assert.deepEqual(resolved({ clip: { selector: "main", padding: 12.5 } }).clip, { selector: "main", padding: 12.5 });
  assert.deepEqual(resolved({ clip: { selector: "main", padding: 0 } }).clip, { selector: "main", padding: 0 });
  for (const id of ["a", "a1", "quota-picker", "a-b-2", "2fa"]) assert.equal(resolved({ id }).id, id);
});

const refusals: [string, unknown, RegExp][] = [
  ["an entry that is not an object", null, /an entry is an object/],
  ["an entry that is an array", [], /an entry is an object/],
  ["a field it does not have", entry({ extra: 1 }), /unknown field extra/],
  ...["", "-a", "a-", "a--b", "A", "a_b", "a b", "a.b", "a/b", "../x", 1, undefined].map((id): [string, unknown, RegExp] => [`the id ${JSON.stringify(id)}`, entry({ id }), /`id` is the output file name/]),
  ...["", 1, null].map((scenario): [string, unknown, RegExp] => [`the scenario ${JSON.stringify(scenario)}`, entry({ scenario }), /`scenario` is a gallery scenario id/]),
  ...[0, -5, 1.5, Number.NaN, Number.POSITIVE_INFINITY, "100"].flatMap((size): [string, unknown, RegExp][] => [
    [`the width ${String(size)}`, entry({ width: size }), /`width` is a positive whole number/],
    [`the height ${String(size)}`, entry({ height: size }), /`height` is a positive whole number/],
  ]),
  ...[0, 3, 1.5, -1, "2", Number.NaN].map((scale): [string, unknown, RegExp] => [`the scale ${JSON.stringify(scale)}`, entry({ scale }), /`scale` is 1 or 2/]),
  ["a theme, since captures are always light", entry({ theme: "dark" }), /unknown field theme/],
  ...[5, true, [], ["main"], {}, { padding: 4 }, { selector: 3 }, { selector: "" }, ""].map((clip): [string, unknown, RegExp] => [`the clip ${JSON.stringify(clip)}`, entry({ clip }), /`clip` is a Playwright selector, or \{ selector, padding \}/]),
  ...["", 1, null, true, [], ["a"], {}].map((hover): [string, unknown, RegExp] => [`the hover ${JSON.stringify(hover)}`, entry({ hover }), /`hover` is a Playwright selector/]),
  ...[[], "", [""], [1], 1, [".tip", null], {}].map((keep): [string, unknown, RegExp] => [`the keep ${JSON.stringify(keep)}`, entry({ clip: "main", keep }), /`keep` is a Playwright selector, or an array of them/]),
  ["a keep with no clip to keep it outside of", entry({ keep: ".tip" }), /`keep` needs a `clip`/],
  ["a clip with a field it does not have", entry({ clip: { selector: "main", margin: 1 } }), /`clip` takes only `selector` and `padding`/],
  ...["4", -1, Number.NaN, Number.POSITIVE_INFINITY].map((padding): [string, unknown, RegExp] => [`the clip padding ${String(padding)}`, entry({ clip: { selector: "main", padding } }), /`clip\.padding` is a number of pixels, 0 or more/]),
];

test("an entry is refused with the message of the field that is wrong, prefixed by where it is", () => {
  for (const [what, raw, message] of refusals) {
    assert.throws(() => normalizeEntry(raw, "accounts[0]"), (error: Error) => error.message.startsWith("accounts[0]: ") && message.test(error.message), what);
  }
});

// A past defect: `clip: null` got through the check of the clip's type and failed destructuring, with a TypeError that
// named neither the entry nor the field.
test("a clip given without padding gets the default transparent margin, and an explicit padding, 0 included, is kept", () => {
  assert.equal(DEFAULT_CLIP_PADDING, 24);
  assert.deepEqual(resolved({ clip: "[role=dialog]" }).clip, { selector: "[role=dialog]", padding: DEFAULT_CLIP_PADDING });
  assert.deepEqual(resolved({ clip: { selector: "main" } }).clip, { selector: "main", padding: DEFAULT_CLIP_PADDING });
  assert.equal(resolved({ clip: { selector: "main", padding: 0 } }).clip?.padding, 0);
});

test("a null clip is refused like any other malformed clip, not with an error from the code's own destructuring", () => {
  assert.throws(() => normalizeEntry(entry({ clip: null }), "accounts[0]"), (error: Error) => error.message.startsWith("accounts[0]: ") && /`clip` is a Playwright selector/.test(error.message));
});

test("a manifest lists the entries group by group in order, tagged with their group, and leaves the entries it was given alone", () => {
  const raw = { id: "b", scenario: "s", clip: "main" };
  const entries = normalizeManifest({ accounts: [entry(), raw], context: [], collaboration: [entry({ id: "c", width: 700 })] });
  assert.deepEqual(entries.map(({ id, group, width }) => [id, group, width]), [["a", "accounts", DEFAULT_WIDTH], ["b", "accounts", DEFAULT_WIDTH], ["c", "collaboration", 700]]);
  assert.deepEqual(raw, { id: "b", scenario: "s", clip: "main" });
  assert.deepEqual(normalizeManifest({}), []);
});

const manifestRefusals: [string, Record<string, unknown>, RegExp][] = [
  ["an id used in two groups, since the output file names are shared", { accounts: [entry()], context: [entry({ scenario: "t" })] }, /^Error: context: the id a is also used in accounts/],
  ["an id used twice in one group", { accounts: [entry(), entry()] }, /^Error: accounts: the id a is also used in accounts/],
  ["a group that is not an array", { accounts: {} }, /^Error: accounts: the manifest default-exports an array/],
  ["a bad entry, named by group, position and id", { accounts: [entry()], context: [entry({ id: "b" }), entry({ id: "c", width: 0 })] }, /^Error: context\[1\] \(c\): `width` is a positive whole number/],
  ["a bad entry that has no id", { accounts: [null] }, /^Error: accounts\[0\]: an entry is an object/],
];

test("a manifest refuses what would make two captures one file, or what is not an entry list", () => {
  for (const [what, groups, message] of manifestRefusals) assert.throws(() => normalizeManifest(groups), message, what);
});

const selectable = normalizeManifest({ accounts: [entry({ id: "quota-picker" }), entry({ id: "quota-used-up" }), entry({ id: "quota" })], context: [entry({ id: "files" })] });
const selections: [string[], string[]][] = [
  [[], ["quota-picker", "quota-used-up", "quota", "files"]],
  [["*"], ["quota-picker", "quota-used-up", "quota", "files"]],
  [["quota"], ["quota"]],
  [["quota-*"], ["quota-picker", "quota-used-up"]],
  [["*-up"], ["quota-used-up"]],
  [["q*a"], ["quota"]],
  [["files", "quota-picker"], ["quota-picker", "files"]],
  [["quota-*", "*-picker"], ["quota-picker", "quota-used-up"]],
];

test("entries are selected by id or wildcard pattern, in manifest order, each once", () => {
  for (const [patterns, expected] of selections) assert.deepEqual(ids(selectEntries(selectable, patterns)), expected, patterns.join(" "));
});

test("a pattern that matches nothing is an error that names it, even when others match; regular expression characters are literal", () => {
  assert.throws(() => selectEntries(selectable, ["quota", "b", "c*"]), /No manifest entry matches: b, c\*/);
  for (const pattern of ["Quota", "quot", "quota-?icker", "quota-(picker)", "quota-[a-z]icker", "quota|files", "^quota$", "quota.+"]) {
    assert.throws(() => selectEntries(selectable, [pattern]), (error: Error) => error.message === `No manifest entry matches: ${pattern}`, pattern);
  }
});

const scenarioOf = (fields: object) => ({ id: "scn", ...fields });
const viewports: [string, object, object, RegExp | undefined][] = [
  ["a scenario with no size of its own takes any window", {}, { width: 500, height: 300 }, undefined],
  ["a scenario fixed at a width takes that width", { width: 1000 }, { width: 1000 }, undefined],
  ["a scenario fixed at a width refuses a narrower window", { width: 1000 }, { width: 900 }, /^a: scn is fixed at 1000px wide, not 900px/],
  ["a scenario fixed at a width refuses a wider window", { width: 1000 }, { width: 1440 }, /^a: scn is fixed at 1000px wide, not 1440px/],
  ["a scenario fixed at a height takes that height", { height: 600 }, { height: 600 }, undefined],
  ["a scenario fixed at a height refuses a shorter window", { height: 600 }, { height: 500 }, /^a: scn is fixed at 600px high, not 500px/],
  ["a scenario fixed at a height refuses a taller window", { height: 600 }, { height: 900 }, /^a: scn is fixed at 600px high, not 900px/],
  ["a scenario with a minimum takes that width", { minWidth: 800 }, { width: 800 }, undefined],
  ["a scenario with a minimum takes a wider window", { minWidth: 800 }, { width: 1440 }, undefined],
  ["a scenario with a minimum refuses a narrower window", { minWidth: 800 }, { width: 799 }, /^a: scn exists only from 800px wide, not 799px/],
];

test("an entry's window is checked against the size the scenario is fixed at, or the width it exists from", () => {
  for (const [what, scenario, window, message] of viewports) {
    const problem = viewportProblem(resolved(window), scenarioOf(scenario));
    if (message) assert.match(problem ?? "", message, what);
    else assert.equal(problem, undefined, what);
  }
});

test("the frame address carries the scenario, the light theme, English and mouse input, and nothing else", () => {
  assert.equal(frameUrl("http://localhost:5173", resolved({ scenario: "some-scenario" })), "http://localhost:5173/gallery-frame.html?scenario=some-scenario&theme=light&lang=en&input=mouse");
  assert.equal(frameUrl("http://h", resolved({ scenario: "some-scenario", hover: "[data-row]", clip: "main", width: 700 })), "http://h/gallery-frame.html?scenario=some-scenario&theme=light&lang=en&input=mouse");
  const url = new URL(frameUrl("http://h", resolved({ scenario: "a b&c=d" })));
  assert.equal(url.pathname, "/gallery-frame.html");
  assert.deepEqual([...url.searchParams], [["scenario", "a b&c=d"], ["theme", "light"], ["lang", "en"], ["input", "mouse"]]);
});

const roots: string[] = [];
after(() => {
  for (const root of roots) rmSync(root, { recursive: true, force: true });
});
const manifestDir = (files: Record<string, string>) => {
  const dir = mkdtempSync(path.join(os.tmpdir(), "docs-manifest-"));
  roots.push(dir);
  for (const [name, source] of Object.entries(files)) writeFileSync(path.join(dir, `${name}.mjs`), source);
  return dir;
};
const group = (...entries: object[]) => `export default ${JSON.stringify(entries)};`;

test("a manifest directory is read from one module per group, in the order accounts, context, collaboration", async () => {
  const entries = await loadManifest(manifestDir({ collaboration: group(entry({ id: "d" })), accounts: group(entry()), context: group(entry({ id: "c", clip: "main" })) }));
  assert.deepEqual(entries.map(({ id, group }) => [id, group]), [["a", "accounts"], ["c", "context"], ["d", "collaboration"]]);
  assert.deepEqual(entries[1]?.clip, { selector: "main", padding: DEFAULT_CLIP_PADDING });
});

test("a manifest directory is refused for a group module that is missing, not an array, or holds a bad or repeated entry", async () => {
  const [accounts, context, collaboration] = [group(), group(), group()];
  await assert.rejects(loadManifest(manifestDir({ accounts, context })), /collaboration\.mjs/);
  await assert.rejects(loadManifest(manifestDir({ accounts, context: "export default { id: 'a' };", collaboration })), /context: the manifest default-exports an array/);
  await assert.rejects(loadManifest(manifestDir({ accounts, context, collaboration: group(entry({ id: "BAD" })) })), /collaboration\[0\] \(BAD\): `id` is the output file name/);
  await assert.rejects(loadManifest(manifestDir({ accounts: group(entry()), context, collaboration: group(entry()) })), /collaboration: the id a is also used in accounts/);
});

// The images of the Features pages: the manifest is what the capture script writes into `src/assets/features/`, and the
// pages import what is there, so a page naming an image nobody captures, or a capture no page shows, is caught here.
const docs = new URL("../src/content/docs/", import.meta.url);
const assets = new URL("../src/assets/features/", import.meta.url);
const pagesOf = (directory: string) =>
  readdirSync(new URL(directory, docs))
    .filter((file) => /\.mdx?$/.test(file))
    .map((file) => ({ file: `${directory}${file}`, source: readFileSync(new URL(`${directory}${file}`, docs), "utf8") }));
// A page shows a capture with `<Screenshot id="…" alt="…" />` (`src/components/Screenshot.astro`), quoted attributes only.
const TAG = /<Screenshot((?:\s+\w+="[^"]*")*)\s*\/>/g;
const attribute = (attributes: string, name: string) => new RegExp(`\\b${name}="([^"]*)"`).exec(attributes)?.[1];
const pages = [...pagesOf("features/"), ...pagesOf("zh-CN/features/")];
const images = pages.flatMap(({ file, source }) => [...source.matchAll(TAG)].map((match) => ({ file, id: attribute(match[1] ?? "", "id") ?? "", alt: attribute(match[1] ?? "", "alt") ?? "" })));

test("every screenshot a Features page shows is written as the component, names a capture in src/assets/features, and has a description", () => {
  assert.ok(images.length > 0);
  const present = new Set(readdirSync(assets));
  for (const { file, source } of pages) {
    assert.equal(source.match(/<Screenshot\b/g)?.length ?? 0, images.filter((image) => image.file === file).length, `${file}: a Screenshot tag is not written as id and alt attributes in quotes`);
    assert.doesNotMatch(source, /!\[[^\]]*\]\([^)]*assets\/features\//, `${file} shows a capture as a Markdown image, not with Screenshot`);
  }
  for (const { file, id, alt } of images) {
    assert.ok(present.has(`${id}.png`), `${file} shows ${id}, which is not in src/assets/features`);
    assert.notEqual(alt.trim(), "", `${file}: ${id} has no alt text`);
  }
});

test("the committed manifest is valid, every entry is shown by a page in each language, and every capture on disk is an entry", async () => {
  const entries = await loadManifest();
  assert.ok(entries.length > 0);
  const manifestIds = ids(entries);
  for (const directory of ["features/", "zh-CN/features/"]) {
    const shown = new Set(images.filter(({ file }) => file.startsWith(directory)).map(({ id }) => id));
    for (const id of manifestIds) assert.ok(shown.has(id), `${id} is captured but no page under ${directory} shows it`);
    for (const id of shown) assert.ok(manifestIds.includes(id), `${directory} shows ${id}, which the manifest does not capture`);
  }
  assert.deepEqual(readdirSync(assets).sort(), manifestIds.map((id) => `${id}.png`).sort());
});

test("every capture is a PNG of its entry's size at the entry's scale, or smaller when it is a clip", async () => {
  for (const { id, width, height, scale, clip } of await loadManifest()) {
    const bytes = readFileSync(new URL(`${id}.png`, assets));
    assert.deepEqual([...bytes.subarray(0, 8)], [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a], `${id} is not a PNG`);
    const [pixelsWide, pixelsHigh] = [bytes.readUInt32BE(16), bytes.readUInt32BE(20)];
    // A clip is the element and its margin, which the window's edge cuts short, so it is never larger than the window.
    if (clip) assert.ok(pixelsWide > 0 && pixelsHigh > 0 && pixelsWide <= width * scale && pixelsHigh <= height * scale, `${id}: ${pixelsWide}x${pixelsHigh}`);
    else assert.deepEqual([pixelsWide, pixelsHigh], [width * scale, height * scale], id);
  }
});
