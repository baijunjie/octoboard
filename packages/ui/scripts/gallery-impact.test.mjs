import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";

import { afterAll, expect, test } from "vitest";

import { branchChanges, cssImpact, judgeChanges, judgeFromChanges } from "./gallery-impact.mjs";

const STYLE = "packages/ui/src/style.css";
const base = { commit: "abc123" };
/** Judges `changes`, a map from each changed path to its content before and after. */
const judge = (changes) => judgeChanges(base, Object.keys(changes), (file) => changes[file]);

const css = `
:root {
  --accent: oklch(0.6 0.2 250);
  --accent-hover: var(--accent);
  --sidebar-width: 240px;
}
/* The sidebar. */
.sidebar { display: flex; width: var(--sidebar-width); color: #333; background: white; border-radius: 4px; }
@media (min-width: 800px) {
  .sidebar .row { position: relative; box-shadow: 0 0 0 1px oklch(0 0 0 / 0.1); }
}
`;
const edited = (from, to) => css.replace(from, to);

test("a change that cannot be judged sweeps", async () => {
  expect((await judgeChanges({ reason: "no recorded target branch" }, null, () => ({}))).sweep).toBe(true);
  expect((await judgeChanges(base, null, () => ({}))).sweep).toBe(true);
  expect((await judgeChanges(base, [], () => ({}))).sweep).toBe(true);
});

test("paths the gallery does not load cannot make a page fail", async () => {
  const judged = await judge({
    "docs/product/sessions.md": {},
    "apps/daemon/src/main.rs": {},
    "packages/ui/README.md": {},
    "packages/ui/src/components/Badge.test.tsx": {},
    "apps/web/nuxt.config.ts": {},
    "CLAUDE.md": {},
  });
  expect(judged).toEqual({ sweep: false, reason: "none of the 6 paths changed since the merge base can make a page fail" });
});

test.each([
  ["a colour value", "color: #333", "color: #444"],
  ["a colour custom property", "oklch(0.6 0.2 250)", "oklch(0.62 0.2 250)"],
  ["a colour custom property to a reference to another", "--accent-hover: var(--accent)", "--accent-hover: color-mix(in oklab, var(--accent) 80%, black)"],
  ["a background and a radius", "background: white; border-radius: 4px", "background: linear-gradient(red, blue); border-radius: 8px"],
  ["a shadow in a media query", "0 0 0 1px oklch(0 0 0 / 0.1)", "0 1px 2px black"],
  ["an added appearance-only declaration", "color: #333;", "color: #333; outline: 2px solid red; font-weight: 600; -webkit-font-smoothing: antialiased;"],
  ["a hex custom property to a named colour", "--sidebar-width: 240px;", "--sidebar-width: 240px; --ink: #333;", "--sidebar-width: 240px; --ink: white;"],
  ["only a comment", "/* The sidebar. */", "/* The navigation sidebar,\n   down the left. */"],
  ["only formatting", ".sidebar { display: flex;", ".sidebar {\n  display:   flex;"],
])("in CSS, %s cannot make a page fail", (_, from, to, to2) => {
  // With a third value, the change is from the second to the third.
  const [before, after] = to2 === undefined ? [css, edited(from, to)] : [edited(from, to), edited(from, to2)];
  expect(cssImpact(before, after)).toBeUndefined();
});

test.each([
  ["display", "display: flex", "display: none", /changes display/],
  ["a size", "width: var(--sidebar-width)", "width: 200px", /changes width/],
  ["a size custom property", "--sidebar-width: 240px", "--sidebar-width: 0", /changes --sidebar-width/],
  ["position in a media query", "position: relative", "position: absolute", /changes position in @media \(min-width: 800px\) > \.sidebar \.row,/],
  ["a colour custom property to a length", "--accent: oklch(0.6 0.2 250)", "--accent: 4px", /changes --accent/],
  ["an added layout declaration", "color: #333;", "color: #333; pointer-events: none;", /changes pointer-events/],
  ["opacity, which a step reads", "color: #333;", "color: #333; opacity: 0.98;", /changes opacity/],
  ["scrollbar-color, which turns off the scrollbar's own styling", "color: #333;", "color: #333; scrollbar-color: red blue;", /changes scrollbar-color/],
  ["a custom property that is a colour followed by more", "--sidebar-width: 240px", "--sidebar-width: rgb(0 0 0) calc(1px)", /changes --sidebar-width/],
  ["an added rule", "/* The sidebar. */", ".hidden { color: red; }", /adds, removes, renames or moves a rule/],
  ["a renamed selector", ".sidebar .row", ".sidebar > .row", /a rule/],
  ["a removed rule", /@media[^]*$/, "", /a rule/],
  ["an at-rule's parameters", "min-width: 800px", "min-width: 900px", /an at-rule/],
])("in CSS, a change to %s can make a page fail", (_, from, to, reason) => {
  expect(cssImpact(css, edited(from, to))).toMatch(reason);
});

