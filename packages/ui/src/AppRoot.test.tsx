// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { AppRoot } from "./AppRoot";
import { testDaemon } from "./test/testDaemon";
import { BROWSER_HOST } from "./test/hosts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

beforeEach(() => {
  vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: false, addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
  return () => { vi.unstubAllGlobals(); };
});

async function mount(element: React.ReactElement): Promise<HTMLElement> {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  await act(async () => root.render(element));
  return container;
}

it("shows its children under the providers", async () => {
  const container = await mount(
    <AppRoot platform={{ kind: "browser", host: BROWSER_HOST }} daemon={undefined} nativeSync={false}>
      <p>screen</p>
    </AppRoot>,
  );
  expect(container.textContent).toBe("screen");
});

it("gives the crash page the daemon, which is outside the error boundary", async () => {
  const daemon = testDaemon({ state: {
    homeDir: "/Users/alice",
    projects: new Map([["p", { name: "Zeta Project", path: "/Users/alice/zeta" }]]) as never,
  } });
  function Crashing(): React.ReactElement {
    throw new Error("Cannot open Zeta Project at /Users/alice/zeta/a.ts");
  }
  const container = await mount(
    <AppRoot platform={{ kind: "browser", host: BROWSER_HOST }} daemon={daemon} nativeSync={false}>
      <Crashing />
    </AppRoot>,
  );
  const prefilled = new URL(container.querySelector<HTMLAnchorElement>("a[data-external-link]")!.href).searchParams;
  expect(`${prefilled.get("title")}${prefilled.get("body")}`).not.toMatch(/Zeta|alice/);
});
