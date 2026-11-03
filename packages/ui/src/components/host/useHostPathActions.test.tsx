// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { CodedError } from "../../errors/codedError";
import { DaemonProvider } from "../../store";
import { testDaemon } from "../../test/testDaemon";
import { errorAlerts } from "../alerts/errorAlerts";
import { type HostPath, useHostPathActions } from "./useHostPathActions";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** Runs a host action that the daemon refuses with `error`, and returns the title and remedy of the error alert it
 * raises. */
async function refused(type: "open_project_path" | "reveal_project_path", error: Error): Promise<[string | undefined, string | undefined]> {
  const daemon = testDaemon({ request: vi.fn().mockRejectedValue(error), showNotice: vi.fn() });
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
    while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
  });
  act(() => root.render(<DaemonProvider value={daemon}><Subject /></DaemonProvider>));
  await act(async () => hostAction(type, { project: "p1", path: "src/a.ts" }));
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
  const [alert] = errorAlerts.getSnapshot();
  return [alert.title as string, alert.recovery];
}

it.each(["file_not_found", "path_not_found"])("gives a %s its own remedy, finding the item again, not the remedy of a path typed in a form", async (code) => {
  const [title, recovery] = await refused("reveal_project_path", new CodedError("gone", code, { path: "src/a.ts" }));
  expect(title).toBe("Could not open in the file manager");
  expect(recovery).toBe("hostPathGone");
});

it("leaves any other refusal to the remedy of its code", async () => {
  const [title, recovery] = await refused("open_project_path", new CodedError("no", "path_open_failed", { path: "src/a.ts" }));
  expect(title).toBe("Could not open in the default application");
  expect(recovery).toBeUndefined();
});
