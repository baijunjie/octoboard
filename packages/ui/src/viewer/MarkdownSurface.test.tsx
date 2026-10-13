// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { isolate } from "../i18n/isolate";
import { currentLanguage, setLanguage } from "../i18n/language";
import type { PlatformAdapter } from "../platform";
import { RENDER_BUDGETS } from "./budgets";
import { PREFERENCE_KEYS } from "../preferenceKeys";
import { ThemeProvider } from "../theme";
import type { Daemon } from "../store";
import type { ViewerSubject } from "./content";
import { FileViewer } from "./FileViewer";
import { markdownView } from "./markdownView";
import type { MarkdownLocation } from "./markdownLocation";
import { openableAddress } from "./MarkdownDocument";
import { ViewerHost } from "./viewerTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The rendering library does not run under jsdom; a stand-in shows the text and grammar it is handed.
vi.mock("./renderer", () => ({
  HighlightedFile: ({ text, language, onDrawn }: { text: string; language?: string; onDrawn?: () => void }) => {
    onDrawn?.();
    return (
      <pre data-renderer="" data-language={language}>
        {text}
      </pre>
    );
  },
  RenderedDiff: () => <div data-renderer="" />,
  endRendererPool: () => {},
}));

vi.mock("./MermaidBlock", () => ({
  MermaidBlock: ({ source, overBudget }: { source: string; overBudget: boolean }) => (
    <div data-diagram="" data-over-budget={overBudget ? "true" : "false"}>
      {source}
    </div>
  ),
}));

window.matchMedia ??= (query: string) =>
  ({ matches: false, media: query, addEventListener() {}, removeEventListener() {} }) as unknown as MediaQueryList;

beforeEach(() => {
  markdownView.set("document");
  localStorage.clear();
});

const SOURCE = `# Title

A [link](https://example.com/page) and a [relative](./other.md) one, and [script](javascript:alert(1)).

![remote picture](https://example.com/tracker.png)

- item one
- [x] done

| Name | Value |
| ---- | ----- |
| a    | 1     |

<script>window.__pwned = true</script>
<img src="https://example.com/raw.png" onerror="window.__pwned = true">

\`\`\`ts
const answer = 42;
\`\`\`
`;

const markdownFile = (path = "docs/README.md", text = SOURCE): ViewerSubject => ({
  key: path,
  path,
  content: { state: "file", body: { kind: "text", text, size: text.length } },
});

/** Mounts the viewer on `subject`, with the page's link capability, and lets the lazily loaded
 * document renderer arrive. */
async function open(
  subject: ViewerSubject,
  links?: PlatformAdapter["links"],
  extra?: { location?: Omit<MarkdownLocation, "path">; request?: Daemon["request"] },
): Promise<HTMLElement> {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() =>
    root.render(
      <ViewerHost links={links} request={extra?.request}>
        <ThemeProvider>
          <FileViewer subject={subject} onClose={() => {}} location={extra?.location} />
        </ThemeProvider>
      </ViewerHost>,
    ),
  );
  for (let i = 0; i < 5; i += 1) await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
  return document.querySelector<HTMLElement>("[role=dialog]")!;
}

const region = () => document.querySelector<HTMLElement>("[role=region]")!;
const choice = (label: string) => document.querySelector<HTMLElement>(`button[aria-label="${label}"]`)!;
// The one Markdown view button, whichever view it would switch to.
const viewToggle = () => document.querySelector<HTMLElement>('button[aria-label="Show source"], button[aria-label="Show document"]');

/** Makes the app's language `language` for the test, and puts the earlier one back after it. */
function withLanguage(language: Parameters<typeof setLanguage>[0]): void {
  const before = currentLanguage();
  act(() => setLanguage(language));
  onTestFinished(() => act(() => setLanguage(before)));
}

