// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import { CodedError } from "../errors/codedError";
import { LanguageProvider } from "../i18n/react";
import type { DaemonUpdateStatus } from "../protocol";
import { applyDaemonEvent, DaemonProvider } from "../store";
import { testDaemon } from "../test/testDaemon";
import { useReloadAfterRestart } from "./useReloadAfterRestart";

vi.mock("./reloadPage", () => ({ reloadPage: vi.fn() }));
const { reloadPage } = await import("./reloadPage");

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => vi.mocked(reloadPage).mockClear());

const IDLE: DaemonUpdateStatus = { phase: "idle", available: null, progress: null, installed: null, last_check: null, install: null, manual: null, updated_from: null };

function mount(version: string) {
  const daemon = testDaemon({ state: { daemon: { version, generation: 1 }, daemonUpdate: IDLE } });
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
  return { daemon, restart: current.restart, comesBack, hook: () => current };
}

it("reloads the page that asked once the daemon is back on the target version with its own status, and no other", async () => {
  const { hook, comesBack } = mount("0.1.0");
  comesBack("0.2.0");
  expect(reloadPage).not.toHaveBeenCalled();
  await act(async () => void (await hook().restart("0.3.0", async () => {})));
  comesBack("0.2.5");
  expect(reloadPage).not.toHaveBeenCalled();
  comesBack("0.3.0");
  expect(reloadPage).toHaveBeenCalledOnce();
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
