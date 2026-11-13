// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import type { ConnectionState } from "../daemon-client";
import { CodedError } from "../errors/codedError";
import { LanguageProvider } from "../i18n/react";
import type { DaemonUpdateStatus } from "../protocol";
import { applyDaemonEvent, DaemonProvider } from "../store";
import { testDaemon } from "../test/testDaemon";
import { fakeTimers } from "../test/clock";
import { RESTART_RECONNECT_BUDGET_MS, useReloadAfterRestart } from "./useReloadAfterRestart";

vi.mock("../platform/reloadPage", () => ({ reloadPage: vi.fn() }));
const { reloadPage } = await import("../platform/reloadPage");

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => vi.mocked(reloadPage).mockClear());

const IDLE: DaemonUpdateStatus = { phase: "idle", available: null, progress: null, installed: null, last_check: null, install: null, manual: null, updated_from: null };

function mount(version: string) {
  const reconnect = vi.fn();
  const daemon = testDaemon({ state: { daemon: { version, generation: 1 }, daemonUpdate: IDLE }, reconnect });
  let current!: ReturnType<typeof useReloadAfterRestart>;
  const Probe = () => {
    current = useReloadAfterRestart();
    return null;
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() =>
    root.render(
      <LanguageProvider>
        <DaemonProvider value={daemon}>
          <Probe />
        </DaemonProvider>
      </LanguageProvider>,
    ),
  );
  /** The daemon coming back: its greeting, then its status. */
  const comesBack = (to: string) => {
    act(() => applyDaemonEvent(daemon.store, { type: "hello", daemon_version: to, generation: 1 } as never));
    act(() => applyDaemonEvent(daemon.store, { type: "daemon_update_status_updated", status: IDLE }));
  };
  const connection = (state: ConnectionState) => act(() => daemon.store.setState({ connectionState: state }));
  return { daemon, reconnect, connection, restart: current.restart, comesBack, hook: () => current };
}

it("reloads the page that asked once the daemon is back on the target version with its own status", async () => {
  const { hook, comesBack } = mount("0.1.0");
  comesBack("0.2.0");
  expect(reloadPage).not.toHaveBeenCalled();
  await act(async () => void (await hook().restart("0.3.0", async () => {})));
  comesBack("0.3.0");
  expect(reloadPage).toHaveBeenCalledOnce();
});

it("ends the wait when the daemon comes back on another version than the target, leaving the page to the notice", async () => {
  const { hook, comesBack, connection } = mount("0.1.0");
  await act(async () => void (await hook().restart("0.3.0", async () => {})));
  comesBack("0.2.5");
  expect(reloadPage).not.toHaveBeenCalled();
  comesBack("0.3.0");
  connection("incompatible");
  expect(reloadPage).not.toHaveBeenCalled();
});

it("waits for the status of the daemon that greeted, not the previous one's", async () => {
  const { daemon, hook } = mount("0.1.0");
  await act(async () => void (await hook().restart("0.2.0", async () => {})));
  act(() => applyDaemonEvent(daemon.store, { type: "hello", daemon_version: "0.2.0", generation: 1 } as never));
  expect(reloadPage).not.toHaveBeenCalled();
  act(() => applyDaemonEvent(daemon.store, { type: "daemon_update_status_updated", status: IDLE }));
  expect(reloadPage).toHaveBeenCalledOnce();
});

it("reloads on any other version when the target is not known, and not on the version it was asked from (a rollback)", async () => {
  const { hook, comesBack } = mount("0.1.0");
  await act(async () => void (await hook().restart(null, async () => {})));
  comesBack("0.1.0");
  expect(reloadPage).not.toHaveBeenCalled();
  comesBack("0.2.0");
  expect(reloadPage).toHaveBeenCalledOnce();
});

it("stops waiting when the daemon refuses the restart, but not when the request is only cut short", async () => {
  const refused = mount("0.1.0");
  await act(async () => {
    await refused.hook().restart("0.2.0", async () => { throw new CodedError("Shutting down.", "daemon_shutting_down", {}); }).catch(() => {});
  });
  refused.comesBack("0.2.0");
  expect(reloadPage).not.toHaveBeenCalled();

  const cut = mount("0.1.0");
  await act(async () => {
    await cut.hook().restart("0.2.0", async () => { throw new CodedError("closed", "connection_closed", {}); }).catch(() => {});
  });
  cut.comesBack("0.2.0");
  expect(reloadPage).toHaveBeenCalledOnce();
});