it("opens a Markdown file as a document, with the choice in the header's controls", async () => {
  const dialog = await open(markdownFile());
  expect(region().querySelector("h1")?.textContent).toBe("Title");
  expect(region().querySelectorAll("li")).toHaveLength(2);
  expect(region().querySelector("table th")?.textContent).toBe("Name");
  expect(region().getAttribute("aria-label")).toBe(`Contents of ${isolate("README.md")}`);
  expect(region().tabIndex).toBe(0);
  expect(region().parentElement?.classList.contains("surface--tertiary")).toBe(true);
  expect(choice("Show source").closest("[data-viewer-description]")).not.toBeNull();
  expect(choice("Show source").getAttribute("aria-pressed")).toBe("false");
  // The document needs no wrap choice: its code is always wrapped.
  expect([...dialog.querySelectorAll("button")].some((b) => b.getAttribute("aria-label") === "Wrap lines")).toBe(false);
});

it("highlights a fenced block through the code renderer, by the fence's grammar", async () => {
  await open(markdownFile());
  const code = region().querySelector("[data-renderer]")!;
  expect(code.getAttribute("data-language")).toBe("ts");
  expect(code.textContent).toBe("const answer = 42;");
});

it("switches to the source and back, and stores the choice", async () => {
  await open(markdownFile());
  act(() => choice("Show source").click());
  expect(document.querySelector("h1")).toBeNull();
  expect(choice("Show document").getAttribute("aria-pressed")).toBe("true");
  expect(region().textContent).toContain("# Title");
  expect(localStorage.getItem(PREFERENCE_KEYS.markdownView)).toBe("source");
  expect(markdownView.get()).toBe("source");
  act(() => choice("Show document").click());
  expect(region().querySelector("h1")?.textContent).toBe("Title");
  expect(localStorage.getItem(PREFERENCE_KEYS.markdownView)).toBe("document");
});

it("keeps the document inert: no raw HTML runs, no image is fetched, no script element exists", async () => {
  await open(markdownFile());
  expect(region().querySelector("script, img, iframe")).toBeNull();
  expect(region().textContent).toContain("<script>window.__pwned = true</script>");
  expect(region().textContent).toContain("remote picture");
  expect((window as { __pwned?: boolean }).__pwned).toBeUndefined();
});

it("opens a link through the platform, and never navigates the viewer", async () => {
  const opened: string[] = [];
  await open(markdownFile(), { open: async (url) => void opened.push(url) });
  const anchors = [...region().querySelectorAll("a")];
  // Only the absolute web link is a link; a relative path and a script address are text.
  expect(anchors.map((a) => a.textContent)).toEqual(["link"]);
  const click = new MouseEvent("click", { bubbles: true, cancelable: true });
  act(() => void anchors[0].dispatchEvent(click));
  expect(click.defaultPrevented).toBe(true);
  expect(opened).toEqual(["https://example.com/page"]);
  expect(region().textContent).toContain("relative");
  expect(region().textContent).toContain("script");
});

it("draws no link where the platform cannot open one", async () => {
  await open(markdownFile());
  expect(region().querySelector("a")).toBeNull();
  expect(region().textContent).toContain("link");
});

it("offers the choice for Markdown only, and not for a Markdown diff", async () => {
  await open({ key: "a", path: "a.ts", content: { state: "file", body: { kind: "text", text: "a\n", size: 2 } } });
  expect(viewToggle()).toBeNull();
  await open({
    key: "d",
    path: "docs/README.md",
    content: {
      state: "change",
      change: {
        old: { state: "present", path: "docs/README.md", kind: "file" },
        new: { state: "present", path: "docs/README.md", kind: "file" },
        patch: "@@ -1 +1 @@\n-a\n+b\n",
      },
    },
  });
  expect(document.querySelectorAll("h1")).toHaveLength(0);
  expect(viewToggle()).toBeNull();
});

