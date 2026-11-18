// @vitest-environment jsdom
import { expect, it } from "vitest";

import { FIXTURE_PROVIDERS } from "../../fixtures/fixtureProviders";
import { LanguageProvider } from "../../i18n/react";
import type { ProviderInfo, SettingDescriptor, SettingKind } from "../../protocol";
import { mountHost } from "../../test/mountHost";
import { testDaemon } from "../../test/testDaemon";
import { AgentProviderSettings } from "./AgentProviderSettings";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** Claude Code's provider with its Codex compatibility setting of the given kind, as the daemon would serve it. */
function mountClaude(kind: SettingKind) {
  const providers: ProviderInfo[] = FIXTURE_PROVIDERS.map((provider) => provider.id !== "claude" ? provider : {
    ...provider,
    settings: provider.settings.map((setting): SettingDescriptor => setting.id === "codex_compatibility" ? { ...setting, kind } : setting),
  });
  return mountHost(<LanguageProvider><AgentProviderSettings agent="claude" /></LanguageProvider>, { daemon: testDaemon({ state: { providers } }) });
}

const compatibility = (container: HTMLElement) => container.querySelector<HTMLInputElement>('input[role="switch"][aria-label^="Codex compatibility"]');

it("shows a switch setting, on or off by its declared default while nothing is stored", () => {
  expect(compatibility(mountClaude({ type: "switch", default: "off" }))!.checked).toBe(false);
  expect(compatibility(mountClaude({ type: "switch", default: "on" }))!.checked).toBe(true);
});

it("reads a switch default this build does not know as off", () => {
  const container = mountClaude({ type: "switch", default: "from_a_newer_daemon" });
  expect(compatibility(container)).not.toBeNull();
  expect(compatibility(container)!.checked).toBe(false);
});

it("shows no row for a setting of a kind this build does not know", () => {
  const container = mountClaude({ type: "from_a_newer_daemon" } as unknown as SettingKind);
  expect(compatibility(container)).toBeNull();
  expect(container.textContent).not.toContain("Codex compatibility");
});
