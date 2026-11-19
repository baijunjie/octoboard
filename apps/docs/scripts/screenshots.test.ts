import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import test, { after } from "node:test";
import { fileURLToPath } from "node:url";

// `screenshots.mjs` is run as a process, as the package script runs it, with its browser, dev server and heavy-step slot
// replaced (`screenshotsTestSupport.mjs`). It runs from a copy of itself and of the manifest module, so what it writes
// lands in a temporary `src/assets/features/` and not among the committed images, and the manifest it reads is the small
// one written here, not the committed one. What needs a real browser (what the gallery renders, the pixels) is not covered.

const here = (file: string) => new URL(file, import.meta.url);
const support = here("./screenshotsTestSupport.mjs").href;
const galleryBrowser = here("../../../packages/ui/scripts/gallery-browser.mjs").href;

const roots: string[] = [];
after(() => {
  for (const root of roots) rmSync(root, { recursive: true, force: true });
});

type Entry = Record<string, unknown>;
const MANIFEST: Record<string, Entry[] | string> = {
  accounts: [
    { id: "plain", scenario: "scn-plain", width: 800, height: 600 },
    { id: "wide", scenario: "scn-wide", width: 700, height: 500, scale: 1 },
  ],
  context: [{ id: "clipped", scenario: "scn-clip", width: 1000, height: 700, clip: { selector: "[role=dialog]", padding: 16 } }],
  collaboration: [{ id: "second", scenario: "scn-second" }],
};
const SCENARIOS: { id: string; width?: number; minWidth?: number }[] = [{ id: "scn-plain" }, { id: "scn-wide" }, { id: "scn-clip" }, { id: "scn-second" }];

interface Options {
  manifest?: Record<string, Entry[] | string>;
  scenarios?: unknown;
  mode?: string;
  fail?: string;
  box?: object;
  env?: Record<string, string>;
  /** The directory of an earlier run, to run again over what it wrote. */
  root?: string;
  /** Put in the output directory before the run: a file with this content, or a directory where it is `null`. */
  seed?: Record<string, string | null>;
}

function prepare(args: string[], options: Options) {
  const root = options.root ?? mkdtempSync(path.join(os.tmpdir(), "docs-screenshots-"));
  roots.push(root);
  mkdirSync(path.join(root, "scripts"), { recursive: true });
  mkdirSync(path.join(root, "screenshots"), { recursive: true });
  cpSync(fileURLToPath(here("./screenshots.mjs")), path.join(root, "scripts", "screenshots.mjs"));
  for (const module of ["manifest", "entries"]) cpSync(fileURLToPath(here(`../screenshots/${module}.mjs`)), path.join(root, "screenshots", `${module}.mjs`));
  for (const [group, entries] of Object.entries(options.manifest ?? MANIFEST)) {
    writeFileSync(path.join(root, "screenshots", `${group}.mjs`), typeof entries === "string" ? entries : `export default ${JSON.stringify(entries)};`);
  }
  const outDir = path.join(root, "src", "assets", "features");
  mkdirSync(outDir, { recursive: true });
  for (const [name, content] of Object.entries(options.seed ?? {})) {
    if (content === null) mkdirSync(path.join(outDir, name));
    else writeFileSync(path.join(outDir, name), content);
  }
  const env: NodeJS.ProcessEnv = { ...process.env };
  delete env.OCTOBOARD_PLAYWRIGHT;
  delete env.OCTOBOARD_CHROMIUM;
  Object.assign(env, {
    GALLERY_TEST_BROWSER: galleryBrowser,
    GALLERY_TEST_SCENARIOS: JSON.stringify(options.scenarios ?? SCENARIOS),
    ...(options.mode ? { GALLERY_TEST_MODE: options.mode } : {}),
    ...(options.fail ? { GALLERY_TEST_FAIL: options.fail } : {}),
    ...(options.box ? { GALLERY_TEST_BOX: JSON.stringify(options.box) } : {}),
    ...options.env,
  });
  const argv = ["--no-warnings", "--experimental-loader", support, path.join(root, "scripts", "screenshots.mjs"), ...args];
  const written = () => readdirSync(outDir).sort();
  const image = (id: string) => JSON.parse(readFileSync(path.join(outDir, `${id}.png`), "utf8")) as Record<string, unknown>;
  return { root, env, argv, written, image };
}

