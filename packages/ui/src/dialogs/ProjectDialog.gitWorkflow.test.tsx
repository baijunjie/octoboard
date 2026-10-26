// @vitest-environment jsdom
import { act } from "react";
import { expect, it } from "vitest";

import { projectOf } from "../gallery/fixtures/builders";
import { answer, click, fillDirectory, input, pickerNamed, render, sentOf, settle, stored, type } from "./projectDialogTestSupport";

const choose = async (picker: string, option: string) => {
  await click(pickerNamed(picker));
  await click(Array.from(document.body.querySelectorAll<HTMLElement>('[role="option"]')).find((o) => o.textContent?.startsWith(option)));
};
const save = (label: string) => click(Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === label));

it("offers every item as following the application-wide value, with that value in a badge", () => {
  render();
  expect(Array.from(document.body.querySelectorAll('[role="group"]')).some((g) => document.getElementById(g.getAttribute("aria-labelledby")!)?.textContent === "Git workflow")).toBe(true);
  for (const name of ["Where sessions develop", "Rebase before merging", "Delivery", "Revert gate"]) {
    expect(pickerNamed(name).textContent).toContain("Same as app setting");
  }
  expect(pickerNamed("Delivery").textContent).toContain("Ask each time");
  expect(pickerNamed("Where sessions develop").textContent).toContain("In a worktree of its own");
});

it("lists the pickers in order: Where sessions develop, Rebase before merging, Delivery, Revert gate", () => {
  render();
  const pickers = ["Where sessions develop", "Rebase before merging", "Delivery", "Revert gate"].map(pickerNamed);
  pickers.slice(1).forEach((picker, i) => expect(pickers[i].compareDocumentPosition(picker) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy());
});

it("adds with only the items set to a value of their own", async () => {
  const pending = render();
  await fillDirectory(pending);
  await choose("Delivery", "Merge locally");
  await choose("Where sessions develop", "On a branch of its own");
  await save("Add");
  expect(sentOf(pending, "add_project")).toMatchObject({ git_workflow: { development: "branch", delivery: "merge" } });
  expect((sentOf(pending, "add_project") as { git_workflow: object }).git_workflow).not.toHaveProperty("revert_gate");
});

it("adds without git_workflow when every item follows", async () => {
  const pending = render();
  await fillDirectory(pending);
  await save("Add");
  expect(sentOf(pending, "add_project")).not.toHaveProperty("git_workflow");
});

it("shows the project's own values and sends only the changed items when editing, null for one back to following", async () => {
  const pending = render(projectOf("p-1", "c-1", "Site", { git_workflow: { development: "in_place", rebase_before_merge: null, delivery: "merge", revert_gate: null } }));
  expect(pickerNamed("Where sessions develop").textContent).toContain("On the current branch");
  await choose("Where sessions develop", "Same as app setting");
  expect(pickerNamed("Delivery").textContent).toContain("Merge locally");
  await choose("Delivery", "Same as app setting");
  await choose("Revert gate", "Off");
  await save("Save");
  expect(sentOf(pending, "update_project")).toMatchObject({ git_workflow: { development: null, delivery: null, revert_gate: false } });
  expect((sentOf(pending, "update_project") as { git_workflow: object }).git_workflow).not.toHaveProperty("rebase_before_merge");
});

it("sends no git_workflow when editing leaves them alone", async () => {
  const pending = render(stored);
  await save("Save");
  expect(sentOf(pending, "update_project")).not.toHaveProperty("git_workflow");
});

it("keeps the pickers disabled while the form is locked and enables them once it opens", async () => {
  const pending = render();
  type(input("Directory"), "~/code/x");
  const disabled = () => pickerNamed("Revert gate").hasAttribute("disabled") || pickerNamed("Revert gate").getAttribute("aria-disabled") === "true";
  expect(disabled()).toBe(true);
  await settle(500);
  await answer(pending[0], null);
  expect(disabled()).toBe(false);
});

const shown = (picker: string) => Array.from(document.body.querySelectorAll("[aria-label]")).some((el) => el.getAttribute("aria-label") === picker);

it("shows the development picker and the revert gate while the project's own development is on the current branch, and keeps the hidden values", async () => {
  const pending = render(projectOf("p-1", "c-1", "Site", { git_workflow: { development: "in_place", rebase_before_merge: false, delivery: "merge", revert_gate: null } }));
  for (const name of ["Rebase before merging", "Delivery"]) expect(shown(name)).toBe(false);
  for (const name of ["Where sessions develop", "Revert gate"]) expect(shown(name)).toBe(true);
  await save("Save");
  expect(sentOf(pending, "update_project")).not.toHaveProperty("git_workflow");
});

it.each(["On a branch of its own", "In a worktree of its own"])("shows rebase and delivery again when Where sessions develop is turned back to %s", async (option) => {
  render(projectOf("p-1", "c-1", "Site", { git_workflow: { development: "in_place", rebase_before_merge: null, delivery: "merge", revert_gate: null } }));
  await choose("Where sessions develop", option);
  for (const name of ["Rebase before merging", "Revert gate", "Delivery"]) expect(shown(name)).toBe(true);
  expect(pickerNamed("Delivery").textContent).toContain("Merge locally");
});

it("judges a project that follows by the application-wide value", async () => {
  const pending = render();
  expect(shown("Delivery")).toBe(true);
  await act(async () => pending.store.setState((state) => ({ settings: { ...state.settings, git_workflow: { ...state.settings.git_workflow, development: "in_place" } } })));
  expect(shown("Delivery")).toBe(false);
  expect(shown("Revert gate")).toBe(true);
});

it("hands focus to the development picker when the picker it was on is hidden from elsewhere", async () => {
  const pending = render();
  await act(async () => pickerNamed("Delivery").focus());
  expect(document.activeElement).toBe(pickerNamed("Delivery"));
  await act(async () => pending.store.setState((state) => ({ settings: { ...state.settings, git_workflow: { ...state.settings.git_workflow, development: "in_place" } } })));
  expect(document.activeElement).toBe(pickerNamed("Where sessions develop"));
});
