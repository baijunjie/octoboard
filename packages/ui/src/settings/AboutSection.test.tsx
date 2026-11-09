// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { appConfig } from "../appConfig";
import { LanguageProvider } from "../i18n/react";
import type { PlatformAdapter } from "../platform";
import { mountHost } from "../test/mountHost";
import { UpdatesProvider } from "../updates/UpdatesProvider";
import { AboutSection } from "./AboutSection";
import { BROWSER_HOST } from "../test/hosts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The list is a chunk of its own, which the dialog's own test covers.
vi.mock("../about/ThirdPartyLicensesDialog", () => ({ ThirdPartyLicensesDialog: () => <p>Licenses dialog</p> }));

function mount(platform: PlatformAdapter) {
  return mountHost(
    <LanguageProvider>
      <UpdatesProvider firstCheckDelayMs={null}>
        <AboutSection />
      </UpdatesProvider>
    </LanguageProvider>,
    { platform },
  );
}

it("names the application and links to its website, repository and issue tracker from the shared config", () => {
  const container = mount({ kind: "browser", host: BROWSER_HOST });
  expect(container.querySelector("h3")?.textContent).toBe(appConfig.name);
  const hrefs = Array.from(container.querySelectorAll("a")).map((link) => link.getAttribute("href"));
  expect(hrefs).toContain(new URL(appConfig.websiteUrl).href);
  expect(hrefs).toContain(new URL(appConfig.repositoryUrl).href);
  expect(hrefs).toContain(new URL(appConfig.issueUrl).href);
});

it("states the license and the copyright line with the author from the config", () => {
  const text = mount({ kind: "browser", host: BROWSER_HOST }).textContent ?? "";
  expect(text).toContain(appConfig.license);
  expect(text).toContain(`© ${appConfig.copyrightYear} `);
  expect(text).toContain(appConfig.author.name);
});

it("places third-party licenses after the license and copyright last", () => {
  const text = (mount({ kind: "browser", host: BROWSER_HOST }).textContent ?? "").replace(/[\u2066-\u2069]/g, "");
  expect(text).toMatch(/License.*Third-party licenses.*Copyright/s);
  expect(text.endsWith(`Copyright© ${appConfig.copyrightYear} ${appConfig.author.name}`)).toBe(true);
});

it("has no version and no update controls where the platform cannot update itself", () => {
  const text = mount({ kind: "browser", host: BROWSER_HOST }).textContent ?? "";
  expect(text).not.toContain("Version ");
  expect(text).not.toContain("Check for updates");
  expect(text).not.toContain("Update channel");
});

it("shows the update controls and the version where the platform can update itself", async () => {
  const container = mount({ kind: "tauri", host: BROWSER_HOST, updater: { currentVersion: async () => "0.1.0", check: async () => undefined, install: async () => {} } });
  await act(async () => {});
  expect(container.textContent).toContain("Check for updates");
  expect(container.textContent).toContain("Update channel");
  expect(container.querySelector("h3")?.parentElement?.textContent?.replace(/[\u2066-\u2069]/g, "")).toContain("Version 0.1.0");
});

it("opens the third-party licenses", () => {
  const container = mount({ kind: "browser", host: BROWSER_HOST });
  const open = Array.from(container.querySelectorAll("button")).find((button) => button.textContent === "View licenses")!;
  act(() => open.click());
  expect(container.textContent).toContain("Licenses dialog");
});
