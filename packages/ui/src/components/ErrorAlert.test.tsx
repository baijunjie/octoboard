// @vitest-environment jsdom
import { act, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { DaemonRequestError } from "../daemon-client";
import { errorAlerts } from "../errorAlerts";
import { format } from "../i18n/catalog";
import { ViewerHost } from "../viewer/viewerTestSupport";
import { Failure } from "../viewer/viewerStates";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(children: ReactNode): HTMLDivElement {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
  });
  act(() => root.render(<ViewerHost>{children}</ViewerHost>));
  return container;
}

it("retains the full structured failure and acknowledgement when the viewer cannot shorten it", () => {
  const error = new DaemonRequestError("Environment unavailable", "shell_environment_timeout", { shell: "/bin/zsh", command: "env -0", timeout: "10s" });
  const container = mount(<Failure title="Could not read the file" message={error} path="README.md" />);
  expect(container.textContent).toContain("Could not read the file");
  expect(container.querySelector("code")?.textContent).toBe("/bin/zsh -l -i -c 'env -0'");
  expect(container.textContent).toContain("10s");
  for (const key of ["daemon.shell_environment_timeout.cause", "daemon.shell_environment_timeout.consequence", "daemon.shell_environment_timeout.recovery"] as const) {
    expect(container.textContent?.split(format("en", key))).toHaveLength(2);
  }
  expect(errorAlerts.getSnapshot()[0].error).toBe(error);
});

it("uses the current path to shorten visible failure copy without losing the acknowledgement metadata", () => {
  const error = new DaemonRequestError("Read failed", "file_not_found", { path: "src/gone.ts" });
  const container = mount(<Failure title="Could not read the file" message={error} path="src/gone.ts" />);
  expect(container.textContent).toContain(format("en", "viewer.error.fileNotFound"));
  expect(container.textContent).not.toContain("src/gone.ts");
  expect(errorAlerts.getSnapshot()[0].error).toBe(error);
});