test("a custom property defined only elsewhere is not known to be a colour", () => {
  expect(cssImpact(":root { --muted: var(--heroui-muted); }", ":root { --muted: var(--heroui-other); }")).toMatch(/changes --muted/);
});

test("a stylesheet that does not parse sweeps", () => {
  expect(cssImpact(css, `${css} .broken {`)).toMatch(/does not parse/);
});

test("a colour change in style.css alone loads no page", async () => {
  expect((await judge({ [STYLE]: { before: css, after: edited("#333", "#444") } })).sweep).toBe(false);
});

test("code changed beyond its comments sweeps, a comment or formatting alone does not", async () => {
  const before = "// A badge.\nexport function Badge() {\n  return <span className=\"badge\">x</span>;\n}\n";
  const comments = "/** A badge, the small kind. */\nexport function Badge() {\n  // The label.\n  return <span className=\"badge\">x</span>\n}\n";
  const code = before.replace('"badge"', '"badge hidden"');
  const file = "packages/ui/src/components/Badge.tsx";
  expect((await judge({ [file]: { before, after: comments } })).sweep).toBe(false);
  const judged = await judge({ [file]: { before, after: code } });
  expect(judged).toEqual({ sweep: true, reason: `${file} changes code, not only comments or formatting` });
  expect((await judge({ [file]: { before, after: "export function Badge( {" } })).reason).toMatch(/does not parse/);
});

test.each([
  ["a lint suppression the React Compiler reads", "// eslint-disable-next-line react-hooks/exhaustive-deps\n"],
  ["a JSX pragma", "/** @jsxImportSource preact */\n"],
  ["the end of a lint suppression", "/* eslint-enable react-hooks/rules-of-hooks */\n"],
  ["a lint suppression further into a comment", "// TODO: eslint-disable-next-line react-hooks/rules-of-hooks\n"],
  ["a JSX pragma on a later line of a doc comment", "/**\n * The badge.\n * @jsxImportSource preact\n */\n"],
])("adding %s counts as a code change", async (_, comment) => {
  const before = "export function Badge() {\n  return <span>x</span>;\n}\n";
  const judged = await judge({ "packages/ui/src/components/Badge.tsx": { before, after: comment + before } });
  expect(judged.reason).toMatch(/changes only comments or formatting, but holds a comment the React Compiler or the JSX transform reads/);
});

test("a lint suppression moved to another function counts as a code change", async () => {
  const suppression = "  // eslint-disable-next-line react-hooks/rules-of-hooks\n";
  const fn = (name, suppressed) => `export function ${name}() {\n${suppressed ? suppression : ""}  useThing();\n}\n`;
  const judged = await judge({ "packages/ui/src/components/Badge.tsx": { before: fn("A", true) + fn("B", false), after: fn("A", false) + fn("B", true) } });
  expect(judged.reason).toMatch(/but holds a comment the React Compiler/);
});

test("a judgement that throws sweeps", async () => {
  const judged = await judgeFromChanges({ base, files: [STYLE], read: () => { throw new Error("no such object"); } });
  expect(judged).toEqual({ sweep: true, reason: "the changes could not be judged (no such object)" });
});

