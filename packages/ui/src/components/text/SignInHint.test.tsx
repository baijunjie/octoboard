// @vitest-environment jsdom
import { expect, it } from "vitest";

import { LanguageProvider } from "../../i18n/react";
import { mountHost } from "../../test/mountHost";
import { testDaemon } from "../../test/testDaemon";
import { TEST_PROVIDERS } from "../../test/testProviders";
import type { Daemon } from "../../store";
import { SignInHint } from "./SignInHint";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const mount = (dir: string, daemon: Daemon = testDaemon()) => mountHost(<LanguageProvider><SignInHint providers={TEST_PROVIDERS} agent="test_agent" dir={dir} /></LanguageProvider>, { daemon });
const directory = (root: ParentNode) => [...root.querySelectorAll("[data-inline-reference]")].map((chip) => chip.textContent).at(-1);

it("shows the directory as the window shows the host's paths, under the home directory as `~`", () => {
  const daemon = testDaemon({ state: { homeDir: "/home/dev" } });
  const container = mount("/home/dev/.test-work", daemon);
  expect(directory(container)).toBe("~/.test-work");
  expect(container.textContent).toContain("with ~/.test-work as its configuration directory");
  expect(directory(mount("/srv/other/.test-work", daemon))).toBe("/srv/other/.test-work");
});

it("shows the directory as it is where the host's home is not known", () => {
  expect(directory(mount("/home/dev/.test-work"))).toBe("/home/dev/.test-work");
});

it("shows a Windows host's directory in full, whatever its home", () => {
  const daemon = testDaemon({ state: { homeDir: "C:\\Users\\dev", host: { path_style: "windows" } as never } });
  expect(directory(mount("C:\\Users\\dev\\.test-work", daemon))).toBe("C:\\Users\\dev\\.test-work");
});

it("sets the provider's command and the directory apart as references", () => {
  const chips = [...mount("/accounts/work").querySelectorAll("[data-inline-reference]")].map((chip) => chip.textContent);
  expect(chips).toEqual(["test-agent login", "/accounts/work"]);
});
