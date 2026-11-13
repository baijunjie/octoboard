// @vitest-environment jsdom
import { act } from "react";
import { beforeEach, expect, it } from "vitest";

import { setLanguage, t } from "../i18n/language";
import { mountHost } from "../test/mountHost";
import { KeyRefusedScreen } from "./KeyRefusedScreen";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

beforeEach(() => act(() => setLanguage("en")));

it("says the key changed, the way to a working address with the command as code, and offers only Report issue", async () => {
  const container = mountHost(<KeyRefusedScreen />);
  await act(async () => {});
  const panel = container.querySelector('[data-slot="empty-state"]')!;
  expect(container.querySelector("h1")?.textContent).toBe("This address no longer opens the daemon");
  const lines = [...panel.querySelectorAll("p")].map((line) => line.textContent);
  expect(lines.slice(0, 2)).toEqual(["Its key has changed.", "On the server, run octoboardd address and open the address it prints."]);
  expect(container.querySelector("code")?.textContent).toBe("octoboardd address");
  // Reloading meets the same refusal and nothing is retried, so there is no recovery button; Report issue stands alone.
  const row = panel.lastElementChild!;
  expect([...row.children].map((child) => child.textContent)).toEqual([t("error.issue.report")]);
  const report = new URL((row.firstElementChild as HTMLAnchorElement).href);
  expect(report.searchParams.get("title")).toBe("This address no longer opens the daemon");
  expect(report.searchParams.get("body")).toContain("The daemon refused the key this page was opened with.");
  expect(report.searchParams.get("body")).toContain("Kind: known");
});

it("says the address does not open the daemon and carries no key, instead of that its key changed, on a page opened without one", async () => {
  const container = mountHost(<KeyRefusedScreen keyless />);
  await act(async () => {});
  expect(container.querySelector("h1")?.textContent).toBe("This address does not open the daemon");
  const lines = [...container.querySelectorAll('[data-slot="empty-state"] p')].map((line) => line.textContent);
  expect(lines.slice(0, 2)).toEqual(["It carries no key.", "On the server, run octoboardd address and open the address it prints."]);
  expect(container.textContent).not.toContain("key has changed");
  const report = new URL((container.querySelector('[data-slot="empty-state"]')!.lastElementChild!.firstElementChild as HTMLAnchorElement).href);
  expect(report.searchParams.get("title")).toBe("This address does not open the daemon");
  expect(report.searchParams.get("body")).toContain("The page was opened from the daemon's address with no key.");
});
