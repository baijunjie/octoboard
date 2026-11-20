// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { CodedError } from "../../errors/codedError";
import type { Session } from "../../protocol";
import { sessionOf } from "../../fixtures/builders";
import type { RequestHandler } from "../../requests";
import { DaemonProvider, type Daemon } from "../../store";
import { testDaemon } from "../../test/testDaemon";
import { errorAlerts } from "../alerts/errorAlerts";
import { useOpenSessionInApp } from "./useOpenSessionInApp";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

type Mounted = { open: (session: Session) => void; openDialog: ReturnType<typeof vi.fn<(session: Session, label: string) => void>>; daemon: Daemon };

function mountHookWith(request: RequestHandler): Mounted {
  const daemon = testDaemon({ request, showNotice: vi.fn() });
  const openDialog = vi.fn<(session: Session, label: string) => void>();
  let open!: (session: Session) => void;
  function Subject() {
    open = useOpenSessionInApp(openDialog);
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(<DaemonProvider value={daemon}><Subject /></DaemonProvider>));
  return { open: (session) => open(session), openDialog, daemon };
}

function mountHook(request: RequestHandler): (session: Session) => void {
  return mountHookWith(request).open;
}

it("asks the daemon to open the session's conversation in its agent's app, by the session's id, and raises nothing", async () => {
  const request = vi.fn<RequestHandler>().mockResolvedValue({ type: "ack", id: null });
  const open = mountHook(request);

  await act(async () => open(sessionOf("s-1", "c", "p", "Title", "archived")));

  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "open_session_in_app", session: "s-1" });
  expect(errorAlerts.getSnapshot()).toHaveLength(0);
});

it("raises a refusal as an alert titled for the app, about the session", async () => {
  const request = vi.fn<RequestHandler>().mockRejectedValue(new CodedError("The application could not be opened: not signed in", "native_app_unavailable", { detail: "not signed in" }));
  const open = mountHook(request);

  await act(async () => open(sessionOf("s-1", "c", "p", "Title", "archived")));

  expect(errorAlerts.getSnapshot()).toHaveLength(1);
  const [alert] = errorAlerts.getSnapshot();
  expect(alert.title).toBe("Could not open the agent's app");
  expect(alert.session).toBe("s-1");
  expect((alert.error as CodedError).code).toBe("native_app_unavailable");
});

it("opens an archived session at once, with no confirmation", async () => {
  const request = vi.fn<RequestHandler>().mockResolvedValue({ type: "ack", id: null });
  const { open, openDialog } = mountHookWith(request);

  await act(async () => open(sessionOf("s-1", "c", "p", "Title", "archived")));

  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "open_session_in_app", session: "s-1" });
  expect(openDialog).not.toHaveBeenCalled();
});

it.each(["interrupted", "idle"] as const)("asks first for a %s session, whatever its processes, sending nothing", async (status) => {
  const request = vi.fn<RequestHandler>().mockResolvedValue({ type: "ack", id: null });
  const { open, openDialog } = mountHookWith(request);
  const session = sessionOf("s-1", "c", "p", "Title", status);

  await act(async () => open(session));

  expect(openDialog).toHaveBeenCalledExactlyOnceWith(session, "Open in Claude app");
  expect(request).not.toHaveBeenCalled();
});

it("sends nothing and asks nothing for a non-archived session of an agent with no app, which offers no action", async () => {
  const request = vi.fn<RequestHandler>().mockResolvedValue({ type: "ack", id: null });
  const { open, openDialog } = mountHookWith(request);

  await act(async () => open(sessionOf("s-1", "c", "p", "Title", "idle", { agent: "test_agent" })));

  expect(request).not.toHaveBeenCalled();
  expect(openDialog).not.toHaveBeenCalled();
});
