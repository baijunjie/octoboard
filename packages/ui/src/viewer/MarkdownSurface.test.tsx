// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import type { PlatformAdapter } from "../platform";
import { PlatformProvider } from "../platform/react";
import { RENDER_BUDGETS } from "./budgets";
import { PREFERENCE_KEYS } from "../preferenceKeys";
import { ThemeProvider } from "../theme";
import type { ViewerSubject } from "./content";
import { FileViewer } from "./FileViewer";
import { markdownView } from "./markdownView";
import { openableAddress } from "./MarkdownDocument";

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

class StubResizeObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
globalThis.ResizeObserver ??= StubResizeObserver as unknown as typeof ResizeObserver;
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
async function open(subject: ViewerSubject, links?: PlatformAdapter["links"]): Promise<HTMLElement> {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() =>
    root.render(
      <PlatformProvider value={{ kind: "browser", links }}>
        <ThemeProvider>
          <FileViewer subject={subject} onClose={() => {}} />
        </ThemeProvider>
      </PlatformProvider>,
    ),
  );
  for (let i = 0; i < 5; i += 1) await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
  return document.querySelector<HTMLElement>("[role=dialog]")!;
}

const region = () => document.querySelector<HTMLElement>("[role=region]")!;
const choice = (label: string) => [...document.querySelectorAll("button")].find((b) => b.textContent === label)!;

it("opens a Markdown file as a document, with the choice in the header's controls", async () => {
  const dialog = await open(markdownFile());
  expect(region().querySelector("h1")?.textContent).toBe("Title");
  expect(region().querySelectorAll("li")).toHaveLength(2);
  expect(region().querySelector("table th")?.textContent).toBe("Name");
  expect(region().getAttribute("aria-label")).toBe("Contents of README.md");
  expect(region().tabIndex).toBe(0);
  expect(choice("Document").closest("[data-viewer-description]")).not.toBeNull();
  expect(choice("Document").getAttribute("aria-pressed") ?? choice("Document").getAttribute("aria-checked")).toBe("true");
  // The document needs no wrap choice: its code is always wrapped.
  expect([...dialog.querySelectorAll("button")].some((b) => b.textContent === "Wrap lines")).toBe(false);
});

it("highlights a fenced block through the code renderer, by the fence's grammar", async () => {
  await open(markdownFile());
  const code = region().querySelector("[data-renderer]")!;
  expect(code.getAttribute("data-language")).toBe("ts");
  expect(code.textContent).toBe("const answer = 42;");
});

it("switches to the source and back, and stores the choice", async () => {
  await open(markdownFile());
  act(() => choice("Source").click());
  expect(document.querySelector("h1")).toBeNull();
  expect(region().textContent).toContain("# Title");
  expect(localStorage.getItem(PREFERENCE_KEYS.markdownView)).toBe("source");
  expect(markdownView.get()).toBe("source");
  act(() => choice("Document").click());
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
  expect(choice("Document")).toBeUndefined();
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
  expect(choice("Document")).toBeUndefined();
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
  expect(choice("Source")).toBeDefined();
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
  expect(choice("Document")).toBeUndefined();
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
  expect(choice("Document")).toBeUndefined();
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