it("marks the restart as awaited in the store, takes it back when the daemon refuses, keeps it when the request is only cut short, and ends it at the next hello", async () => {
  const refused = mount("0.1.0");
  let seenWhileSending: boolean | undefined;
  await act(async () => {
    await refused.hook().restart("0.2.0", async () => { seenWhileSending = refused.daemon.store.getState().restartAwaited; throw new CodedError("Shutting down.", "daemon_shutting_down", {}); }).catch(() => {});
  });
  expect(seenWhileSending).toBe(true);
  expect(refused.daemon.store.getState().restartAwaited).toBe(false);

  const cut = mount("0.1.0");
  await act(async () => {
    await cut.hook().restart("0.2.0", async () => { throw new CodedError("closed", "connection_closed", {}); }).catch(() => {});
  });
  expect(cut.daemon.store.getState().restartAwaited).toBe(true);
  act(() => applyDaemonEvent(cut.daemon.store, { type: "hello", daemon_version: "0.2.0", generation: 1 } as never));
  expect(cut.daemon.store.getState().restartAwaited).toBe(false);
});

it("resolves with what the restart request answered, and passes a refusal on to the caller", async () => {
  const { hook } = mount("0.1.0");
  let answered: string | undefined;
  await act(async () => void (answered = await hook().restart("0.2.0", async () => "sent")));
  expect(answered).toBe("sent");
  const refusal = new CodedError("Shutting down.", "daemon_shutting_down", {});
  let caught: unknown;
  await act(async () => void (await hook().restart("0.2.0", async () => { throw refusal; }).catch((error) => (caught = error))));
  expect(caught).toBe(refusal);
});

it("reconnects the connection each time it closes while the restart is awaited, until the budget after the restart is spent", async () => {
  fakeTimers();
  const { hook, reconnect, connection } = mount("0.1.0");
  await act(async () => void (await hook().restart("0.2.0", async () => {})));
  expect(reconnect).not.toHaveBeenCalled();
  connection("closed");
  expect(reconnect).toHaveBeenCalledTimes(1);
  connection("connecting");
  connection("closed");
  expect(reconnect).toHaveBeenCalledTimes(2);
  await act(async () => void (await vi.advanceTimersByTimeAsync(RESTART_RECONNECT_BUDGET_MS - 1_000)));
  connection("reconnecting");
  connection("closed");
  expect(reconnect).toHaveBeenCalledTimes(3);
  await act(async () => void (await vi.advanceTimersByTimeAsync(2_000)));
  connection("reconnecting");
  connection("closed");
  // Past the budget the connection's own Retry is the way back.
  expect(reconnect).toHaveBeenCalledTimes(3);
});

it("counts the budget from the moment the restart was sent, not from when the hook mounted", async () => {
  fakeTimers();
  const { hook, reconnect, connection } = mount("0.1.0");
  await act(async () => void (await vi.advanceTimersByTimeAsync(RESTART_RECONNECT_BUDGET_MS * 2)));
  await act(async () => void (await hook().restart("0.2.0", async () => {})));
  connection("closed");
  expect(reconnect).toHaveBeenCalledOnce();
});

it("does not reconnect a closed connection when no restart is awaited, or once the daemon refused it", async () => {
  fakeTimers();
  const idle = mount("0.1.0");
  idle.connection("closed");
  expect(idle.reconnect).not.toHaveBeenCalled();

  const refused = mount("0.1.0");
  await act(async () => {
    await refused.hook().restart("0.2.0", async () => { throw new CodedError("Shutting down.", "daemon_shutting_down", {}); }).catch(() => {});
  });
  refused.connection("closed");
  expect(refused.reconnect).not.toHaveBeenCalled();
});

it("stops reconnecting once the page was reloaded for the new daemon", async () => {
  fakeTimers();
  const { hook, reconnect, connection, comesBack } = mount("0.1.0");
  await act(async () => void (await hook().restart("0.2.0", async () => {})));
  comesBack("0.2.0");
  expect(reloadPage).toHaveBeenCalledOnce();
  connection("closed");
  expect(reconnect).not.toHaveBeenCalled();
});

it("reloads the page once when the daemon comes back speaking another protocol generation while a restart is awaited", async () => {
  const { hook, connection } = mount("0.1.0");
  await act(async () => void (await hook().restart("0.2.0", async () => {})));
  connection("incompatible");
  expect(reloadPage).toHaveBeenCalledOnce();
  // The wait has ended: nothing reloads again.
  connection("connecting");
  connection("incompatible");
  expect(reloadPage).toHaveBeenCalledOnce();
});

it("does not reload for an incompatible generation when no restart is awaited", () => {
  const { connection } = mount("0.1.0");
  connection("incompatible");
  expect(reloadPage).not.toHaveBeenCalled();
});
