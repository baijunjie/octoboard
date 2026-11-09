// @vitest-environment jsdom
import { act } from "react";
import { expect, it } from "vitest";

import { CodedError } from "../errors/codedError";
import { PROJECT_VERSIONS, projectOf } from "../fixtures/builders";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/announce/StatusAnnouncer";
import { buttonText, click, input, render, sentOf, settle, type } from "./projectDialogTestSupport";

const textarea = () => document.querySelector<HTMLTextAreaElement>("textarea")!;

it("sends only the fields that changed, each with the version the form read", async () => {
  const pending = render(projectOf("p-1", "c-1", "Site", { description: "A site." }));
  type(input("Name"), "Renamed");
  await click(buttonText("Save"));
  expect(sentOf(pending, "update_project")).toEqual({ type: "update_project", project: "p-1", name: "Renamed", base_versions: { name: 1 } });
});

it("sends the tags only when they changed", async () => {
  const pending = render(projectOf("p-1", "c-1", "Site", { tags: ["web"] }));
  type(input("Name"), "Renamed");
  await click(buttonText("Save"));
  expect(sentOf(pending, "update_project")).not.toHaveProperty("tags");
});

it("puts a conflicting field back to the current value, keeps the other edit and bases the retry on the current version", async () => {
  const project = projectOf("p-1", "c-1", "Site", { description: "A site." });
  const pending = render(project);
  type(input("Name"), "Mine");
  type(textarea(), "My description");
  await click(buttonText("Save"));
  expect(sentOf(pending, "update_project")).toMatchObject({ name: "Mine", description: "My description", base_versions: { name: 1, description: 1 } });
  await act(async () => pending[0]!.reject(new CodedError("changed elsewhere", "write_conflict", { fields: "name", versions: '{"name":4}' })));
  // The other client's change has not arrived yet.
  expect(input("Name").value).toBe("Mine");
  act(() => pending.store.setState({ projects: new Map([["p-1", projectOf("p-1", "c-1", "Theirs", { description: "A site.", versions: { name: 4, description: 1 } })]]) }));
  expect(input("Name").value).toBe("Theirs");
  expect(textarea().value).toBe("My description");
  await click(buttonText("Save"));
  const retry = pending[1]!.body;
  expect(retry).toEqual({ type: "update_project", project: "p-1", description: "My description", base_versions: { description: 1 } });
  await act(async () => pending[1]!.resolve({ type: "ack", id: null }));
  type(input("Name"), "Mine again");
  await click(buttonText("Save"));
  expect(pending[2]!.body).toMatchObject({ name: "Mine again", base_versions: { name: 4, description: 1 } });
});

/** The store's project as another client left it, with `moved` at a newer version. */
const elsewhere = (pending: ReturnType<typeof render>, name: string, moved: Record<string, number>, extra: Parameters<typeof projectOf>[3] = {}) =>
  act(() => pending.store.setState({ projects: new Map([["p-1", projectOf("p-1", "c-1", name, { versions: { ...PROJECT_VERSIONS, ...moved }, ...extra })]]) }));

it("replaces a field with the current value as soon as the store shows a newer version of it, and bases the write on that", async () => {
  const pending = render(projectOf("p-1", "c-1", "Site"));
  type(input("Name"), "Mine");
  elsewhere(pending, "Theirs", { name: 6 });
  expect(input("Name").value).toBe("Theirs");
  type(input("Name"), "Renamed");
  await click(buttonText("Save"));
  expect(sentOf(pending, "update_project")).toEqual({ type: "update_project", project: "p-1", name: "Renamed", base_versions: { name: 6 } });
});

it("replaces edited tags with the current ones, and sends no tags", async () => {
  const pending = render(projectOf("p-1", "c-1", "Site", { tags: ["web"] }));
  const tags = input("Tags");
  type(tags, "mine");
  await act(async () => tags.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })));
  expect(document.body.textContent).toContain("mine");
  elsewhere(pending, "Site", { tags: 4 }, { tags: ["elsewhere"] });
  expect(document.body.textContent).not.toContain("mine");
  expect(document.body.textContent).toContain("elsewhere");
  type(input("Name"), "Renamed");
  await click(buttonText("Save"));
  expect(sentOf(pending, "update_project")).toEqual({ type: "update_project", project: "p-1", name: "Renamed", base_versions: { name: 1 } });
});

it("keeps an edit when a version moves over the value the form read, and bases the write on the new version", async () => {
  const pending = render(projectOf("p-1", "c-1", "Site"));
  type(input("Name"), "Mine");
  elsewhere(pending, "Site", { name: 6 });
  expect(input("Name").value).toBe("Mine");
  await click(buttonText("Save"));
  expect(sentOf(pending, "update_project")).toEqual({ type: "update_project", project: "p-1", name: "Mine", base_versions: { name: 6 } });
});

it("names a field the user had edited when it is replaced, and says nothing of one they had not touched", async () => {
  const pending = render(projectOf("p-1", "c-1", "Site", { description: "A site." }));
  type(input("Name"), "Mine");
  elsewhere(pending, "Theirs", { name: 6, description: 4 }, { description: "Other." });
  expect(input("Name").value).toBe("Theirs");
  expect(textarea().value).toBe("Other.");
  await settle(AFTER_FOCUS_MOVE_DELAY_MS + 150);
  const said = Array.from(document.querySelectorAll("[role=status]")).map((region) => region.textContent).filter(Boolean);
  expect(said).toEqual(["Changed elsewhere: Name. The current value is shown."]);
});
