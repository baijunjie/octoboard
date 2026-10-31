// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import { errorAlerts } from "./errorAlerts";
import { t } from "./i18n/language";
import type { ExitCapability, PlatformAdapter } from "./platform";
import { PlatformProvider } from "./platform/react";
import { DaemonFailedScreen, NoAddressMessage, StartupScreen } from "./StartupScreen";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => { act(() => { while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge(); }); });

const DIAGNOSTIC = "Error: listen EADDRINUSE: address already in use 127.0.0.1:4580";

function mount(screen: React.ReactElement = <DaemonFailedScreen error={DIAGNOSTIC} />, platform: PlatformAdapter = { kind: "browser" }): HTMLElement {
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
  // What the daemon is comes first, then what to do; the remedy stands outside the announcement.
  const lines = [...panel.querySelectorAll("p")].map((line) => line.textContent);
  expect(lines.slice(0, 2)).toEqual([t("startup.daemonFailed.description"), t("startup.daemonFailed.remedy")]);
  expect(panel.querySelector(':scope > [data-slot="surface"] pre')?.textContent).toBe(DIAGNOSTIC);
  // Report issue stands alone in the row, a link built from the daemon's own text under a neutral English title.
  const row = panel.lastElementChild!;
  expect([...row.children].map((child) => child.textContent)).toEqual([t("error.issue.report")]);
  const report = new URL((row.firstElementChild as HTMLAnchorElement).href);
  expect(report.searchParams.get("title")).toBe("The daemon failed to start");
  expect(report.searchParams.get("body")).toContain("EADDRINUSE");
  expect(report.searchParams.get("body")).toContain("Kind: known");
  expect(report.searchParams.get("body")).not.toContain("Kind: unexpected");
  // The sentence about reporting heads the surface, ahead of the raw text, and does not call the failure a bug.
  const [guidance, raw] = [...panel.querySelector(':scope > [data-slot="surface"]')!.children];
  expect(guidance.tagName).toBe("P");
  expect(guidance.textContent).toMatch(/^If the steps above do not resolve it, use Report issue to file an Issue/);
  expect(guidance.textContent).not.toContain("bug");
  expect(raw.querySelector("pre")?.textContent).toBe(DIAGNOSTIC);
  expect(container.querySelector("button")).toBeNull();
});

it.each([
  ["the daemon-failed page", <DaemonFailedScreen error={DIAGNOSTIC} />],
  ["the no-address screen", <StartupScreen message={<NoAddressMessage />} />],
])("has no Quit on %s and registers nothing with the shell, which quits natively", (_name, screen) => {
  const register = vi.fn(() => () => {});
  const exit: ExitCapability = { register, heartbeat: async () => {}, confirmQuit: async () => {} };
  const container = mount(screen, { kind: "tauri", exit });
  expect([...container.querySelectorAll("button")].map((button) => button.textContent)).not.toContain(t("common.quit"));
  expect(register).not.toHaveBeenCalled();
});

it("offers Report issue on the no-address screen, after the instructions and a sentence that does not call it a bug", () => {
  const container = mount(<StartupScreen message={<NoAddressMessage />} />);
  const link = container.querySelector<HTMLAnchorElement>("main a[data-external-link]")!;
  expect(link.textContent).toBe(t("error.issue.report"));
  const report = new URL(link.href);
  expect(report.searchParams.get("title")).toBe("No daemon address or key");
  expect(report.searchParams.get("body")).toContain("The UI was given no daemon address or key to connect with.");
  expect(report.searchParams.get("body")).not.toContain(t("startup.noAddress.description"));
  expect(report.searchParams.get("body")).toContain("Kind: known");
  expect(report.searchParams.get("body")).not.toContain("Kind: unexpected");
  const guidance = link.parentElement!.previousElementSibling!;
  expect(guidance.textContent).toMatch(/^If the steps above do not resolve it, use Report issue to file an Issue/);
  expect(guidance.textContent).not.toContain("bug");
  expect(guidance.previousElementSibling?.tagName).toBe("UL");
});

it("words the report guidance for a platform that cannot open links without calling the failure a bug", () => {
  const container = mount(<DaemonFailedScreen error={DIAGNOSTIC} />, { kind: "tauri" });
  const panel = container.querySelector('[data-slot="empty-state"]')!;
  expect(panel.querySelector("a[data-external-link]")).toBeNull();
  const guidance = panel.querySelector(':scope > [data-slot="surface"] p')!;
  expect(guidance.textContent).toMatch(/^If the steps above do not resolve it, open /);
  expect(guidance.textContent).not.toContain("bug");
});

it("keeps no danger tone on the page outside a glyph or an Alert", () => {
  for (const container of [mount(), mount(<NoAddressMessage />)]) {
    const tinted = [...container.querySelectorAll('[data-color="danger"]')].filter((element) => !element.closest('[role="alert"], [data-slot="alert"], .alert'));
    expect(tinted).toEqual([]);
  }
});

it("opens no error dialog over the page and announces the failure from the page itself, after the first paint", async () => {
  const container = mount();
  expect(errorAlerts.getSnapshot()).toEqual([]);
  expect(container.querySelector('[role="alert"]')).toBeNull();
  await act(() => new Promise<void>((resolve) => setTimeout(resolve, 0)));
  const announced = container.querySelector('[role="alert"]')!;
  expect(announced.querySelector("h1")?.textContent).toBe(t("startup.daemonFailed.title"));
  expect(announced.textContent).toContain(t("startup.daemonFailed.description"));
  expect(announced.textContent).not.toContain(DIAGNOSTIC);
});
