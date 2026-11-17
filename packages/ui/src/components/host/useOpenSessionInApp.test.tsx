// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { CodedError } from "../../errors/codedError";
import type { Session } from "../../protocol";
import { sessionOf } from "../../fixtures/builders";
import type { RequestHandler } from "../../requests";
import { DaemonProvider } from "../../store";
import { testDaemon } from "../../test/testDaemon";
import { errorAlerts } from "../alerts/errorAlerts";
import { useOpenSessionInApp } from "./useOpenSessionInApp";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mountHook(request: RequestHandler): (session: Session) => void {
  const daemon = testDaemon({ request, showNotice: vi.fn() });
  let open!: (session: Session) => void;
  function Subject() {
    open = useOpenSessionInApp();
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(<DaemonProvider value={daemon}><Subject /></DaemonProvider>));
  return (session) => open(session);
}

it("asks the daemon to open the session's conversation in its agent's app, by the session's id, and raises nothing", async () => {
  const request = vi.fn<RequestHandler>().mockResolvedValue({ type: "ack", id: null });
  const open = mountHook(request);

  await act(async () => open(sessionOf("s-1", "c", "p", "Title", "interrupted")));

  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "open_session_in_app", session: "s-1" });
  expect(errorAlerts.getSnapshot()).toHaveLength(0);
});

it("raises a refusal as an alert titled for the app, about the session", async () => {
  const request = vi.fn<RequestHandler>().mockRejectedValue(new CodedError("The application could not be opened: not signed in", "native_app_unavailable", { detail: "not signed in" }));
  const open = mountHook(request);

  await act(async () => open(sessionOf("s-1", "c", "p", "Title", "interrupted")));

  expect(errorAlerts.getSnapshot()).toHaveLength(1);
  const [alert] = errorAlerts.getSnapshot();
  expect(alert.title).toBe("Could not open the agent's app");
  expect(alert.session).toBe("s-1");
  expect((alert.error as CodedError).code).toBe("native_app_unavailable");
});
