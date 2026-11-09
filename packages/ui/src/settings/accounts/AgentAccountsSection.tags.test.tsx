// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { errorAlerts } from "../../components/alerts/errorAlerts";
import { CodedError } from "../../errors/codedError";
import { isolate } from "../../i18n/isolate";
import { request, settings } from "./agentAccountsSectionTestState";
import { mountAgentSettings } from "./agentAccountsSectionTestSupport";

vi.mock("../../components/icons/AgentIcon", async (importOriginal) => (await import("./agentAccountsSectionTestState")).agentIconStandIn(await importOriginal()));
vi.mock("../../components/text/FadeOverflow", async () => ({ FadeOverflow: (await import("./agentAccountsSectionTestState")).FadeOverflowStandIn }));
vi.mock("../../components/text/MarkedPath", async () => ({ MarkedPath: (await import("./agentAccountsSectionTestState")).MarkedPathStandIn }));
vi.mock("../useSectionRefocus", async (importOriginal) => (await import("./agentAccountsSectionTestState")).sectionRefocusStandIn(await importOriginal()));
vi.mock("../../components/form/OptionSelect", async () => ({ OptionSelect: (await import("./agentAccountsSectionTestState")).OptionSelectStandIn }));

const tagsInput = (container: HTMLElement, name = "Codex") =>
  [...container.querySelectorAll<HTMLLabelElement>("label")].find((label) => label.textContent === `Tags for ${isolate(name)}`)?.control as HTMLInputElement;
const type = (input: HTMLInputElement, text: string) => act(() => {
  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, text);
  input.dispatchEvent(new Event("input", { bubbles: true }));
});
const enter = (input: HTMLInputElement) => act(() => input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })));
const pickedTags = (container: HTMLElement) => [...container.querySelectorAll('[role="row"]')].map((row) => row.textContent);
// A tag being removed stays in the box, without its list-motion mark, until its exit animation ends.
const liveTags = (container: HTMLElement) => [...container.querySelectorAll('[role="row"][data-list-motion]')].map((row) => row.textContent);

