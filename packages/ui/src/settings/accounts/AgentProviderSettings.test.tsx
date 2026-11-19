// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { FIXTURE_PROVIDERS } from "../../fixtures/fixtureProviders";
import { CATALOGS, type MessageKey } from "../../i18n/catalog";
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

/** Claude Code's rows as the fixture daemon's provider declares them, with `request` answering what the rows send. */
function mountClaudeRows(request = vi.fn(() => Promise.resolve({ type: "ack", id: null }))) {
  const container = mountHost(<LanguageProvider><AgentProviderSettings agent="claude" /></LanguageProvider>, { daemon: testDaemon({ state: { providers: FIXTURE_PROVIDERS }, request: request as never }) });
  return { container, request };
}

const simpleShell = (container: HTMLElement) => container.querySelector<HTMLInputElement>('input[role="switch"][aria-label^="Simple shell commands"]');

it("shows Claude Code's simple shell commands switch after Codex compatibility, off until chosen, with its help", async () => {
  const { container } = mountClaudeRows();
  const input = simpleShell(container)!;
  expect(input).not.toBeNull();
  expect(input.checked).toBe(false);
  expect(input.getAttribute("aria-label")).toBe("Simple shell commands for \u2068Claude Code\u2069");
  expect(container.textContent).toContain("Simple shell commands");
  expect(compatibility(container)!.compareDocumentPosition(input) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  const help = container.querySelectorAll<HTMLButtonElement>('button[aria-label="About simple shell commands"]');
  expect(help).toHaveLength(1);
  await act(async () => help[0].click());
  const tip = document.querySelector('[role="dialog"][aria-label="About simple shell commands"]');
  expect(tip).not.toBeNull();
  // The tip carries the catalog's description, whatever its wording, with the names filled in.
  const description = CATALOGS.en!["provider.claude.setting.simple_shell_commands.description" as MessageKey] as string;
  const literals = description.split(/\{\w+\}/).filter((part) => part !== "");
  expect(literals.length).toBeGreaterThan(0);
  for (const literal of literals) expect(tip!.textContent).toContain(literal);
  expect(tip!.textContent).toContain("Claude Code");
});

it("saves the simple shell commands switch for the agent when toggled", async () => {
  const { container, request } = mountClaudeRows();
  await act(async () => simpleShell(container)!.click());
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "set_provider_setting", agent: "claude", setting: "simple_shell_commands", value: true });
});
