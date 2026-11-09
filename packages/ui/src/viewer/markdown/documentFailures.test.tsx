// @vitest-environment jsdom
import katex from "katex";
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { CodedError } from "../../errors/codedError";
import { errorAlerts } from "../../components/alerts/errorAlerts";
import { press, tap } from "../../components/hint/tooltipTestSupport";
import { DocumentImage } from "./DocumentImage";
import { FormulaView } from "./FormulaView";
import { MarkdownDocument } from "./MarkdownDocument";
import { LAZY_IMPORT_TIMEOUT_MS } from "../../test/lazyImport";
import { markNames, ViewerHost } from "../viewerTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const failureShown = () => document.querySelector("[data-status-mark]") !== null || document.querySelector('[role="img"]') !== null;

async function mount(content: React.ReactNode, ready = failureShown): Promise<void> {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    vi.restoreAllMocks();
  });
  act(() => root.render(content));
  // The renderer and its stylesheet load on first use, which takes seconds on a loaded machine.
  await vi.waitFor(() => expect(ready()).toBe(true), { timeout: LAZY_IMPORT_TIMEOUT_MS });
}

/** A failure of what the document shows is on the screen only, so no error alert dialog is queued. */
const queued = () => errorAlerts.getSnapshot();

it.each([false, true])("says a KaTeX parse error where the formula stands, with no error alert dialog, inline %s", async (inline) => {
  await mount(<p id="paragraph"><FormulaView source="\\frac{" inline={inline} display={!inline} overBudget={false} /></p>);
  const paragraph = document.getElementById("paragraph")!;
  const sentence = "This formula could not be drawn, so its source is shown.";
  expect(markNames(paragraph)).toEqual([sentence]);
  expect(paragraph.querySelector("button")?.getAttribute("aria-label")).toBe(sentence);
  expect(document.body.textContent).toContain("\\frac{");
  expect(queued()).toEqual([]);
});

it("says an arbitrary formula renderer exception the same way", async () => {
  vi.spyOn(katex, "renderToString").mockImplementationOnce(() => { throw new TypeError("Renderer invariant failed"); });
  await mount(<FormulaView source="x" inline={false} display overBudget={false} />);
  expect(markNames()).toEqual(["This formula could not be drawn, so its source is shown."]);
  expect(queued()).toEqual([]);
});

it.each([false, true])("notes an over-budget formula with a mark beside its source that opens a tooltip, inline %s", async (inline) => {
  await mount(<div id="host">{inline ? <p id="paragraph"><FormulaView source="x" inline display={false} overBudget /></p> : <FormulaView source="x" inline={false} display overBudget />}</div>);
  const sentence = "Shown as source because the document is large.";
  expect(markNames()).toEqual([sentence]);
  const mark = document.querySelector("[data-status-mark]")!;
  // A note is a button like every mark, which is phrasing content, valid in a paragraph.
  expect(mark.tagName).toBe("BUTTON");
  expect(document.querySelector("[role=status]")?.textContent ?? "").toBe("");
});

it.each(["file_not_found", "permission_denied", "unexpected_error"])("marks a project image whose read fails with %s, with no error alert dialog", async (code) => {
  const error = new CodedError("Image read failed", code, { path: "image.png" });
  await mount(
    <ViewerHost request={async () => { throw error; }}>
      <p id="paragraph"><DaemonImageFixture /></p>
    </ViewerHost>,
  );
  expect(document.querySelector("#paragraph button")?.getAttribute("aria-label")).toBe("This image could not be shown.");
  expect(queued()).toEqual([]);
});

it("marks a document image whose data cannot be decoded, with no error alert dialog", async () => {
  await mount(
    <ViewerHost request={async () => ({ type: "project_file", id: null, project: "project", worktree: null, source: { kind: "live", root_id: "root", version: "version" }, path: "image.png", file: { kind: "binary", text: null, size: 3, media_type: "image/png", data: "AAEC" } })}>
      <p id="paragraph"><DaemonImageFixture /></p>
    </ViewerHost>,
    // The image is only marked once the browser fails to decode it, which this test makes happen.
    () => document.querySelector("img") !== null,
  );
  act(() => document.querySelector("img")!.dispatchEvent(new Event("error")));
  expect(document.querySelector("#paragraph button")?.getAttribute("aria-label")).toBe("This image could not be shown.");
  expect(queued()).toEqual([]);
});

