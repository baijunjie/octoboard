// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { t } from "../i18n/language";
import { ViewerHost } from "../viewer/viewerTestSupport";
import { reloadPage } from "../platform/reloadPage";
import { LazyProjectBrowser } from "./LazyProjectBrowser";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The chunk is gone: what a page the daemon served before it updated asks for.
vi.mock("./ProjectBrowser", () => {
  throw new Error("Failed to fetch dynamically imported module");
});
vi.mock("../platform/reloadPage", () => ({ reloadPage: vi.fn() }));

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

async function mountFailed(): Promise<HTMLElement> {
  const container = document.body.appendChild(document.createElement("div"));
  root = createRoot(container);
  await act(async () => {
    root!.render(
      <ViewerHost>
        <LazyProjectBrowser {...({ project: { name: "Project" } } as unknown as Parameters<typeof LazyProjectBrowser>[0])} focusTerminal={() => {}} />
      </ViewerHost>,
    );
  });
  return container;
}

const button = (container: HTMLElement) => container.querySelector<HTMLButtonElement>("button")!;

it("offers Try again for a failed load outside a page the daemon serves", async () => {
  const container = await mountFailed();
  expect(container.textContent).toContain(t("browser.loadFailed"));
  expect(button(container).textContent).toBe(t("error.tryAgain"));
  expect(container.textContent).not.toContain(t("browser.loadFailed.reload"));
});

it("offers Reload, which reloads the page, in a page the daemon serves", async () => {
  vi.stubEnv("DEV", false);
  window.history.replaceState({}, "", "/?key=abc123");
  const container = await mountFailed();
  expect(container.textContent).toContain(t("browser.loadFailed"));
  expect(container.textContent).toContain(t("browser.loadFailed.reload"));
  expect(button(container).textContent).toBe(t("error.reload"));
  act(() => button(container).click());
  expect(reloadPage).toHaveBeenCalledTimes(1);
});