it("offers the document for an untracked or added Markdown file, but not for a conflict body", async () => {
  const body = { kind: "text" as const, text: "# Added\n", size: 8 };
  await open({
    key: "u",
    path: "docs/new.md",
    status: "untracked",
    content: { state: "change", change: { old: { state: "absent" }, new: { state: "present", path: "docs/new.md", kind: "file", body } } },
  });
  expect(region().querySelector("h1")?.textContent).toBe("Added");
  expect(viewToggle()).not.toBeNull();
});

it("shows a Markdown conflict body as source only", async () => {
  await open({
    key: "c",
    path: "docs/merge.md",
    content: {
      state: "conflict",
      conflict: "Both modified.",
      body: { kind: "text", text: "<<<<<<< ours\n# A\n=======\n# B\n>>>>>>> theirs\n", size: 40 },
    },
  });
  expect(document.querySelector("h1")).toBeNull();
  expect(viewToggle()).toBeNull();
  expect(region().textContent).toContain("<<<<<<< ours");
});

it("keeps its own styling on the elements the library gives classes of its own", async () => {
  await open(markdownFile("docs/tasks.md", "- [x] done\n- [ ] open\n\n- plain\n"));
  const lists = [...region().querySelectorAll("ul")];
  expect(lists[0].className).toContain("contains-task-list");
  expect(lists[0].className).toContain("list-disc");
  expect(lists[0].className).toContain("ps-6");
  expect(region().querySelector("li.task-list-item")!.className).toContain("my-1");
});

it("words the footnotes in the app's language and follows them within the document", async () => {
  // jsdom does not scroll, and has no `scrollIntoView`.
  const scrolled = vi.fn();
  Element.prototype.scrollIntoView = scrolled;
  onTestFinished(() => void delete (Element.prototype as Partial<Element>).scrollIntoView);
  await open(markdownFile("docs/notes.md", "A claim[^1].\n\n[^1]: The proof.\n"));
  const heading = region().querySelector("h2")!;
  expect(heading.textContent).toBe("Footnotes");
  expect(heading.className).toBe("sr-only");
  const back = region().querySelector<HTMLAnchorElement>("a[data-footnote-backref]")!;
  expect(back.getAttribute("aria-label")).toBe("Back to reference 1");
  const reference = region().querySelector<HTMLAnchorElement>("a[data-footnote-ref]")!;
  const click = new MouseEvent("click", { bubbles: true, cancelable: true });
  act(() => void reference.dispatchEvent(click));
  expect(click.defaultPrevented).toBe(true);
  expect(document.activeElement?.id).toBe("user-content-fn-1");
  expect(scrolled).toHaveBeenCalled();
});

it("keeps the reference link in the Tab order after following a footnote back", async () => {
  Element.prototype.scrollIntoView = vi.fn();
  onTestFinished(() => void delete (Element.prototype as Partial<Element>).scrollIntoView);
  await open(markdownFile("docs/notes.md", "A claim[^1].\n\n[^1]: The proof.\n"));
  const back = region().querySelector<HTMLAnchorElement>("a[data-footnote-backref]")!;
  act(() => void back.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true })));
  const reference = document.activeElement as HTMLElement;
  expect(reference.id).toBe("user-content-fnref-1");
  expect(reference.tabIndex).toBe(0);
});

it("follows a footnote whose label is not ASCII", async () => {
  Element.prototype.scrollIntoView = vi.fn();
  onTestFinished(() => void delete (Element.prototype as Partial<Element>).scrollIntoView);
  await open(markdownFile("docs/notes.md", "声明[^注1]。\n\n[^注1]: 来源。\n"));
  const reference = region().querySelector<HTMLAnchorElement>("a[data-footnote-ref]")!;
  act(() => void reference.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true })));
  expect(document.activeElement?.tagName).toBe("LI");
  expect(document.activeElement?.id).toBe(reference.getAttribute("href")!.slice(1));
});

