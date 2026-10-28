// @vitest-environment node
import { existsSync, readdirSync, readFileSync } from "node:fs";

import { expect, it } from "vitest";

// `style.css` imports the HeroUI stylesheets of the components this app renders instead of the whole
// package, so a component newly taken from `@heroui/react` renders unstyled until its sheet is added.
// The sheets wanted are those of the components the source imports plus those they are built from
// (read from the `@heroui/react` package's own module imports), and they keep the order of the
// package's `components/index.css`.

const SRC = new URL("./", import.meta.url);
const REACT_DIST = new URL("../node_modules/@heroui/react/dist/", import.meta.url);
const STYLES_DIST = new URL("../node_modules/@heroui/styles/dist/components/", import.meta.url);

function sourceFiles(): string[] {
  return readdirSync(SRC, { recursive: true, encoding: "utf8" })
    .filter((file) => /\.tsx?$/.test(file) && !/\.test\.tsx?$/.test(file))
    .map((file) => readFileSync(new URL(file, SRC), "utf8"));
}

/** Component directory by exported name, from the package's own index. */
function directoriesByExport(): Map<string, string> {
  const index = readFileSync(new URL("index.js", REACT_DIST), "utf8");
  const directories = new Map<string, string>();
  for (const [, names, directory] of index.matchAll(/export \{([^}]*)\} from '\.\/components\/([\w-]+)\//g)) {
    for (const name of names.split(",")) directories.set(name.trim(), directory);
  }
  return directories;
}

function importedComponents(): string[] {
  const directories = directoriesByExport();
  const pattern = /import\s+(?:type\s+)?\{([^}]*)\}\s+from\s+["']@heroui\/react["']/g;
  const used = new Set<string>();
  for (const source of sourceFiles()) {
    for (const [, names] of source.matchAll(pattern)) {
      for (const name of names.split(",")) {
        const directory = directories.get(name.trim().replace(/^type\s+/, "").split(/\s+as\s+/)[0]);
        if (directory) used.add(directory);
      }
    }
  }
  return [...used];
}