test.each([
  ["packages/ui/src/gallery/fixtures/sessions.ts", { after: "export const x = 1;" }, /was added/],
  ["packages/ui/src/i18n/messages/en.ts", { before: "export default {};" }, /was removed/],
  ["packages/ui/src/i18n/messages/zh-Hans.ts", { before: 'export default { a: "甲" };', after: 'export default { a: "乙" };' }, /changes code/],
  ["pnpm-lock.yaml", { before: "a", after: "b" }, /is loaded by the gallery or decides what it runs on/],
  ["packages/ui/package.json", { before: "{}", after: '{"a":1}' }, /is loaded by the gallery/],
  ["packages/ui/gallery-frame.html", { before: "a", after: "b" }, /is loaded by the gallery/],
  ["scripts/verify/stamp.mjs", { before: "export const a = 1;", after: "export const a = 2;" }, /changes code/],
  ["apps/desktop/src-tauri/icons/128x128.png", { before: "a" }, /is loaded by the gallery/],
  [".env", { before: "A=1", after: "A=2" }, /is loaded by the gallery/],
])("%s can make a page fail", async (file, contents, reason) => {
  const judged = await judge({ [file]: contents });
  expect(judged.sweep).toBe(true);
  expect(judged.reason).toMatch(reason);
});

test("in a mixed change, one path that can make a page fail sweeps, and is named", async () => {
  const judged = await judge({
    "docs/project-map.md": {},
    [STYLE]: { before: css, after: edited("#333", "#444") },
    "packages/ui/src/components/Row.css": { before: ".row { color: red; }", after: ".row { color: red; overflow: hidden; }" },
  });
  expect(judged.sweep).toBe(true);
  expect(judged.reason).toBe("packages/ui/src/components/Row.css changes overflow in .row, which can move, hide or cover what a step acts on");
});

const scratch = mkdtempSync(path.join(os.tmpdir(), "gallery-impact-"));
afterAll(() => rmSync(scratch, { recursive: true, force: true }));

// A repository with one commit on `main`, then `feat` branched from it with its target branch recorded when
// `targetBranch` is set. `edit` changes the working tree of `feat`.
function checkout({ targetBranch = true, edit = () => {} }) {
  const root = mkdtempSync(path.join(scratch, "repo-"));
  const git = (...args) => execFileSync("git", args, { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
  const write = (file, content) => {
    mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
    writeFileSync(path.join(root, file), content);
  };
  git("init", "-q", "-b", "main");
  write(STYLE, css);
  write("packages/ui/src/components/Badge.tsx", "export const a = 1;\n");
  write(".gitignore", "packages/ui/src/gallery/preview/\n");
  git("add", ".");
  git("-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "x");
  git("switch", "-q", "-c", "feat");
  if (targetBranch) git("config", "branch.feat.targetBranch", "main");
  edit(write);
  return root;
}

test("from a real checkout, a colour change in style.css loads no page", async () => {
  const root = checkout({ edit: (write) => write(STYLE, edited("#333", "#444")) });
  expect((await judgeFromChanges(branchChanges(root))).sweep).toBe(false);
});

test("from a real checkout, a display change or a code change sweeps", async () => {
  expect((await judgeFromChanges(branchChanges(checkout({ edit: (write) => write(STYLE, edited("display: flex", "display: grid")) })))).reason).toBe(
    `${STYLE} changes display in .sidebar, which can move, hide or cover what a step acts on`,
  );
  expect((await judgeFromChanges(branchChanges(checkout({ edit: (write) => write("packages/ui/src/components/Badge.tsx", "export const a = 2;\n") })))).sweep).toBe(true);
});

test("from a real checkout, an untracked file counts as a change", async () => {
  const root = checkout({ edit: (write) => write("packages/ui/src/components/New.tsx", "export const b = 1;\n") });
  expect((await judgeFromChanges(branchChanges(root))).reason).toBe("packages/ui/src/components/New.tsx was added");
});

test("from a real checkout, an extra scenario git ignores counts as a change", async () => {
  const root = checkout({ edit: (write) => write("packages/ui/src/gallery/preview/extra.tsx", "export default [];\n") });
  expect((await judgeFromChanges(branchChanges(root))).reason).toBe("packages/ui/src/gallery/preview/extra.tsx was added");
});

test("from a real checkout without a recorded target branch, the sweep runs", async () => {
  const judged = await judgeFromChanges(branchChanges(checkout({ targetBranch: false, edit: (write) => write(STYLE, edited("#333", "#444")) })));
  expect(judged).toEqual({ sweep: true, reason: "no recorded target branch, so there are no changes to judge by" });
});

test("outside a git working copy the sweep runs", async () => {
  expect((await judgeFromChanges(branchChanges(null))).sweep).toBe(true);
});