function run(args: string[], options: Options = {}) {
  const prepared = prepare(args, options);
  const result = spawnSync(process.execPath, prepared.argv, { env: prepared.env, encoding: "utf8", timeout: 60_000 });
  return { root: prepared.root, code: result.status, stdout: result.stdout, stderr: result.stderr, written: prepared.written(), image: prepared.image };
}

const lines = (output: string, prefix: string) => output.split("\n").filter((line) => line.startsWith(prefix));
/** The pages loaded to capture an entry; the one that reads the list of scenarios loads a frame of its own first. */
const captured = (output: string) => lines(output, "fake: goto ").filter((line) => line.includes("lang=en"));
const USAGE = /Usage: pnpm --filter @octoboard\/docs screenshots/;

test("--list prints the ids in manifest order and starts nothing; --only narrows them by id or wildcard, repeatably", () => {
  const cases: [string[], string][] = [
    [[], "plain\nwide\nclipped\nsecond\n"],
    [["--only", "wide"], "wide\n"],
    [["--only", "p*", "--only", "second"], "plain\nsecond\n"],
    [["--only", "*d"], "clipped\nsecond\n"],
  ];
  for (const [args, stdout] of cases) {
    const result = run(["--list", ...args]);
    assert.equal(result.code, 0, args.join(" "));
    assert.equal(result.stdout, stdout, args.join(" "));
    assert.equal(lines(result.stdout + result.stderr, "fake:").length, 0, "nothing is started");
    assert.deepEqual(result.written, []);
  }
});

test("--help and -h print the usage and succeed", () => {
  for (const flag of ["--help", "-h"]) {
    const result = run([flag]);
    assert.equal(result.code, 0, flag);
    assert.match(result.stdout, USAGE);
    for (const option of ["--only", "--url", "--list", "--playwright", "--chromium"]) assert.ok(result.stdout.includes(option), `${flag}: ${option}`);
  }
});

test("a run that cannot start exits 2 with the refuser's message, and captures nothing", () => {
  const missing = path.join(os.tmpdir(), "docs-screenshots-no-such-playwright");
  const cases: [string, string[], Options, RegExp][] = [
    ["a --only pattern that matches nothing", ["--only", "plain", "--only", "nope"], {}, /No manifest entry matches: nope/],
    ["an invalid manifest", ["--list"], { manifest: { ...MANIFEST, context: [{ id: "Bad_Id", scenario: "s" }] } }, /context\[0\] \(Bad_Id\): `id` is the output file name/],
    ["an unknown option", ["--nope"], {}, /Unknown option '--nope'[\s\S]*Usage:/],
    ["a stray argument", ["plain"], {}, /Unexpected argument 'plain'[\s\S]*Usage:/],
    ["an option without its value", ["--url"], {}, /Option '--url[^']*' argument missing[\s\S]*Usage:/],
    ["a Playwright directory that does not exist", ["--only", "plain", "--playwright", missing], {}, new RegExp(`names ${missing}, which does not exist`)],
    ["a Playwright directory that does not exist, from the environment", ["--only", "plain"], { env: { OCTOBOARD_PLAYWRIGHT: missing } }, new RegExp(`names ${missing}, which does not exist`)],
    ["a gallery that cannot be read", ["--only", "plain"], { mode: "gallery-unreadable" }, /^Could not load the gallery from http:\/\/localhost:1234: import of the fixtures failed$/m],
    ["a scenario the gallery does not have", [], { scenarios: SCENARIOS.filter((scenario) => !["scn-clip", "scn-second"].includes(scenario.id)) }, /The gallery has no scenario for: clipped \(scn-clip, in context\), second \(scn-second, in collaboration\)/],
    ["a window that the scenario's width does not allow", [], { scenarios: SCENARIOS.map((scenario) => (scenario.id === "scn-plain" ? { ...scenario, width: 900 } : scenario)) }, /plain: scn-plain is fixed at 900px wide, not 800px/],
    ["a window narrower than the scenario exists in", [], { scenarios: SCENARIOS.map((scenario) => (scenario.id === "scn-wide" ? { ...scenario, minWidth: 720 } : scenario)) }, /wide: scn-wide exists only from 720px wide, not 700px/],
  ];
  for (const [what, args, options, message] of cases) {
    const result = run(args, options);
    assert.equal(result.code, 2, what);
    assert.match(result.stderr, message, what);
    assert.deepEqual(result.written, [], what);
    assert.equal(captured(result.stdout).length, 0, what);
  }
});

