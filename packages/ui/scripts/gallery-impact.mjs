// Whether what a branch changed can make a gallery sweep fail, so that `pnpm gallery:sweep` loads no page
// after a change that cannot. A page fails the sweep by never reaching `html[data-gallery-ready]`, by
// raising the `[data-gallery-failure]` overlay, or by an uncaught error; the sweep compares no pixels
// and no colours. So a change cannot make it fail when it is outside what the gallery loads, is only
// comments or formatting, or in CSS only changes properties that decide how an element looks but not
// where it is, whether it is there, or what receives the pointer. Everything else, and anything that
// cannot be judged, runs the sweep. Kept apart from `gallery-sweep.mjs`, which runs on import, so that the
// rule has tests of its own.

import { spawnSync } from "node:child_process";
import postcss from "postcss";
import { changedFiles, resolveBase } from "../../../scripts/verify/changes.mjs";
import { workingTree } from "../../../scripts/verify/tree.mjs";

/** What a page's result depends on, relative to the repository root: this package (the app, the gallery,
 * the fixtures, the configuration of Vite and TypeScript), what it imports from outside it (`config/app.json`,
 * `scripts/env.mjs`, the desktop icon the About section shows), and what decides the dependency versions
 * (the manifests, the lockfile, the patches). The sweep keys its records of clean pages on these as one key,
 * not one per scenario: which scenarios a source file reaches is not known without tracing imports, and a key
 * that missed an input would pass a page that no longer renders. A change anywhere else cannot reach a page. */
export const STAMP_INPUTS = [
  "packages/ui",
  "config",
  "scripts/env.mjs",
  "scripts/env.d.mts",
  "package.json",
  "pnpm-lock.yaml",
  "pnpm-workspace.yaml",
  ".npmrc",
  "patches",
  "apps/desktop/src-tauri/icons/128x128.png",
];
/** Git-ignored, yet part of the swept gallery: the checkout's own extra scenarios. */
export const STAMP_FORCED = ["packages/ui/src/gallery/preview"];

/** What the sweep's result depends on: what the pages load, the root's tracked `.env`, which sets the daemon
 * port the dev server and the app run with (the records count it with the other env files Vite reads), and
 * the sweep's own code, which imports from `scripts/verify/`. */
const SWEEP_INPUTS = [...STAMP_INPUTS, ".env", "scripts/verify"];

const within = (file, entry) => file === entry || file.startsWith(`${entry}/`);

/** Files inside the inputs that no page loads: the gallery imports no Markdown, and no page imports a test.
 * Tailwind scans them, and comments, for class names, so this, like the rule on comments, assumes no class
 * name is built at runtime that only a comment, a doc or a test spells out. */
const NOT_LOADED = [/\.md$/, /\.test\.[cm]?[jt]sx?$/];

const CODE = /\.[cm]?[jt]sx?$/;

// ---- CSS -----------------------------------------------------------------------------------------

/** Properties that decide only how an element is painted: not its box, not whether it is displayed or
 * visible, not what the pointer hits, and nothing a step reads: not the cursor (a step checks it), not
 * opacity (a step waits for a floating pane to be fully opaque), and not an animation or a transition (a
 * step waits for an exit transition to end). Matched without a vendor prefix. Left out though they paint:
 * `filter` and `backdrop-filter`, which make a containing block for fixed descendants; `scrollbar-color`,
 * which turns off the `::-webkit-scrollbar` styling that sizes a scrollbar; `font-size`, `font`,
 * `line-height` and `letter-spacing`, which size the text's lines and boxes directly; `border-style` and
 * `border-width`, which size the box. The font family, weight and the like change text widths a little,
 * and are counted as appearance all the same: no step targets an element by where text ends. */
const APPEARANCE_ONLY = [
  /^color$/,
  /^background(-.+)?$/,
  /^(box|text)-shadow$/,
  /^border(-(block|inline|top|right|bottom|left)(-(start|end))?)?-color$/,
  /^border(-(top|bottom|start|end)-(left|right|start|end))?-radius$/,
  /^outline(-.+)?$/,
  /^text-decoration(-.+)?$/,
  /^text-underline-(offset|position)$/,
  /^text-emphasis(-.+)?$/,
  /^(caret|accent|stop|flood|lighting|tap-highlight)-color$/,
  /^(fill|stroke)(-opacity)?$/,
  /^stop-opacity$/,
  /^color-scheme$/,
  /^font-(family|weight|style|stretch|kerning|optical-sizing|smoothing|feature-settings|variation-settings)$/,
  /^font-(variant|synthesis)(-.+)?$/,
  /^text-rendering$/,
];

