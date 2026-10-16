import { readdirSync, readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

// A scroll container in this package fades the edge it hides content behind, and the fade comes only
// from `components/scrollFade.tsx`, through its shared wrappers and hooks. This is the check that holds that true.
//
// The check reads this package's source, not the rendered page (jsdom has no layout, and Tailwind's
// overflow arrives as a class, so nothing there reports a scroll container) and not the built styles
// (they would show HeroUI's own internals and lose the way back to the site to fix). What it sees is
// therefore exactly what this package writes: an `overflow-auto`-style utility, a CSS or style-object
// `overflow: auto` declaration, found after comments are stripped and the fade's own two call shapes
// are cut out, so a site cannot pass by sitting in a file that imports the module. It cannot see a
// scroller that gets its overflow some other way (a library stylesheet, another package's DOM, an
// overflow class assembled from string pieces, or the package's `*.html` entry shells, which sit
// outside `src`), and that is not exemption but invisibility, which is why the scrollers this package
// does not write are written down in EXEMPTIONS below, each saying plainly whether the scan could
// have seen it.

const SRC = new URL("../", import.meta.url);

const FADE_MODULE = "components/scrollFade.tsx";

type Exemption = {
  surface: string;
  reason: string;
  /** Whether the scan can see the surface. A seen one names the `site` the scan consults. */
  seen: boolean;
  /**
   * The file (relative to `src`) and the matched text that the scan lets through, for exactly `count`
   * occurrences (default 1): a further one in the same file is reported like any other.
   */
  site?: { file: string; text: string; count?: number };
};

/**
 * The one place a surface that scrolls without this package's fade is declared. A scroller the scan
 * cannot see is listed too, so the list is the whole account of what the rule leaves alone.
 */
const EXEMPTIONS: Exemption[] = [
  {
    surface: "App.tsx: the window's main column",
    reason:
      "Scrolls sideways only, dropped at the `docked` breakpoint. The rule is the vertical edges; what it owes the other axis is keeping the mask off a sideways scrollbar, which the capability does itself.",
    seen: true,
    site: { file: "App.tsx", text: "overflow-x-auto" },
  },
  {
    surface: "HeroUI popovers: `.dropdown__popover` (every ActionMenu) and `.select__popover` (every Select)",
    reason:
      "Overflow comes from HeroUI's stylesheet, with react-aria capping the height. Each is the element that animates away, and the fade's `animation-timeline: scroll()` animation never settles the `getAnimations()` an exit transition waits on, so the fade cannot go on it. Exempt as a class.",
    seen: false,
  },
  {
    surface: "HeroUI `Tabs.ListContainer` (the settings dialog's sections, the Git mode's Uncommitted / Compare row, the session dialog's Conversation tabs)",
    reason:
      "The library wraps the strip in a `ScrollShadow` of its own, left as the library draws it. The settings column also cannot overflow: `docked:self-start` keeps its wrapper at content height.",
    seen: false,
  },
  {
    surface: "xterm's `.xterm-viewport` in the terminal pane",
    reason: "The scrollback viewport is xterm's own DOM; the wrapper this package renders is `overflow-hidden`, and there is no element to put the fade on.",
    seen: false,
  },
  {
    surface: "The report page",
    reason: "A document the console session's model wrote, inside an `<iframe srcDoc>`; it scrolls as a document.",
    seen: false,
  },
];

const SCROLL_UTILITY = /\boverflow-(?:[xy]-)?(?:auto|scroll)\b/g;
const SCROLL_DECLARATION = /\boverflow(?:-[xy])?\s*:\s*(?:auto|scroll)\b/gi;
const SCROLL_STYLE_PROP = /\boverflow[XY]?\s*:\s*["'](?:auto|scroll)["']/g;

type Site = { line: number; text: string };

const blank = (text: string) => text.replace(/[^\n]/g, " ");

/** The end of the quoted run opening at `start` (a quote or a backtick); a plain quote ends at the line. */
function endOfString(src: string, start: number): number {
  const quote = src[start];
  for (let i = start + 1; i < src.length; i++) {
    if (src[i] === "\\") i++;
    else if (src[i] === quote) return i + 1;
    else if (src[i] === "\n" && quote !== "`") return i;
  }
  return src.length;
}

/** The source with its comments blanked to spaces, so offsets and line numbers still point at the original. */
function stripComments(src: string, css = false): string {
  let out = "";
  for (let i = 0; i < src.length; ) {
    if (src.startsWith("/*", i)) {
      const end = src.indexOf("*/", i + 2);
      const stop = end < 0 ? src.length : end + 2;
      out += blank(src.slice(i, stop));
      i = stop;
    } else if (!css && src.startsWith("//", i)) {
      const end = src.indexOf("\n", i);
      const stop = end < 0 ? src.length : end;
      out += blank(src.slice(i, stop));
      i = stop;
    } else if (/["'`]/.test(src[i] ?? "")) {
      const stop = endOfString(src, i);
      out += src.slice(i, stop);
      i = stop;
    } else {
      out += src[i];
      i++;
    }
  }
  return out;
}

/** The end of the region that opens at `from` and closes when `close` meets depth zero outside a string. */
function endOfRegion(src: string, from: number, close: (char: string, depth: number) => boolean): number {
  let depth = 0;
  for (let i = from; i < src.length; i++) {
    const char = src[i] ?? "";
    if (/["'`]/.test(char)) i = endOfString(src, i) - 1;
    else if (close(char, depth)) return i + 1;
    else if ("({[".includes(char)) depth++;
    else if (")}]".includes(char)) depth--;
  }
  return src.length;
}

/** Blanks every region that legitimately carries a scroll utility: the fade's shared wrappers and hooks. */
function withoutFadedRegions(src: string): string {
  let out = src;
  const blankRegions = (opening: RegExp, close: (char: string, depth: number) => boolean) => {
    for (const match of src.matchAll(opening)) {
      const start = match.index + match[0].length;
      const end = endOfRegion(src, start, close);
      out = out.slice(0, start) + blank(out.slice(start, end)) + out.slice(end);
    }
  };
  // `useScrollFade(...)` or `useStickyScrollFade(...)`: up to the parenthesis that closes the call.
  blankRegions(/\buse(?:Sticky)?ScrollFade\s*(?:<[^>(]*>)?\(/g, (char, depth) => char === ")" && depth === 0);
  // `<FadedScroller ...>`: up to the `>` that closes the opening tag, not one inside a `{}` attribute.
  blankRegions(/<FadedScroller(?=[\s/>])/g, (char, depth) => char === ">" && depth === 0);
  return out;
}

/** Scroll containers in `source` that do not take their fade from the module. */
function findUnfadedScrollers(source: string, css = false): Site[] {
  const code = withoutFadedRegions(stripComments(source, css));
  // A stylesheet takes the utility pattern too, for a `@apply overflow-y-auto`; it can match nothing
  // else there.
  const patterns = css ? [SCROLL_DECLARATION, SCROLL_UTILITY] : [SCROLL_UTILITY, SCROLL_DECLARATION, SCROLL_STYLE_PROP];
  const found: Site[] = [];
  for (const pattern of patterns) {
    for (const match of code.matchAll(pattern)) {
      found.push({ line: code.slice(0, match.index).split("\n").length, text: match[0] });
    }
  }
  return found.sort((a, b) => a.line - b.line);
}

/** Lines that name `ScrollShadow`, which only the fade module may reach for. */
function findScrollShadowUse(source: string): Site[] {
  const code = stripComments(source);
  return [...code.matchAll(/\bScrollShadow\b/g)].map((match) => ({
    line: code.slice(0, match.index).split("\n").length,
    text: match[0],
  }));
}

function sourceFiles(): string[] {
  return readdirSync(SRC, { recursive: true, encoding: "utf8" })
    .filter((file) => /\.(?:tsx?|css)$/.test(file) && !/\.test\.tsx?$/.test(file))
    .sort();
}

/** The sites left once each exemption has covered the occurrences it declares, and no more. */
function withoutExempt(file: string, sites: Site[], exemptions: Exemption[]): Site[] {
  const left = new Map<Exemption, number>();
  for (const e of exemptions) if (e.site?.file === file) left.set(e, e.site.count ?? 1);
  return sites.filter((site) => {
    for (const [e, remaining] of left) {
      if (e.site?.text !== site.text || remaining === 0) continue;
      left.set(e, remaining - 1);
      return false;
    }
    return true;
  });
}

function unfadedProblems(file: string, source: string, exemptions = EXEMPTIONS): string[] {
  return withoutExempt(file, findUnfadedScrollers(source, file.endsWith(".css")), exemptions)
    .map(
      (site) =>
        `${file}:${site.line}: \`${site.text}\` is a scroll container with no fade. Take the fade from packages/ui/src/${FADE_MODULE}: ` +
        "`FadedScroller` if the scroller can be a wrapper element, `useScrollFade` if it has to be the scroll container itself. " +
        "If it already takes the fade, write the class inline in that call or tag, which is where this check looks. " +
        "If it is genuinely not a scroller this package can fade, declare it in EXEMPTIONS in " +
        "packages/ui/src/components/scrollFadeCoverage.test.ts with its reason.",
    );
}

describe("a scroll container in the UI package", () => {
  // The scanned set is what every check below rests on: narrowed by a drifting extension filter, or
  // by this file moving a directory deeper (`SRC` is relative to it), they would all pass over a
  // smaller set, or none at all, and report nothing.
  it("is looked for across the whole package", () => {
    const files = sourceFiles();
    expect(files).toContain(FADE_MODULE);
    expect(files).toContain("style.css");
    expect(files.length).toBeGreaterThan(200);
  });

  it("takes its fade from the scroll fade module or is declared exempt", () => {
    const problems = sourceFiles().flatMap((file) =>
      unfadedProblems(file, readFileSync(new URL(file, SRC), "utf8")),
    );
    expect(problems).toEqual([]);
  });

  it("is reached only through the module, never through HeroUI's ScrollShadow", () => {
    const problems = sourceFiles()
      .filter((file) => file !== FADE_MODULE)
      .flatMap((file) =>
        findScrollShadowUse(readFileSync(new URL(file, SRC), "utf8")).map(
          (site) =>
            `${file}:${site.line}: \`ScrollShadow\` is used outside packages/ui/src/${FADE_MODULE}, the only way in; use \`FadedScroller\` or \`useScrollFade\` from it.`,
        ),
      );
    expect(problems).toEqual([]);
  });

  it("keeps no exemption for a site that is gone", () => {
    const seen = EXEMPTIONS.filter((e) => e.seen);
    expect(seen.filter((e) => !e.site).map((e) => e.surface)).toEqual([]);
    for (const exemption of seen) {
      const { file, text, count = 1 } = exemption.site!;
      const sites = findUnfadedScrollers(readFileSync(new URL(file, SRC), "utf8"));
      expect(sites.filter((site) => site.text === text).length, exemption.surface).toBe(count);
    }
  });
});

describe("the scan", () => {
  const bare = "const A = () => <div className=\"max-h-40 overflow-y-auto p-1\">x</div>;";

  it("reports a scroll container with no fade, with its line", () => {
    expect(findUnfadedScrollers(`const a = 1;\n${bare}`)).toEqual([{ line: 2, text: "overflow-y-auto" }]);
  });

  it("reports one in a file that imports the module but does not use it there", () => {
    const source = `import { FadedScroller } from "./scrollFade";\n${bare}`;
    expect(findUnfadedScrollers(source)).toHaveLength(1);
  });

  it("reports the other spellings of a scroll container", () => {
    expect(findUnfadedScrollers("<div className=\"overflow-scroll\" />")).toHaveLength(1);
    expect(findUnfadedScrollers("<div className=\"docked:overflow-x-auto\" />")).toHaveLength(1);
    expect(findUnfadedScrollers("<div style={{ overflowY: \"auto\" }} />")).toHaveLength(1);
    expect(findUnfadedScrollers("<div className=\"[overflow:auto]\" />")).toHaveLength(1);
  });

  it("passes the same container faded through FadedScroller", () => {
    const source = "const A = () => <FadedScroller region={region} className=\"max-h-40 overflow-y-auto p-1\">x</FadedScroller>;";
    expect(findUnfadedScrollers(source)).toEqual([]);
  });

  it("passes the same container faded through useScrollFade, including a template literal", () => {
    expect(findUnfadedScrollers("const fade = useScrollFade({ className: \"max-h-40 overflow-y-auto p-1\" });")).toEqual([]);
    const source = "const fade = useScrollFade<HTMLPreElement>({\n  ref,\n  className: `min-h-0 overflow-auto ${wrap ? \"a\" : \"b\"}`,\n});";
    expect(findUnfadedScrollers(source)).toEqual([]);
  });

  it("does not let a faded site cover an unfaded one beside it", () => {
    const source = `${bare}\nconst fade = useScrollFade({ className: "overflow-auto" });\n<FadedScroller onChange={() => 1} className="overflow-auto">x</FadedScroller>;\n${bare}`;
    expect(findUnfadedScrollers(source).map((site) => site.line)).toEqual([1, 4]);
  });

  it("ignores comments and a scroll utility named only in prose", () => {
    const source = "// overflow-auto here\n/* and overflow-y-auto\n   here */\nconst a = 1;";
    expect(findUnfadedScrollers(source)).toEqual([]);
  });

  it("does not take a // inside a string for a comment", () => {
    const source = "const url = \"http://x\"; const b = <div className=\"overflow-auto\" />;";
    expect(findUnfadedScrollers(source)).toHaveLength(1);
  });

  it("does not treat clipping as scrolling", () => {
    expect(findUnfadedScrollers("<div className=\"overflow-hidden overflow-visible overflow-clip\" />")).toEqual([]);
  });

  it("reports a scroll container declared in a stylesheet", () => {
    const css = "/* overflow-y: auto; */\n.pane {\n  overflow-y: auto;\n}\n";
    expect(findUnfadedScrollers(css, true)).toEqual([{ line: 3, text: "overflow-y: auto" }]);
    expect(findUnfadedScrollers(".pane { overflow: hidden; }", true)).toEqual([]);
  });

  it("reports a stylesheet reaching for the utility through @apply", () => {
    expect(findUnfadedScrollers(".pane { @apply overflow-y-auto; }", true)).toHaveLength(1);
  });

  it("reports ScrollShadow named outside a comment, and only there", () => {
    expect(findScrollShadowUse("import { ScrollShadow } from \"@heroui/react\";")).toHaveLength(1);
    expect(findScrollShadowUse("import * as H from \"@heroui/react\";\nconst S = H.ScrollShadow;")).toHaveLength(1);
    expect(findScrollShadowUse("// a bare ScrollShadow\nimport { useScrollShadow } from \"x\";")).toEqual([]);
  });

  it("lets an exemption cover only the occurrences it declares", () => {
    const exempt: Exemption[] = [
      { surface: "main", reason: "r", seen: true, site: { file: "App.tsx", text: "overflow-x-auto" } },
    ];
    const source = "<main className=\"overflow-x-auto\" />\n<div className=\"overflow-x-auto\" />";
    const problems = unfadedProblems("App.tsx", source, exempt);
    expect(problems).toHaveLength(1);
    expect(problems[0]).toContain("App.tsx:2");
  });

  it("builds a message that names the file and the surface and says where the fade and an exemption go", () => {
    const [message] = unfadedProblems("sidebar/Example.tsx", bare);
    expect(message).toContain("sidebar/Example.tsx:1");
    expect(message).toContain("overflow-y-auto");
    expect(message).toContain("packages/ui/src/components/scrollFade.tsx");
    expect(message).toContain("FadedScroller");
    expect(message).toContain("useScrollFade");
    expect(message).toContain("inline");
    expect(message).toContain("EXEMPTIONS");
  });
});
