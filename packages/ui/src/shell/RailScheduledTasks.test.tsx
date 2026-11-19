// @vitest-environment jsdom
// What the rail's button draws of the count of running tasks; which tasks count is `useRunningTaskCount.test.tsx`'s.
import { expect, it } from "vitest";

import { sessionOf } from "../fixtures/builders";
import { scheduledTaskOf } from "../fixtures/scheduledTaskBuilders";
import { LanguageProvider } from "../i18n/react";
import type { Session } from "../protocol";
import type { State } from "../store";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { RailScheduledTasks } from "./RailScheduledTasks";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const session = (id: string, status: Parameters<typeof sessionOf>[4]) => sessionOf(id, "c", undefined, id, status);
const taskOf = (id: string, runSessions: string[]) => scheduledTaskOf(id, "c", { run_sessions: runSessions });
const held = (...sessions: Session[]) => new Map(sessions.map((s) => [s.id, s]));

function mount(state: Partial<State>) {
  const daemon = testDaemon({ state });
  const container = mountHost(<LanguageProvider><RailScheduledTasks shown={false} onPress={() => {}} /></LanguageProvider>, { daemon });
  return { daemon, container };
}

const button = (container: ParentNode) => container.querySelector<HTMLButtonElement>("button")!;

it("is named Scheduled tasks and draws no badge where none is running", () => {
  const { container } = mount({ scheduledTasks: new Map([["t-1", taskOf("t-1", ["s-1"])]]), sessions: held(session("s-1", "idle")) });
  expect(button(container).getAttribute("aria-label")).toBe("Scheduled tasks");
  expect(container.textContent).toBe("");
});

it("draws the count in the badge and in the button's name", () => {
  const { container } = mount({
    scheduledTasks: new Map([["t-1", taskOf("t-1", ["s-1"])], ["t-2", taskOf("t-2", ["s-2"])]]),
    sessions: held(session("s-1", "working"), session("s-2", "starting")),
  });
  expect(button(container).getAttribute("aria-label")).toBe("Scheduled tasks, running: 2");
  expect(container.textContent).toBe("2");
});

it("shows the overflow label past 99", () => {
  const ids = Array.from({ length: 100 }, (_, i) => `s-${i}`);
  const { container } = mount({
    scheduledTasks: new Map(ids.map((id) => [`t-${id}`, taskOf(`t-${id}`, [id])])),
    sessions: held(...ids.map((id) => session(id, "working"))),
  });
  expect(container.textContent).toBe("99+");
});
