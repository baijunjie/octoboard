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
  expect(Array.from(document.body.querySelectorAll('[role="group"]')).some((g) => document.getElementById(g.getAttribute("aria-labelledby")!)?.textContent === "Worktrees")).toBe(true);
  for (const name of ["Use worktrees for development", "Rebase before merging", "Revert gate", "Delivery"]) {
    expect(pickerNamed(name).textContent).toContain("Same as app setting");
  }
  expect(pickerNamed("Delivery").textContent).toContain("Ask each time");
  expect(pickerNamed("Use worktrees for development").textContent).toContain("On");
});

it("lists the pickers in order: Use worktrees, Rebase before merging, Revert gate, Delivery", () => {
  render();
  const pickers = ["Use worktrees for development", "Rebase before merging", "Revert gate", "Delivery"].map(pickerNamed);
  pickers.slice(1).forEach((picker, i) => expect(pickers[i].compareDocumentPosition(picker) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy());
});

it("adds with only the items set to a value of their own", async () => {
  const pending = render();
  await fillDirectory(pending);
  await choose("Delivery", "Merge locally");
  await choose("Use worktrees for development", "Off");
  await save("Add");
  expect(sentOf(pending, "add_project")).toMatchObject({ worktrees: { use_worktrees: false, delivery: "merge" } });
  expect((sentOf(pending, "add_project") as { worktrees: object }).worktrees).not.toHaveProperty("revert_gate");
});

it("adds without worktrees when every item follows", async () => {
  const pending = render();
  await fillDirectory(pending);
  await save("Add");
  expect(sentOf(pending, "add_project")).not.toHaveProperty("worktrees");
});

it("shows the project's own values and sends only the changed items when editing, null for one back to following", async () => {
  const pending = render(projectOf("p-1", "c-1", "Site", { worktrees: { use_worktrees: false, rebase_before_merge: null, delivery: "merge", revert_gate: null } }));
  expect(pickerNamed("Use worktrees for development").textContent).toContain("Off");
  await choose("Use worktrees for development", "Same as app setting");
  expect(pickerNamed("Delivery").textContent).toContain("Merge locally");
  await choose("Delivery", "Same as app setting");
  await choose("Revert gate", "Off");
  await save("Save");
  expect(sentOf(pending, "update_project")).toMatchObject({ worktrees: { use_worktrees: null, delivery: null, revert_gate: false } });
  expect((sentOf(pending, "update_project") as { worktrees: object }).worktrees).not.toHaveProperty("rebase_before_merge");
});

it("sends no worktrees when editing leaves them alone", async () => {
  const pending = render(stored);
  await save("Save");
  expect(sentOf(pending, "update_project")).not.toHaveProperty("worktrees");
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

it("shows only Use worktrees while the project's own Use worktrees is off, and keeps the hidden values", async () => {
  const pending = render(projectOf("p-1", "c-1", "Site", { worktrees: { use_worktrees: false, rebase_before_merge: false, delivery: "merge", revert_gate: null } }));
  for (const name of ["Rebase before merging", "Revert gate", "Delivery"]) expect(shown(name)).toBe(false);
  expect(shown("Use worktrees for development")).toBe(true);
  await save("Save");
  expect(sentOf(pending, "update_project")).not.toHaveProperty("worktrees");
});

it("shows them again when Use worktrees is turned back on", async () => {
  render(projectOf("p-1", "c-1", "Site", { worktrees: { use_worktrees: false, rebase_before_merge: null, delivery: "merge", revert_gate: null } }));
  await choose("Use worktrees for development", "On");
  for (const name of ["Rebase before merging", "Revert gate", "Delivery"]) expect(shown(name)).toBe(true);
  expect(pickerNamed("Delivery").textContent).toContain("Merge locally");
});

it("judges a project that follows by the application-wide value", async () => {
  const pending = render();
  expect(shown("Delivery")).toBe(true);
  await act(async () => pending.store.setState((state) => ({ settings: { ...state.settings, worktrees: { ...state.settings.worktrees, use_worktrees: false } } })));
  expect(shown("Delivery")).toBe(false);
});

it("hands focus to Use worktrees when the picker it was on is hidden from elsewhere", async () => {
  const pending = render();
  await act(async () => pickerNamed("Delivery").focus());
  expect(document.activeElement).toBe(pickerNamed("Delivery"));
  await act(async () => pending.store.setState((state) => ({ settings: { ...state.settings, worktrees: { ...state.settings.worktrees, use_worktrees: false } } })));
  expect(document.activeElement).toBe(pickerNamed("Use worktrees for development"));
});
