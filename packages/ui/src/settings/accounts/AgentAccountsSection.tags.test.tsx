// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { errorAlerts } from "../../components/alerts/errorAlerts";
import { bus, request, settings } from "./agentAccountsSectionTestState";
import { mount } from "./agentAccountsSectionTestSupport";

vi.mock("../../store", async (importOriginal) => (await import("./agentAccountsSectionTestState")).storeStandIn(await importOriginal()));
vi.mock("../../components/icons/AgentIcon", async (importOriginal) => (await import("./agentAccountsSectionTestState")).agentIconStandIn(await importOriginal()));
vi.mock("../../components/text/FadeOverflow", async () => ({ FadeOverflow: (await import("./agentAccountsSectionTestState")).FadeOverflowStandIn }));
vi.mock("../../components/text/MarkedPath", async () => ({ MarkedPath: (await import("./agentAccountsSectionTestState")).MarkedPathStandIn }));
vi.mock("../useSectionRefocus", async (importOriginal) => (await import("./agentAccountsSectionTestState")).sectionRefocusStandIn(await importOriginal()));
vi.mock("../../components/form/OptionSelect", async () => ({ OptionSelect: (await import("./agentAccountsSectionTestState")).OptionSelectStandIn }));

const tagsInput = (container: HTMLElement, name = "Codex") =>
  [...container.querySelectorAll<HTMLLabelElement>('[role="tabpanel"] label')].find((label) => label.textContent === `Tags for ${name}`)?.control as HTMLInputElement;
const type = (input: HTMLInputElement, text: string) => act(() => {
  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, text);
  input.dispatchEvent(new Event("input", { bubbles: true }));
});
const enter = (input: HTMLInputElement) => act(() => input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })));
const pickedTags = (container: HTMLElement) => [...container.querySelectorAll('[role="tabpanel"] [role="row"]')].map((row) => row.textContent);
// A tag being removed stays in the box, without its list-motion mark, until its exit animation ends.
const liveTags = (container: HTMLElement) => [...container.querySelectorAll('[role="tabpanel"] [role="row"][data-list-motion]')].map((row) => row.textContent);

it.each(["Claude Code", "Codex", "Grok Build"] as const)("puts the Tags row first in the %s tab, above the other per-agent rows and the table", (name) => {
  const { container } = mount(name);
  const panel = container.querySelector('[role="tabpanel"]')!;
  const input = tagsInput(container, name);
  expect(input).not.toBeNull();
  expect(panel.textContent).toContain(`Applies to every ${name} account`);
  const following = (other: Element | null) => other === null || (input.compareDocumentPosition(other) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0;
  expect(following(panel.querySelector("table"))).toBe(true);
  expect(following(panel.querySelector('[aria-label^="Permission approval"]'))).toBe(true);
  expect(following(panel.querySelector('input[role="switch"]'))).toBe(true);
});

it("shows the stored tags of the agent in view", () => {
  settings.agent_tags = { codex: ["review", "tests"], grok: ["research"] };
  expect(pickedTags(mount("Codex").container)).toEqual(["review", "tests"]);
  expect(pickedTags(mount("Grok Build").container)).toEqual(["research"]);
});

it("saves the whole list of the agent when a tag is typed", async () => {
  settings.agent_tags = { codex: ["review"] };
  const { container } = mount("Codex");
  const input = tagsInput(container);
  type(input, " tests ");
  await act(async () => enter(input));
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "update_settings", agent_tags: { codex: ["review", "tests"] } });
  // Until the store holds the change, the field shows what was asked for.
  expect(pickedTags(container)).toEqual(["review", "tests"]);
  await act(async () => {});
});

it("keeps showing the requested tags when the reply comes before the broadcast, and builds on them", async () => {
  settings.agent_tags = { codex: ["review"] };
  const { container } = mount("Codex");
  const input = tagsInput(container);
  type(input, "tests");
  await act(async () => enter(input));
  // The request is answered but the store has not heard of the change yet.
  await act(async () => {});
  expect(pickedTags(container)).toEqual(["review", "tests"]);
  type(input, "more");
  await act(async () => enter(input));
  expect(request).toHaveBeenLastCalledWith({ type: "update_settings", agent_tags: { codex: ["review", "tests", "more"] } });
  // The broadcasts then bring the saved tags, and the field follows the store again.
  settings.agent_tags = { codex: ["review", "tests", "more"] };
  act(() => bus.notify());
  settings.agent_tags = { codex: ["review", "tests", "more", "other"] };
  act(() => bus.notify());
  expect(pickedTags(container)).toEqual(["review", "tests", "more", "other"]);
});