test("a manifest or option mistake is reported by its message alone, without a stack", () => {
  const result = run(["--only", "nope"]);
  assert.equal(result.stderr.trim(), "No manifest entry matches: nope");
});

test("every entry is captured into <id>.png at its scale (2 unless the entry says 1), from the frame address the manifest gives, and the run cleans up", () => {
  const result = run([]);
  assert.equal(result.code, 0, result.stdout + result.stderr);
  assert.deepEqual(result.written, ["clipped.png", "plain.png", "second.png", "wide.png"]);
  assert.match(result.stdout, /^ok {4}plain /m);
  assert.match(result.stdout, /4 captured, 0 failed/);
  assert.deepEqual(
    lines(result.stdout, "fake: context ").map((line) => JSON.parse(line.slice("fake: context ".length))),
    [800, 700, 1000, 1440].map((width, index) => ({ viewport: { width, height: [600, 500, 700, 900][index] }, deviceScaleFactor: [2, 1, 2, 2][index] })),
  );
  assert.deepEqual(captured(result.stdout), [
    "fake: goto http://localhost:1234/gallery-frame.html?scenario=scn-plain&theme=light&lang=en&input=mouse",
    "fake: goto http://localhost:1234/gallery-frame.html?scenario=scn-wide&theme=light&lang=en&input=mouse",
    "fake: goto http://localhost:1234/gallery-frame.html?scenario=scn-clip&theme=light&lang=en&input=mouse",
    "fake: goto http://localhost:1234/gallery-frame.html?scenario=scn-second&theme=light&lang=en&input=mouse",
  ]);
  assert.equal(result.image("plain").type, "png");
  assert.equal(result.image("plain").clip, undefined);
  assert.equal(result.image("plain").omitBackground, undefined, "a whole-window capture keeps its background");
  assert.deepEqual(lines(result.stdout, "fake: isolate"), ["fake: isolate [role=dialog] 16"], "only the clip entry is isolated, with its padding");
  assert.equal(lines(result.stdout, "fake: keep").length, 0, "nothing is kept that the entry does not name");
  assert.equal(lines(result.stdout, "fake: mouse move").length, 0, "the mouse stays where it is when an entry has no hover");
  for (const text of ["fake: closed browser", "fake: closed server", "fake: slot released"]) assert.ok(result.stdout.includes(text), text);
});

test("--url uses the gallery at that origin, without its trailing slashes, and starts no server", () => {
  const result = run(["--only", "plain", "--url", "http://example.test:9///"]);
  assert.equal(result.code, 0, result.stdout + result.stderr);
  assert.deepEqual(captured(result.stdout), ["fake: goto http://example.test:9/gallery-frame.html?scenario=scn-plain&theme=light&lang=en&input=mouse"]);
  assert.ok(!result.stdout.includes("closed server"));
  assert.ok(result.stdout.includes("closed browser"));
});

test("--chromium names the executable to launch, and OCTOBOARD_CHROMIUM does when the option is absent", () => {
  const cases: [string[], Record<string, string>, string][] = [
    [["--chromium", "/opt/chrome"], {}, "/opt/chrome"],
    [[], { OCTOBOARD_CHROMIUM: "/env/chrome" }, "/env/chrome"],
    [["--chromium", "/opt/chrome"], { OCTOBOARD_CHROMIUM: "/env/chrome" }, "/opt/chrome"],
  ];
  for (const [args, env, executable] of cases) assert.ok(run(["--only", "plain", ...args], { env }).stdout.includes(`fake: launch ${executable}\n`), executable);
});

test("a clip is the element's box grown by the padding and kept inside the viewport", () => {
  const cases: [{ x: number; y: number; width: number; height: number }, object][] = [
    [{ x: 100, y: 80, width: 300, height: 200 }, { x: 84, y: 64, width: 332, height: 232 }],
    [{ x: 5, y: 5, width: 5000, height: 5000 }, { x: 0, y: 0, width: 1000, height: 700 }],
  ];
  for (const [box, clip] of cases) {
    const result = run(["--only", "clipped"], { box });
    assert.equal(result.code, 0, result.stdout + result.stderr);
    assert.deepEqual(result.image("clipped").clip, clip);
    assert.equal(result.image("clipped").omitBackground, true, "outside the element is transparent");
    assert.ok(result.stdout.includes("fake: locator [role=dialog]"));
  }
});

