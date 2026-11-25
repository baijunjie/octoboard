// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";

import { providerMessageKey } from "../../agents/providers";
import { CATALOGS } from "../../i18n/catalog";
import { providerPresentation, providerReferences } from "../../providers/registry";
import { SESSION_NOTE_COMMAND } from "../../test/sessionNoteProvider";
import { TEST_AGENTS, TEST_PROVIDERS } from "../../test/testProviders";
import { settings } from "./agentAccountsSectionTestState";
import { mountAgentSettings } from "./agentAccountsSectionTestSupport";

vi.mock("../../components/icons/AgentIcon", async (importOriginal) => (await import("./agentAccountsSectionTestState")).agentIconStandIn(await importOriginal()));
vi.mock("../../components/text/FadeOverflow", async () => ({ FadeOverflow: (await import("./agentAccountsSectionTestState")).FadeOverflowStandIn }));
vi.mock("../../components/text/MarkedPath", async () => ({ MarkedPath: (await import("./agentAccountsSectionTestState")).MarkedPathStandIn }));
vi.mock("../useSectionRefocus", async (importOriginal) => (await import("./agentAccountsSectionTestState")).sectionRefocusStandIn(await importOriginal()));
// The test-only provider is given a note about its sessions, so the note's tests lean on no real agent's module.
vi.mock("../../providers/registry", async (importOriginal) => (await import("../../test/sessionNoteProvider")).withSessionNote(await importOriginal()));
vi.mock("../../components/form/OptionSelect", async () => ({ OptionSelect: (await import("./agentAccountsSectionTestState")).OptionSelectStandIn }));

/** The scope line's catalog entry split around the agent's name, which the dialog sets as a chip between the two. */
const [BEFORE, AFTER] = (CATALOGS.en!["settings.accounts.agentSettings.scope"] as string).split("{agent}");
const scopeLines = (container: HTMLElement, name: string) =>
  [...container.querySelectorAll("p")].filter((p) => p.textContent!.startsWith(BEFORE) && p.textContent!.endsWith(AFTER) && p.textContent!.slice(BEFORE.length, -AFTER.length).includes(name));
const tagsInput = (container: HTMLElement, name: string) =>
  [...container.querySelectorAll<HTMLLabelElement>("label")].find((label) => label.textContent === `Tags for \u2068${name}\u2069`)?.control;
/** The lines of secondary text under the agent's name and above the rows: the scope line, and what stands beside it. */
const openingLines = (container: HTMLElement, name: string) => [...scopeLines(container, name)[0]!.parentElement!.querySelectorAll("p")];
/** The id of the served provider labelled `name`. */
const providerId = (name: string) => TEST_PROVIDERS.find((provider) => provider.label === name)!.id;

describe.each(["Claude Code", "Codex", "Grok Build"] as const)("the %s settings dialog", (name) => {
  it("says once, under the agent's name and above the Tags row, that its settings apply to every account of the agent", async () => {
    const { container } = await mountAgentSettings(name);
    const lines = scopeLines(container, name);
    expect(lines).toHaveLength(1);
    const line = lines[0];
    const heading = [...container.querySelectorAll("span")].find((el) => el.textContent === name)!;
    expect(heading.compareDocumentPosition(line) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(line.compareDocumentPosition(tagsInput(container, name)!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("gives no row, the Tags row included, a description of its own: the scope line speaks for all of them", async () => {
    const { container } = await mountAgentSettings(name);
    expect(tagsInput(container, name)).not.toBeNull();
    expect(container.querySelectorAll("[data-setting-description]")).toHaveLength(0);
  });

  it("draws nothing beside the scope line for a provider whose module has no note about its sessions", async () => {
    expect(providerMessageKey(providerId(name), "sessionNote")).toBeUndefined();
    const { container } = await mountAgentSettings(name);
    expect(openingLines(container, name)).toHaveLength(1);
  });
});

describe("the settings dialog of a provider whose module has a note about its sessions", () => {
  const NAME = "Test Agent";
  const mountNoted = async () => {
    settings.added_agents = ["claude", "test_agent"];
    return mountAgentSettings(NAME);
  };

  it("shows the note once, as a line of its own after the scope line and above the rows", async () => {
    const { container } = await mountNoted();
    const [scope, note, ...rest] = openingLines(container, NAME);
    expect(rest).toHaveLength(0);
    expect(scope).toBe(scopeLines(container, NAME)[0]);
    expect(note!.textContent).toContain("forgets what it changed in its own configuration when it ends");
    expect([...container.querySelectorAll("p")].filter((p) => p.textContent!.includes("forgets what it changed"))).toEqual([note]);
    expect(note!.compareDocumentPosition(tagsInput(container, NAME)!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("is secondary text, as the scope line is", async () => {
    const { container } = await mountNoted();
    const [scope, note] = openingLines(container, NAME);
    expect(note!.className).toBe(scope!.className);
    expect(note!.classList.contains("text-app-muted")).toBe(true);
  });

  it("names the agent as a name and sets the command its module declares apart as a left-to-right literal", async () => {
    const { container } = await mountNoted();
    const note = openingLines(container, NAME)[1]!;
    const references = [...note.querySelectorAll("[data-inline-reference]")];
    expect(references.map((reference) => reference.textContent)).toEqual([NAME, SESSION_NOTE_COMMAND]);
    expect(references[0]!.getAttribute("dir")).toBe("auto");
    expect(references[0]!.querySelector("code")).toBeNull();
    expect(references[1]!.getAttribute("dir")).toBe("ltr");
    expect(references[1]!.querySelector("code")!.textContent).toBe(SESSION_NOTE_COMMAND);
    expect(note.textContent).not.toMatch(/\{[A-Za-z]+\}/);
  });
});

describe("the notes about their sessions the providers declare", () => {
  it.each(TEST_AGENTS)("%s words a declared sessionNote in every language it carries, and in every catalog", (agent) => {
    const module = providerPresentation(agent)!;
    const key = `provider.${agent}.sessionNote`;
    const declared = Object.hasOwn(module.messages.en, key);
    for (const [language, messages] of Object.entries(module.messages) as [string, Record<string, string>][]) {
      expect(Object.hasOwn(messages, key), `${agent} ${language}`).toBe(declared);
    }
    for (const [language, catalog] of Object.entries(CATALOGS)) {
      const text = (catalog as Record<string, string>)[key];
      expect(typeof text === "string" && text.trim().length > 0, `${language} ${key}`).toBe(declared);
    }
  });

  it("fill every placeholder of a note from the agent's name, the product's or a reference its module declares", () => {
    for (const agent of TEST_AGENTS) {
      const module = providerPresentation(agent)!;
      const key = `provider.${agent}.sessionNote`;
      const references = Object.keys(providerReferences(agent, "sessionNote"));
      for (const [language, messages] of Object.entries(module.messages) as [string, Record<string, string>][]) {
        const placeholders = [...(messages[key] ?? "").matchAll(/\{([A-Za-z][A-Za-z0-9_]*)\}/g)].map((match) => match[1]!);
        for (const name of placeholders) expect([...references, "agent", "appName"], `${language} ${key} uses {${name}}`).toContain(name);
      }
    }
    expect(providerReferences("test_agent", "sessionNote")).toEqual({ command: SESSION_NOTE_COMMAND });
  });
});
