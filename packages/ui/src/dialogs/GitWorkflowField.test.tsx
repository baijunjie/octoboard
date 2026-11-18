// @vitest-environment jsdom
import { expect, it, vi } from "vitest";

import { DEFAULT_GIT_WORKFLOW, FOLLOWING_GIT_WORKFLOW } from "../git/gitWorkflow";
import { LanguageProvider } from "../i18n/react";
import type { GitWorkflowOverride, GitWorkflowSettings } from "../protocol";
import { DEFAULT_SETTINGS } from "../store";
import { click, optionElements, pickerNamed } from "../test/dom";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { GitWorkflowField } from "./GitWorkflowField";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(value: GitWorkflowOverride, global: GitWorkflowSettings = DEFAULT_GIT_WORKFLOW) {
  const onChange = vi.fn();
  const daemon = testDaemon({ state: { settings: { ...DEFAULT_SETTINGS, git_workflow: global } } });
  const container = mountHost(<LanguageProvider><GitWorkflowField value={value} onChange={onChange} /></LanguageProvider>, { daemon });
  return { container, onChange };
}

const choose = async (picker: string, option: string) => {
  await click(pickerNamed(picker));
  await click(optionElements().find((candidate) => candidate.textContent?.startsWith(option)));
};

it.each([
  { name: "Where sessions develop", global: { ...DEFAULT_GIT_WORKFLOW, development: "in_place" as const }, badge: "On the current branch" },
  { name: "Delivery", global: { ...DEFAULT_GIT_WORKFLOW, development: "branch" as const, delivery: "merge" as const }, badge: "Merge locally" },
])("badges the application-wide choice of $name that a project follows, for a value this build knows", ({ name, global, badge }) => {
  mount(FOLLOWING_GIT_WORKFLOW, global);
  expect(pickerNamed(name).textContent).toContain(badge);
});

it("keeps the rows of a project that follows an application-wide development and delivery this build does not know, with no badge and no raw value", () => {
  const { container } = mount(FOLLOWING_GIT_WORKFLOW, { ...DEFAULT_GIT_WORKFLOW, development: "from_a_newer_daemon", delivery: "carrier_pigeon" });
  for (const name of ["Where sessions develop", "Delivery"]) {
    const text = pickerNamed(name).textContent;
    expect(text, name).toContain("Same as global setting");
    expect(text, name).not.toMatch(/from_a_newer_daemon|carrier_pigeon/);
  }
  // Neither choice is badged with a value: the badge is the known choice's label, and an unknown one has none.
  expect(pickerNamed("Where sessions develop").textContent).not.toMatch(/branch|worktree/i);
  expect(pickerNamed("Delivery").textContent).not.toMatch(/merge|pull request|Ask each time/i);
  expect(container.textContent).not.toMatch(/from_a_newer_daemon|carrier_pigeon/);
});

it("keeps the rows of a project holding a development and a delivery of its own this build does not know, showing an empty value, and writes a known one chosen", async () => {
  const value: GitWorkflowOverride = { ...FOLLOWING_GIT_WORKFLOW, development: "from_a_newer_daemon", delivery: "carrier_pigeon" };
  const { onChange } = mount(value);
  for (const name of ["Where sessions develop", "Delivery"]) {
    const text = pickerNamed(name).textContent;
    expect(text, name).not.toMatch(/from_a_newer_daemon|carrier_pigeon|Same as global setting/);
  }
  expect(onChange).not.toHaveBeenCalled();
  await choose("Where sessions develop", "On the current branch");
  expect(onChange).toHaveBeenLastCalledWith({ ...value, development: "in_place" });
  await choose("Delivery", "Merge locally");
  expect(onChange).toHaveBeenLastCalledWith({ ...value, delivery: "merge" });
});
