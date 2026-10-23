// @vitest-environment jsdom
import katex from "katex";
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { DaemonRequestError } from "../daemon-client";
import { errorAlerts } from "../errorAlerts";
import { DocumentImage } from "./DocumentImage";
import { FormulaView } from "./FormulaView";
import { ViewerHost } from "./viewerTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const failureShown = () => document.body.textContent?.includes("could not be drawn") === true || document.querySelector('[role="img"]') !== null;

async function mount(content: React.ReactNode): Promise<void> {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
    vi.restoreAllMocks();
  });
  await act(async () => {
    root.render(content);
    // Imports and effects settle several microtask hops after the first paint.
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
  // The renderer and its stylesheet load on first use, which takes more than a few ticks.
  for (let i = 0; i < 100 && !failureShown(); i += 1) await act(async () => new Promise((resolve) => setTimeout(resolve, 5)));
}

/** A failure of what the document shows is on the screen only, so no error alert dialog is queued. */
const queued = () => errorAlerts.getSnapshot();

it.each([false, true])("says a KaTeX parse error where the formula stands, with no error alert dialog, inline %s", async (inline) => {
  await mount(<p id="paragraph"><FormulaView source="\\frac{" inline={inline} display={!inline} overBudget={false} /></p>);
  const paragraph = document.getElementById("paragraph")!;
  const sentence = "This formula could not be drawn, so its source is shown.";
  if (inline) expect(paragraph.querySelector('[role="img"]')?.getAttribute("aria-label")).toBe(sentence);
  else expect(document.body.textContent).toContain(sentence);
  expect(document.body.textContent).toContain("\\frac{");
  expect(queued()).toEqual([]);
});

it("says an arbitrary formula renderer exception the same way", async () => {
  vi.spyOn(katex, "renderToString").mockImplementationOnce(() => { throw new TypeError("Renderer invariant failed"); });
  await mount(<FormulaView source="x" inline={false} display overBudget={false} />);
  expect(document.body.textContent).toContain("This formula could not be drawn, so its source is shown.");
  expect(queued()).toEqual([]);
});

it.each(["file_not_found", "permission_denied", "unexpected_error"])("marks a project image whose read fails with %s, with no error alert dialog", async (code) => {
  const error = new DaemonRequestError("Image read failed", code, { path: "image.png" });
  await mount(
    <ViewerHost request={async () => { throw error; }}>
      <p id="paragraph"><DaemonImageFixture /></p>
    </ViewerHost>,
  );
  expect(document.querySelector('#paragraph [role="img"]')?.getAttribute("aria-label")).toBe("This image could not be shown.");
  expect(queued()).toEqual([]);
});

it("marks a document image that is not an image, with no error alert dialog", async () => {
  await mount(
    <ViewerHost request={async () => ({ type: "project_file", project: "project", worktree: null, source: { kind: "live", root_id: "root", version: "version" }, path: "image.png", file: { kind: "text", text: "not an image", size: 12, media_type: null, data: null } })}>
      <p id="paragraph"><DaemonImageFixture /></p>
    </ViewerHost>,
  );
  expect(document.querySelector('#paragraph [role="img"]')).not.toBeNull();
  expect(queued()).toEqual([]);
});

it("marks a document image whose data cannot be decoded, with no error alert dialog", async () => {
  await mount(
    <ViewerHost request={async () => ({ type: "project_file", project: "project", worktree: null, source: { kind: "live", root_id: "root", version: "version" }, path: "image.png", file: { kind: "binary", text: null, size: 3, media_type: "image/png", data: "AAEC" } })}>
      <p id="paragraph"><DaemonImageFixture /></p>
    </ViewerHost>,
  );
  act(() => document.querySelector("img")!.dispatchEvent(new Event("error")));
  expect(document.querySelector('#paragraph [role="img"]')?.getAttribute("aria-label")).toBe("This image could not be shown.");
  expect(queued()).toEqual([]);
});

function DaemonImageFixture(): React.ReactElement {
  return <DocumentImage src="image.png" documentPath="README.md" project="project" overBudget={false} />;
}

it("stands a failed image in the paragraph as a named glyph, with no sentence of error text", async () => {
  await mount(
    <ViewerHost request={async () => ({ type: "project_file", project: "project", worktree: null, source: { kind: "live", root_id: "root", version: "version" }, path: "image.png", file: { kind: "text", text: "not an image", size: 12, media_type: null, data: null } })}>
      <p id="paragraph"><DocumentImage src="image.png" alt="Diagram" documentPath="README.md" project="project" overBudget={false} /></p>
    </ViewerHost>,
  );
  const paragraph = document.getElementById("paragraph")!;
  const mark = paragraph.querySelector('[role="img"]')!;
  expect(mark.getAttribute("aria-label")).toBe("This image could not be shown.");
  expect(mark.textContent).toBe("");
  expect(paragraph.textContent).toBe("Diagram");
  expect(paragraph.querySelector("div")).toBeNull();
});
