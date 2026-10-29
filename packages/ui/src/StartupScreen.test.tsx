// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, onTestFinished } from "vitest";

import { errorAlerts } from "./errorAlerts";
import { t } from "./i18n/language";
import type { PlatformAdapter } from "./platform";
import { PlatformProvider } from "./platform/react";
import { DaemonFailedScreen, NoAddressMessage } from "./StartupScreen";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => { act(() => { while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge(); }); });

const DIAGNOSTIC = "Error: listen EADDRINUSE: address already in use 127.0.0.1:4580";

function mount(screen: React.ReactElement = <DaemonFailedScreen error={DIAGNOSTIC} />): HTMLElement {
  const platform: PlatformAdapter = { kind: "browser" };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(<PlatformProvider value={platform}>{screen}</PlatformProvider>));
  return container;
}

it("says the daemon's failure to start in the application's words and keeps its own message on a surface under them", () => {
  const container = mount();
  const panel = container.querySelector('[data-slot="empty-state"]')!;
  expect(container.querySelector("h1")?.textContent).toBe(t("startup.daemonFailed.title"));
  expect(container.textContent).toContain(t("startup.daemonFailed.remedy"));
  expect(panel.querySelector(':scope > [data-slot="surface"] pre')?.textContent).toBe(DIAGNOSTIC);
  // A plain browser has no quit flow, so the page has no button at all.
  expect(container.querySelector("button")).toBeNull();
});

it("keeps no danger tone on the page outside a glyph or an Alert", () => {
  for (const container of [mount(), mount(<NoAddressMessage />)]) {
    const tinted = [...container.querySelectorAll('[data-color="danger"]')].filter((element) => !element.closest('[role="alert"], [data-slot="alert"], .alert'));
    expect(tinted).toEqual([]);
  }
});