it("aligns table cells as their column says", async () => {
  await open(markdownFile("docs/table.md", "| a | b | c |\n| :-- | :-: | --: |\n| 1 | 2 | 3 |\n"));
  const aligned = [...region().querySelectorAll<HTMLElement>("th, td")].map((cell) => cell.style.textAlign);
  expect(aligned).toEqual(["left", "center", "right", "left", "center", "right"]);
  expect(region().querySelector("td")!.className).toContain("text-start");
});

it("highlights only the fenced blocks within the document's budget, and the rest as plain text", async () => {
  const fences = RENDER_BUDGETS.documentFences + 1;
  await open(markdownFile("docs/fences.md", Array.from({ length: fences }, (_, i) => `\`\`\`ts\nconst a${i} = 1;\n\`\`\``).join("\n\n")));
  expect(region().querySelectorAll("[data-renderer]")).toHaveLength(RENDER_BUDGETS.documentFences);
  expect(region().querySelectorAll("pre:not([data-renderer])")).toHaveLength(1);
  expect(region().querySelector("pre:not([data-renderer])")!.textContent).toBe(`const a${fences - 1} = 1;`);
});

it("counts only the highlighted fences against the length budget", async () => {
  const huge = "x".repeat(RENDER_BUDGETS.documentFenceChars + 1);
  await open(markdownFile("docs/big.md", `\`\`\`ts\n${huge}\n\`\`\`\n\n\`\`\`ts\nconst small = 1;\n\`\`\`\n`));
  expect(region().querySelectorAll("[data-renderer]")).toHaveLength(1);
  expect(region().querySelector("[data-renderer]")!.textContent).toBe("const small = 1;");
});

it("highlights a fence of exactly the length budget, and counts only highlighted fences against the count budget", async () => {
  const exact = "x".repeat(RENDER_BUDGETS.documentFenceChars);
  await open(markdownFile("docs/exact.md", `\`\`\`ts\n${exact}\n\`\`\`\n`));
  expect(region().querySelectorAll("[data-renderer]")).toHaveLength(1);
});

it("does not let a plain fence use up a place in the count budget", async () => {
  const huge = "x".repeat(RENDER_BUDGETS.documentFenceChars + 1);
  const small = (i: number) => `\`\`\`ts\nconst a${i} = 1;\n\`\`\``;
  const fences = [`\`\`\`ts\n${huge}\n\`\`\``, ...Array.from({ length: RENDER_BUDGETS.documentFences }, (_, i) => small(i))];
  await open(markdownFile("docs/count.md", fences.join("\n\n")));
  expect(region().querySelectorAll("[data-renderer]")).toHaveLength(RENDER_BUDGETS.documentFences);
});

it("shows an empty fenced block once, as plain text", async () => {
  await open(markdownFile("docs/empty.md", "```ts\n```\n"));
  expect(region().querySelectorAll("pre")).toHaveLength(1);
  expect(region().querySelector("[data-renderer]")).toBeNull();
});

it("shows a Markdown file past the highlighting budget as plain source, with no document view", async () => {
  await open(markdownFile("docs/huge.md", "# Heading\n".repeat(RENDER_BUDGETS.highlightLines + 1)));
  expect(viewToggle()).toBeNull();
  expect(document.querySelector("h1")).toBeNull();
  expect(region().tagName).toBe("PRE");
});

it("accepts only web and mail addresses as links", () => {
  expect(openableAddress("https://example.com")).toBe("https://example.com/");
  expect(openableAddress("mailto:a@example.com")).toBe("mailto:a@example.com");
  for (const href of ["javascript:alert(1)", "./a.md", "#top", "/a", "file:///etc/passwd", "", undefined]) {
    expect(openableAddress(href)).toBeUndefined();
  }
});

it("gives a loose Arabic list and an Arabic quote a right-to-left direction of their own", async () => {
  await open(markdownFile("a.md", "- مرحبا\n\n- عالم\n\n> اقتباس\n"));
  expect(region().querySelector("ul")?.getAttribute("dir")).toBe("rtl");
  expect(region().querySelector("blockquote")?.getAttribute("dir")).toBe("rtl");
  expect(region().querySelector("[data-markdown-document]")?.getAttribute("dir")).toBe("rtl");
  expect(region().querySelector("[dir=auto]")).toBeNull();
});

