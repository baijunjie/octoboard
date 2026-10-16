// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { expect, it, onTestFinished, vi } from "vitest";

import { createStateStore, DaemonProvider, type Daemon } from "../store";
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