function DaemonImageFixture(): React.ReactElement {
  return <DocumentImage src="image.png" documentPath="README.md" project="project" overBudget={false} />;
}

it("stands a failed image in the paragraph as a named glyph, with no sentence of error text", async () => {
  await mount(
    <ViewerHost request={async () => ({ type: "project_file", id: null, project: "project", worktree: null, source: { kind: "live", root_id: "root", version: "version" }, path: "image.png", file: { kind: "text", text: "not an image", size: 12, media_type: null, data: null } })}>
      <p id="paragraph"><DocumentImage src="image.png" alt="Diagram" documentPath="README.md" project="project" overBudget={false} /></p>
    </ViewerHost>,
  );
  const paragraph = document.getElementById("paragraph")!;
  const mark = paragraph.querySelector("button")!;
  expect(mark.getAttribute("aria-label")).toBe("This image could not be shown.");
  expect(mark.textContent).toBe("");
  expect(paragraph.textContent).toBe("Diagram");
  expect(paragraph.querySelector("div")).toBeNull();
  expect(queued()).toEqual([]);
});

async function mountLinkedFailedImage() {
  const opened: string[] = [];
  await mount(
    <ViewerHost request={async () => ({ type: "project_file", id: null, project: "project", worktree: null, source: { kind: "live", root_id: "root", version: "version" }, path: "image.png", file: { kind: "text", text: "not an image", size: 12, media_type: null, data: null } })}>
      <MarkdownDocument
        text="[![Diagram](image.png)](other.md)"
        path="README.md"
        project="project"
        theme="light"
        renderCode={() => null}
        onOpenFile={(path) => void opened.push(path)}
        isDirectory={() => false}
      />
    </ViewerHost>,
  );
  await vi.waitFor(() => expect(document.querySelector("button")).not.toBeNull(), { timeout: LAZY_IMPORT_TIMEOUT_MS });
  return { opened, link: document.querySelector("a")!, mark: document.querySelector<HTMLButtonElement>("button")! };
}

it("draws a failed linked image's mark right after the link, never inside it", async () => {
  const { link, mark } = await mountLinkedFailedImage();
  expect(mark).not.toBeNull();
  expect(link.querySelector("button")).toBeNull();
  expect(link.contains(mark)).toBe(false);
  expect(link.nextElementSibling?.contains(mark)).toBe(true);
});

it("opens a failed linked image's popover on a touch press without activating the link", async () => {
  const { opened, mark } = await mountLinkedFailedImage();
  expect(mark).not.toBeNull();
  press(mark, "touch");
  expect(document.querySelector('[role="dialog"]')?.textContent).toBe("This image could not be shown.");
  expect(opened).toEqual([]);
});

it("does nothing on a mouse click on a failed linked image's mark, and does not activate the link", async () => {
  const { opened, mark } = await mountLinkedFailedImage();
  press(mark, "mouse");
  expect(document.querySelector('[role="dialog"]')).toBeNull();
  expect(opened).toEqual([]);
});

it("opens a failed linked image's popover on Enter without activating the link", async () => {
  const { opened, mark } = await mountLinkedFailedImage();
  act(() => mark.focus());
  tap(mark, "Enter");
  expect(document.querySelector('[role="dialog"]')).not.toBeNull();
  expect(opened).toEqual([]);
});

it("still activates the link on a press elsewhere on it", async () => {
  const { opened, link } = await mountLinkedFailedImage();
  const description = [...link.querySelectorAll("span")].find((span) => span.textContent === "Diagram")!;
  act(() => void description.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, button: 0 })));
  expect(opened).toEqual(["other.md"]);
});
