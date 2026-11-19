// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";

import { CATALOGS } from "../../i18n/catalog";
import { mountAgentSettings } from "./agentAccountsSectionTestSupport";

vi.mock("../../components/icons/AgentIcon", async (importOriginal) => (await import("./agentAccountsSectionTestState")).agentIconStandIn(await importOriginal()));
vi.mock("../../components/text/FadeOverflow", async () => ({ FadeOverflow: (await import("./agentAccountsSectionTestState")).FadeOverflowStandIn }));
vi.mock("../../components/text/MarkedPath", async () => ({ MarkedPath: (await import("./agentAccountsSectionTestState")).MarkedPathStandIn }));
vi.mock("../useSectionRefocus", async (importOriginal) => (await import("./agentAccountsSectionTestState")).sectionRefocusStandIn(await importOriginal()));
vi.mock("../../components/form/OptionSelect", async () => ({ OptionSelect: (await import("./agentAccountsSectionTestState")).OptionSelectStandIn }));

/** The scope line's catalog entry split around the agent's name, which the dialog sets as a chip between the two. */
const [BEFORE, AFTER] = (CATALOGS.en!["settings.accounts.agentSettings.scope"] as string).split("{agent}");
const scopeLines = (container: HTMLElement, name: string) =>
  [...container.querySelectorAll("p")].filter((p) => p.textContent!.startsWith(BEFORE) && p.textContent!.endsWith(AFTER) && p.textContent!.slice(BEFORE.length, -AFTER.length).includes(name));
const tagsInput = (container: HTMLElement, name: string) =>
  [...container.querySelectorAll<HTMLLabelElement>("label")].find((label) => label.textContent === `Tags for \u2068${name}\u2069`)?.control;

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
});