it("keeps a paragraph's own direction inside a list that reads the other way", async () => {
  await open(markdownFile("a.md", "- مرحبا\n\n- hello\n"));
  expect([...region().querySelectorAll("p")].map((p) => p.getAttribute("dir"))).toEqual(["rtl", "ltr"]);
});

it("takes the app's direction for the frame of a document with no strong character", async () => {
  withLanguage("ar");
  await open(markdownFile("a.md", "1. 2\n"));
  expect(region().querySelector("[data-markdown-document]")?.getAttribute("dir")).toBe("rtl");
  expect(region().querySelector("ol")?.hasAttribute("dir")).toBe(false);
});

it("gives headings distinct ids and does not take one a footnote already has", async () => {
  await open(markdownFile("docs/notes.md", "# Title\n\n# Title\n\n# user-content-fn-1\n\nA claim[^1].\n\n[^1]: The proof.\n"));
  expect([...region().querySelectorAll("h1")].map((heading) => heading.id)).toEqual(["title", "title-1", "user-content-fn-1-1"]);
  expect(region().querySelector("li")?.id).toBe("user-content-fn-1");
  expect(region().querySelector("h2.sr-only")?.id).toBe("footnote-label");
});

it("names a heading with no slug section, and keeps the next one on that name", async () => {
  await open(markdownFile("docs/notes.md", "# ???\n\n# ???\n\n# section\n"));
  expect([...region().querySelectorAll("h1")].map((heading) => heading.id)).toEqual(["section", "section-1", "section-2"]);
});

it("scrolls to a heading the link names, and follows one that opens another file", async () => {
  const scrolled = vi.fn();
  Element.prototype.scrollIntoView = scrolled;
  onTestFinished(() => void delete (Element.prototype as Partial<Element>).scrollIntoView);
  const opened: string[] = [];
  await open(markdownFile("docs/notes.md", "# Title\n\nSee [below](#title) and [other](./other.md#there)."), undefined, {
    location: {
      anchor: "title",
      onOpenFile: (path, fragment) => opened.push(`${path}${fragment ? `#${fragment}` : ""}`),
    },
  });
  expect(document.activeElement?.id).toBe("title");
  expect(scrolled).toHaveBeenCalled();
  const links = [...region().querySelectorAll("a")];
  expect(links.map((link) => link.textContent)).toEqual(["below", "other"]);
  act(() => void links[1]?.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true })));
  expect(opened).toEqual(["docs/other.md#there"]);
  act(() => void links[0]?.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true })));
  expect(document.activeElement?.id).toBe("title");
});

it("does not open a path the listing says is a directory", async () => {
  await open(markdownFile("docs/notes.md", "[folder](./images)"), undefined, {
    location: { onOpenFile: () => {}, isDirectory: (path) => path === "docs/images" },
  });
  expect(region().querySelector("a")).toBeNull();
  expect(region().textContent).toContain("folder");
});

it("keeps a formula left to right inside a right-to-left paragraph, and a math fence as code", async () => {
  await open(markdownFile("docs/math.md", "مرحبا $E=mc^2$ بالعالم\n\n```math\nx\n```\n"));
  for (let i = 0; i < 30 && region().querySelector(".katex") === null; i += 1) {
    await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
  }
  const formula = region().querySelector(".katex")?.parentElement;
  expect(formula?.getAttribute("dir")).toBe("ltr");
  expect(formula?.className).toContain("[unicode-bidi:isolate]");
  expect(region().querySelector("p")?.getAttribute("dir")).toBe("rtl");
  expect(region().querySelector("[data-renderer]")?.getAttribute("data-language")).toBe("math");
  expect(region().querySelectorAll(".katex")).toHaveLength(1);
});