it.each(["Claude Code", "Codex", "Grok Build"] as const)("puts the Tags row first in the %s settings dialog, above the other per-agent rows, and none in the tab", async (name) => {
  const { container, section } = await mountAgentSettings(name);
  const input = tagsInput(container, name);
  expect(input).not.toBeNull();
  expect(container.textContent).toContain(`Applies to every ${isolate(name)} account`);
  const following = (other: Element | null) => other === null || (input.compareDocumentPosition(other) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0;
  expect(following(container.querySelector('[aria-label^="Permission approval"]'))).toBe(true);
  expect(following(container.querySelector('input[role="switch"]'))).toBe(true);
  expect(section.querySelector('[role="tabpanel"] [aria-label^="Tags for"]')).toBeNull();
  expect(section.querySelector('[role="tabpanel"] table')).not.toBeNull();
});

it.each([["Codex", ["review", "tests"]], ["Grok Build", ["research"]]] as const)("shows the stored tags of the agent in view: %s", async (name, tags) => {
  settings.agent_tags = { codex: ["review", "tests"], grok: ["research"] };
  expect(pickedTags((await mountAgentSettings(name)).container)).toEqual(tags);
});

it("saves the whole list of the agent when a tag is typed", async () => {
  settings.agent_tags = { codex: ["review"] };
  const { container } = await mountAgentSettings("Codex");
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
  const { container, setSettings } = await mountAgentSettings("Codex");
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
  setSettings({ agent_tags: { codex: ["review", "tests", "more"] } });
  setSettings({ agent_tags: { codex: ["review", "tests", "more", "other"] } });
  expect(pickedTags(container)).toEqual(["review", "tests", "more", "other"]);
});

it("takes the spelling of a tag the added agents carry, and none from a removed agent", async () => {
  settings.agent_tags = { claude: ["UI"], grok: ["Research"] };
  settings.added_agents = ["claude", "codex"];
  const { container, setSettings } = await mountAgentSettings("Codex");
  const input = tagsInput(container);
  type(input, "ui");
  await act(async () => enter(input));
  // The daemon's broadcast brings the saved tags back.
  setSettings({ agent_tags: { ...settings.agent_tags, codex: ["UI"] } });
  type(input, "research");
  await act(async () => enter(input));
  expect(request).toHaveBeenNthCalledWith(1, { type: "update_settings", agent_tags: { codex: ["UI"] } });
  expect(request).toHaveBeenNthCalledWith(2, { type: "update_settings", agent_tags: { codex: ["UI", "research"] } });
});

it("reports a failed save and builds the next change on the stored tags once no save is outstanding", async () => {
  settings.agent_tags = { codex: ["review"] };
  const { container } = await mountAgentSettings("Codex");
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
  const { container, setSettings } = await mountAgentSettings("Codex");
  const input = tagsInput(container);
  type(input, "tests");
  await act(async () => enter(input));
  await act(async () => {});
  // The reply is in and the store has not heard of the change; another writer then changes the same agent.
  expect(pickedTags(container)).toEqual(["review", "tests"]);
  setSettings({ agent_tags: { codex: ["elsewhere"] } });
  expect(liveTags(container)).toEqual(["elsewhere"]);
  type(input, "more");
  await act(async () => enter(input));
  expect(request).toHaveBeenLastCalledWith({ type: "update_settings", agent_tags: { codex: ["elsewhere", "more"] } });
});

it("shows no intermediate list while saves are pending, when a tag is added and removed before any broadcast", async () => {
  settings.agent_tags = { codex: ["review"] };
  const replies: (() => void)[] = [];
  request.mockImplementation(() => new Promise<object>((resolve) => replies.push(() => resolve({}))));
  const { container, setSettings } = await mountAgentSettings("Codex");
  const input = tagsInput(container);
  type(input, "tests");
  enter(input);
  act(() => input.dispatchEvent(new KeyboardEvent("keydown", { key: "Backspace", bubbles: true })));
  // One save is on the wire at a time: the removal waits for the first save's reply.
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "update_settings", agent_tags: { codex: ["review", "tests"] } });
  expect(liveTags(container)).toEqual(["review"]);
  // The first save's broadcast arrives before the second's.
  setSettings({ agent_tags: { codex: ["review", "tests"] } });
  expect(liveTags(container)).toEqual(["review"]);
  await act(async () => replies.forEach((reply) => reply()));
  expect(request).toHaveBeenLastCalledWith({ type: "update_settings", agent_tags: { codex: ["review"] } });
  expect(liveTags(container)).toEqual(["review"]);
  setSettings({ agent_tags: { codex: ["review"] } });
  expect(liveTags(container)).toEqual(["review"]);
});

it("shows a concurrent writer's value that arrives before its own reply, and builds the next change on it", async () => {
  settings.agent_tags = { codex: ["review"] };
  let reply = () => {};
  request.mockImplementation(() => new Promise<object>((resolve) => { reply = () => resolve({}); }));
  const { container, setSettings } = await mountAgentSettings("Codex");
  const input = tagsInput(container);
  type(input, "tests");
  enter(input);
  setSettings({ agent_tags: { codex: ["elsewhere"] } });
  expect(pickedTags(container)).toEqual(["review", "tests"]);
  await act(async () => reply());
  expect(liveTags(container)).toEqual(["elsewhere"]);
  type(input, "more");
  await act(async () => enter(input));
  expect(request).toHaveBeenLastCalledWith({ type: "update_settings", agent_tags: { codex: ["elsewhere", "more"] } });
});

// What a daemon does with a save based on a version: the first one at the current version applies, and advances it.
function checkingDaemon(version: { current: number }) {
  const replies: (() => void)[] = [];
  request.mockImplementation((body) => {
    if (body.type !== "update_settings") return Promise.resolve({});
    const based = (body as { base_versions?: Record<string, number> }).base_versions?.["agent_tags.codex"];
    if (based !== undefined && based !== version.current) {
      return Promise.reject(new CodedError("changed elsewhere", "write_conflict", { fields: "agent_tags.codex", versions: JSON.stringify({ "agent_tags.codex": version.current }) }));
    }
    version.current += 1;
    return new Promise<object>((resolve) => replies.push(() => resolve({})));
  });
  return replies;
}

