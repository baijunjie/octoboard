// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { fakeTimers } from "../test/clock";
import type { DialogRequest } from "../dialogs/dialogRequest";
import { consoleOf } from "../fixtures/builders";
import { DaemonProvider } from "../store";
import { testDaemon } from "../test/testDaemon";
import { CREATED_CONSOLE_WAIT_MS, useDialogRouting } from "./useDialogRouting";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const first = consoleOf("c1", "First");
const created = consoleOf("c2", "Created");

function mount() {
  fakeTimers();
  const switchConsole = vi.fn();
  const setDialogRequest = vi.fn();
  const daemon = testDaemon();
  let routing!: ReturnType<typeof useDialogRouting>;
  let consoleList = [first];
  const Probe = () => {
    routing = useDialogRouting({ consoles: new Map(consoleList.map((c) => [c.id, c])), consoleList, sidebarOpen: false, setDialogRequest, switchConsole });
    return null;
  };
  const root = createRoot(document.body.appendChild(document.createElement("div")));
  const render = () => act(() => root.render(<DaemonProvider value={daemon}><Probe /></DaemonProvider>));
  render();
  onTestFinished(() => {
    act(() => root.unmount());
  });
  return {
    routing: () => routing,
    switchConsole,
    appear: () => {
      consoleList = [first, created];
      render();
    },
  };
}

const newConsole: DialogRequest = { kind: "new-console" };

it("switches to the console the new-console dialog created, when its broadcast trails the close within the wait", () => {
  const hook = mount();
  act(() => hook.routing().openDialog(newConsole));
  act(() => hook.routing().closeDialog());
  act(() => void vi.advanceTimersByTime(CREATED_CONSOLE_WAIT_MS - 1));
  hook.appear();
  expect(hook.switchConsole).toHaveBeenCalledWith(created.id);
});

it("keeps looking for the created console when another dialog is opened before the wait after the close ends", () => {
  const hook = mount();
  act(() => hook.routing().openDialog(newConsole));
  act(() => hook.routing().closeDialog());
  act(() => void vi.advanceTimersByTime(CREATED_CONSOLE_WAIT_MS - 1));
  act(() => hook.routing().openDialog({ kind: "edit-console", console: first }));
  // The wait that started at the close would have ended by now, and the one the other dialog's opening restarted has not.
  act(() => void vi.advanceTimersByTime(CREATED_CONSOLE_WAIT_MS - 1));
  hook.appear();
  expect(hook.switchConsole).toHaveBeenCalledWith(created.id);
});

it("stops looking for the created console once the wait after the close has ended", () => {
  const hook = mount();
  act(() => hook.routing().openDialog(newConsole));
  act(() => hook.routing().closeDialog());
  act(() => void vi.advanceTimersByTime(CREATED_CONSOLE_WAIT_MS));
  hook.appear();
  expect(hook.switchConsole).not.toHaveBeenCalled();
});