it("leaves a price as text and draws a formula, without a link from an untrusted command", async () => {
  await open(markdownFile("docs/math.md", "It costs $20 and $30, and $100/2$ of it, and $E=mc^2$.\n\n$$\\href{https://example.com}{x}$$\n"));
  expect(region().textContent).toContain("$20 and $30");
  for (let i = 0; i < 30 && region().querySelectorAll(".katex").length < 2; i += 1) {
    await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
  }
  expect(region().querySelectorAll(".katex").length).toBeGreaterThanOrEqual(2);
  expect(region().querySelector("math")).not.toBeNull();
  expect(region().querySelector(".katex a, .katex img")).toBeNull();
});

it("draws a mermaid fence as a diagram and does not count it as highlighted code", async () => {
  const fences = Array.from({ length: RENDER_BUDGETS.documentFences }, (_, i) => `\`\`\`ts\nconst a${i} = 1;\n\`\`\``);
  await open(markdownFile("docs/diagram.md", `\`\`\`mermaid\ngraph TD\nA-->B\n\`\`\`\n\n${fences.join("\n\n")}`));
  expect(region().querySelector("[data-diagram]")?.textContent).toContain("A-->B");
  expect(region().querySelectorAll("[data-renderer]")).toHaveLength(RENDER_BUDGETS.documentFences);
});

it("shows a diagram past the count as its source", async () => {
  const fences = Array.from({ length: RENDER_BUDGETS.documentDiagrams + 1 }, (_, i) => `\`\`\`mermaid\ngraph TD\nA${i}-->B\n\`\`\``);
  await open(markdownFile("docs/diagrams.md", fences.join("\n\n")));
  const diagrams = [...region().querySelectorAll("[data-diagram]")];
  expect(diagrams.filter((diagram) => diagram.getAttribute("data-over-budget") === "false")).toHaveLength(RENDER_BUDGETS.documentDiagrams);
  expect(diagrams.at(-1)?.getAttribute("data-over-budget")).toBe("true");
});

it("shows a project image and does not fetch one that leaves the project", async () => {
  const requested: string[] = [];
  const png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
  await open(markdownFile("docs/a.md", "![chart](pic.png)\n\n![secret](../../secret.png)\n\n![web](https://example.com/a.png)\n"), undefined, {
    location: { project: "p" },
    request: (async (body) => {
      if (body.type !== "read_project_file") throw new Error(body.type);
      requested.push(body.path);
      if (body.slot !== undefined) throw new Error("slot");
      return {
        type: "project_file" as const,
        project: body.project,
        worktree: null,
        path: body.path,
        source: { kind: "live" as const, root_id: "t", version: "1" },
        file: { size: 8, kind: "binary" as const, media_type: "image/png", text: null, data: png },
      };
    }) as Daemon["request"],
  });
  for (let i = 0; i < 20 && region().querySelector("img") === null; i += 1) {
    await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
  }
  const image = region().querySelector("img");
  expect(image?.getAttribute("alt")).toBe("chart");
  expect(image?.getAttribute("src")?.startsWith("data:image/png")).toBe(true);
  expect(requested).toEqual(["docs/pic.png"]);
  expect(region().querySelectorAll("img")).toHaveLength(1);
  expect(region().textContent).toContain("secret");
  expect(region().textContent).toContain("web");
});

it("says when a project image cannot be shown", async () => {
  await open(markdownFile("docs/a.md", "![chart](missing.png)"), undefined, {
    location: { project: "p" },
    request: (async () => {
      throw new Error("missing");
    }) as Daemon["request"],
  });
  for (let i = 0; i < 20 && !region().textContent?.includes("could not be shown"); i += 1) {
    await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
  }
  expect(region().querySelector("img")).toBeNull();
  expect(region().textContent).toContain("chart");
  expect(region().textContent).toContain("This image could not be shown.");
});
