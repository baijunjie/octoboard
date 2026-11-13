// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { t } from "../../i18n/language";
import { ViewerHost } from "../../viewer/viewerTestSupport";
import { LazyLinkedFileViewer } from "./LazyLinkedFileViewer";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The chunk is gone: what a page the daemon served before it updated asks for.
vi.mock("./LinkedFileViewer", () => {
  throw new Error("Failed to fetch dynamically imported module");
});

let root: Root | undefined;
beforeEach(() => {
  vi.spyOn(console, "warn").mockImplementation(() => {});
  vi.spyOn(console, "error").mockImplementation(() => {});
});
afterEach(() => {
  act(() => root?.unmount());
  root = undefined;
  vi.restoreAllMocks();
  vi.unstubAllEnvs();
  window.history.replaceState({}, "", "/");
});

/** The messages of the error alerts a failed load raises, and whether it closed the viewer. */
async function mountFailed(): Promise<{ alerts: string[]; closed: ReturnType<typeof vi.fn> }> {
  const alerts: string[] = [];
  const closed = vi.fn();
  const container = document.body.appendChild(document.createElement("div"));
  root = createRoot(container);
  await act(async () => {
    root!.render(
      <ViewerHost notifications={{ notice: () => {}, error: (message) => alerts.push(message) }}>
        <LazyLinkedFileViewer {...({ file: { project: "p", path: "a.ts" }, onClose: closed } as unknown as Parameters<typeof LazyLinkedFileViewer>[0])} />
      </ViewerHost>,
    );
  });
  return { alerts, closed };
}

it("says to open the file again when the load failed outside a page the daemon serves", async () => {
  const { alerts, closed } = await mountFailed();
  expect(alerts).toEqual([t("viewer.loadFailed")]);
  expect(closed).toHaveBeenCalled();
});

it("says to reload the page when the load failed in a page the daemon serves", async () => {
  vi.stubEnv("DEV", false);
  window.history.replaceState({}, "", "/?key=abc123");
  const { alerts, closed } = await mountFailed();
  expect(alerts).toEqual([t("viewer.loadFailed.reload")]);
  expect(closed).toHaveBeenCalled();
});