it("never conflicts with its own earlier save: an edit made meanwhile waits and goes on the version the store has then", async () => {
  settings.agent_tags = { codex: ["review"] };
  settings.versions = { "agent_tags.codex": 1 };
  const version = { current: 1 };
  const replies = checkingDaemon(version);
  const { container, setSettings } = await mountAgentSettings("Codex");
  const input = tagsInput(container);
  type(input, "tests");
  enter(input);
  type(input, "more");
  enter(input);
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "update_settings", agent_tags: { codex: ["review", "tests"] }, base_versions: { "agent_tags.codex": 1 } });
  // The broadcast of the first save, then its reply.
  setSettings({ agent_tags: { codex: ["review", "tests"] }, versions: { "agent_tags.codex": 2 } });
  await act(async () => replies.forEach((reply) => reply()));
  expect(request).toHaveBeenLastCalledWith({ type: "update_settings", agent_tags: { codex: ["review", "tests", "more"] }, base_versions: { "agent_tags.codex": 2 } });
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it("sends again, silently, a save the daemon refused because the store was behind its own earlier save", async () => {
  settings.agent_tags = { codex: ["review"] };
  settings.versions = { "agent_tags.codex": 1 };
  const version = { current: 1 };
  const replies = checkingDaemon(version);
  const { container, setSettings } = await mountAgentSettings("Codex");
  const input = tagsInput(container);
  type(input, "tests");
  enter(input);
  // The reply comes before the broadcast, so the next edit is based on the old version and is refused.
  await act(async () => replies.forEach((reply) => reply()));
  await act(async () => {});
  type(input, "more");
  await act(async () => enter(input));
  expect(request).toHaveBeenCalledTimes(2);
  expect(pickedTags(container)).toEqual(["review", "tests", "more"]);
  setSettings({ agent_tags: { codex: ["review", "tests"] }, versions: { "agent_tags.codex": 2 } });
  await act(async () => {});
  expect(request).toHaveBeenLastCalledWith({ type: "update_settings", agent_tags: { codex: ["review", "tests", "more"] }, base_versions: { "agent_tags.codex": 2 } });
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it("reports another client's change as a conflict and shows its tags", async () => {
  onTestFinished(() => { while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge(); });
  settings.agent_tags = { codex: ["review"] };
  settings.versions = { "agent_tags.codex": 1 };
  const version = { current: 3 };
  checkingDaemon(version);
  const { container, setSettings } = await mountAgentSettings("Codex");
  const input = tagsInput(container);
  type(input, "tests");
  await act(async () => enter(input));
  // Another client changed the tags, which the store has not heard of yet.
  expect(pickedTags(container)).toEqual(["review", "tests"]);
  setSettings({ agent_tags: { codex: ["elsewhere"] }, versions: { "agent_tags.codex": 3 } });
  await act(async () => {});
  expect(errorAlerts.getSnapshot().map((alert) => alert.title)).toEqual(["Could not save the tags"]);
  expect(liveTags(container)).toEqual(["elsewhere"]);
  expect(request).toHaveBeenCalledTimes(1);
});

it("sends one save for two edits made in the same event, the second waiting for the first", async () => {
  settings.agent_tags = { codex: ["review"] };
  const replies: (() => void)[] = [];
  request.mockImplementation(() => new Promise<object>((resolve) => replies.push(() => resolve({}))));
  const { container } = await mountAgentSettings("Codex");
  const input = tagsInput(container);
  // No render comes between the two edits, so neither can have seen the other's save start.
  act(() => {
    type(input, "tests");
    enter(input);
    type(input, "more");
    enter(input);
  });
  expect(request).toHaveBeenCalledTimes(1);
  await act(async () => replies.splice(0).forEach((reply) => reply()));
  expect(request).toHaveBeenCalledTimes(2);
  await act(async () => replies.splice(0).forEach((reply) => reply()));
});

it("drops an edit that waited when another client's list reached the store meanwhile, and shows that list", async () => {
  onTestFinished(() => { while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge(); });
  settings.agent_tags = { codex: ["review"] };
  const replies: (() => void)[] = [];
  request.mockImplementation(() => new Promise<object>((resolve) => replies.push(() => resolve({}))));
  const { container, setSettings } = await mountAgentSettings("Codex");
  const input = tagsInput(container);
  type(input, "tests");
  enter(input);
  type(input, "more");
  enter(input);
  setSettings({ agent_tags: { codex: ["elsewhere"] } });
  await act(async () => replies.splice(0).forEach((reply) => reply()));
  // Sending the waiting edit would overwrite the other client's list.
  expect(request).toHaveBeenCalledTimes(1);
  expect(errorAlerts.getSnapshot().map((alert) => alert.title)).toEqual(["Could not save the tags"]);
  expect(liveTags(container)).toEqual(["elsewhere"]);
});