const HOVER_MANIFEST = { accounts: [{ id: "hovered", scenario: "scn-plain", width: 800, height: 600, hover: "[data-hover]" }], context: [], collaboration: [] };

test("an entry with a hover moves the mouse to the centre of its element after the page has settled, and waits before the capture", () => {
  const result = run([], { manifest: HOVER_MANIFEST, box: { x: 100, y: 80, width: 300, height: 200 } });
  assert.equal(result.code, 0, result.stdout + result.stderr);
  const steps = lines(result.stdout, "fake: ").filter((line) => /^fake: (wait \d+|mouse move .*|locator .*)$/.test(line));
  assert.deepEqual(steps.map((line) => line.replace(/^fake: (wait|mouse move|locator).*/, "$1")), ["wait", "locator", "mouse move", "wait"]);
  assert.ok(steps.includes("fake: locator [data-hover]"));
  assert.ok(steps.includes("fake: mouse move 250 180"));
  assert.equal(result.image("hovered").type, "png");
});

test("a hover is followed by the clip, when the entry has one", () => {
  const manifest = { ...HOVER_MANIFEST, accounts: [{ ...HOVER_MANIFEST.accounts[0], clip: "[role=dialog]" }] };
  const result = run([], { manifest, box: { x: 100, y: 80, width: 300, height: 200 } });
  assert.equal(result.code, 0, result.stdout + result.stderr);
  // The entry's clip has no padding of its own, so the default margin of 24 is round the element, inside the 800 by 600 window.
  assert.deepEqual(result.image("hovered").clip, { x: 76, y: 56, width: 348, height: 248 });
  assert.ok(result.stdout.indexOf("fake: mouse move") < result.stdout.indexOf("fake: locator [role=dialog]"));
});

test("a hover selector that matches no element, several, or an invisible one fails the entry without moving the mouse", () => {
  for (const [mode, message] of [
    ["hover-none", /hover selector \[data-hover\] matches 0 elements, not 1/],
    ["hover-two", /hover selector \[data-hover\] matches 2 elements, not 1/],
    ["hover-hidden", /hover selector \[data-hover\] is not visible/],
  ] as const) {
    const result = run([], { manifest: HOVER_MANIFEST, mode });
    assert.equal(result.code, 1, mode);
    assert.match(result.stdout, message, mode);
    assert.match(result.stdout, /^FAIL {2}hovered /m, mode);
    assert.equal(lines(result.stdout, "fake: mouse move").length, 0, mode);
    assert.deepEqual(result.written, [], mode);
  }
});

const KEEP_MANIFEST = { accounts: [{ id: "kept", scenario: "scn-plain", clip: { selector: "[role=dialog]", padding: 8 }, keep: [".tip", "[role=menu]"] }], context: [], collaboration: [] };

test("the layers an entry keeps are found, each by its selector, before the clipped element is isolated", () => {
  const result = run([], { manifest: KEEP_MANIFEST });
  assert.equal(result.code, 0, result.stdout + result.stderr);
  assert.deepEqual(
    lines(result.stdout, "fake: ").filter((line) => /^fake: (keep|isolate) /.test(line)),
    ["fake: keep .tip", "fake: keep [role=menu]", "fake: isolate [role=dialog] 8"],
  );
  assert.equal(result.image("kept").omitBackground, true);
});

test("a keep selector that matches nothing fails the entry, naming it, and writes nothing", () => {
  const result = run([], { manifest: KEEP_MANIFEST, mode: "keep-none" });
  assert.equal(result.code, 1);
  assert.match(result.stdout, /^FAIL {2}kept /m);
  assert.match(result.stdout, /the keep selector \.tip matches no element/);
  assert.equal(lines(result.stdout, "fake: isolate").length, 0);
  assert.deepEqual(result.written, []);
});

