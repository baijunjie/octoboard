// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { expect, it, onTestFinished, vi } from "vitest";

import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { errorAlerts } from "../errorAlerts";
import { ErrorAlerts } from "./ErrorAlerts";
import { DirectoryPicker } from "./DirectoryPicker";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// Enter lists the path typed, which needs focus in the path field as the picker opens.
it("opens with focus in the path field", async () => {
  act(() => setInteractionModality("pointer"));
  const daemon: Daemon = {
    store: createStateStore(),
    request: vi.fn(async () => ({ type: "dir_listing", path: "/home", entries: [] })) as Daemon["request"],
    toastError: vi.fn(),
    toastNotice: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  await act(async () =>
    root.render(
      <DaemonProvider value={daemon}>
        <DirectoryPicker title="Choose directory" onPick={() => {}} onClose={() => {}} />
      </DaemonProvider>,
    ),
  );
  expect(document.activeElement?.getAttribute("aria-label")).toBe("Directory path");
});

it("reports the same immediate listing failure again after acknowledgement and retry", async () => {
  const daemon = { store: createStateStore(), request: vi.fn().mockRejectedValue(new Error("Permission denied")) } as unknown as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
  });
  await act(async () => root.render(<DaemonProvider value={daemon}><DirectoryPicker title="Choose directory" onPick={() => {}} onClose={() => {}} /><ErrorAlerts daemon={daemon} /></DaemonProvider>));
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("Permission denied");
  act(() => errorAlerts.acknowledge());
  await act(async () => document.querySelector("form")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));
  expect(daemon.request).toHaveBeenCalledTimes(2);
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("Permission denied");
});