const isAppearanceOnly = (prop) => {
  const bare = prop.toLowerCase().replace(/^-(webkit|moz|ms|o)-/, "");
  return APPEARANCE_ONLY.some((re) => re.test(bare));
};

const NAMED_COLOURS = new Set(
  (
    "transparent currentcolor black silver gray grey white maroon red purple fuchsia green lime olive yellow navy blue teal aqua " +
    "orange aliceblue antiquewhite aquamarine azure beige bisque blanchedalmond blueviolet brown burlywood cadetblue chartreuse " +
    "chocolate coral cornflowerblue cornsilk crimson cyan darkblue darkcyan darkgoldenrod darkgray darkgrey darkgreen darkkhaki " +
    "darkmagenta darkolivegreen darkorange darkorchid darkred darksalmon darkseagreen darkslateblue darkslategray darkslategrey " +
    "darkturquoise darkviolet deeppink deepskyblue dimgray dimgrey dodgerblue firebrick floralwhite forestgreen gainsboro " +
    "ghostwhite gold goldenrod greenyellow honeydew hotpink indianred indigo ivory khaki lavender lavenderblush lawngreen " +
    "lemonchiffon lightblue lightcoral lightcyan lightgoldenrodyellow lightgray lightgrey lightgreen lightpink lightsalmon " +
    "lightseagreen lightskyblue lightslategray lightslategrey lightsteelblue lightyellow limegreen linen magenta mediumaquamarine " +
    "mediumblue mediumorchid mediumpurple mediumseagreen mediumslateblue mediumspringgreen mediumturquoise mediumvioletred " +
    "midnightblue mintcream mistyrose moccasin navajowhite oldlace olivedrab orangered orchid palegoldenrod palegreen " +
    "paleturquoise palevioletred papayawhip peachpuff peru pink plum powderblue rebeccapurple rosybrown royalblue saddlebrown " +
    "salmon sandybrown seagreen seashell sienna skyblue slateblue slategray slategrey snow springgreen steelblue tan thistle " +
    "tomato turquoise violet wheat whitesmoke yellowgreen"
  ).split(" "),
);
const COLOUR_FUNCTION = /^(rgba?|hsla?|hwb|lab|lch|oklab|oklch|color|color-mix|light-dark)\(/;

/** Whether `value` is one function call: its first parenthesis closes at its last character. */
function isOneCall(value) {
  let depth = 0;
  for (let i = value.indexOf("("); i < value.length; i++) {
    if (value[i] === "(") depth++;
    else if (value[i] === ")" && --depth === 0) return i === value.length - 1;
  }
  return false;
}
const VAR_ONLY = /^var\(\s*(--[\w-]+)\s*(,.*)?\)$/s;

const normalise = (value) => value.trim().replace(/\s+/g, " ");

/** The custom properties every definition of which, in `roots`, is a colour: a colour literal, or a
 * reference to another such property. Whatever reads a colour-valued property reads a colour after a
 * change to another colour, so the property behaves as an appearance-only one. A property defined
 * elsewhere (a library's) is not known to be a colour. */
function colourProperties(roots) {
  const definitions = new Map();
  for (const root of roots) {
    root.walkDecls(/^--/, (decl) => {
      if (!definitions.has(decl.prop)) definitions.set(decl.prop, []);
      definitions.get(decl.prop).push(normalise(decl.value).toLowerCase());
    });
  }
  const memo = new Map();
  const isColourProperty = (name) => {
    if (memo.has(name)) return memo.get(name);
    memo.set(name, false); // A reference cycle is not a colour.
    const result = definitions.has(name) && definitions.get(name).every(isColour);
    memo.set(name, result);
    return result;
  };
  const isColour = (value) => {
    if (/^#[0-9a-f]{3,8}$/.test(value) || NAMED_COLOURS.has(value) || (COLOUR_FUNCTION.test(value) && isOneCall(value))) return true;
    const reference = VAR_ONLY.exec(value);
    return reference !== null && isColourProperty(reference[1]) && (reference[2] === undefined || isColour(normalise(reference[2].slice(1))));
  };
  return new Set([...definitions.keys()].filter(isColourProperty));
}

/** `container`'s children as they matter to a page: its rules and at-rules in order, by selector or name
 * and parameters, and the declarations that are not appearance-only, in order. Comments are dropped. */
function significant(container, colours) {
  const nested = [];
  const declarations = [];
  for (const node of container.nodes ?? []) {
    if (node.type === "rule") nested.push({ node, key: normalise(node.selector) });
    else if (node.type === "atrule") nested.push({ node, key: `@${node.name.toLowerCase()} ${normalise(node.params)}`.trim() + (node.nodes ? " {}" : ";") });
    else if (node.type === "decl") {
      const appearance = node.prop.startsWith("--") ? colours.has(node.prop) : isAppearanceOnly(node.prop);
      if (!appearance) declarations.push(`${node.prop.startsWith("--") ? node.prop : node.prop.toLowerCase()}: ${normalise(node.value)}${node.important ? " !important" : ""}`);
    }
  }
  return { nested, declarations };
}

/** Where `before` and `after` differ in a way a page can notice, or undefined. `where` names the container,
 * as the path of selectors and at-rules down to it. */
function compareContainers(before, after, colours, where) {
  const a = significant(before, colours);
  const b = significant(after, colours);
  const keys = (list) => list.map((n) => n.key).join("\n");
  if (keys(a.nested) !== keys(b.nested)) return `adds, removes, renames or moves a rule or an at-rule${where ? ` in ${where}` : ""}`;
  if (a.declarations.join("\n") !== b.declarations.join("\n")) {
    const changed = [...new Set([...a.declarations, ...b.declarations].filter((d) => !a.declarations.includes(d) || !b.declarations.includes(d)))];
    const props = [...new Set(changed.map((d) => d.slice(0, d.indexOf(":"))))];
    return `changes ${props.join(", ")}${where ? ` in ${where}` : ""}, which can move, hide or cover what a step acts on`;
  }
  for (let i = 0; i < a.nested.length; i++) {
    const name = a.nested[i].key.replace(/ \{\}$|;$/, "");
    const found = compareContainers(a.nested[i].node, b.nested[i].node, colours, where ? `${where} > ${name}` : name);
    if (found) return found;
  }
  return undefined;
}

/** Why the change from `before` to `after` of a stylesheet can make a page fail, or undefined. */
export function cssImpact(before, after) {
  let roots;
  try {
    roots = [postcss.parse(before), postcss.parse(after)];
  } catch (error) {
    return `does not parse as CSS (${error.reason ?? error.message}), so it cannot be judged`;
  }
  return compareContainers(roots[0], roots[1], colourProperties(roots), "");
}

// ---- code ----------------------------------------------------------------------------------------

let ts;

/** Comments that change what a script compiles to: the React Compiler skips a function that disables or
 * re-enables a rule of `react-hooks` or its own, or carries a Flow suppression of one, wherever in a comment
 * it is written, and a JSX pragma changes how JSX compiles. What such a comment applies to depends on where
 * it sits, so in a file that holds one before or after, any change to comments counts as code. Matched
 * anywhere in the raw text, so one in prose or in a string only makes the change count as code. */
const DIRECTIVE_COMMENT = /eslint-(disable|enable)|@jsx|\$Flow/;

/** Why the change from `before` to `after` of a script can make a page fail, or undefined: printed without
 * comments, through TypeScript's own parser, the two are the same program, and neither holds a comment a
 * compiler reads. */
export async function codeImpact(before, after, file) {
  ts ??= (await import("typescript")).default;
  const kind = /\.[cm]?tsx$/.test(file) ? ts.ScriptKind.TSX : /\.[cm]?jsx$/.test(file) ? ts.ScriptKind.JSX : /\.[cm]?ts$/.test(file) ? ts.ScriptKind.TS : ts.ScriptKind.JS;
  const printer = ts.createPrinter({ removeComments: true });
  const print = (text) => {
    const source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, false, kind);
    // `parseDiagnostics` is not public API: where it is missing, the file is taken as not parsing.
    return source.parseDiagnostics?.length === 0 ? printer.printFile(source) : undefined;
  };
  const [a, b] = [print(before), print(after)];
  if (a === undefined || b === undefined) return "does not parse, so it cannot be judged";
  if (a !== b) return "changes code, not only comments or formatting";
  return DIRECTIVE_COMMENT.test(before) || DIRECTIVE_COMMENT.test(after) ? "changes only comments or formatting, but holds a comment the React Compiler or the JSX transform reads" : undefined;
}

// ---- the branch's changes ------------------------------------------------------------------------

/** Why the change of `file` can make a page fail, or undefined when it cannot. `read(file)` gives the file
 * as it was at the base and as it is now, each undefined where it does not exist. */
export async function fileImpact(file, read) {
  if (!SWEEP_INPUTS.some((entry) => within(file, entry))) return undefined;
  if (NOT_LOADED.some((re) => re.test(file))) return undefined;
  const isCss = file.endsWith(".css");
  if (!isCss && !CODE.test(file)) return "is loaded by the gallery or decides what it runs on, and is not CSS or code whose change can be judged";
  const { before, after } = read(file);
  if (before === undefined) return "was added";
  if (after === undefined) return "was removed";
  return isCss ? cssImpact(before, after) : codeImpact(before, after, file);
}

/** Whether to sweep, and why, from `base` (what `resolveBase` returned), `files`, the paths changed between
 * the base and the working tree (null when git could not say), and `read`, as `fileImpact` takes it. */
export async function judgeChanges(base, files, read) {
  if (!base.commit) return { sweep: true, reason: `${base.reason}, so there are no changes to judge by` };
  if (files === null) return { sweep: true, reason: "git could not list the changes since the merge base" };
  if (files.length === 0) return { sweep: true, reason: "nothing differs from the merge base, so there are no changes to judge by" };
  for (const file of files) {
    const why = await fileImpact(file, read);
    if (why) return { sweep: true, reason: `${file} ${why}` };
  }
  return { sweep: false, reason: files.length === 1 ? "the one path changed since the merge base cannot make a page fail" : `none of the ${files.length} paths changed since the merge base can make a page fail` };
}

/** What the branch of the working copy at `root` (null when there is none) changed: `base`, what
 * `resolveBase` returned, `files`, the paths changed between it and the working tree as it is now, the
 * git-ignored extra scenarios included (null when git cannot say), and `read`, as `fileImpact` takes it.
 * Taken once per sweep, for its languages and for whether to sweep at all; where git fails in a way the
 * helpers do not catch, the changes are taken as unknown. */
export function branchChanges(root) {
  if (!root) return { base: { reason: "not inside a git working copy" }, files: null, read: () => ({}) };
  try {
    return readChanges(root);
  } catch (error) {
    return { base: { reason: `the changes could not be read (${String(error?.message ?? error).split("\n")[0]})` }, files: null, read: () => ({}) };
  }
}

function readChanges(root) {
  const run = (args) => spawnSync("git", args, { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"], maxBuffer: 64 * 1024 * 1024 });
  const git = (...args) => {
    const result = run(args);
    return result.status === 0 ? result.stdout.trim() : null;
  };
  const blob = (object, file) => {
    const result = run(["cat-file", "blob", `${object}:${file}`]);
    return result.status === 0 ? result.stdout : undefined;
  };
  const base = resolveBase(git);
  const tree = base.commit ? workingTree(root, { forced: STAMP_FORCED }) : null;
  const files = tree ? changedFiles(git, base.commit, tree) : null;
  return { base, files, read: (file) => ({ before: blob(base.commit, file), after: blob(tree, file) }) };
}

/** `judgeChanges` for what `branchChanges` returned. A judgement that throws sweeps. */
export async function judgeFromChanges({ base, files, read }) {
  try {
    return await judgeChanges(base, files, read);
  } catch (error) {
    return { sweep: true, reason: `the changes could not be judged (${String(error?.message ?? error).split("\n")[0]})` };
  }
}