it("takes the spelling of a tag the added agents carry, and none from a removed agent", async () => {
  settings.agent_tags = { claude: ["UI"], grok: ["Research"] };
  settings.added_agents = ["claude", "codex"];
  const { container } = mount("Codex");
  const input = tagsInput(container);
  type(input, "ui");
  await act(async () => enter(input));
  // The daemon's broadcast brings the saved tags back.
  settings.agent_tags = { ...settings.agent_tags, codex: ["UI"] };
  act(() => bus.notify());
  type(input, "research");
  await act(async () => enter(input));
  expect(request).toHaveBeenNthCalledWith(1, { type: "update_settings", agent_tags: { codex: ["UI"] } });
  expect(request).toHaveBeenNthCalledWith(2, { type: "update_settings", agent_tags: { codex: ["UI", "research"] } });
});

it("reports a failed save and builds the next change on the stored tags once no save is outstanding", async () => {
  settings.agent_tags = { codex: ["review"] };
  const { container } = mount("Codex");
  request.mockRejectedValueOnce(new Error("save refused"));
  const input = tagsInput(container);
  type(input, "tests");
  await act(async () => enter(input));
  await act(async () => {});
  expect(errorAlerts.getSnapshot().map((alert) => alert.title)).toEqual(["Could not save the tags"]);
  type(input, "more");
  await act(async () => enter(input));
  expect(request).toHaveBeenLastCalledWith({ type: "update_settings", agent_tags: { codex: ["review", "more"] } });
});

it("shows a concurrent writer's value once its own save has settled, and not the value it asked for", async () => {
  settings.agent_tags = { codex: ["review"] };
  const { container } = mount("Codex");
  const input = tagsInput(container);
  type(input, "tests");
  await act(async () => enter(input));
  await act(async () => {});
  // The reply is in and the store has not heard of the change; another writer then changes the same agent.
  expect(pickedTags(container)).toEqual(["review", "tests"]);
  settings.agent_tags = { codex: ["elsewhere"] };
  act(() => bus.notify());
  expect(liveTags(container)).toEqual(["elsewhere"]);
  type(input, "more");
  await act(async () => enter(input));
  expect(request).toHaveBeenLastCalledWith({ type: "update_settings", agent_tags: { codex: ["elsewhere", "more"] } });
});

it("shows no intermediate list while saves are pending, when a tag is added and removed before any broadcast", async () => {
  settings.agent_tags = { codex: ["review"] };
  const replies: (() => void)[] = [];
  request.mockImplementation(() => new Promise<object>((resolve) => replies.push(() => resolve({}))));
  const { container } = mount("Codex");
  const input = tagsInput(container);
  type(input, "tests");
  enter(input);
  act(() => input.dispatchEvent(new KeyboardEvent("keydown", { key: "Backspace", bubbles: true })));
  expect(request).toHaveBeenLastCalledWith({ type: "update_settings", agent_tags: { codex: ["review"] } });
  expect(liveTags(container)).toEqual(["review"]);
  // The first save's broadcast arrives before the second's.
  settings.agent_tags = { codex: ["review", "tests"] };
  act(() => bus.notify());
  expect(liveTags(container)).toEqual(["review"]);
  await act(async () => replies.forEach((reply) => reply()));
  expect(liveTags(container)).toEqual(["review"]);
  settings.agent_tags = { codex: ["review"] };
  act(() => bus.notify());
  expect(liveTags(container)).toEqual(["review"]);
});

it("shows a concurrent writer's value that arrives before its own reply, and builds the next change on it", async () => {
  settings.agent_tags = { codex: ["review"] };
  let reply = () => {};
  request.mockImplementation(() => new Promise<object>((resolve) => { reply = () => resolve({}); }));
  const { container } = mount("Codex");
  const input = tagsInput(container);
  type(input, "tests");
  enter(input);
  settings.agent_tags = { codex: ["elsewhere"] };
  act(() => bus.notify());
  expect(pickedTags(container)).toEqual(["review", "tests"]);
  await act(async () => reply());
  expect(liveTags(container)).toEqual(["elsewhere"]);
  type(input, "more");
  await act(async () => enter(input));
  expect(request).toHaveBeenLastCalledWith({ type: "update_settings", agent_tags: { codex: ["elsewhere", "more"] } });
});
