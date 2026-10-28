// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { DaemonRequestError } from "../daemon-client";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { type HostPath, useHostPathActions } from "./useHostPathActions";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** Runs a host action that the daemon refuses with `error`, and returns what the error alert was asked to show. */
async function refused(type: "open_project_path" | "reveal_project_path", error: Error): Promise<unknown[]> {
  const toastError = vi.fn();
  const daemon: Daemon = {
    store: createStateStore({}),
    request: vi.fn().mockRejectedValue(error),
    toastError,
    toastNotice: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
  let hostAction!: (type: "open_project_path" | "reveal_project_path", host: HostPath) => void;
  function Subject() {
    hostAction = useHostPathActions();
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(<DaemonProvider value={daemon}><Subject /></DaemonProvider>));
  await act(async () => hostAction(type, { project: "p1", path: "src/a.ts" }));
  expect(toastError).toHaveBeenCalledOnce();
  return toastError.mock.calls[0];
}

it.each(["file_not_found", "path_not_found"])("gives a %s its own remedy, finding the item again, not the remedy of a path typed in a form", async (code) => {
  const [, , title, recovery] = await refused("reveal_project_path", new DaemonRequestError("gone", code, { path: "src/a.ts" }));
  expect(title).toBe("Could not open in the file manager");
  expect(recovery).toBe("hostPathGone");
});

it("leaves any other refusal to the remedy of its code", async () => {
  const [, , title, recovery] = await refused("open_project_path", new DaemonRequestError("no", "path_open_failed", { path: "src/a.ts" }));
  expect(title).toBe("Could not open in the default application");
  expect(recovery).toBeUndefined();
});