test("an entry that cannot be captured fails with the reason, and nothing is written for it", () => {
  const cases: [string, string, RegExp][] = [
    ["banner-failure", "plain", /^FAIL {2}plain {2}\(scn-plain\)\n {6}the gallery failed: fixture failed \| second line$/m],
    ["page-error", "plain", /^ {6}the gallery failed: uncaught: boom$/m],
    ["never-ready", "plain", /^ {6}the gallery failed: the page never became ready within 60s$/m],
    ["ready-error", "plain", /^ {6}net::ERR_CONNECTION_REFUSED$/m],
    ["clip-none", "clipped", /clip selector \[role=dialog\] matches 0 elements, not 1/],
    ["clip-two", "clipped", /clip selector \[role=dialog\] matches 2 elements, not 1/],
    ["clip-hidden", "clipped", /clip selector \[role=dialog\] is not visible/],
  ];
  for (const [mode, id, message] of cases) {
    const result = run(["--only", id], { mode });
    assert.equal(result.code, 1, mode);
    assert.match(result.stdout, message, mode);
    assert.match(result.stdout, /0 captured, 1 failed/, mode);
    assert.equal(lines(result.stdout, "fake: isolate").length, 0, `${mode}: nothing is isolated for a clip that cannot be taken`);
    assert.deepEqual(result.written, [], mode);
    assert.ok(result.stdout.includes("fake: closed browser") && result.stdout.includes("fake: slot released"), mode);
  }
});

test("one entry failing does not stop the others, and only the ones that worked are written", () => {
  const result = run([], { mode: "banner-failure", fail: "scn-wide" });
  assert.equal(result.code, 1);
  assert.deepEqual(result.written, ["clipped.png", "plain.png", "second.png"]);
  assert.match(result.stdout, /^FAIL {2}wide /m);
  assert.match(result.stdout, /3 captured, 1 failed/);
});

test("a failed entry leaves the image of an earlier run as it was", () => {
  const first = run(["--only", "clipped"], { box: { x: 100, y: 80, width: 300, height: 200 } });
  assert.equal(first.code, 0);
  const second = run(["--only", "clipped"], { root: first.root, mode: "banner-failure", box: { x: 10, y: 10, width: 50, height: 50 } });
  assert.equal(second.code, 1);
  assert.deepEqual(second.written, ["clipped.png"]);
  assert.deepEqual(second.image("clipped"), first.image("clipped"));
});

test("a half-written file a killed run left is removed at the next start, and a finished image is not", () => {
  const result = run(["--only", "plain"], { seed: { "old.png.tmp": "half", "other.png.tmp": "half", "kept.png": "finished" } });
  assert.equal(result.code, 0, result.stdout + result.stderr);
  assert.deepEqual(result.written, ["kept.png", "plain.png"]);
});

test("an image that cannot be put in place fails its entry and leaves no file beside it", () => {
  // A directory under the image's name cannot be replaced by a file.
  const result = run(["--only", "plain"], { seed: { "plain.png": null } });
  assert.equal(result.code, 1, result.stdout + result.stderr);
  assert.match(result.stdout, /^FAIL {2}plain /m);
  assert.deepEqual(result.written, ["plain.png"]);
});

test("an interrupt stops the run at once: the resources close, the exit code is the signal's, and no entry is reported failed", async () => {
  const prepared = prepare([], { mode: "hang-capture" });
  const child = spawn(process.execPath, prepared.argv, { env: prepared.env, stdio: ["ignore", "pipe", "pipe"] });
  let output = "";
  let signalled = false;
  const guard = setTimeout(() => child.kill("SIGKILL"), 30_000);
  for (const stream of [child.stdout, child.stderr]) {
    stream.on("data", (chunk) => {
      output += chunk;
      if (!signalled && output.includes("fake: ready for signal")) {
        signalled = true;
        child.kill("SIGTERM");
      }
    });
  }
  const code = await new Promise<number | null>((resolve) => child.on("close", resolve));
  clearTimeout(guard);
  assert.equal(code, 143, output);
  assert.ok(output.includes("SIGTERM: stopping"), output);
  assert.ok(output.includes("fake: closed browser") && output.includes("fake: closed server"), output);
  assert.ok(!output.includes("FAIL") && !output.includes("captured,") && !output.includes("ok    "), output);
  assert.deepEqual(prepared.written(), []);
  assert.ok(existsSync(prepared.root));
});
