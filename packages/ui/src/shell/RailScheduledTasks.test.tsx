// @vitest-environment jsdom
// What the rail's button draws of the count of enabled tasks; which tasks count is `useEnabledTaskCount.test.tsx`'s.
import { expect, it } from "vitest";

import { scheduledTaskOf } from "../fixtures/scheduledTaskBuilders";
import { LanguageProvider } from "../i18n/react";
import type { ScheduledTask } from "../protocol";
import type { State } from "../store";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { RailScheduledTasks } from "./RailScheduledTasks";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const taskOf = (id: string, extra: Partial<ScheduledTask> = {}) => scheduledTaskOf(id, "c", extra);

function mount(state: Partial<State>) {
  const daemon = testDaemon({ state });
  const container = mountHost(<LanguageProvider><RailScheduledTasks shown={false} onPress={() => {}} /></LanguageProvider>, { daemon });
  return { daemon, container };
}

const button = (container: ParentNode) => container.querySelector<HTMLButtonElement>("button")!;

it.each<[string, Partial<State>]>([
  ["the tasks are not listed", { scheduledTasks: undefined }],
  ["no task exists", { scheduledTasks: new Map() }],
  ["every task is disabled", { scheduledTasks: new Map([["t-1", taskOf("t-1", { enabled: false })], ["t-2", taskOf("t-2", { enabled: false })]]) }],
])("is named Scheduled tasks and draws no badge where %s", (_, state) => {
  const { container } = mount(state);
  expect(button(container).getAttribute("aria-label")).toBe("Scheduled tasks");
  expect(container.textContent).toBe("");
});

it("draws the count of enabled tasks in the badge and in the button's name", () => {
  const { container } = mount({ scheduledTasks: new Map([["t-1", taskOf("t-1")], ["t-2", taskOf("t-2")], ["t-3", taskOf("t-3", { enabled: false })]]) });
  expect(button(container).getAttribute("aria-label")).toBe("Scheduled tasks, enabled: 2");
  expect(container.textContent).toBe("2");
});

it("shows the overflow label past 99", () => {
  const { container } = mount({ scheduledTasks: new Map(Array.from({ length: 100 }, (_, i) => [`t-${i}`, taskOf(`t-${i}`)] as const)) });
  expect(container.textContent).toBe("99+");
});

it("shows the count itself at 99", () => {
  const { container } = mount({ scheduledTasks: new Map(Array.from({ length: 99 }, (_, i) => [`t-${i}`, taskOf(`t-${i}`)] as const)) });
  expect(container.textContent).toBe("99");
});
