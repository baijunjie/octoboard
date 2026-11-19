// @vitest-environment jsdom
import { beforeEach, expect, it } from "vitest";

import { buttons, click, inputLabelled, type } from "../../test/dom";
import { fakeTimers, runPendingTimers } from "../../test/clock";
import { deletedEvent, entryOf, PROJECT, PROJECT_SCOPE, savedEvent } from "../../fixtures/memoryBuilders";
import { renderMemory, settled } from "./memoryTestSupport";

beforeEach(() => fakeTimers());

const target = { kind: "project", project: PROJECT } as const;
const text = () => document.body.textContent ?? "";
const button = (label: string) => buttons().find((candidate) => candidate.textContent === label);
const settle = async () => {
  await settled();
  runPendingTimers();
};
const mine = (id: string, extra = {}) => entryOf(id, { scope: PROJECT_SCOPE, ...extra });

it("opens the project's memory under its own title and intro, naming the project", async () => {
  const { request } = renderMemory({ target, entries: [mine("a")] });
  await settle();
  expect(request).toHaveBeenCalledWith({ type: "list_memories", project: PROJECT.id });
  expect(document.querySelector('[role="dialog"] h2')?.textContent).toBe("Project memory");
  expect(text()).toContain("need to know about the machine the project is on, such as local paths");
  expect(text()).toContain("1 of 200 entries");
});

it("says the project's memory is empty in its own words", async () => {
  renderMemory({ target });
  await settle();
  expect(text()).toContain("No memory yet");
  expect(text()).toContain("Agents working in the project save here what they need to know about the machine the project is on");
});

it("shows no machine-only tag or switch, since every entry of a project's memory holds only here", async () => {
  renderMemory({ target, entries: [mine("a", { local: true })] });
  await settle();
  expect(text()).not.toContain("This machine only");
  await click(button("Add entry"));
  await settle();
  expect(document.querySelector("textarea")).not.toBeNull();
  expect(inputLabelled("This machine only")).toBeUndefined();
});

it("adds an entry to the project, sending no flag", async () => {
  const { sent } = renderMemory({ target });
  await settle();
  await click(button("Add entry"));
  await settle();
  type(inputLabelled("Summary"), "Port 5432");
  type(document.querySelector("textarea")!, "The database listens on 5432.");
  await click(button("Add"));
  expect(sent()).toEqual([{ type: "save_memory", project: PROJECT.id, summary: "Port 5432", content: "The database listens on 5432." }]);
});

it("deletes one entry or all of them through the project", async () => {
  const { sent } = renderMemory({ target, entries: [mine("a")] });
  await settle();
  await click(button("Delete all"));
  await settle();
  type(document.querySelector<HTMLInputElement>('input[type="text"], input:not([type])')!, "delete");
  await click(buttons().filter((candidate) => candidate.textContent === "Delete all").at(-1));
  expect(sent()).toEqual([{ type: "delete_memories", project: PROJECT.id }]);
});

it("follows the changes of its place, whichever project made them, and ignores another place and the console's memory", async () => {
  const { broadcast } = renderMemory({ target, entries: [mine("a")] });
  await settle();
  broadcast(savedEvent(mine("b", { summary: "Fresh" })));
  expect(text()).toContain("Fresh");
  broadcast(savedEvent(mine("c", { summary: "Elsewhere", scope: { kind: "place", host_id: "other", path: "/work/app" } })));
  broadcast(savedEvent(mine("d", { summary: "Another folder", scope: { kind: "place", host_id: "local", path: "/work/other" } })));
  broadcast(savedEvent(entryOf("e", { summary: "A console's" })));
  broadcast(savedEvent(mine("f", { summary: "Unknown kind", scope: { kind: "cluster" } as never })));
  expect(text()).not.toContain("Elsewhere");
  expect(text()).not.toContain("Another folder");
  expect(text()).not.toContain("A console's");
  expect(text()).not.toContain("Unknown kind");
  broadcast(deletedEvent(["a"], PROJECT_SCOPE));
  expect(text()).not.toContain("About a");
});