/** The components themselves and everything their modules import from a sibling component. */
function closure(roots: string[]): string[] {
  const seen = new Set(roots);
  const pending = [...roots];
  for (let directory = pending.pop(); directory; directory = pending.pop()) {
    const folder = new URL(`components/${directory}/`, REACT_DIST);
    for (const file of readdirSync(folder).filter((name) => name.endsWith(".js"))) {
      for (const [, dependency] of readFileSync(new URL(file, folder), "utf8").matchAll(/from ["']\.\.\/([\w-]+)\//g)) {
        if (seen.has(dependency)) continue;
        seen.add(dependency);
        pending.push(dependency);
      }
    }
  }
  return [...seen].filter((name) => existsSync(new URL(`${name}.css`, STYLES_DIST)));
}

const imported = [...readFileSync(new URL("style.css", SRC), "utf8").matchAll(/@heroui\/styles\/components\/([\w-]+)\.css/g)].map(
  (match) => match[1],
);
const needed = closure(importedComponents());

it("imports the stylesheet of every HeroUI component the source uses or depends on", () => {
  expect(needed.filter((name) => !imported.includes(name)).sort(), "sheets missing from style.css").toEqual([]);
});

it("imports no HeroUI stylesheet that nothing uses", () => {
  expect(imported.filter((name) => !needed.includes(name)).sort(), "unused sheets in style.css").toEqual([]);
});

it("keeps the stylesheets in the order of the package's components/index.css", () => {
  const index = readFileSync(new URL("index.css", STYLES_DIST), "utf8");
  const packageOrder = [...index.matchAll(/@import "\.\/([\w-]+)\.css"/g)].map((match) => match[1]);
  expect(imported, "sheets in style.css").toEqual(packageOrder.filter((name) => imported.includes(name)));
});

it("pins tw-animate-css to the version @heroui/styles depends on", () => {
  const read = (url: URL) => JSON.parse(readFileSync(url, "utf8")) as { devDependencies?: Record<string, string>; dependencies?: Record<string, string> };
  const own = read(new URL("../package.json", import.meta.url)).dependencies?.["tw-animate-css"];
  const heroui = read(new URL("../node_modules/@heroui/styles/package.json", import.meta.url)).dependencies?.["tw-animate-css"];
  expect(own, "tw-animate-css in package.json").toBe(heroui);
});

// HeroUI's popover-level overlays (menus, popovers, tooltips, select and combo box lists) zoom and fade in and out
// whatever the motion preference, so `style.css` stops every overlay's entering and exiting animation itself. The
// rule has to stay outside any cascade layer: an unlayered declaration is the only kind that wins over the
// components layer the sheets are imported into, whatever their selectors' specificity.
it("stops every imported overlay's enter and exit animation under reduced motion, outside the cascade layers", () => {
  const style = readFileSync(new URL("style.css", SRC), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const animated = imported.filter((name) => /\[data-(entering|exiting)="true"\][^{]*\{[^}]*animate-(in|out)/.test(readFileSync(new URL(`${name}.css`, STYLES_DIST), "utf8")));
  expect(animated, "imported sheets that animate an overlay in or out").not.toEqual([]);
  expect(style).not.toMatch(/@layer\s+[\w-]+\s*\{/);
  const reduced = [...style.matchAll(/@media \(prefers-reduced-motion: reduce\) \{([\s\S]*?)\n\}/g)].map((match) => match[1]);
  expect(reduced.some((block) => /\[data-entering="true"\],\s*\[data-exiting="true"\]\s*\{\s*animation: none;\s*\}/.test(block))).toBe(true);
});

/** The blocks of a stylesheet whose own declarations start a transition with no `motion-reduce:transition-none`
 * beside it, as the path of their nested selectors. */
function unguardedTransitions(sheet: string): string[][] {
  const css = sheet.replace(/\/\*[\s\S]*?\*\//g, "");
  const found: string[][] = [];
  const path: string[] = [];
  const bodies: string[] = [""];
  let text = "";
  for (const char of css) {
    if (char === "{") {
      path.push(text.split(";").at(-1)!.trim());
      bodies.push("");
      text = "";
    } else if (char === "}") {
      const body = bodies.pop()! + text;
      const transitions = /(^|[\s;])transition\s*:(?!\s*none)|@apply[^;]*\btransition(-[\w[\],-]+)?(?=[\s;])(?<!transition-none)/.test(body);
      if (transitions && !body.includes("motion-reduce:transition-none")) found.push([...path]);
      path.pop();
      text = "";
    } else {
      text += char;
      if (char === ";") {
        bodies[bodies.length - 1] += text;
        text = "";
      }
    }
  }
  return found;
}

// HeroUI guards its transitions with `motion-reduce:transition-none`, all but a few, which `style.css` guards itself.
// Left out: an overlay entering or leaving, whose animation the rule above stops (nothing changes its opacity or
// transform outside the keyframes, so no transition runs there), and the autofill rule's 9999s background
// transition, which keeps the browser's autofill colour from showing rather than animating anything.
it("stops under reduced motion every transition an imported HeroUI sheet leaves running", () => {
  const style = readFileSync(new URL("style.css", SRC), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const guarded = new Set(
    [...style.matchAll(/@media \(prefers-reduced-motion: reduce\) \{\s*([^{]+)\{\s*transition: none;\s*\}/g)].flatMap(([, selectors]) => selectors.split(",").map((selector) => selector.trim())),
  );
  const missing = imported.flatMap((name) =>
    unguardedTransitions(readFileSync(new URL(`${name}.css`, STYLES_DIST), "utf8"))
      .filter((path) => !path.some((selector) => /data-(entering|exiting)|autofill/.test(selector)))
      .filter((path) => {
        // Every selector of the list counts on its own, and of each only the subject: the compound after the last
        // descendant or child combinator, a nested `&` standing for the parent's selector.
        const members = path.at(-1)!.split(",").map((member) => member.trim().replace(/^&/, path.at(-2) ?? ""));
        return !members.every((member) => {
          const subject = member.split(/\s*>\s*|\s+(?![^[]*\])/).at(-1)!;
          return (subject.match(/\.[\w-]+|\[data-slot="[\w-]+"\]/g) ?? []).some((name) => guarded.has(name));
        });
      })
      .map((path) => `${name}.css: ${path.join(" ")}`),
  );
  expect(missing, "unguarded transitions").toEqual([]);
});

// A transition started inside content a disclosure has folded away (`hidden="until-found"`) never reports its end, and
// react-aria holds a focus move after a press without a pointer until no transition runs on the page, so `style.css`
// lets nothing transition there. jsdom runs no transition, so this reads the rule; the gallery's directory picker
// over Add project (`dialog-directory-picker`), closed by a script's click on Cancel, shows the focus it guards.
it("lets nothing transition inside folded disclosure content, outside the cascade layers", () => {
  const style = readFileSync(new URL("style.css", SRC), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  expect(imported).toContain("disclosure");
  expect(style).not.toMatch(/@layer\s+[\w-]+\s*\{/);
  expect(style).toMatch(/(^|\n)\[hidden="until-found"\] \*\s*\{\s*transition-property: none;\s*\}/);
});
